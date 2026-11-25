/// Writes a table forward into bytes of exactly its length, which a
/// ``TableSizer`` counted.
///
/// It doesn't record the fields first, as ``TableBuilder`` does before it
/// writes them. `TableSizer.bytes(_:)` gives it to the closure that writes
/// the table, which takes the fields that the sizer added, in the same order,
/// each with its index in its table. It writes into the bytes that the sizer
/// returns, so it can't be copied, and lives only while the closure runs.
///
/// Each field's offset is written as the writer reaches the field. A nested
/// table follows its parent's ``setTable(_:alignment:)``, between
/// ``startTable(fields:)`` and ``endTable(_:)``; the items of a vector follow
/// its ``setVector(_:count:)`` the same way, each followed by
/// ``setItemEnd(_:item:)``, which writes where it ends. Arrays and nested
/// tables are padded so that their elements are aligned from their table's
/// start, as the sizer counts them. A struct's scalars follow its
/// ``setStruct(_:)``, and an array's structs its
/// ``setStructArray(_:count:alignment:)``, each written with
/// ``writeScalar(_:)``, as the code that tafla-gen generates writes them
/// without collecting their bytes first.
///
/// It's the target of the code that tafla-gen generates, whose calls follow
/// from the schema, so it doesn't check them again: fields are set in index
/// order, each once, as the sizer added them, and each vector item is
/// followed by its `setItemEnd(_:item:)`. A field index past its table's
/// fields, or an item past its vector's count, writes an offset or item end
/// over another slot or field, so the table's bytes are undefined, though
/// never outside the table's bytes. Hand-written tables use `TableBuilder`,
/// which checks every field.
///
/// Fields that differ from those that the sizer added never write outside
/// the table's bytes either: a write past them is a programming error, which
/// stops the program, as a negative index, count or size does, and
/// `TableSizer.bytes(_:)` throws ``BuildError/notAsSized(written:length:)``
/// for fields that don't fill the table.
public struct TableWriter: ~Copyable {
    /// The table's bytes, and where the next field goes
    @usableFromInline
    var output: ByteWriter
    /// The start of the table being written, which its offsets count from: 0
    /// for the root
    @usableFromInline
    var table = 0
    /// The position of that table's first offset
    @usableFromInline
    var slots: Int
    /// The byte length of the root's header slots, which its offsets follow
    @usableFromInline
    let headerSize: Int
    @usableFromInline
    let offsetType: OffsetType
    /// The first error of an array of structs given bytes that aren't whole
    /// structs
    @usableFromInline
    var notWholeStructs: BuildError?

    /// Writes the header of the root table of `fields` fields into `bytes`,
    /// of the table's length, which the sizer checked are at least the
    /// header and at most what the offset type addresses, and zeros its
    /// offsets, which each field's setter writes
    @inlinable
    init(bytes: UnsafeMutableRawBufferPointer, fields: Int, layout: TableLayout) {
        output = ByteWriter(bytes: bytes)
        headerSize = layout.headerSize
        slots = layout.headerSize
        offsetType = layout.offsetType

        if layout.isSized {
            output.writeSlot(UInt32(truncatingIfNeeded: bytes.count), offsetType: offsetType)
        }
        if let version = layout.version {
            output.writeSlot(version, offsetType: offsetType)
        }
        output.writeZeros(fields &* offsetType.size)
    }

    /// The error that `TableSizer.bytes(_:)` throws after the writer wrote a
    /// table of `length` bytes, if any
    @inlinable
    func failure(length: Int) -> BuildError? {
        if let notWholeStructs {
            return notWholeStructs
        }
        guard output.position == length else {
            return .notAsSized(written: output.position, length: length)
        }

        return nil
    }

    /// Starts the fields of a nested table or vector item of `fields`
    /// fields, after its parent's ``setTable(_:alignment:)`` or
    /// ``setVector(_:count:)``.
    @inlinable
    public mutating func startTable(fields: Int) -> WriterParent {
        let parent = WriterParent(table: table)

        table = output.position
        slots = output.position
        // Each field's offset is written as the writer reaches the field
        output.writeZeros(fields * offsetType.size)

        return parent
    }

    /// Ends the fields of the table that ``startTable(fields:)`` started.
    @inlinable
    public mutating func endTable(_ parent: WriterParent) {
        table = parent.table
        // Only the root starts at 0, and only its offsets follow a header
        slots = parent.table == 0 ? headerSize : parent.table
    }

    /// Writes `int8` field `index`.
    @inlinable
    public mutating func setInt8(_ index: Int, _ value: Int8) { setScalar(index, value) }

