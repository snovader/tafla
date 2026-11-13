package tables

import (
	"encoding/binary"

	tafla "github.com/snovader/tafla/go"
)

// Writes tables as code that lays out their fields of fixed size itself
// does, as the code that tafla-gen generates for Go does: a table's first
// such fields with TableSizer.AddFields and TableWriter.ReserveFields, nested
// tables of only such fields with AddTableBytes and ReserveTableBytes, and a
// table of only such fields, and a string after them, with AppendTable.

// WriteLaidOut writes the table after dst, laying out its fields of fixed
// size itself. The tables nested in it are written in place, so they must
// have no header and its offset type, as NestedInPlace makes them.
func (t Table) WriteLaidOut(dst []byte) ([]byte, error) {
	width := t.Layout.OffsetType.Size()
	first := leadingFixed(t.Fields, width)
	rest := t.Fields[first:]
	if _, isString := lastOf(rest).(String); len(rest) == 0 || len(rest) == 1 && isString {
		return t.appendLaidOut(dst, first)
	}

	sizer, err := t.Sizer()
	if err != nil {
		return nil, err
	}
	fixed := fixedLength(t.Fields[:first], width)
	sizer.AddFields(first, fixed)
	for _, field := range rest {
		addLaidOut(&sizer, field, width)
	}

	writer, err := sizer.AppendWriter(dst)
	if err != nil {
		return nil, err
	}
	putFixed(writer.ReserveFields(fixed), t.Layout.HeaderSize(), len(t.Fields), t.Fields[:first], width)
	for index, field := range rest {
		writeLaidOut(&writer, first+index, field, width)
	}

	return writer.Bytes()
}

// appendLaidOut writes the table, whose fields after its first fields of
// fixed size are at most a string, with AppendTable
func (t Table) appendLaidOut(dst []byte, first int) ([]byte, error) {
	width := t.Layout.OffsetType.Size()
	header := t.Layout.HeaderSize()
	end := header + len(t.Fields)*width + fixedLength(t.Fields[:first], width)
	length := uint64(end)
	value, hasString := lastOf(t.Fields[first:]).(String)
	if hasString {
		length += uint64(width + len(value))
	}

	layout := t.Layout.TableLayout()
	var whole, table []byte
	var err error
	if t.Layout.Versioned {
		whole, table, err = tafla.AppendVersionedTable(dst, len(t.Fields), layout, t.Version, length)
	} else {
		whole, table, err = tafla.AppendTable(dst, len(t.Fields), layout, length)
	}
	if err != nil {
		return nil, err
	}

	putFixed(table, header, len(t.Fields), t.Fields[:first], width)
	if hasString {
		putOffset(table, header+first*width, width, end)
		putOffset(table, end, width, len(value))
		copy(table[end+width:], value)
	}

	return whole, nil
}

func addLaidOut(sizer *tafla.TableSizer, field Field, width int) {
	switch value := field.(type) {
	case TableField:
		sizer.AddTable(FieldAlignment(value))
		value.Nested.Table.addNestedLaidOut(sizer, width)
	case Vector:
		sizer.AddVector(len(value))
		if length, ok := sameFixedLength(value, width); ok {
			sizer.AddTableBytes(len(value), length)
			return
		}
		for _, item := range value {
			item.Table.addNestedLaidOut(sizer, width)
		}
	default:
		Add(sizer, field)
	}
}

func (t Table) addNestedLaidOut(sizer *tafla.TableSizer, width int) {
	first := leadingFixed(t.Fields, width)
	if first == len(t.Fields) {
		sizer.AddTableBytes(1, len(t.Fields)*width+fixedLength(t.Fields, width))
		return
	}

	parent := sizer.StartTable(len(t.Fields))
	sizer.AddFields(first, fixedLength(t.Fields[:first], width))
	for _, field := range t.Fields[first:] {
		addLaidOut(sizer, field, width)
	}
	sizer.EndTable(parent)
}

func writeLaidOut(writer *tafla.TableWriter, index int, field Field, width int) {
	switch value := field.(type) {
	case TableField:
		writer.SetTable(index, FieldAlignment(value))
		value.Nested.Table.writeNestedLaidOut(writer, width)
	case Vector:
		vector := writer.SetVector(index, len(value))
		for item, nested := range value {
			nested.Table.writeNestedLaidOut(writer, width)
			writer.SetItemEnd(vector, item)
		}
	default:
		Write(writer, index, field)
	}
}

