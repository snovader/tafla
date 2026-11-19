/// Builds a table, which a ``TableReader`` reads back.
///
/// A table is a header with one offset per field, after the table's total
/// length and version when its layout has them, followed by the fields in the
/// order they're set. Arrays, strings and vectors start with their count, and
/// a vector then holds the end of each item. Arrays of scalars and of structs
/// are padded so their elements are aligned from the start of the table.
/// Fields are read back by index with `TableReader.rootOffset(_:)`, or
/// `TableReader.fieldOffset(_:inTableAt:)` for a table nested in another.
///
/// Setting a field records it, keeping its value, and ``bytes()`` then writes
/// every field into bytes of the table's exact length. A builder is a value:
/// it keeps the arrays, strings and nested builders that it's given as
/// values too, which Swift shares rather than copies until one of them
/// changes, so changing them afterwards changes neither the builder nor its
/// table. Nested builders are written straight into their parent.
///
/// A builder records up to four fields within itself, and a builder nested
/// in it with ``setTable(_:alignment:)`` records its fields among them, so
/// that a small table, nested or not, allocates nothing until it's written.
/// More fields go in an array, which a parent shares rather than copies.
///
/// A setter throws a ``BuildError``, and leaves the builder as it was, when
/// the table already holds all its fields or the field would make it too long
/// for the layout's offset type: longer than 255 bytes, 64 KiB or 4 GiB, or
/// than an `Int` on a 32-bit host.
///
/// ```swift
/// var series = try TableBuilder(fields: 2)
/// try series.setUInt32Array([0, 86_400])
/// try series.setFloat32Array([21.75, 18.5])
///
/// var builder = try TableBuilder(fields: 3, layout: layout, version: 1)
/// try builder.setFloat64(59.3293)
/// try builder.setString("Europe/Stockholm")
/// try builder.setTable(.builder(series))
/// let bytes = try builder.bytes()
/// ```
public struct TableBuilder: Sendable {
    /// The table's layout, version and number of fields, and its length so
    /// far, which is where the next field goes
    var header: TableHeader
    /// The fields set so far, each followed by those of a table nested from
    /// a builder that it records
    var fields: Fields
    /// The number of fields set so far
    var fieldsSet: Int
    /// The largest alignment that a field needs from the table's start, which
    /// a parent pads this table to when it nests it
    var alignment: Alignment

    /// The layout that the table is written in, which its readers must use
    public var layout: TableLayout { header.layout }

    /// Starts a table of `fields` fields, which mustn't be negative, in
    /// `layout`, which its readers must use too, with `version` in its
    /// header. A versioned layout needs a version, and a layout without a
    /// version slot takes none: anything else is a programming error, which
    /// stops the program.
    ///
    /// Throws ``BuildError/versionTooLarge(version:offsetType:)`` if the
    /// version doesn't fit in the layout's offset type, or
    /// ``BuildError/headerTooLong(fields:offsetType:)`` if the header alone is
    /// too long for it.
    public init(fields: Int, layout: TableLayout = .default, version: UInt32? = nil) throws(BuildError) {
        precondition(fields >= 0, "A table can't hold \(fields) fields")
        precondition(
            layout.isVersioned == (version != nil),
            layout.isVersioned ? "A versioned layout needs a version" : "The layout has no version slot"
        )

        if let version {
            try layout.check(version: version)
        }
        let headerLength = try layout.headerLength(fields: fields)

        self.header = TableHeader(
            layout: layout,
            version: version ?? 0,
            fieldCount: UInt32(fields),
            headerLength: headerLength,
            length: headerLength
        )
        self.fields = Fields()
        self.fieldsSet = 0
        self.alignment = .one
    }

    /// Writes the table, at its exact size, into new bytes.
    ///
    /// Throws ``BuildError/incomplete(set:fields:)`` if fewer fields were set
    /// than the table holds.
    public func bytes() throws(BuildError) -> [UInt8] {
        try checkComplete()

        // Each setter checked that the table's length fits in an Int
        let length = Int(header.length)

        return [UInt8](unsafeUninitializedCapacity: length) { buffer, initialized in
            var writer = ByteWriter(bytes: UnsafeMutableRawBufferPointer(buffer))
            write(to: &writer)

            // The writer wrote every byte, since the lengths agree
            precondition(writer.position == length, "The table's length changed")
            initialized = length
        }
    }

    /// Sets an `int8` field.
    ///
    /// Throws ``BuildError/tooManyFields(fields:)`` if the table holds all its
    /// fields already, or ``BuildError/tooLong(index:length:offsetType:)`` if
    /// the field would make it too long for its offset type.
    public mutating func setInt8(_ value: Int8) throws(BuildError) {
        try setScalar(value)
    }

    /// Sets a `uint8` field, and throws as ``setInt8(_:)`` does.
    public mutating func setUInt8(_ value: UInt8) throws(BuildError) {
        try setScalar(value)
    }

    /// Sets an `int16` field, and throws as ``setInt8(_:)`` does.
    public mutating func setInt16(_ value: Int16) throws(BuildError) {
        try setScalar(value)
    }