    /// Writes `uint8` field `index`.
    @inlinable
    public mutating func setUInt8(_ index: Int, _ value: UInt8) { setScalar(index, value) }

    /// Writes `int16` field `index`.
    @inlinable
    public mutating func setInt16(_ index: Int, _ value: Int16) { setScalar(index, value) }

    /// Writes `uint16` field `index`.
    @inlinable
    public mutating func setUInt16(_ index: Int, _ value: UInt16) { setScalar(index, value) }

    /// Writes `int32` field `index`.
    @inlinable
    public mutating func setInt32(_ index: Int, _ value: Int32) { setScalar(index, value) }

    /// Writes `uint32` field `index`.
    @inlinable
    public mutating func setUInt32(_ index: Int, _ value: UInt32) { setScalar(index, value) }

    /// Writes `int64` field `index`.
    @inlinable
    public mutating func setInt64(_ index: Int, _ value: Int64) { setScalar(index, value) }

    /// Writes `uint64` field `index`.
    @inlinable
    public mutating func setUInt64(_ index: Int, _ value: UInt64) { setScalar(index, value) }

    /// Writes `float32` field `index`, of the value's bits.
    @inlinable
    public mutating func setFloat32(_ index: Int, _ value: Float) { setScalar(index, value) }

    /// Writes `float64` field `index`, of the value's bits.
    @inlinable
    public mutating func setFloat64(_ index: Int, _ value: Double) { setScalar(index, value) }

    /// Writes scalar field `index` of type `T`, as the setter of its type
    /// does
    @inlinable
    public mutating func setScalar<T: ArrayElement>(_ index: Int, _ value: T) {
        setOffset(index)
        output.write(value)
    }

    /// Writes array field `index` of `int8`s.
    @inlinable
    public mutating func setInt8Array(_ index: Int, _ values: [Int8]) { setArray(index, values) }

    /// Writes array field `index` of `uint8`s.
    @inlinable
    public mutating func setUInt8Array(_ index: Int, _ values: [UInt8]) { setArray(index, values) }

    /// Writes array field `index` of `int16`s.
    @inlinable
    public mutating func setInt16Array(_ index: Int, _ values: [Int16]) { setArray(index, values) }

    /// Writes array field `index` of `uint16`s.
    @inlinable
    public mutating func setUInt16Array(_ index: Int, _ values: [UInt16]) { setArray(index, values) }

    /// Writes array field `index` of `int32`s.
    @inlinable
    public mutating func setInt32Array(_ index: Int, _ values: [Int32]) { setArray(index, values) }

    /// Writes array field `index` of `uint32`s.
    @inlinable
    public mutating func setUInt32Array(_ index: Int, _ values: [UInt32]) { setArray(index, values) }

    /// Writes array field `index` of `int64`s.
    @inlinable
    public mutating func setInt64Array(_ index: Int, _ values: [Int64]) { setArray(index, values) }

    /// Writes array field `index` of `uint64`s.
    @inlinable
    public mutating func setUInt64Array(_ index: Int, _ values: [UInt64]) { setArray(index, values) }

    /// Writes array field `index` of `float32`s.
    @inlinable
    public mutating func setFloat32Array(_ index: Int, _ values: [Float]) { setArray(index, values) }

    /// Writes array field `index` of `float64`s.
    @inlinable
    public mutating func setFloat64Array(_ index: Int, _ values: [Double]) { setArray(index, values) }

    /// Writes array field `index` of `T`, as the setter of its type does
    @inlinable
    public mutating func setArray<T: ArrayElement>(_ index: Int, _ values: [T]) {
        startArray(index, count: values.count, alignment: T.alignment)
        output.writeElements(values)
    }

    /// Writes string field `index`, as its UTF-8.
    @inlinable
    public mutating func setString(_ index: Int, _ value: String) {
        setOffset(index)

        let count = value.utf8.count
        output.writeSlot(UInt32(truncatingIfNeeded: count), offsetType: offsetType)
        output.writeUTF8(of: value, count: count)
    }

    /// Writes struct field `index` from its bytes: its scalars,
    /// little-endian, one after another, which are written as they are,
    /// without padding.
    @inlinable
    public mutating func setStruct(_ index: Int, _ bytes: [UInt8]) {
        setOffset(index)
        output.writeBytes(bytes)
    }

    /// Writes the offset of struct field `index`, whose scalars the caller
    /// then writes with ``writeScalar(_:)``, one after another, before the
    /// next field.
    @inlinable
    public mutating func setStruct(_ index: Int) {
        setOffset(index)
    }

