package tafla_test

// Tables that TableBuilder writes follow the format's rules, as SPEC.md at
// the root of the repository states them: the header holds the length and
// version, offsets lie within the table, values follow each other in field
// order with only the padding that aligns arrays and nested tables,
// multi-byte values are little-endian, and nested tables and vector items
// hold the bytes of their tables.

import (
	"bytes"
	"encoding/binary"
	"math"
	"testing"

	"github.com/snovader/tafla/go/internal/tables"
	"pgregory.net/rapid"
)

// checkFormat checks a property of every table in the bytes of a table
// that any description builds.
func checkFormat(t *testing.T, property func(t *rapid.T, found tableIn)) {
	rapid.Check(t, func(t *rapid.T) {
		table := tables.Tables().Draw(t, "table")
		built, err := table.Bytes()
		if err != nil {
			t.Fatal(err)
		}

		for _, found := range tablesIn(built, table) {
			property(t, found)
		}
	})
}

func TestSizedTablesHoldTheirLengthInTheHeader(t *testing.T) {
	checkFormat(t, func(t *rapid.T, found tableIn) {
		layout := found.table.Layout
		if layout.Sized {
			width := layout.OffsetType.Size()
			if !bytes.Equal(found.bytes[:width], slotBytes(len(found.bytes), width)) {
				t.Fatalf("the header holds % x in a table of %d bytes", found.bytes[:width], len(found.bytes))
			}
		}
	})
}

func TestVersionedTablesHoldTheirVersionInTheHeader(t *testing.T) {
	checkFormat(t, func(t *rapid.T, found tableIn) {
		layout := found.table.Layout
		if layout.Versioned {
			width := layout.OffsetType.Size()
			position := 0
			if layout.Sized {
				position = width
			}

			slot := found.bytes[position : position+width]
			if !bytes.Equal(slot, slotBytes(found.table.Version, width)) {
				t.Fatalf("the header holds % x for version %d", slot, found.table.Version)
			}
		}
	})
}

// Every offset of every table lies after its header and offsets, and its
// field ends within the table. A table nested with no fields is empty, and
// can end its parent.
func TestOffsetsLieWithinTheirTable(t *testing.T) {
	checkFormat(t, func(t *rapid.T, found tableIn) {
		offsetsEnd := found.offsetsEnd()

		for index, field := range found.fields {
			end := field.position + field.length
			if offsetsEnd > field.position || end > len(found.bytes) {
				t.Fatalf("field %d at %d of %d bytes, in a table of %d bytes with offsets to %d",
					index, field.position, field.length, len(found.bytes), offsetsEnd)
			}
		}
	})
}

// From the start of the table that holds them, which is aligned in turn
// when it's nested: an array of scalars at a multiple of their size, and an
// array of structs of their largest scalar's
func TestArraysStartAtAMultipleOfTheirAlignment(t *testing.T) {
	checkFormat(t, func(t *rapid.T, found tableIn) {
		width := found.table.Layout.OffsetType.Size()

		for _, field := range found.fields {
			if value, ok := valueOf(field.field); ok && value.array {
				start := field.position + width
				if start%value.alignment != 0 {
					t.Fatalf("%+v at %d", field.field, start)
				}
			}
		}
	})
}

// Scalars hold their values' little-endian bytes, structs their scalars',
// arrays and strings their counts' and their elements', and vectors their
// counts' and their items' ends', which count from the vector's position,
// after the ends
func TestValuesAreLittleEndian(t *testing.T) {
	checkFormat(t, func(t *rapid.T, found tableIn) {
		width := found.table.Layout.OffsetType.Size()

		for _, field := range found.fields {
			bytesOf := found.bytes[field.position : field.position+field.length]

			if value, ok := valueOf(field.field); ok {
				if !value.array && !bytes.Equal(bytesOf, value.bytes) {
					t.Fatalf("%+v holds % x", field.field, bytesOf)
				}
				if value.array && (!bytes.Equal(bytesOf[:width], slotBytes(value.count, width)) ||
					!bytes.Equal(bytesOf[width:], value.bytes)) {
					t.Fatalf("%+v holds % x", field.field, bytesOf)
				}
				continue
			}

			items, ok := field.field.(tables.Vector)
			if !ok {
				continue
			}
			if !bytes.Equal(bytesOf[:width], slotBytes(len(items), width)) {
				t.Fatalf("a vector of %d items holds the count % x", len(items), bytesOf[:width])
			}

			end := (len(items) + 1) * width
			for index, item := range items {
				end += len(must(item.Table.Bytes()))

				slot := bytesOf[(index+1)*width : (index+2)*width]
				if !bytes.Equal(slot, slotBytes(end, width)) {
					t.Fatalf("item %d ends at % x rather than %d", index, slot, end)
				}
			}
		}
	})
}