func (t Table) writeNestedLaidOut(writer *tafla.TableWriter, width int) {
	first := leadingFixed(t.Fields, width)
	if first == len(t.Fields) {
		table := writer.ReserveTableBytes(len(t.Fields)*width + fixedLength(t.Fields, width))
		putFixed(table, 0, len(t.Fields), t.Fields, width)
		return
	}

	parent := writer.StartTable(len(t.Fields))
	putFixed(writer.ReserveFields(fixedLength(t.Fields[:first], width)), 0, len(t.Fields), t.Fields[:first], width)
	for index, field := range t.Fields[first:] {
		writeLaidOut(writer, first+index, field, width)
	}
	writer.EndTable(parent)
}

// leadingFixed returns how many of the first fields have fixed sizes: a
// scalar, a bool, a struct, or a table nested in place without padding,
// whose fields all have fixed sizes
func leadingFixed(fields []Field, width int) int {
	for index, field := range fields {
		if _, ok := fixedSize(field, width); !ok {
			return index
		}
	}

	return len(fields)
}

// fixedSize returns the byte length of a field of fixed size, or false for
// any other field
func fixedSize(field Field, width int) (int, bool) {
	switch value := field.(type) {
	case Bool:
		return 1, true
	case Struct:
		return StructSize(StructTypes(value)), true
	case TableField:
		table := value.Nested.Table
		if value.Nested.From != FromBuilder || FieldAlignment(value) != tafla.Align1 ||
			leadingFixed(table.Fields, width) < len(table.Fields) {
			return 0, false
		}
		return len(table.Fields)*width + fixedLength(table.Fields, width), true
	case Int8, Uint8, Int16, Uint16, Int32, Uint32, Int64, Uint64, Float32, Float64:
		return ScalarTypeOf(value).Size(), true
	default:
		return 0, false
	}
}

// fixedLength returns the byte length of fields of fixed size
func fixedLength(fields []Field, width int) int {
	length := 0
	for _, field := range fields {
		size, _ := fixedSize(field, width)
		length += size
	}

	return length
}

// sameFixedLength returns the byte length of tables of only fields of fixed
// size, when every one of them is such a table of the same length
func sameFixedLength(items []Nested, width int) (int, bool) {
	length := -1
	for _, item := range items {
		fields := item.Table.Fields
		if leadingFixed(fields, width) < len(fields) {
			return 0, false
		}
		itemLength := len(fields)*width + fixedLength(fields, width)
		if length >= 0 && itemLength != length {
			return 0, false
		}
		length = itemLength
	}

	return length, length >= 0
}

// putFixed writes the offsets and values of a table's first fields, of fixed
// size, into its bytes, after the header of header bytes, and returns where
// they end
func putFixed(table []byte, header, fields int, fixed []Field, width int) int {
	position := header + fields*width
	for index, field := range fixed {
		putOffset(table, header+index*width, width, position)
		position = putValue(table, position, field, width)
	}

	return position
}

// putValue writes a field of fixed size at position, and returns where it
// ends
func putValue(table []byte, position int, field Field, width int) int {
	switch value := field.(type) {
	case Bool:
		table[position] = boolByte(bool(value))
		return position + 1
	case Struct:
		return position + copy(table[position:], StructBytes(value))
	case TableField:
		nested := value.Nested.Table
		return position + putFixed(table[position:], 0, len(nested.Fields), nested.Fields, width)
	default:
		return position + copy(table[position:], AppendScalar(nil, field))
	}
}

// putOffset writes an offset, a count or a length in width bytes
func putOffset(table []byte, position, width, value int) {
	switch width {
	case 1:
		table[position] = uint8(value)
	case 2:
		binary.LittleEndian.PutUint16(table[position:], uint16(value))
	default:
		binary.LittleEndian.PutUint32(table[position:], uint32(value))
	}
}

func lastOf(fields []Field) Field {
	if len(fields) == 0 {
		return nil
	}

	return fields[len(fields)-1]
}

// FixedFields returns the table with only its first fields of fixed size,
// which WriteLaidOut writes with AppendTable.
func (t Table) FixedFields() Table {
	first := leadingFixed(t.Fields, t.Layout.OffsetType.Size())
	return Table{Layout: t.Layout, Version: t.Version, Fields: t.Fields[:first:first]}
}