    /// Sets a `uint16` field, and throws as ``setInt8(_:)`` does.
    public mutating func setUInt16(_ value: UInt16) throws(BuildError) {
        try setScalar(value)
    }

    /// Sets an `int32` field, and throws as ``setInt8(_:)`` does.
    public mutating func setInt32(_ value: Int32) throws(BuildError) {
        try setScalar(value)
    }

    /// Sets a `uint32` field, and throws as ``setInt8(_:)`` does.
    public mutating func setUInt32(_ value: UInt32) throws(BuildError) {
        try setScalar(value)
    }

    /// Sets an `int64` field, and throws as ``setInt8(_:)`` does.
    public mutating func setInt64(_ value: Int64) throws(BuildError) {
        try setScalar(value)
    }

    /// Sets a `uint64` field, and throws as ``setInt8(_:)`` does.
    public mutating func setUInt64(_ value: UInt64) throws(BuildError) {
        try setScalar(value)
    }

    /// Sets a `float32` field, of the value's bits, and throws as
    /// ``setInt8(_:)`` does.
    public mutating func setFloat32(_ value: Float) throws(BuildError) {
        try setScalar(value)
    }

    /// Sets a `float64` field, of the value's bits, and throws as
    /// ``setInt8(_:)`` does.
    public mutating func setFloat64(_ value: Double) throws(BuildError) {
        try setScalar(value)
    }

    /// Sets the scalar field of type `T`, as the setter of its type does
    public mutating func setScalar<T: ArrayElement>(_ value: T) throws(BuildError) {
        try add(.scalar(bits: UInt64(value._taflaBits), size: MemoryLayout<T>.size))
    }

    /// Sets an array field of `int8`s, and throws as ``setInt8(_:)`` does.
    public mutating func setInt8Array(_ values: [Int8]) throws(BuildError) {
        try setArray(values)
    }

    /// Sets an array field of `uint8`s, and throws as ``setInt8(_:)`` does.
    public mutating func setUInt8Array(_ values: [UInt8]) throws(BuildError) {
        try setArray(values)
    }

    /// Sets an array field of `int16`s, and throws as ``setInt8(_:)`` does.
    public mutating func setInt16Array(_ values: [Int16]) throws(BuildError) {
        try setArray(values)
    }

    /// Sets an array field of `uint16`s, and throws as ``setInt8(_:)`` does.
    public mutating func setUInt16Array(_ values: [UInt16]) throws(BuildError) {
        try setArray(values)
    }

    /// Sets an array field of `int32`s, and throws as ``setInt8(_:)`` does.
    public mutating func setInt32Array(_ values: [Int32]) throws(BuildError) {
        try setArray(values)
    }

    /// Sets an array field of `uint32`s, and throws as ``setInt8(_:)`` does.
    public mutating func setUInt32Array(_ values: [UInt32]) throws(BuildError) {
        try setArray(values)
    }

    /// Sets an array field of `int64`s, and throws as ``setInt8(_:)`` does.
    public mutating func setInt64Array(_ values: [Int64]) throws(BuildError) {
        try setArray(values)
    }

    /// Sets an array field of `uint64`s, and throws as ``setInt8(_:)`` does.
    public mutating func setUInt64Array(_ values: [UInt64]) throws(BuildError) {
        try setArray(values)
    }

    /// Sets an array field of `float32`s, and throws as ``setInt8(_:)`` does.
    public mutating func setFloat32Array(_ values: [Float]) throws(BuildError) {
        try setArray(values)
    }

    /// Sets an array field of `float64`s, and throws as ``setInt8(_:)`` does.
    public mutating func setFloat64Array(_ values: [Double]) throws(BuildError) {
        try setArray(values)
    }

    /// Sets the array field of `T`, as the setter of its type does
    public mutating func setArray<T: ArrayElement>(_ values: [T]) throws(BuildError) {
        try add(.array(Elements(values)))
    }

    /// Sets a string field, of its UTF-8, which a `String` always holds
    /// valid. Throws as ``setInt8(_:)`` does.
    public mutating func setString(_ value: String) throws(BuildError) {
        try add(.string(value, utf8Count: value.utf8.count))
    }

    /// Sets a struct field from its bytes: its scalars, little-endian, one
    /// after another, which are written as they are, without padding. Throws
    /// as ``setInt8(_:)`` does.
    public mutating func setStruct(_ bytes: [UInt8]) throws(BuildError) {
        try add(.structBytes(bytes))
    }

    /// Sets an array of structs of `size` bytes each, from their bytes one
    /// after another, which it counts. It's padded so that the structs start
    /// at a multiple of `alignment` from the start of the table, which is the
    /// size of the structs' largest scalar.
    ///
    /// Throws ``BuildError/notWholeStructs(length:size:)`` if `size` isn't
    /// positive or the bytes aren't a whole number of structs, or as
    /// ``setInt8(_:)`` does.
    public mutating func setStructArray(_ bytes: [UInt8], size: Int, alignment: Alignment)
        throws(BuildError)
    {
        guard size > 0, bytes.count % size == 0 else {
            throw .notWholeStructs(length: bytes.count, size: size)
        }

        try add(.structArray(bytes, size: size, alignment: alignment))
    }

