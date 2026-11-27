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
/// ``setTable(fields:_:)`` and ``setVector(count:fields:_:)`` record a
/// nested table, or each item of a vector, in place, without a builder of
/// its own.
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
    /// `layout`, which its readers must use too, with the layout's version
    /// in its header when it has one.
    ///
    /// Throws ``BuildError/versionTooLarge(version:offsetType:)`` if the
    /// layout's version doesn't fit in its offset type, or
    /// ``BuildError/headerTooLong(fields:offsetType:)`` if the header alone is
    /// too long for it.
    public init(fields: Int, layout: TableLayout = .default) throws(BuildError) {
        precondition(fields >= 0, "A table can't hold \(fields) fields")

        try layout.checkVersion()
        let headerLength = try layout.headerLength(fields: fields)

        self.header = TableHeader(
            layout: layout,
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

    /// Sets a vector of strings, each of its UTF-8, which a `String` always
    /// holds valid, after the end of each, which lets readers find any string
    /// without reading the others. It takes no more bytes for each string
    /// than the end of it, where a vector of tables of one string takes each
    /// table's offset and the string's count too. Throws as ``setInt8(_:)``
    /// does.
    public mutating func setStringVector(_ strings: [String]) throws(BuildError) {
        var utf8Count: UInt64 = 0
        for string in strings {
            utf8Count = saturating(utf8Count, plus: UInt64(string.utf8.count))
        }

        try add(.stringVector(strings, utf8Count: utf8Count))
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

    /// Records a table of `fields` fields, which mustn't be negative, nested
    /// in this one, in place: `fill` sets its fields, in order, on the
    /// ``Item`` that it's given, and each is recorded straight among this
    /// builder's fields, rather than by a builder of the table's own. The
    /// table has no header and this table's offset type, and is padded as
    /// ``setTable(_:alignment:)`` pads a builder, to the largest alignment of
    /// its arrays and nested tables, so the bytes are those that nesting a
    /// builder of the same fields writes. The item's setters count the
    /// table's own fields and length, as that builder's would, and a table
    /// nested in it can be recorded in place too.
    ///
    /// ```swift
    /// var builder = try TableBuilder(fields: 2)
    /// try builder.setFloat64(59.3293)
    /// try builder.setTable(fields: 2) { series throws(BuildError) in
    ///     try series.setUInt32Array([0, 86_400])
    ///     try series.setFloat32Array([21.75, 18.5])
    /// }
    /// ```
    ///
    /// `fill` throws a ``BuildError``, as the item's setters do, and a closure
    /// that throws says so, since Swift infers that it throws `any Error`.
    ///
    /// Throws ``BuildError/tooManyFields(fields:)`` if this table holds all
    /// its fields already, ``BuildError/headerTooLong(fields:offsetType:)``
    /// if the nested table's offsets alone are too long for the offset type,
    /// the error of an item's setter, which `fill` passes on,
    /// ``BuildError/incomplete(set:fields:)`` if `fill` leaves fields of the
    /// nested table unset, or ``BuildError/tooLong(index:length:offsetType:)``
    /// if the nested table makes this one too long. Whatever is thrown, the
    /// builder is left as it was.
    public mutating func setTable(fields: Int, _ fill: (inout Item) throws(BuildError) -> Void) throws(BuildError) {
        precondition(fields >= 0, "A table can't hold \(fields) fields")
        try checkRoom()
        let table = try nestedHeader(fields: fields)

        let parent = counters
        let start = self.fields.count
        self.fields.append(.unset, fieldCount: Int(header.fieldCount))

        do {
            try record(table, fill)

            let field = Field.table(header, alignment: alignment)
            counters = parent
            try add(field, at: start)
        } catch {
            rollBack(to: start, parent: parent)
            throw error
        }
    }

    /// Records a vector of `count` tables of `fields` fields each in place,
    /// neither of which may be negative, as ``setTable(fields:_:)`` records
    /// one: `fill` sets the fields of each item, in order, on the ``Item``
    /// that it's given with the item's index. The vector's bytes are those
    /// that ``setVector(_:)`` writes from builders of the items, which aren't
    /// padded.
    ///
    /// Throws ``BuildError/tooLong(index:length:offsetType:)`` before
    /// recording any item if the vector's count and the ends of its items
    /// alone make this table too long, with the length that they make it,
    /// or once its items are recorded if they do, and otherwise as
    /// ``setTable(fields:_:)`` throws, for each item. Whatever is thrown,
    /// the builder is left as it was.
    public mutating func setVector(
        count: Int,
        fields: Int,
        _ fill: (Int, inout Item) throws(BuildError) -> Void
    ) throws(BuildError) {
        precondition(count >= 0, "A vector can't hold \(count) items")
        precondition(fields >= 0, "A table can't hold \(fields) fields")
        try checkRoom()

        // The count and the ends of the items, which fail before any item is
        // recorded, and only an item for its header, as its builder would
        var length = saturating(UInt64(layout.offsetSize), plus: Tafla.length(of: count, size: layout.offsetSize))
        _ = try checked(saturating(UInt64(header.length), plus: length))
        let itemHeader = try nestedHeader(fields: count > 0 ? fields : 0)

        let parent = counters
        let start = self.fields.count
        self.fields.append(.unset, fieldCount: Int(header.fieldCount))

        do {
            for index in 0..<count {
                let itemStart = self.fields.count
                self.fields.append(.unset, fieldCount: Int(header.fieldCount))
                try record(itemHeader) { item throws(BuildError) in try fill(index, &item) }

                self.fields.replace(at: itemStart, with: .table(header, alignment: .one))
                length = saturating(length, plus: UInt64(header.length))
                if index == 0 {
                    reserveItems(count - 1, records: self.fields.count - itemStart, length: parent.header.length)
                }
            }

            counters = parent
            try add(.vectorInPlace(count: count, length: length), at: start)
        } catch {
            rollBack(to: start, parent: parent)
            throw error
        }
    }

    /// Records a field after the padding that it needs. Checking the table's
    /// length here, rather than when writing it, reports a field too long for
    /// the layout from its own setter and leaves the builder as it was.
    @inline(__always)
    mutating func add(_ field: consuming Field) throws(BuildError) {
        try checkRoom()
        let length = try length(after: field)

        header.length = length
        alignment = max(alignment, field.alignment)
        fields.append(field, fieldCount: Int(header.fieldCount))
        fieldsSet += 1
    }

    /// Records a table, or vector of tables, recorded in place, in the field
    /// at `start` that it held while its fields were recorded after it, as
    /// ``add(_:)`` records a field
    mutating func add(_ field: consuming Field, at start: Int) throws(BuildError) {
        let length = try length(after: field)

        header.length = length
        alignment = max(alignment, field.alignment)
        fields.replace(at: start, with: field)
        fieldsSet += 1
    }

    /// The table's length with `field` after it, and the padding that it
    /// needs. Every offset, count, length and item end is at most the
    /// table's length, so that's the one value to check.
    @inline(__always)
    func length(after field: Field) throws(BuildError) -> UInt32 {
        let width = UInt64(layout.offsetSize)
        let start = UInt64(header.length) + field.padding(at: UInt64(header.length), width: width)

        return try checked(saturating(start, plus: field.byteLength(width: width)))
    }

    /// `length`, or ``BuildError/tooLong(index:length:offsetType:)`` for the
    /// field being set if it's too long for the table's offset type or an
    /// `Int`
    @inline(__always)
    func checked(_ length: UInt64) throws(BuildError) -> UInt32 {
        guard length <= UInt64(layout.offsetType.maxValue), length <= UInt64(Int.max) else {
            throw .tooLong(index: fieldsSet, length: length, offsetType: layout.offsetType)
        }

        return UInt32(length)
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

    /// The header of a table of `fields` fields recorded in place, with no
    /// header slots and this table's offset type, or
    /// ``BuildError/headerTooLong(fields:offsetType:)`` if its offsets alone
    /// are too long for it
    func nestedHeader(fields: Int) throws(BuildError) -> TableHeader {
        let layout = TableLayout(offsetType: layout.offsetType)
        let headerLength = try layout.headerLength(fields: fields)

        return TableHeader(
            layout: layout,
            fieldCount: UInt32(fields),
            headerLength: headerLength,
            length: headerLength
        )
    }

    /// Records the fields of a table in place: counts the table's fields
    /// rather than this one's, and gives `fill` an item of this builder to
    /// set them on, which must set every one
    mutating func record(_ table: TableHeader, _ fill: (inout Item) throws(BuildError) -> Void) throws(BuildError) {
        counters = Counters(header: table, fieldsSet: 0, alignment: .one)

        try withUnsafeMutablePointer(to: &self) { builder throws(BuildError) in
            var item = Item(builder: builder)
            try fill(&item)
        }
        try checkComplete()
    }

    /// Makes room for `count` more items of a vector recorded in place, each
    /// of `records` fields with their tables', as its first item recorded,
    /// though for no more than the table, whose length is `length` before
    /// the vector, can hold, since each takes an offset or an item's end
    mutating func reserveItems(_ count: Int, records: Int, length: UInt32) {
        let maxLength = min(UInt64(layout.offsetType.maxValue), UInt64(Int.max))
        let room = (maxLength - min(UInt64(length), maxLength)) / UInt64(layout.offsetSize)
        let (wanted, overflow) = UInt64(count).multipliedReportingOverflow(by: UInt64(records))

        self.fields.reserveCapacity(self.fields.count + Int(min(overflow ? .max : wanted, room)))
    }

    /// Removes the fields of a table, or vector of tables, recorded in place
    /// from `start` on, and counts the parent's fields again, after the table
    /// failed
    mutating func rollBack(to start: Int, parent: Counters) {
        fields.removeFields(from: start)
        counters = parent
    }

    /// The counters of the table whose fields are being set, which a table
    /// recorded in place counts its own fields with while it's recorded
    var counters: Counters {
        get { Counters(header: header, fieldsSet: fieldsSet, alignment: alignment) }
        set {
            header = newValue.header
            fieldsSet = newValue.fieldsSet
            alignment = newValue.alignment
        }
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

/// What a builder counts of the table whose fields are being set
struct Counters {
    var header: TableHeader
    var fieldsSet: Int
    var alignment: Alignment
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
