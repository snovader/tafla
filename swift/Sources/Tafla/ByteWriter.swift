/// A field that a `TableBuilder` recorded, with its value
enum Field: Sendable {
    /// A scalar's little-endian bits, of `size` bytes
    case scalar(bits: UInt64, size: Int)
    case array(Elements)
    case string(String, utf8Count: Int)
    case structBytes([UInt8])
    /// The bytes of structs of `size` bytes each, aligned to `alignment`
    case structArray([UInt8], size: Int, alignment: Alignment)
    /// A table nested from its bytes, aligned to `alignment`
    case tableBytes([UInt8], alignment: Alignment)
    /// A table nested from a builder, aligned to `alignment`, whose fields
    /// follow this one among its parent's
    case table(TableHeader, alignment: Alignment)
    /// A table nested from a builder, aligned to `alignment`, whose fields
    /// are those of the builder's array, which it shares
    case sharedTable(fields: [Field], TableHeader, alignment: Alignment)
    case vector([NestedTable])
    /// A vector of `count` tables recorded in place, `length` bytes long with
    /// its count and the ends of its items, whose items follow it among its
    /// parent's fields, each a `table` field followed by its own
    case vectorInPlace(count: Int, length: UInt64)

    /// What a slot of `Fields` without a field holds, which is never
    /// written: a scalar of no size, which stops the program if it is
    static var unset: Field { .scalar(bits: 0, size: 0) }

    /// The alignment that the field needs from the table's start
    var alignment: Alignment {
        switch self {
        case .array(let values): values.alignment
        case .structArray(_, _, let alignment), .tableBytes(_, let alignment), .table(_, let alignment),
            .sharedTable(_, _, let alignment):
            alignment
        case .scalar, .string, .structBytes, .vector, .vectorInPlace: .one
        }
    }

    /// The padding before the field when it would start at `offset` in a
    /// table of `width`-byte offsets, which aligns an array's elements after
    /// its count, and a nested table's start
    func padding(at offset: UInt64, width: UInt64) -> UInt64 {
        switch self {
        case .array, .structArray: alignment.padding(at: offset + width)
        case .tableBytes, .table, .sharedTable: alignment.padding(at: offset)
        case .scalar, .string, .structBytes, .vector, .vectorInPlace: 0
        }
    }

    /// The field's byte length after its padding, in a table of `width`-byte
    /// offsets. A vector that nests a table many times can be longer than any
    /// table, or than a `UInt64` holds, so its length saturates.
    func byteLength(width: UInt64) -> UInt64 {
        switch self {
        case .scalar(_, let size):
            return UInt64(size)
        case .array(let values):
            return width + UInt64(values.byteCount)
        case .string(_, let utf8Count):
            return width + UInt64(utf8Count)
        case .structBytes(let bytes):
            return UInt64(bytes.count)
        case .structArray(let bytes, _, _):
            return width + UInt64(bytes.count)
        case .tableBytes(let bytes, _):
            return UInt64(bytes.count)
        case .table(let header, _), .sharedTable(_, let header, _):
            return UInt64(header.length)
        case .vector(let items):
            var length = width
            for item in items {
                let (sum, overflow) = length.addingReportingOverflow(width + item.length)
                length = overflow ? .max : sum
            }
            return length
        case .vectorInPlace(_, let length):
            return length
        }
    }