// Values follow the header and offsets in field order, each after the
// previous one, with only the zeroed padding that aligns an array's elements
// and a nested table to their alignment between them, and the last one ends
// the table
func TestValuesFollowEachOtherWithOnlyZeroedPaddingToAlignThem(t *testing.T) {
	checkFormat(t, func(t *rapid.T, found tableIn) {
		width := found.table.Layout.OffsetType.Size()
		end := found.offsetsEnd()

		for _, field := range found.fields {
			position := end + padding(field.field, end, width)
			if field.position != position {
				t.Fatalf("%+v at %d rather than %d", field.field, field.position, position)
			}
			for _, padding := range found.bytes[end:position] {
				if padding != 0 {
					t.Fatalf("the padding before %+v holds % x", field.field, found.bytes[end:position])
				}
			}

			end = position + field.length
		}

		if end != len(found.bytes) {
			t.Fatalf("the fields end at %d of %d bytes", end, len(found.bytes))
		}
	})
}

// The bytes that a builder writes are the same nested in another table as on
// their own
func TestNestedTablesAndVectorItemsHoldTheBytesOfTheirTables(t *testing.T) {
	checkFormat(t, func(t *rapid.T, found tableIn) {
		if found.ownBytes != nil && !bytes.Equal(found.bytes, found.ownBytes) {
			t.Fatalf("a nested table holds % x rather than % x", found.bytes, found.ownBytes)
		}
	})
}

// tableIn is a table in the bytes of a table: the table itself, a table
// nested in it or one of its vector items, or a table in one of those
type tableIn struct {
	bytes  []byte
	table  tables.Table
	fields []fieldIn
	// The bytes that the builder of a table in another writes on its own
	ownBytes []byte
}

// fieldIn is a field and where it lies in its table's bytes
type fieldIn struct {
	field    tables.Field
	position int
	length   int
}

// offsetsEnd returns the length of the header and offsets, where the values
// start
func (t tableIn) offsetsEnd() int {
	return t.table.Layout.HeaderSize() + len(t.fields)*t.table.Layout.OffsetType.Size()
}

// tablesIn finds every table in bytes, which hold table, and the fields of
// each. Fields lie at the positions that their offsets hold, and are as long
// as the values that the description gives, and as the tables that their
// builders write on their own.
func tablesIn(bytes []byte, table tables.Table) []tableIn {
	var found []tableIn
	findTables(bytes, table, nil, &found)

	return found
}

func findTables(bytes []byte, table tables.Table, ownBytes []byte, found *[]tableIn) {
	width := table.Layout.OffsetType.Size()
	offsets := table.Layout.HeaderSize()
	fields := make([]fieldIn, 0, len(table.Fields))

	for index, field := range table.Fields {
		position := slot(bytes, offsets+index*width, width)

		var length int
		switch field := field.(type) {
		case tables.TableField:
			length = findNested(bytes, position, field.Nested, found)
		case tables.Vector:
			// The items follow the count and the ends
			end := position + (len(field)+1)*width
			for _, item := range field {
				end += findNested(bytes, end, item, found)
			}
			length = end - position
		default:
			value, _ := valueOf(field)
			length = value.byteLength(width)
		}

		fields = append(fields, fieldIn{field: field, position: position, length: length})
	}

	*found = append(*found, tableIn{bytes: bytes, table: table, fields: fields, ownBytes: ownBytes})
}

// findNested finds a nested table or vector item at position, and the tables
// in it, and returns its length
func findNested(bytes []byte, position int, nested tables.Nested, found *[]tableIn) int {
	ownBytes := must(nested.Table.Bytes())
	length := len(ownBytes)

	if position+length > len(bytes) {
		panic("a nested table leaves its parent")
	}
	findTables(bytes[position:position+length], nested.Table, ownBytes, found)

	return length
}

// value is the value of a scalar, array, string or struct field: the
// little-endian bytes of a scalar, or of a struct's scalars, or the count of
// an array's elements, of a string's bytes or of an array's structs, and
// their little-endian bytes, which start at a multiple of alignment from the
// table's start
type value struct {
	array     bool
	alignment int
	count     int
	bytes     []byte
}

