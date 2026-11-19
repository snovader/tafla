import Tafla

// Builds tables from their descriptions with TableBuilder, from a builder of
// each table nested in them, or with the tables recorded in place

extension Table {
    /// A builder of the table's layout, version and number of fields, with no
    /// field set
    func builder() throws(BuildError) -> TableBuilder {
        try TableBuilder(fields: fields.count, layout: layout, version: version)
    }

    /// Builds the table, setting every field
    func build() throws(BuildError) -> TableBuilder {
        var builder = try builder()
        for field in fields {
            try builder.set(field)
        }

        return builder
    }

    /// Writes the table with a `TableBuilder`
    func bytes() throws(BuildError) -> [UInt8] {
        try build().bytes()
    }

    /// Writes the table with a `TableBuilder`, as ``bytes()`` does, but
    /// records each table nested from a builder that it records in place with
    /// `setTable(fields:_:)`, and each vector of them whose items hold as many
    /// fields with `setVector(count:fields:_:)`, rather than from their
    /// builders. Its bytes and errors are those of ``bytes()``, but for a
    /// vector whose count and item ends alone make its table too long, which
    /// fails before its items are recorded.
    func bytesInPlace() throws(BuildError) -> [UInt8] {
        // The tables that its builder records in place: from a builder,
        // without a header, with this table's offset type, and padded to their
        // own alignment
        let layout = headerless(layout.offsetType)
        let inPlace = { (nested: Nested) in nested.from == .builder && nested.table.layout == layout }

        var builder = try builder()
        for field in fields {
            try builder.set(field, inPlace: inPlace)
        }

        return try builder.bytes()
    }
}

extension Table {
    /// The table with every table nested in it made one that
    /// ``bytesInPlace()`` records in place: from its builder, without a
    /// header, with the root's offset type, padded to its own alignment, and
    /// the items of each vector cut to the fields of the shortest
    func recordedInPlace() -> Table {
        let nested = headerless(layout.offsetType)
        return Table(layout: layout, version: version, fields: fields.map { $0.recordedInPlace(nested) })
    }
}

extension Field {
    /// The field with the tables nested in it made ones that
    /// `Table.bytesInPlace()` records in place, in `layout`
    func recordedInPlace(_ layout: TableLayout) -> Field {
        func inPlace(_ fields: some Collection<Field>) -> Nested {
            Nested(from: .builder, table: Table(layout: layout, fields: fields.map { $0.recordedInPlace(layout) }))
        }

        switch self {
        case .table(let nested, _):
            return .table(inPlace(nested.table.fields), alignment: nil)
        case .vector(let items):
            let shortest = items.map(\.table.fields.count).min() ?? 0
            return .vector(items.map { inPlace($0.table.fields.prefix(shortest)) })
        default:
            return self
        }
    }
}

extension Nested {
    /// The nested table, from its builder or from its builder's bytes
    func nestedTable() throws(BuildError) -> NestedTable {
        switch from {
        case .builder: .builder(try table.build())
        case .bytes: .bytes(try table.bytes())
        }
    }
}

/// What a field is set on: a `TableBuilder`, or the item of a table that one
/// records in place
protocol FieldTarget: ~Copyable {
    mutating func setInt8(_ value: Int8) throws(BuildError)
    mutating func setUInt8(_ value: UInt8) throws(BuildError)
    mutating func setInt16(_ value: Int16) throws(BuildError)
    mutating func setUInt16(_ value: UInt16) throws(BuildError)
    mutating func setInt32(_ value: Int32) throws(BuildError)
    mutating func setUInt32(_ value: UInt32) throws(BuildError)
    mutating func setInt64(_ value: Int64) throws(BuildError)
    mutating func setUInt64(_ value: UInt64) throws(BuildError)
    mutating func setFloat32(_ value: Float) throws(BuildError)
    mutating func setFloat64(_ value: Double) throws(BuildError)
    mutating func setInt8Array(_ values: [Int8]) throws(BuildError)
    mutating func setUInt8Array(_ values: [UInt8]) throws(BuildError)
    mutating func setInt16Array(_ values: [Int16]) throws(BuildError)
    mutating func setUInt16Array(_ values: [UInt16]) throws(BuildError)
    mutating func setInt32Array(_ values: [Int32]) throws(BuildError)
    mutating func setUInt32Array(_ values: [UInt32]) throws(BuildError)
    mutating func setInt64Array(_ values: [Int64]) throws(BuildError)
    mutating func setUInt64Array(_ values: [UInt64]) throws(BuildError)
    mutating func setFloat32Array(_ values: [Float]) throws(BuildError)
    mutating func setFloat64Array(_ values: [Double]) throws(BuildError)
    mutating func setString(_ value: String) throws(BuildError)
    mutating func setStruct(_ bytes: [UInt8]) throws(BuildError)
    mutating func setStructArray(_ bytes: [UInt8], size: Int, alignment: Alignment) throws(BuildError)
    mutating func setTable(_ table: NestedTable, alignment: Alignment?) throws(BuildError)
    mutating func setVector(_ items: [NestedTable]) throws(BuildError)
    mutating func setTable(fields: Int, _ fill: (inout TableBuilder.Item) throws(BuildError) -> Void)
        throws(BuildError)
    mutating func setVector(count: Int, fields: Int, _ fill: (Int, inout TableBuilder.Item) throws(BuildError) -> Void)
        throws(BuildError)
}

