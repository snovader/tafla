// Package tables is the tests' one model of tables: the conformance suite's
// descriptions, which cases.go parses from cases.json and strategy.go
// generates, with the builder and reader that every test builds and reads
// them with, and the sizer and writer that write them in place. See
// conformance/README.md at the root of the repository.
package tables

import (
	"math"
	"unsafe"

	tafla "github.com/snovader/tafla/go"
)

// Table is a table: its layout, its version and its fields, in the order
// that they're set.
type Table struct {
	Layout Layout
	// The version in the header, present only when the layout is versioned
	Version uint32
	Fields  []Field
}

// Layout is a table's layout, as a description gives it.
type Layout struct {
	Sized      bool
	Versioned  bool
	OffsetType tafla.OffsetType
}

// Field is a field of each type that a table holds, with its value: one of
// the types below. Equal fields hold floats of the same bits, so that NaN
// equals itself and -0 doesn't equal 0.
type Field interface {
	isField()
}

type (
	Int8    int8
	Uint8   uint8
	Int16   int16
	Uint16  uint16
	Int32   int32
	Uint32  uint32
	Int64   int64
	Uint64  uint64
	Float32 float32
	Float64 float64
	// Bool is a bool, which the runtimes have no API for: it's written as
	// the uint8 1 or 0, and read as true when its byte isn't 0. It's a field
	// only, rather than a scalar of structs.
	Bool         bool
	Int8Array    []int8
	Uint8Array   []uint8
	Int16Array   []int16
	Uint16Array  []uint16
	Int32Array   []int32
	Uint32Array  []uint32
	Int64Array   []int64
	Uint64Array  []uint64
	Float32Array []float32
	Float64Array []float64
	String       string
	// Struct is a struct of scalar fields, one after another.
	Struct []Field
	// StructArray is an array of structs of scalars of Types, as the scalar
	// fields of each struct.
	StructArray struct {
		Types []ScalarType
		Items [][]Field
	}
	// TableField is a table nested with SetTable, or with SetTableAligned
	// when the description gives an alignment, which is 0 otherwise.
	TableField struct {
		Nested    Nested
		Alignment tafla.Alignment
	}
	Vector []Nested
)

func (Int8) isField()         {}
func (Uint8) isField()        {}
func (Int16) isField()        {}
func (Uint16) isField()       {}
func (Int32) isField()        {}
func (Uint32) isField()       {}
func (Int64) isField()        {}
func (Uint64) isField()       {}
func (Float32) isField()      {}
func (Float64) isField()      {}
func (Bool) isField()         {}
func (Int8Array) isField()    {}
func (Uint8Array) isField()   {}
func (Int16Array) isField()   {}
func (Uint16Array) isField()  {}
func (Int32Array) isField()   {}
func (Uint32Array) isField()  {}
func (Int64Array) isField()   {}
func (Uint64Array) isField()  {}
func (Float32Array) isField() {}
func (Float64Array) isField() {}
func (String) isField()       {}
func (Struct) isField()       {}
func (StructArray) isField()  {}
func (TableField) isField()   {}
func (Vector) isField()       {}

// Nested is a table nested from its builder or from the bytes that its
// builder writes.
type Nested struct {
	From  Source
	Table Table
}

// Source says what a nested table is set from.
type Source uint8

const (
	FromBuilder Source = iota
	FromBytes
)

// ScalarType is the type of a scalar, of which structs are made.
type ScalarType uint8

const (
	Int8Type ScalarType = iota
	Uint8Type
	Int16Type
	Uint16Type
	Int32Type
	Uint32Type
	Int64Type
	Uint64Type
	Float32Type
	Float64Type
)

// ScalarTypes are every scalar type.
var ScalarTypes = []ScalarType{
	Int8Type, Uint8Type, Int16Type, Uint16Type, Int32Type, Uint32Type,
	Int64Type, Uint64Type, Float32Type, Float64Type,
}

