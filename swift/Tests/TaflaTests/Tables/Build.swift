import Tafla

// Builds tables from their descriptions with TableBuilder

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

extension TableBuilder {
    /// Sets a field as its description describes it
    mutating func set(_ field: Field) throws(BuildError) {
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
        case .table(let nested, let alignment): try setTable(nested.nestedTable(), alignment: alignment)
        case .vector(let items):
            var tables: [NestedTable] = []
            for item in items {
                tables.append(try item.nestedTable())
            }
            try setVector(tables)
        }
    }
}