    /// Writes the field at the writer's position, after its padding, in a
    /// table of `offsetType` offsets. A table nested from a builder whose
    /// fields follow it takes them from `fields`, from `next`, and moves
    /// `next` past them.
    func write<Records: RandomAccessCollection<Field>>(
        to writer: inout ByteWriter,
        offsetType: OffsetType,
        following fields: Records,
        from next: inout Int
    ) where Records.Index == Int {
        switch self {
        case .scalar(let bits, let size):
            writer.writeScalar(bits, size: size)
        case .array(let values):
            writer.writeSlot(UInt32(values.count), offsetType: offsetType)
            values.write(to: &writer)
        case .string(let value, let utf8Count):
            writer.writeSlot(UInt32(utf8Count), offsetType: offsetType)
            writer.writeUTF8(of: value, count: utf8Count)
        case .structBytes(let bytes):
            writer.writeBytes(bytes)
        case .structArray(let bytes, let size, _):
            writer.writeSlot(UInt32(bytes.count / size), offsetType: offsetType)
            writer.writeBytes(bytes)
        case .tableBytes(let bytes, _):
            writer.writeBytes(bytes)
        case .table(let header, _):
            header.write(fields: fields, from: &next, to: &writer)
        case .sharedTable(let shared, let header, _):
            var first = 0
            header.write(fields: shared, from: &first, to: &writer)
        case .vector(let items):
            writeVector(items, to: &writer, offsetType: offsetType)
        case .vectorInPlace(let count, _):
            writeVector(count: count, following: fields, from: &next, to: &writer, offsetType: offsetType)
        }
    }

    /// Writes a vector's count, then each item followed by its end, counted
    /// from the vector's position, in the slots after the count
    private func writeVector(_ items: [NestedTable], to writer: inout ByteWriter, offsetType: OffsetType) {
        let position = writer.position
        let width = offsetType.size

        writer.writeSlot(UInt32(items.count), offsetType: offsetType)
        let ends = writer.reserve(items.count * width)

        for index in items.indices {
            items[index].write(to: &writer)
            writer.writeSlot(UInt32(writer.position - position), at: ends + index * width, offsetType: offsetType)
        }
    }

    /// Writes a vector of `count` tables recorded in place, as
    /// ``writeVector(_:to:offsetType:)`` writes their builders, from the
    /// fields from `next`: each item's `table` field and then its own. It's
    /// kept out of ``write(to:offsetType:following:from:)``, which is inlined
    /// into the writer of each table only while it's short.
    @inline(never)
    private func writeVector<Records: RandomAccessCollection<Field>>(
        count: Int,
        following fields: Records,
        from next: inout Int,
        to writer: inout ByteWriter,
        offsetType: OffsetType
    ) where Records.Index == Int {
        let position = writer.position
        let width = offsetType.size

        writer.writeSlot(UInt32(count), offsetType: offsetType)
        let ends = writer.reserve(count * width)

        for index in 0..<count {
            guard case .table(let header, _) = fields[next] else {
                preconditionFailure("A vector's item is a table")
            }
            next += 1

            header.write(fields: fields, from: &next, to: &writer)
            writer.writeSlot(UInt32(writer.position - position), at: ends + index * width, offsetType: offsetType)
        }
    }
}

extension TableHeader {
    /// Writes the table at the writer's position: its header, and then each
    /// field after its padding, with its offset from the table's start. Its
    /// fields are those of `fields` from `next`, each followed by those of a
    /// table nested from a builder that it records, and it moves `next` past
    /// them.
    func write<Records: RandomAccessCollection<Field>>(
        fields: Records,
        from next: inout Int,
        to writer: inout ByteWriter
    ) where Records.Index == Int {
        let start = writer.position
        let width = layout.offsetSize
        let offsetType = layout.offsetType

        let header = writer.reserve(Int(headerLength))
        if layout.isSized {
            writer.writeSlot(length, at: header, offsetType: offsetType)
        }
        if layout.isVersioned {
            writer.writeSlot(version, at: header + (layout.isSized ? width : 0), offsetType: offsetType)
        }

        var slot = header + layout.headerSize
        for _ in 0..<Int(fieldCount) {
            let field = fields[next]
            next += 1

            let offset = UInt64(writer.position - start)
            writer.writeZeros(Int(field.padding(at: offset, width: UInt64(width))))
            writer.writeSlot(UInt32(writer.position - start), at: slot, offsetType: offsetType)
            field.write(to: &writer, offsetType: offsetType, following: fields, from: &next)
            slot += width
        }
    }
}

extension TableBuilder {
    /// Writes the table at the writer's position
    func write(to writer: inout ByteWriter) {
        var next = 0
        header.write(fields: fields, from: &next, to: &writer)
    }
}