    /// Writes field `index`, an array of structs of `size` bytes each, from
    /// their bytes one after another, which it counts. It's padded so that
    /// the structs start at a multiple of `alignment` from the start of the
    /// table, which is the size of the structs' largest scalar.
    ///
    /// `TableSizer.bytes(_:)` throws
    /// ``BuildError/notWholeStructs(length:size:)`` if `size` isn't positive
    /// or the bytes aren't a whole number of structs, as `TableBuilder` does.
    @inlinable
    public mutating func setStructArray(_ index: Int, _ bytes: [UInt8], size: Int, alignment: Alignment) {
        guard size > 0, bytes.count % size == 0 else {
            if notWholeStructs == nil {
                notWholeStructs = .notWholeStructs(length: bytes.count, size: size)
            }
            return
        }

        startArray(index, count: bytes.count / size, alignment: alignment)
        output.writeBytes(bytes)
    }

    /// Writes the offset and count of field `index`, an array of `count`
    /// structs, which mustn't be negative, padded so that they start at a
    /// multiple of `alignment` from the start of the table, which is the size
    /// of the structs' largest scalar. The caller then writes each struct's
    /// scalars with ``writeScalar(_:)``, struct after struct, before the next
    /// field.
    @inlinable
    public mutating func setStructArray(_ index: Int, count: Int, alignment: Alignment) {
        precondition(count >= 0, "A negative count")
        startArray(index, count: count, alignment: alignment)
    }

    /// Writes a struct's scalar where the writer is, little-endian, after
    /// its struct's ``setStruct(_:)`` or ``setStructArray(_:count:alignment:)``
    /// and the scalars before it. A `bool` is written as the `uint8` 1 or 0.
    @inlinable
    public mutating func writeScalar<T: ArrayElement>(_ value: T) {
        output.write(value)
    }

    /// Writes the offset of nested table field `index`, padded to start at a
    /// multiple of `alignment`, whose fields then follow between
    /// ``startTable(fields:)`` and ``endTable(_:)``.
    @inlinable
    public mutating func setTable(_ index: Int, alignment: Alignment) {
        let padding = alignment.padding(at: UInt64(output.position &- table))
        output.writeZeros(Int(padding))
        setOffset(index)
    }

    /// Writes the offset and count of vector field `index`, of `count`
    /// tables, which mustn't be negative, whose items then follow, each
    /// between ``startTable(fields:)`` and ``endTable(_:)`` and then
    /// ``setItemEnd(_:item:)``.
    @inlinable
    public mutating func setVector(_ index: Int, count: Int) -> VectorPosition {
        precondition(count >= 0, "A negative count")
        setOffset(index)

        // The count, and then the end of each item, which is written after
        // the item
        let position = output.position
        output.writeSlot(UInt32(truncatingIfNeeded: count), offsetType: offsetType)
        output.writeZeros(count * offsetType.size)

        return VectorPosition(position: position)
    }

    /// Writes the end of item `item` of the vector, which was just written.
    @inlinable
    public mutating func setItemEnd(_ vector: VectorPosition, item: Int) {
        // Each end counts from the vector's position, which is its count's
        let slot = vector.position + (item + 1) * offsetType.size
        let end = output.position &- vector.position
        output.writeSlot(UInt32(truncatingIfNeeded: end), at: slot, offsetType: offsetType)
    }

    /// Pads an array so that its elements, after its count, are aligned, and
    /// writes its offset and count
    @inlinable
    mutating func startArray(_ index: Int, count: Int, alignment: Alignment) {
        // Wrapping, since only the remainder matters
        let padding = alignment.padding(at: UInt64(truncatingIfNeeded: output.position &+ offsetType.size &- table))
        output.writeZeros(Int(padding))
        setOffset(index)
        output.writeSlot(UInt32(truncatingIfNeeded: count), offsetType: offsetType)
    }

    /// Writes the offset of field `index`, which mustn't be negative, of the
    /// table being written, which is where the field starts. The table's
    /// length fits in its offsets, so the offset does.
    @inlinable
    mutating func setOffset(_ index: Int) {
        precondition(index >= 0, "A negative field index")

        let slot = slots + index * offsetType.size
        let offset = output.position &- table
        output.writeSlot(UInt32(truncatingIfNeeded: offset), at: slot, offsetType: offsetType)
    }
}

/// The table that a nested table or vector item was started in, which
/// `TableWriter.endTable(_:)` takes back.
public struct WriterParent: Sendable {
    @usableFromInline
    let table: Int

    @inlinable
    init(table: Int) {
        self.table = table
    }
}

/// The position of a vector that `TableWriter.setVector(_:count:)` wrote,
/// which `TableWriter.setItemEnd(_:item:)` takes.
public struct VectorPosition: Sendable {
    @usableFromInline
    let position: Int

    @inlinable
    init(position: Int) {
        self.position = position
    }
}
