/// Counts the bytes of a table, so that a ``TableWriter`` can write it into
/// bytes of exactly that length.
///
/// It's for code that knows a table's fields before it writes them, such as
/// the code that tafla-gen generates, which writes tables without recording
/// their fields as ``TableBuilder`` does.
///
/// The fields are added in order, each with its type's method, which takes
/// what decides its length: nothing for a scalar, the count of an array, the
/// string itself. A nested table is added with ``addTable(alignment:)``, and
/// a vector with ``addVector(count:)``, and the fields of each such table
/// then follow between ``startTable(fields:)`` and ``endTable(_:)``. The
/// sizer pads arrays and nested tables as `TableBuilder` does, so a writer
/// that takes the same fields in the same order writes each where the sizer
/// counted it.
///
/// Adding a field doesn't throw: ``byteLength`` and ``bytes(_:)`` throw the
/// error of a table too long for its layout's offset type, or of an array of
/// 0-byte structs, as `TableBuilder`'s setter of the field would have.
///
/// ```swift
/// var sizer = try TableSizer(fields: 2)
/// sizer.addFloat64()
/// sizer.addString("Europe/Stockholm")
///
/// let bytes = try sizer.bytes { writer in
///     writer.setFloat64(0, 59.3293)
///     writer.setString(1, "Europe/Stockholm")
/// }
/// ```
public struct TableSizer: Sendable {
    /// The layout that the table is written in, which its readers must use
    public let layout: TableLayout
    /// The number of the root's fields
    @usableFromInline
    let fieldCount: Int
    /// The longest table that the offset type addresses, and an `Int` holds
    @usableFromInline
    let maxLength: UInt64
    /// The table's length so far, which is where the next field goes. It's
    /// counted in a `UInt64`, which saturates far past the longest table,
    /// so that a 32-bit host counts lengths past its `Int` too.
    @usableFromInline
    var position: UInt64
    /// The start of the table being sized, which arrays are aligned from: 0
    /// for the root, and past its header for the tables nested in it
    @usableFromInline
    var table: UInt64 = 0
    /// The root's fields added so far, the last of which a table too long
    /// for its offsets names
    @usableFromInline
    var rootFields = 0
    /// The first error: of an array of 0-byte structs, or of the first root
    /// field that made the table too long
    @usableFromInline
    var failure: BuildError?

    /// Starts sizing a table of `fields` fields, which mustn't be negative, in
    /// `layout`, which its readers must use too, and whose version its writer
    /// writes in the header when it has one. Nested tables use
    /// ``TableLayout/default``.
    ///
    /// Throws ``BuildError/versionTooLarge(version:offsetType:)`` if the
    /// layout's version doesn't fit in its offset type, or
    /// ``BuildError/headerTooLong(fields:offsetType:)`` if the header alone is
    /// too long for it.
    public init(fields: Int, layout: TableLayout = .default) throws(BuildError) {
        precondition(fields >= 0, "A table can't hold \(fields) fields")

        try layout.checkVersion()
        let headerLength = try layout.headerLength(fields: fields)

        self.layout = layout
        self.fieldCount = fields
        self.maxLength = min(UInt64(layout.offsetType.maxValue), UInt64(Int.max))
        self.position = UInt64(headerLength)
    }

    /// The byte length of the table, with every field added.
    ///
    /// Throws ``BuildError/tooLong(index:length:offsetType:)`` if the table
    /// is too long for its layout's offset type: longer than 255 bytes,
    /// 64 KiB or 4 GiB, or than an `Int` on a 32-bit host. It names the root
    /// field that made it too long, and the length that the root's fields up
    /// to it make the table, as `TableBuilder`'s setter of the field throws.
    /// Throws ``BuildError/notWholeStructs(length:size:)`` if an array of
    /// 0-byte structs was added, unless a field before it made the table too
    /// long.
    @inlinable
    public var byteLength: Int {
        get throws(BuildError) {
            if let failure {
                throw failure
            }
            guard position <= maxLength else {
                throw .tooLong(index: max(rootFields &- 1, 0), length: position, offsetType: layout.offsetType)
            }

            return Int(position)
        }
    }

