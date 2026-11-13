package tables

import (
	tafla "github.com/snovader/tafla/go"
)

// Sizes and writes tables as their descriptions set them out, with
// TableSizer and TableWriter, which write tables nested in them in place.

// Sizer starts a sizer of the table's layout, version and number of fields,
// with no field added.
func (t Table) Sizer() (tafla.TableSizer, error) {
	layout := t.Layout.TableLayout()
	if t.Layout.Versioned {
		return tafla.NewVersionedTableSizer(len(t.Fields), layout, t.Version)
	}

	return tafla.NewTableSizer(len(t.Fields), layout)
}

// Write sizes and writes the table. The tables nested in it are written in
// place, so they must have no header and its offset type, as NestedInPlace
// makes them.
func (t Table) Write() ([]byte, error) {
	return t.WriteAs(t, nil)
}

// WriteAs sizes the table, and writes the fields of written after dst, into
// the bytes that it sized, which fails unless they're as long as its own.
func (t Table) WriteAs(written Table, dst []byte) ([]byte, error) {
	sizer, err := t.Sizer()
	if err != nil {
		return nil, err
	}
	for _, field := range t.Fields {
		Add(&sizer, field)
	}

	writer, err := sizer.AppendWriter(dst)
	if err != nil {
		return nil, err
	}
	for index, field := range written.Fields {
		Write(&writer, index, field)
	}

	return writer.Bytes()
}

// NestedInPlace returns the table with every table nested in it made one that
// a TableWriter writes in place: from its builder, without a header, and
// with the root's offset type.
func (t Table) NestedInPlace() Table {
	layout := Headerless(t.Layout.OffsetType)
	fields := make([]Field, len(t.Fields))
	for index, field := range t.Fields {
		fields[index] = nestedInPlace(field, layout)
	}

	return Table{Layout: t.Layout, Version: t.Version, Fields: fields}
}

// Alignment returns the alignment that SetTable pads the table to when given
// its builder: the largest alignment of its arrays, of scalars and of
// structs, and of its nested tables, or 1 if it has none.
func (t Table) Alignment() tafla.Alignment {
	alignment := tafla.Align1
	for _, field := range t.Fields {
		alignment = max(alignment, FieldAlignment(field))
	}

	return alignment
}

// addNested adds the fields of a nested table or vector item to the sizer
func (t Table) addNested(sizer *tafla.TableSizer) {
	parent := sizer.StartTable(len(t.Fields))
	for _, field := range t.Fields {
		Add(sizer, field)
	}
	sizer.EndTable(parent)
}

func (t Table) writeNested(writer *tafla.TableWriter) {
	parent := writer.StartTable(len(t.Fields))
	for index, field := range t.Fields {
		Write(writer, index, field)
	}
	writer.EndTable(parent)
}

// Add adds a field to the sizer, with what decides its length.
func Add(sizer *tafla.TableSizer, field Field) {
	switch value := field.(type) {
	case Int8:
		sizer.AddInt8()
	case Uint8, Bool:
		sizer.AddUint8()
	case Int16:
		sizer.AddInt16()
	case Uint16:
		sizer.AddUint16()
	case Int32:
		sizer.AddInt32()
	case Uint32:
		sizer.AddUint32()
	case Int64:
		sizer.AddInt64()
	case Uint64:
		sizer.AddUint64()
	case Float32:
		sizer.AddFloat32()
	case Float64:
		sizer.AddFloat64()
	case Int8Array:
		sizer.AddInt8Array(len(value))
	case Uint8Array:
		sizer.AddUint8Array(len(value))
	case Int16Array:
		sizer.AddInt16Array(len(value))
	case Uint16Array:
		sizer.AddUint16Array(len(value))
	case Int32Array:
		sizer.AddInt32Array(len(value))
	case Uint32Array:
		sizer.AddUint32Array(len(value))
	case Int64Array:
		sizer.AddInt64Array(len(value))
	case Uint64Array:
		sizer.AddUint64Array(len(value))
	case Float32Array:
		sizer.AddFloat32Array(len(value))
	case Float64Array:
		sizer.AddFloat64Array(len(value))
	case String:
		sizer.AddString(string(value))
	case Struct:
		sizer.AddStruct(StructSize(StructTypes(value)))
	case StructArray:
		sizer.AddStructArray(len(value.Items), StructSize(value.Types), StructAlignment(value.Types))
	case TableField:
		sizer.AddTable(FieldAlignment(value))
		value.Nested.Table.addNested(sizer)
	case Vector:
		sizer.AddVector(len(value))
		for _, item := range value {
			item.Table.addNested(sizer)
		}
	default:
		panic("unknown field")
	}
}