extension NestedTable {
    /// Writes the table at the writer's position, without padding: a builder
    /// in its own layout, and bytes as they are
    func write(to writer: inout ByteWriter) {
        switch self {
        case .builder(let builder): builder.write(to: &writer)
        case .bytes(let bytes): writer.writeBytes(bytes)
        }
    }
}

/// The elements of an array field, as the array of their type, which it
/// shares. Each of the ten element types is a case, rather than an
/// existential of them, which would make every field that a builder copies
/// or reads a value of unknown size, copied through its type's metadata.
enum Elements: Sendable {
    case int8([Int8])
    case uint8([UInt8])
    case int16([Int16])
    case uint16([UInt16])
    case int32([Int32])
    case uint32([UInt32])
    case int64([Int64])
    case uint64([UInt64])
    case float32([Float])
    case float64([Double])

    /// The elements of `values`, whose type is one of the ten, as
    /// `ArrayElement` documents, so that one cast is of the array to its own
    /// type. An array of another type that conforms stops the program.
    init<T: ArrayElement>(_ values: [T]) {
        if T.self == Int8.self {
            self = .int8(unsafeBitCast(values, to: [Int8].self))
        } else if T.self == UInt8.self {
            self = .uint8(unsafeBitCast(values, to: [UInt8].self))
        } else if T.self == Int16.self {
            self = .int16(unsafeBitCast(values, to: [Int16].self))
        } else if T.self == UInt16.self {
            self = .uint16(unsafeBitCast(values, to: [UInt16].self))
        } else if T.self == Int32.self {
            self = .int32(unsafeBitCast(values, to: [Int32].self))
        } else if T.self == UInt32.self {
            self = .uint32(unsafeBitCast(values, to: [UInt32].self))
        } else if T.self == Int64.self {
            self = .int64(unsafeBitCast(values, to: [Int64].self))
        } else if T.self == UInt64.self {
            self = .uint64(unsafeBitCast(values, to: [UInt64].self))
        } else if T.self == Float.self {
            self = .float32(unsafeBitCast(values, to: [Float].self))
        } else if T.self == Double.self {
            self = .float64(unsafeBitCast(values, to: [Double].self))
        } else {
            preconditionFailure("Only the ten element types conform to ArrayElement")
        }
    }

    var count: Int {
        switch self {
        case .int8(let values): values.count
        case .uint8(let values): values.count
        case .int16(let values): values.count
        case .uint16(let values): values.count
        case .int32(let values): values.count
        case .uint32(let values): values.count
        case .int64(let values): values.count
        case .uint64(let values): values.count
        case .float32(let values): values.count
        case .float64(let values): values.count
        }
    }

    /// The elements' size, which they're aligned to from the table's start
    var alignment: Alignment {
        switch self {
        case .int8, .uint8: .one
        case .int16, .uint16: .two
        case .int32, .uint32, .float32: .four
        case .int64, .uint64, .float64: .eight
        }
    }

    /// The elements' byte length, which fits in an `Int` as an array's does
    var byteCount: Int {
        count &* alignment.bytes
    }

    /// Writes the elements' little-endian bytes at the writer's position
    func write(to writer: inout ByteWriter) {
        switch self {
        case .int8(let values): writer.writeElements(values)
        case .uint8(let values): writer.writeElements(values)
        case .int16(let values): writer.writeElements(values)
        case .uint16(let values): writer.writeElements(values)
        case .int32(let values): writer.writeElements(values)
        case .uint32(let values): writer.writeElements(values)
        case .int64(let values): writer.writeElements(values)
        case .uint64(let values): writer.writeElements(values)
        case .float32(let values): writer.writeElements(values)
        case .float64(let values): writer.writeElements(values)
        }
    }
}

/// Writes a table's bytes, one after another, into memory of the table's
/// length that a `TableBuilder` or a `TableSizer` counted. Every write checks
/// that it stays within the memory, which the lengths' agreement ensures, so
/// that a mistake stops the program rather than writing past it.
///
/// `TableBuilder` writes its fields through it, and `TableWriter` the fields
/// that its caller sets.
@usableFromInline
struct ByteWriter {
    /// The memory that the table is written into
    @usableFromInline
    let bytes: UnsafeMutableRawBufferPointer
    /// Where the next value goes
    @usableFromInline
    var position = 0