    /// Writes the table into new bytes of its length, with `write`, which
    /// sets its fields on the writer that it's given, in the order that they
    /// were added, as ``TableWriter`` describes.
    ///
    /// The writer writes into the bytes that it returns, rather than into
    /// bytes of its own that it copies, so it lives only while `write` runs,
    /// and can't be copied.
    ///
    /// Throws the errors of ``byteLength`` before it allocates the bytes, or,
    /// after `write`, ``BuildError/notAsSized(written:length:)`` if the
    /// fields didn't fill the table, as when they differ from those that the
    /// sizer added, or ``BuildError/notWholeStructs(length:size:)`` if an
    /// array of structs was given bytes that aren't whole structs.
    @inlinable
    public func bytes(_ write: (inout TableWriter) -> Void) throws(BuildError) -> [UInt8] {
        let length = try byteLength
        var failure: BuildError?

        let bytes = [UInt8](unsafeUninitializedCapacity: length) { buffer, initialized in
            let memory = UnsafeMutableRawBufferPointer(buffer)
            var writer = TableWriter(bytes: memory, fields: fieldCount, layout: layout)
            write(&writer)

            // A writer can only be exchanged for another of the same type,
            // which writes into the bytes of another table
            precondition(writer.output.bytes.baseAddress == memory.baseAddress, "The writer was exchanged")

            // Every byte before the writer's position was written once
            initialized = writer.output.position
            failure = writer.failure(length: length)
        }

        if let failure {
            throw failure
        }
        return bytes
    }

    /// Starts the fields of a nested table or vector item of `fields`
    /// fields, after its parent's ``addTable(alignment:)`` or
    /// ``addVector(count:)``.
    @inlinable
    public mutating func startTable(fields: Int) -> SizerParent {
        let parent = SizerParent(table: table)
        table = position
        position = saturating(position, plus: length(of: fields, size: layout.offsetSize))

        return parent
    }

    /// Ends the fields of the table that ``startTable(fields:)`` started.
    @inlinable
    public mutating func endTable(_ parent: SizerParent) {
        table = parent.table
    }

    /// Adds an `int8` field.
    @inlinable
    public mutating func addInt8() { add(1) }

    /// Adds a `uint8` field.
    @inlinable
    public mutating func addUInt8() { add(1) }

    /// Adds an `int16` field.
    @inlinable
    public mutating func addInt16() { add(2) }

    /// Adds a `uint16` field.
    @inlinable
    public mutating func addUInt16() { add(2) }

    /// Adds an `int32` field.
    @inlinable
    public mutating func addInt32() { add(4) }

    /// Adds a `uint32` field.
    @inlinable
    public mutating func addUInt32() { add(4) }

    /// Adds an `int64` field.
    @inlinable
    public mutating func addInt64() { add(8) }

    /// Adds a `uint64` field.
    @inlinable
    public mutating func addUInt64() { add(8) }

    /// Adds a `float32` field.
    @inlinable
    public mutating func addFloat32() { add(4) }

    /// Adds a `float64` field.
    @inlinable
    public mutating func addFloat64() { add(8) }

    /// Adds an array field of `count` `int8`s, which mustn't be negative.
    @inlinable
    public mutating func addInt8Array(count: Int) { addArray(count: count, of: Int8.self) }

    /// Adds an array field of `count` `uint8`s, which mustn't be negative.
    @inlinable
    public mutating func addUInt8Array(count: Int) { addArray(count: count, of: UInt8.self) }

    /// Adds an array field of `count` `int16`s, which mustn't be negative.
    @inlinable
    public mutating func addInt16Array(count: Int) { addArray(count: count, of: Int16.self) }

    /// Adds an array field of `count` `uint16`s, which mustn't be negative.
    @inlinable
    public mutating func addUInt16Array(count: Int) { addArray(count: count, of: UInt16.self) }

    /// Adds an array field of `count` `int32`s, which mustn't be negative.
    @inlinable
    public mutating func addInt32Array(count: Int) { addArray(count: count, of: Int32.self) }

    /// Adds an array field of `count` `uint32`s, which mustn't be negative.
    @inlinable
    public mutating func addUInt32Array(count: Int) { addArray(count: count, of: UInt32.self) }

    /// Adds an array field of `count` `int64`s, which mustn't be negative.
    @inlinable
    public mutating func addInt64Array(count: Int) { addArray(count: count, of: Int64.self) }

    /// Adds an array field of `count` `uint64`s, which mustn't be negative.
    @inlinable
    public mutating func addUInt64Array(count: Int) { addArray(count: count, of: UInt64.self) }

    /// Adds an array field of `count` `float32`s, which mustn't be negative.
    @inlinable
    public mutating func addFloat32Array(count: Int) { addArray(count: count, of: Float.self) }

    /// Adds an array field of `count` `float64`s, which mustn't be negative.
    @inlinable
    public mutating func addFloat64Array(count: Int) { addArray(count: count, of: Double.self) }

    /// Adds an array field of `count` bools, which mustn't be negative, a
    /// byte each, which needs no padding.
    @inlinable
    public mutating func addBoolArray(count: Int) { addArray(count: count, of: UInt8.self) }