    /// Embeds a table, from its builder or its bytes, without a length
    /// prefix. Readers find its fields with
    /// `TableReader.fieldOffset(_:inTableAt:)` from the field's position,
    /// which reads offsets of the parent's offset type, so the table should
    /// have no header and the parent's offset type: that's
    /// ``TableLayout/default`` in a parent of `uint32` offsets.
    ///
    /// A builder must have all its fields set, and is written straight into
    /// this table, in its own layout. The table's arrays are aligned from its
    /// own start, so it's padded to start at a multiple of `alignment`, which
    /// they need to be read in place. By default, that's the largest
    /// alignment of a builder's arrays, of scalars and of structs, and of its
    /// nested tables, or 1 if it has neither, and 4 for bytes, which suits
    /// tables of 8-, 16- and 32-bit elements.
    ///
    /// Throws ``BuildError/incomplete(set:fields:)`` if a builder has fields
    /// left to set, or as ``setInt8(_:)`` does.
    public mutating func setTable(_ table: NestedTable, alignment: Alignment? = nil) throws(BuildError) {
        try checkRoom()
        try table.checkComplete()

        let alignment = alignment ?? table.defaultAlignment
        switch table {
        case .builder(let builder): try add(builder, alignment: alignment)
        case .bytes(let bytes): try add(.tableBytes(bytes, alignment: alignment))
        }
    }

    /// Writes a vector of tables, from their builders or their bytes, after
    /// the end of each, which lets readers find any item without reading the
    /// others. Builders must have all their fields set, and are written as
    /// ``setTable(_:alignment:)`` writes them. Items aren't padded, so their
    /// arrays are read in place only when they happen to be aligned.
    ///
    /// Throws ``BuildError/incomplete(set:fields:)`` if a builder has fields
    /// left to set, or as ``setInt8(_:)`` does.
    public mutating func setVector(_ items: [NestedTable]) throws(BuildError) {
        try checkRoom()
        for item in items {
            try item.checkComplete()
        }

        try add(.vector(items))
    }

    /// Records a field after the padding that it needs. Checking the table's
    /// length here, rather than when writing it, reports a field too long for
    /// the layout from its own setter and leaves the builder as it was.
    /// Every offset, count, length and item end is at most the table's
    /// length, so that's the one value to check.
    @inline(__always)
    mutating func add(_ field: consuming Field) throws(BuildError) {
        try checkRoom()

        let width = UInt64(layout.offsetSize)
        let start = UInt64(header.length) + field.padding(at: UInt64(header.length), width: width)
        let (length, overflow) = start.addingReportingOverflow(field.byteLength(width: width))

        guard !overflow, length <= UInt64(layout.offsetType.maxValue), length <= UInt64(Int.max) else {
            throw .tooLong(index: fieldsSet, length: overflow ? .max : length, offsetType: layout.offsetType)
        }

        header.length = UInt32(length)
        alignment = max(alignment, field.alignment)
        fields.append(field, fieldCount: Int(header.fieldCount))
        fieldsSet += 1
    }

    /// Records a table nested from its builder, as ``add(_:)`` records a
    /// field: the builder's fields follow it while they're within the
    /// builder, and otherwise it shares the builder's array of them
    mutating func add(_ builder: TableBuilder, alignment: Alignment) throws(BuildError) {
        guard let shared = builder.fields.array else {
            try add(.table(builder.header, alignment: alignment))
            fields.append(contentsOf: builder.fields, fieldCount: Int(header.fieldCount))
            return
        }

        try add(.sharedTable(fields: shared, builder.header, alignment: alignment))
    }

    func checkRoom() throws(BuildError) {
        let fieldCount = Int(header.fieldCount)
        guard fieldsSet < fieldCount else {
            throw .tooManyFields(fields: fieldCount)
        }
    }

    func checkComplete() throws(BuildError) {
        let fieldCount = Int(header.fieldCount)
        guard fieldsSet == fieldCount else {
            throw .incomplete(set: fieldsSet, fields: fieldCount)
        }
    }
}

/// A table to nest in another with `TableBuilder.setTable(_:alignment:)` or
/// in a vector with `TableBuilder.setVector(_:)`: its builder, or the bytes
/// that a builder wrote.
public enum NestedTable: Sendable {
    /// A builder, which must have all its fields set, and which is written
    /// straight into its parent
    case builder(TableBuilder)
    /// A table's bytes, which are copied into its parent
    case bytes([UInt8])

    /// The table's byte length
    var length: UInt64 {
        switch self {
        case .builder(let builder): UInt64(builder.header.length)
        case .bytes(let bytes): UInt64(bytes.count)
        }
    }

    /// The alignment that `setTable(_:alignment:)` pads the table to by
    /// default
    var defaultAlignment: Alignment {
        switch self {
        case .builder(let builder): builder.alignment
        case .bytes: .four
        }
    }

    func checkComplete() throws(BuildError) {
        if case .builder(let builder) = self {
            try builder.checkComplete()
        }
    }
}