// valueOf returns the value of a field that holds one rather than tables
func valueOf(field tables.Field) (value, bool) {
	scalar := func(bytes []byte) (value, bool) { return value{bytes: bytes}, true }

	switch field := field.(type) {
	case tables.Int8, tables.Uint8, tables.Int16, tables.Uint16, tables.Int32, tables.Uint32,
		tables.Int64, tables.Uint64, tables.Float32, tables.Float64:
		return scalar(tables.AppendScalar(nil, field))
	case tables.Bool:
		if field {
			return scalar([]byte{1})
		}
		return scalar([]byte{0})
	case tables.Int8Array:
		return arrayValue(field, 1, func(b []byte, v int8) []byte { return append(b, byte(v)) })
	case tables.Uint8Array:
		return arrayValue(field, 1, func(b []byte, v uint8) []byte { return append(b, v) })
	case tables.Int16Array:
		return arrayValue(field, 2, func(b []byte, v int16) []byte {
			return binary.LittleEndian.AppendUint16(b, uint16(v))
		})
	case tables.Uint16Array:
		return arrayValue(field, 2, binary.LittleEndian.AppendUint16)
	case tables.Int32Array:
		return arrayValue(field, 4, func(b []byte, v int32) []byte {
			return binary.LittleEndian.AppendUint32(b, uint32(v))
		})
	case tables.Uint32Array:
		return arrayValue(field, 4, binary.LittleEndian.AppendUint32)
	case tables.Int64Array:
		return arrayValue(field, 8, func(b []byte, v int64) []byte {
			return binary.LittleEndian.AppendUint64(b, uint64(v))
		})
	case tables.Uint64Array:
		return arrayValue(field, 8, binary.LittleEndian.AppendUint64)
	case tables.Float32Array:
		return arrayValue(field, 4, func(b []byte, v float32) []byte {
			return binary.LittleEndian.AppendUint32(b, math.Float32bits(v))
		})
	case tables.Float64Array:
		return arrayValue(field, 8, func(b []byte, v float64) []byte {
			return binary.LittleEndian.AppendUint64(b, math.Float64bits(v))
		})
	case tables.String:
		return value{array: true, alignment: 1, count: len(field), bytes: []byte(field)}, true
	case tables.Struct:
		return scalar(tables.StructBytes(field))
	case tables.StructArray:
		var bytes []byte
		for _, item := range field.Items {
			bytes = append(bytes, tables.StructBytes(item)...)
		}
		alignment := int(tables.StructAlignment(field.Types))
		return value{array: true, alignment: alignment, count: len(field.Items), bytes: bytes}, true
	default:
		return value{}, false
	}
}

func arrayValue[T any](values []T, size int, appendLE func([]byte, T) []byte) (value, bool) {
	var bytes []byte
	for _, element := range values {
		bytes = appendLE(bytes, element)
	}

	return value{array: true, alignment: size, count: len(values), bytes: bytes}, true
}

// byteLength returns the value's length in a table of width-byte offsets and
// counts
func (v value) byteLength(width int) int {
	if v.array {
		return width + len(v.bytes)
	}

	return len(v.bytes)
}

// padding returns the padding before field when it would start at end, which
// aligns an array's elements, after its count, and a nested table's start
func padding(field tables.Field, end, width int) int {
	start := end
	if value, ok := valueOf(field); ok && value.array {
		start += width
	}

	alignment := alignmentOf(field)
	return (alignment - start%alignment) % alignment
}

// alignmentOf returns the multiple of bytes from the start of its table that
// a field starts at, after its count when it's an array: an array's
// alignment, a nested table's alignment, and 1 for any other
func alignmentOf(field tables.Field) int {
	if value, ok := valueOf(field); ok && value.array {
		return value.alignment
	}

	nested, ok := field.(tables.TableField)
	switch {
	case !ok:
		return 1
	case nested.Alignment != 0:
		return int(nested.Alignment)
	case nested.Nested.From == tables.FromBytes:
		return 4
	default:
		// A builder's is the largest alignment of its fields
		alignment := 1
		for _, field := range nested.Nested.Table.Fields {
			alignment = max(alignment, alignmentOf(field))
		}
		return alignment
	}
}

// slot reads a little-endian slot of width bytes at position
func slot(bytes []byte, position, width int) int {
	value := 0
	for index := width - 1; index >= 0; index-- {
		value = value<<8 | int(bytes[position+index])
	}

	return value
}

// slotBytes returns the little-endian bytes of value in a slot of width
// bytes
func slotBytes[T int | uint32](value T, width int) []byte {
	bytes := binary.LittleEndian.AppendUint64(nil, uint64(value))
	for _, high := range bytes[width:] {
		if high != 0 {
			panic("the value doesn't fit in the slot")
		}
	}

	return bytes[:width]
}