// Write writes a field as field index of the writer's table.
func Write(writer *tafla.TableWriter, index int, field Field) {
	switch value := field.(type) {
	case Int8:
		writer.SetInt8(index, int8(value))
	case Uint8:
		writer.SetUint8(index, uint8(value))
	case Int16:
		writer.SetInt16(index, int16(value))
	case Uint16:
		writer.SetUint16(index, uint16(value))
	case Int32:
		writer.SetInt32(index, int32(value))
	case Uint32:
		writer.SetUint32(index, uint32(value))
	case Int64:
		writer.SetInt64(index, int64(value))
	case Uint64:
		writer.SetUint64(index, uint64(value))
	case Float32:
		writer.SetFloat32(index, float32(value))
	case Float64:
		writer.SetFloat64(index, float64(value))
	case Bool:
		writer.SetUint8(index, boolByte(bool(value)))
	case Int8Array:
		writer.SetInt8Array(index, value)
	case Uint8Array:
		writer.SetUint8Array(index, value)
	case Int16Array:
		writer.SetInt16Array(index, value)
	case Uint16Array:
		writer.SetUint16Array(index, value)
	case Int32Array:
		writer.SetInt32Array(index, value)
	case Uint32Array:
		writer.SetUint32Array(index, value)
	case Int64Array:
		writer.SetInt64Array(index, value)
	case Uint64Array:
		writer.SetUint64Array(index, value)
	case Float32Array:
		writer.SetFloat32Array(index, value)
	case Float64Array:
		writer.SetFloat64Array(index, value)
	case String:
		writer.SetString(index, string(value))
	case Struct:
		writer.SetStruct(index, StructBytes(value))
	case StructArray:
		var bytes []byte
		for _, item := range value.Items {
			bytes = append(bytes, StructBytes(item)...)
		}
		writer.SetStructArray(index, bytes, StructSize(value.Types), StructAlignment(value.Types))
	case TableField:
		writer.SetTable(index, FieldAlignment(value))
		value.Nested.Table.writeNested(writer)
	case Vector:
		vector := writer.SetVector(index, len(value))
		for item, nested := range value {
			nested.Table.writeNested(writer)
			writer.SetItemEnd(vector, item)
		}
	default:
		panic("unknown field")
	}
}

// FieldAlignment returns the alignment that a field needs from its table's
// start: an array's element size, an array of structs' largest scalar's,
// the alignment that a nested table is set with, or else 1.
func FieldAlignment(field Field) tafla.Alignment {
	switch value := field.(type) {
	case Int16Array, Uint16Array:
		return tafla.Align2
	case Int32Array, Uint32Array, Float32Array:
		return tafla.Align4
	case Int64Array, Uint64Array, Float64Array:
		return tafla.Align8
	case StructArray:
		return StructAlignment(value.Types)
	case TableField:
		if value.Alignment != 0 {
			return value.Alignment
		}
		if value.Nested.From == FromBytes {
			return tafla.Align4
		}
		return value.Nested.Table.Alignment()
	default:
		// Arrays of bytes, and the fields that aren't padded
		return tafla.Align1
	}
}

// nestedInPlace returns the field with the tables nested in it made ones that
// a TableWriter writes in place, in layout
func nestedInPlace(field Field, layout Layout) Field {
	inPlace := func(nested Nested) Nested {
		fields := make([]Field, len(nested.Table.Fields))
		for index, field := range nested.Table.Fields {
			fields[index] = nestedInPlace(field, layout)
		}

		return Nested{From: FromBuilder, Table: Table{Layout: layout, Fields: fields}}
	}

	switch value := field.(type) {
	case TableField:
		return TableField{Nested: inPlace(value.Nested), Alignment: value.Alignment}
	case Vector:
		items := make(Vector, len(value))
		for index, item := range value {
			items[index] = inPlace(item)
		}
		return items
	default:
		return field
	}
}
