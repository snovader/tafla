/// A field that a `TableBuilder` recorded, with its value
enum Field: Sendable {
    /// A scalar's little-endian bits, of `size` bytes
    case scalar(bits: UInt64, size: Int)
    case array(any ElementArray)
    case string(String, utf8Count: Int)
    case structBytes([UInt8])
    /// The bytes of structs of `size` bytes each, aligned to `alignment`
    case structArray([UInt8], size: Int, alignment: Alignment)
    case table(NestedTable, alignment: Alignment)
    case vector([NestedTable])

    /// The alignment that the field needs from the table's start
    var alignment: Alignment {
        switch self {
        case .array(let values): values.elementAlignment
        case .structArray(_, _, let alignment), .table(_, let alignment): alignment
        case .scalar, .string, .structBytes, .vector: .one
        }
    }

    /// The padding before the field when it would start at `offset` in a
    /// table of `width`-byte offsets, which aligns an array's elements after
    /// its count, and a nested table's start
    func padding(at offset: UInt64, width: UInt64) -> UInt64 {
        switch self {
        case .array, .structArray: alignment.padding(at: offset + width)
        case .table: alignment.padding(at: offset)
        case .scalar, .string, .structBytes, .vector: 0
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
        case .table(let table, _):
            return table.length
        case .vector(let items):
            var length = width
            for item in items {
                let (sum, overflow) = length.addingReportingOverflow(width + item.length)
                length = overflow ? .max : sum
            }
            return length
        }
    }

    /// Writes the field at the writer's position, after its padding, in a
    /// table of `offsetType` offsets
    func write(to writer: inout ByteWriter, offsetType: OffsetType) {
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
        case .table(let table, _):
            table.write(to: &writer)
        case .vector(let items):
            writeVector(items, to: &writer, offsetType: offsetType)
        }
    }

    /// Writes a vector's count, then each item followed by its end, counted
    /// from the vector's position, in the slots after the count
    private func writeVector(_ items: [NestedTable], to writer: inout ByteWriter, offsetType: OffsetType) {
        let position = writer.position
        let width = offsetType.size

        writer.writeSlot(UInt32(items.count), offsetType: offsetType)
        let ends = writer.reserve(items.count * width)

        for (index, item) in items.enumerated() {
            item.write(to: &writer)
            writer.writeSlot(UInt32(writer.position - position), at: ends + index * width, offsetType: offsetType)
        }
    }
}

extension TableBuilder {
    /// Writes the table at the writer's position: its header, and then each
    /// field after its padding, with its offset from the table's start
    func write(to writer: inout ByteWriter) {
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
        for field in fields {
            let offset = UInt64(writer.position - start)
            writer.writeZeros(Int(field.padding(at: offset, width: UInt64(width))))
            writer.writeSlot(UInt32(writer.position - start), at: slot, offsetType: offsetType)
            field.write(to: &writer, offsetType: offsetType)
            slot += width
        }
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

/// An array of elements that a `TableBuilder` keeps, of any element type
protocol ElementArray: Sendable {
    var count: Int { get }

    /// The elements' byte length
    var byteCount: Int { get }

    var elementAlignment: Alignment { get }

    /// Writes the elements' little-endian bytes at the writer's position
    func write(to writer: inout ByteWriter)
}

extension Array: ElementArray where Element: ArrayElement {
    var byteCount: Int { count * MemoryLayout<Element>.size }

    var elementAlignment: Alignment { Element.alignment }

    func write(to writer: inout ByteWriter) {
        writer.writeElements(self)
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
        default: bits.store(to: bytes, at: start)
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