// Size returns the scalar's size in bytes, which is also its alignment in an
// array.
func (t ScalarType) Size() int {
	switch t {
	case Int8Type, Uint8Type:
		return 1
	case Int16Type, Uint16Type:
		return 2
	case Int32Type, Uint32Type, Float32Type:
		return 4
	default:
		return 8
	}
}

// Alignment returns the scalar's alignment in an array.
func (t ScalarType) Alignment() tafla.Alignment {
	return tafla.Alignment(t.Size())
}

// StructSize returns the size of a struct of scalars of types: the sum of
// theirs.
func StructSize(types []ScalarType) int {
	size := 0
	for _, scalar := range types {
		size += scalar.Size()
	}

	return size
}

// StructAlignment returns the alignment of an array of structs of scalars of
// types: the largest of their sizes.
func StructAlignment(types []ScalarType) tafla.Alignment {
	alignment := tafla.Align1
	for _, scalar := range types {
		alignment = max(alignment, scalar.Alignment())
	}

	return alignment
}

// StructTypes returns the types of a struct's scalar fields.
func StructTypes(scalars []Field) []ScalarType {
	types := make([]ScalarType, len(scalars))
	for index, scalar := range scalars {
		types[index] = ScalarTypeOf(scalar)
	}

	return types
}

// StructBytes returns the bytes of a struct of scalar fields: theirs, one
// after another.
func StructBytes(scalars []Field) []byte {
	var bytes []byte
	for _, scalar := range scalars {
		bytes = AppendScalar(bytes, scalar)
	}

	return bytes
}

// ScalarTypeOf returns the type of a scalar field. It panics for a field of
// any other type.
func ScalarTypeOf(field Field) ScalarType {
	switch field.(type) {
	case Int8:
		return Int8Type
	case Uint8:
		return Uint8Type
	case Int16:
		return Int16Type
	case Uint16:
		return Uint16Type
	case Int32:
		return Int32Type
	case Uint32:
		return Uint32Type
	case Int64:
		return Int64Type
	case Uint64:
		return Uint64Type
	case Float32:
		return Float32Type
	case Float64:
		return Float64Type
	default:
		panic("a struct holds scalars")
	}
}

// AppendScalar appends the little-endian bytes of a scalar field. It panics
// for a field of any other type.
func AppendScalar(bytes []byte, field Field) []byte {
	switch value := field.(type) {
	case Int8:
		return append(bytes, byte(value))
	case Uint8:
		return append(bytes, byte(value))
	case Int16:
		return appendLE(bytes, uint64(uint16(value)), 2)
	case Uint16:
		return appendLE(bytes, uint64(value), 2)
	case Int32:
		return appendLE(bytes, uint64(uint32(value)), 4)
	case Uint32:
		return appendLE(bytes, uint64(value), 4)
	case Int64:
		return appendLE(bytes, uint64(value), 8)
	case Uint64:
		return appendLE(bytes, uint64(value), 8)
	case Float32:
		return appendLE(bytes, uint64(math.Float32bits(float32(value))), 4)
	case Float64:
		return appendLE(bytes, math.Float64bits(float64(value)), 8)
	default:
		panic("a struct holds scalars")
	}
}

func appendLE(bytes []byte, value uint64, size int) []byte {
	for index := range size {
		bytes = append(bytes, byte(value>>(8*index)))
	}

	return bytes
}

// TableLayout returns the runtime's layout of a description's.
func (l Layout) TableLayout() tafla.TableLayout {
	layout := tafla.TableLayout{}.WithOffsetType(l.OffsetType)
	if l.Sized {
		layout = layout.Sized()
	}
	if l.Versioned {
		layout = layout.Versioned()
	}

	return layout
}

// Headerless returns a layout without header slots, which tables nested
// with SetTable have.
func Headerless(offsetType tafla.OffsetType) Layout {
	return Layout{OffsetType: offsetType}
}

// HeaderSize returns the byte length of the header slots before the field
// offsets.
func (l Layout) HeaderSize() int {
	return l.TableLayout().HeaderSize()
}