    /// Adds the array field of `count` elements of type `T`, as the adder of
    /// its type does
    @inlinable
    public mutating func addArray<T: ArrayElement>(count: Int, of type: T.Type = T.self) {
        addArray(byteLength: length(of: count, size: MemoryLayout<T>.size), alignment: T.alignment)
    }

    /// Adds a string field, of its UTF-8.
    @inlinable
    public mutating func addString(_ value: String) {
        add(saturating(UInt64(layout.offsetSize), plus: UInt64(value.utf8.count)))
    }

    /// Adds a struct field of `size` bytes, which mustn't be negative.
    @inlinable
    public mutating func addStruct(size: Int) {
        add(length(of: 1, size: size))
    }

    /// Adds an array of `count` structs of `size` bytes each, padded so that
    /// they start at a multiple of `alignment` from the start of the table,
    /// which is the size of the structs' largest scalar. `count` mustn't be
    /// negative.
    ///
    /// ``byteLength`` throws ``BuildError/notWholeStructs(length:size:)`` if
    /// `size` isn't positive, as `TableBuilder.setStructArray(_:size:alignment:)`
    /// does.
    @inlinable
    public mutating func addStructArray(count: Int, size: Int, alignment: Alignment) {
        addArray(byteLength: length(of: count, size: max(size, 0)), alignment: alignment)

        // After the array's field is counted, which reports a field before it
        // that made the table too long, as TableBuilder's setter of that
        // field throws first
        if size <= 0 && failure == nil {
            failure = .notWholeStructs(length: 0, size: size)
        }
    }

    /// Adds a nested table, padded to start at a multiple of `alignment`,
    /// whose fields then follow between ``startTable(fields:)`` and
    /// ``endTable(_:)``.
    @inlinable
    public mutating func addTable(alignment: Alignment) {
        startField()
        // Wrapping, since only the remainder matters
        position = saturating(position, plus: alignment.padding(at: position &- table))
    }

    /// Adds a vector of `count` tables, which mustn't be negative, whose
    /// fields then follow, each between ``startTable(fields:)`` and
    /// ``endTable(_:)``.
    @inlinable
    public mutating func addVector(count: Int) {
        // The count, and then the end of each item
        add(saturating(UInt64(layout.offsetSize), plus: length(of: count, size: layout.offsetSize)))
    }

    /// Adds a field of `byteLength` bytes, without padding
    @inlinable
    mutating func add(_ byteLength: UInt64) {
        startField()
        position = saturating(position, plus: byteLength)
    }

    /// Adds an array of `byteLength` bytes after its count, padded so that
    /// its elements are aligned
    @inlinable
    mutating func addArray(byteLength: UInt64, alignment: Alignment) {
        startField()

        let start = saturating(position, plus: UInt64(layout.offsetSize))
        let padding = alignment.padding(at: start &- table)
        position = saturating(saturating(start, plus: padding), plus: byteLength)
    }

    /// Counts a field of the root, after checking the length that the root's
    /// fields before it make the table. The length is checked only between
    /// the root's fields, so that it's the length after the whole field that
    /// made the table too long, as `TableBuilder` reports it.
    @inlinable
    mutating func startField() {
        // Only the root starts at 0, since a nested table follows the offset
        // of the field that holds it
        guard table == 0 else {
            return
        }

        // The root's first field starts after the header, which fits, so a
        // field before this one made the table too long
        if position > maxLength && failure == nil {
            failure = .tooLong(index: max(rootFields &- 1, 0), length: position, offsetType: layout.offsetType)
        }
        rootFields &+= 1
    }
}

/// The table that a nested table or vector item was started in, which
/// `TableSizer.endTable(_:)` takes back.
public struct SizerParent: Sendable {
    @usableFromInline
    let table: UInt64

    @inlinable
    init(table: UInt64) {
        self.table = table
    }
}

/// The byte length of `count` elements of `size` bytes each, neither of
/// which may be negative, or `UInt64.max`, past any table, if it overflows
@inlinable
func length(of count: Int, size: Int) -> UInt64 {
    precondition(count >= 0, "A negative count")
    precondition(size >= 0, "A negative size")

    let (length, overflow) = UInt64(count).multipliedReportingOverflow(by: UInt64(size))
    return overflow ? .max : length
}

/// The sum of two lengths, or `UInt64.max`, past any table, if it overflows
@inlinable
func saturating(_ length: UInt64, plus other: UInt64) -> UInt64 {
    let (sum, overflow) = length.addingReportingOverflow(other)
    return overflow ? .max : sum
}
