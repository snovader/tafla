import Tafla

// Sizes and writes tables as their descriptions set them out, with
// TableSizer and TableWriter, which write the tables nested in them in place,
// as the code that tafla-gen generates does: structs scalar by scalar, after
// their setStruct(_:) and setStructArray(_:count:alignment:).

extension Table {
    /// A sizer of the table's layout, version and number of fields, with no
    /// field added
    func sizer() throws(BuildError) -> TableSizer {
        try TableSizer(fields: fields.count, layout: layout, version: version)
    }

    /// A sizer of the table with every field added
    func sized() throws(BuildError) -> TableSizer {
        var sizer = try sizer()
        for field in fields {
            sizer.add(field)
        }

        return sizer
    }

    /// Sizes the table, and writes the fields of `written`, or its own, into
    /// the bytes that it sized. The tables nested in them are written in
    /// place, so they must have no header and the root's offset type, as
    /// `nestedInPlace()` makes them.
    func write(as written: Table? = nil) throws(BuildError) -> [UInt8] {
        let fields = (written ?? self).fields

        return try sized().bytes { writer in
            for (index, field) in fields.enumerated() {
                writer.set(index, field)
            }
        }
    }

    /// The table with every table nested in it made one that a `TableWriter`
    /// writes in place: from its builder, without a header, and with the
    /// root's offset type
    func nestedInPlace() -> Table {
        let layout = headerless(layout.offsetType)
        return Table(layout: self.layout, version: version, fields: fields.map { $0.nestedInPlace(layout) })
    }
}

extension Field {
    /// The field with the tables nested in it made ones that a `TableWriter`
    /// writes in place, in `layout`
    func nestedInPlace(_ layout: TableLayout) -> Field {
        func inPlace(_ nested: Nested) -> Nested {
            Nested(
                from: .builder,
                table: Table(layout: layout, fields: nested.table.fields.map { $0.nestedInPlace(layout) }))
        }

        switch self {
        case .table(let nested, let alignment): return .table(inPlace(nested), alignment: alignment)
        case .vector(let items): return .vector(items.map(inPlace))
        default: return self
        }
    }

    /// The alignment that a nested table is padded to: the one that it's set
    /// with, or its builder's
    var tableAlignment: Alignment {
        guard case .table(let nested, let alignment) = self else {
            preconditionFailure("Only a nested table is padded to its alignment")
        }

        return alignment ?? defaultAlignment(of: nested)
    }
}

extension TableSizer {
    /// Adds a field, with what decides its length
    mutating func add(_ field: Field) {
        switch field {
        case .int8: addInt8()
        case .uint8, .bool: addUInt8()
        case .int16: addInt16()
        case .uint16: addUInt16()
        case .int32: addInt32()
        case .uint32: addUInt32()
        case .int64: addInt64()
        case .uint64: addUInt64()
        case .float32: addFloat32()
        case .float64: addFloat64()
        case .int8Array(let values): addInt8Array(count: values.count)
        case .uint8Array(let values): addUInt8Array(count: values.count)
        case .int16Array(let values): addInt16Array(count: values.count)
        case .uint16Array(let values): addUInt16Array(count: values.count)
        case .int32Array(let values): addInt32Array(count: values.count)
        case .uint32Array(let values): addUInt32Array(count: values.count)
        case .int64Array(let values): addInt64Array(count: values.count)
        case .uint64Array(let values): addUInt64Array(count: values.count)
        case .float32Array(let bits): addFloat32Array(count: bits.count)
        case .float64Array(let bits): addFloat64Array(count: bits.count)
        case .string(let utf8): addString(String(decoding: utf8, as: UTF8.self))
        case .struct(let scalars): addStruct(size: structBytes(scalars).count)
        case .structArray(let types, let items):
            addStructArray(count: items.count, size: structSize(types), alignment: structAlignment(types))
        case .table(let nested, _):
            addTable(alignment: field.tableAlignment)
            add(nested: nested.table)
        case .vector(let items):
            addVector(count: items.count)
            for item in items {
                add(nested: item.table)
            }
        }
    }

    /// Adds the fields of a nested table or vector item
    mutating func add(nested table: Table) {
        let parent = startTable(fields: table.fields.count)
        for field in table.fields {
            add(field)
        }
        endTable(parent)
    }
}

extension TableWriter {
    /// Writes a field as field `index` of the writer's table
    mutating func set(_ index: Int, _ field: Field) {
        switch field {
        case .int8(let value): setInt8(index, value)
        case .uint8(let value): setUInt8(index, value)
        case .int16(let value): setInt16(index, value)
        case .uint16(let value): setUInt16(index, value)
        case .int32(let value): setInt32(index, value)
        case .uint32(let value): setUInt32(index, value)
        case .int64(let value): setInt64(index, value)
        case .uint64(let value): setUInt64(index, value)
        case .float32(let bits): setFloat32(index, Float(bitPattern: bits))
        case .float64(let bits): setFloat64(index, Double(bitPattern: bits))
        case .bool(let value): setUInt8(index, value ? 1 : 0)
        case .int8Array(let values): setInt8Array(index, values)
        case .uint8Array(let values): setUInt8Array(index, values)
        case .int16Array(let values): setInt16Array(index, values)
        case .uint16Array(let values): setUInt16Array(index, values)
        case .int32Array(let values): setInt32Array(index, values)
        case .uint32Array(let values): setUInt32Array(index, values)
        case .int64Array(let values): setInt64Array(index, values)
        case .uint64Array(let values): setUInt64Array(index, values)
        case .float32Array(let bits): setFloat32Array(index, bits.map(Float.init(bitPattern:)))
        case .float64Array(let bits): setFloat64Array(index, bits.map(Double.init(bitPattern:)))
        case .string(let utf8): setString(index, String(decoding: utf8, as: UTF8.self))
        case .struct(let scalars):
            setStruct(index)
            writeScalars(scalars)
        case .structArray(let types, let items):
            setStructArray(index, count: items.count, alignment: structAlignment(types))
            for item in items {
                writeScalars(item)
            }
        case .table(let nested, _):
            setTable(index, alignment: field.tableAlignment)
            write(nested: nested.table)
        case .vector(let items):
            let vector = setVector(index, count: items.count)
            for (item, nested) in items.enumerated() {
                write(nested: nested.table)
                setItemEnd(vector, item: item)
            }
        }
    }

    /// Writes the fields of a nested table or vector item
    mutating func write(nested table: Table) {
        let parent = startTable(fields: table.fields.count)
        for (index, field) in table.fields.enumerated() {
            set(index, field)
        }
        endTable(parent)
    }

    /// Writes a struct's scalars where the writer is, one after another
    mutating func writeScalars(_ scalars: [Field]) {
        for scalar in scalars {
            switch scalar {
            case .int8(let value): writeScalar(value)
            case .uint8(let value): writeScalar(value)
            case .int16(let value): writeScalar(value)
            case .uint16(let value): writeScalar(value)
            case .int32(let value): writeScalar(value)
            case .uint32(let value): writeScalar(value)
            case .int64(let value): writeScalar(value)
            case .uint64(let value): writeScalar(value)
            case .float32(let bits): writeScalar(bits)
            case .float64(let bits): writeScalar(bits)
            default: preconditionFailure("A struct holds scalars")
            }
        }
    }
}