extension TableBuilder: FieldTarget {}

extension TableBuilder.Item: FieldTarget {}

extension FieldTarget where Self: ~Copyable {
    /// Sets a field as its description describes it, recording the tables
    /// nested in it that `inPlace` accepts in place, as
    /// `Table.bytesInPlace()` does
    mutating func set(_ field: Field, inPlace: (Nested) -> Bool = { _ in false }) throws(BuildError) {
        switch field {
        case .int8(let value): try setInt8(value)
        case .uint8(let value): try setUInt8(value)
        case .int16(let value): try setInt16(value)
        case .uint16(let value): try setUInt16(value)
        case .int32(let value): try setInt32(value)
        case .uint32(let value): try setUInt32(value)
        case .int64(let value): try setInt64(value)
        case .uint64(let value): try setUInt64(value)
        case .float32(let bits): try setFloat32(Float(bitPattern: bits))
        case .float64(let bits): try setFloat64(Double(bitPattern: bits))
        case .bool(let value): try setUInt8(value ? 1 : 0)
        case .int8Array(let values): try setInt8Array(values)
        case .uint8Array(let values): try setUInt8Array(values)
        case .int16Array(let values): try setInt16Array(values)
        case .uint16Array(let values): try setUInt16Array(values)
        case .int32Array(let values): try setInt32Array(values)
        case .uint32Array(let values): try setUInt32Array(values)
        case .int64Array(let values): try setInt64Array(values)
        case .uint64Array(let values): try setUInt64Array(values)
        case .float32Array(let bits): try setFloat32Array(bits.map(Float.init(bitPattern:)))
        case .float64Array(let bits): try setFloat64Array(bits.map(Double.init(bitPattern:)))
        case .string(let utf8): try setString(String(decoding: utf8, as: UTF8.self))
        case .struct(let scalars): try setStruct(structBytes(scalars))
        case .structArray(let types, let items):
            try setStructArray(items.flatMap(structBytes), size: structSize(types), alignment: structAlignment(types))
        case .table(let nested, nil) where inPlace(nested):
            try setTable(fields: nested.table.fields.count) { table throws(BuildError) in
                try table.set(all: nested.table.fields, inPlace: inPlace)
            }
        case .table(let nested, let alignment): try setTable(nested.nestedTable(), alignment: alignment)
        case .vector(let items) where items.allSatisfy(inPlace) && Set(items.map(\.table.fields.count)).count <= 1:
            let fields = items.first?.table.fields.count ?? 0
            try setVector(count: items.count, fields: fields) { index, item throws(BuildError) in
                try item.set(all: items[index].table.fields, inPlace: inPlace)
            }
        case .vector(let items):
            var tables: [NestedTable] = []
            for item in items {
                tables.append(try item.nestedTable())
            }
            try setVector(tables)
        }
    }

    /// Sets each of `fields`, as ``set(_:inPlace:)`` sets it
    mutating func set(all fields: [Field], inPlace: (Nested) -> Bool) throws(BuildError) {
        for field in fields {
            try set(field, inPlace: inPlace)
        }
    }
}