    @inlinable
    init(bytes: UnsafeMutableRawBufferPointer) {
        self.bytes = bytes
    }

    /// Moves past `count` bytes that the caller writes, and returns where
    /// they start
    @inlinable
    mutating func reserve(_ count: Int) -> Int {
        precondition(count >= 0 && count <= bytes.count &- position, "A write past the table's length")

        let start = position
        position &+= count
        return start
    }

    @inlinable
    mutating func writeZeros(_ count: Int) {
        let start = reserve(count)
        for index in start..<start &+ count {
            bytes[index] = 0
        }
    }

    mutating func writeScalar(_ bits: UInt64, size: Int) {
        let start = reserve(size)
        switch size {
        case 1: bytes.storeBytes(of: UInt8(truncatingIfNeeded: bits), toByteOffset: start, as: UInt8.self)
        case 2: UInt16(truncatingIfNeeded: bits).store(to: bytes, at: start)
        case 4: UInt32(truncatingIfNeeded: bits).store(to: bytes, at: start)
        case 8: bits.store(to: bytes, at: start)
        default: preconditionFailure("A scalar of another size than an element's")
        }
    }

    /// Writes a scalar's little-endian bytes
    @inlinable
    mutating func write<T: ArrayElement>(_ value: T) {
        value.store(to: bytes, at: reserve(MemoryLayout<T>.size))
    }

    /// Writes the elements' little-endian bytes: in one copy where the host
    /// stores them so, and one by one otherwise
    @inlinable
    mutating func writeElements<T: ArrayElement>(_ values: [T]) {
        let size = MemoryLayout<T>.size
        // An array's byte length fits in an Int
        let start = reserve(values.count &* size)

        if elementsInHostOrder {
            values.withUnsafeBytes { elements in
                UnsafeMutableRawBufferPointer(rebasing: bytes[start..<start &+ elements.count])
                    .copyMemory(from: elements)
            }
            return
        }

        for (index, element) in values.enumerated() {
            element.store(to: bytes, at: start &+ index &* size)
        }
    }

    /// Writes a slot of the offset type at the writer's position
    @inlinable
    mutating func writeSlot(_ value: UInt32, offsetType: OffsetType) {
        writeSlot(value, at: reserve(offsetType.size), offsetType: offsetType)
    }

    /// Writes a slot of the offset type at `position`, which was reserved.
    /// The table's length fits in the offset type, so its offsets, counts,
    /// lengths and ends do.
    @inlinable
    func writeSlot(_ value: UInt32, at position: Int, offsetType: OffsetType) {
        precondition(position >= 0 && position <= bytes.count &- offsetType.size, "A slot past the table")

        switch offsetType {
        case .uint8: UInt8(truncatingIfNeeded: value).store(to: bytes, at: position)
        case .uint16: UInt16(truncatingIfNeeded: value).store(to: bytes, at: position)
        case .uint32: value.store(to: bytes, at: position)
        }
    }

    @inlinable
    mutating func writeBytes(_ source: [UInt8]) {
        let start = reserve(source.count)
        source.withUnsafeBytes { source in
            UnsafeMutableRawBufferPointer(rebasing: bytes[start..<start &+ source.count])
                .copyMemory(from: source)
        }
    }

    /// Writes the `count` bytes of the string's UTF-8
    @inlinable
    mutating func writeUTF8(of string: String, count: Int) {
        let start = reserve(count)
        var string = string

        // A string's UTF-8 is contiguous, or made so
        string.withUTF8 { utf8 in
            precondition(utf8.count == count, "The string's length changed")
            UnsafeMutableRawBufferPointer(rebasing: bytes[start..<start &+ count])
                .copyMemory(from: UnsafeRawBufferPointer(utf8))
        }
    }
}