// Equal reports whether two tables are the same, with floats compared by
// their bits.
func Equal(a, b Table) bool {
	if a.Layout != b.Layout || a.Version != b.Version || len(a.Fields) != len(b.Fields) {
		return false
	}

	for index := range a.Fields {
		if !EqualFields(a.Fields[index], b.Fields[index]) {
			return false
		}
	}

	return true
}

// EqualFields reports whether two fields are the same, with floats compared
// by their bits.
func EqualFields(a, b Field) bool {
	switch a := a.(type) {
	case Float32:
		b, ok := b.(Float32)
		return ok && math.Float32bits(float32(a)) == math.Float32bits(float32(b))
	case Float64:
		b, ok := b.(Float64)
		return ok && math.Float64bits(float64(a)) == math.Float64bits(float64(b))
	case Float32Array:
		b, ok := b.(Float32Array)
		return ok && equalSlices(a, b, func(x, y float32) bool {
			return math.Float32bits(x) == math.Float32bits(y)
		})
	case Float64Array:
		b, ok := b.(Float64Array)
		return ok && equalSlices(a, b, func(x, y float64) bool {
			return math.Float64bits(x) == math.Float64bits(y)
		})
	case Int8Array:
		b, ok := b.(Int8Array)
		return ok && equalSlices(a, b, equal)
	case Uint8Array:
		b, ok := b.(Uint8Array)
		return ok && equalSlices(a, b, equal)
	case Int16Array:
		b, ok := b.(Int16Array)
		return ok && equalSlices(a, b, equal)
	case Uint16Array:
		b, ok := b.(Uint16Array)
		return ok && equalSlices(a, b, equal)
	case Int32Array:
		b, ok := b.(Int32Array)
		return ok && equalSlices(a, b, equal)
	case Uint32Array:
		b, ok := b.(Uint32Array)
		return ok && equalSlices(a, b, equal)
	case Int64Array:
		b, ok := b.(Int64Array)
		return ok && equalSlices(a, b, equal)
	case Uint64Array:
		b, ok := b.(Uint64Array)
		return ok && equalSlices(a, b, equal)
	case Struct:
		b, ok := b.(Struct)
		return ok && equalSlices(a, b, EqualFields)
	case StructArray:
		b, ok := b.(StructArray)
		return ok && equalSlices(a.Types, b.Types, equal) &&
			equalSlices(a.Items, b.Items, func(x, y []Field) bool {
				return equalSlices(x, y, EqualFields)
			})
	case TableField:
		b, ok := b.(TableField)
		return ok && a.Alignment == b.Alignment && equalNested(a.Nested, b.Nested)
	case Vector:
		b, ok := b.(Vector)
		return ok && equalSlices(a, b, equalNested)
	default:
		return a == b
	}
}

func equalNested(a, b Nested) bool {
	return a.From == b.From && Equal(a.Table, b.Table)
}

func equal[T comparable](a, b T) bool {
	return a == b
}

func equalSlices[T any](a, b []T, equal func(T, T) bool) bool {
	if len(a) != len(b) {
		return false
	}

	for index := range a {
		if !equal(a[index], b[index]) {
			return false
		}
	}

	return true
}

// AtByteOffset returns a copy of bytes that starts byteOffset bytes past a
// multiple of 8 in a larger buffer of 0xff. Its arrays are aligned in memory
// when byteOffset is a multiple of 8.
func AtByteOffset(bytes []byte, byteOffset int) []byte {
	// A slice of uint64s, which Go aligns to 8
	backing := make([]uint64, (8+byteOffset+len(bytes)+8)/8+1)
	all := unsafe.Slice((*byte)(unsafe.Pointer(unsafe.SliceData(backing))), len(backing)*8)
	for index := range all {
		all[index] = 0xff
	}

	start := byteOffset
	end := start + len(bytes)
	copy(all[start:end], bytes)

	return all[start:end:end]
}
