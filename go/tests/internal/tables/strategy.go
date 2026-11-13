package tables

import (
	"math"

	tafla "github.com/snovader/tafla/go"
	"pgregory.net/rapid"
)

// Generators of table descriptions in the conformance suite's shape: tables
// of every layout, with fields of every type holding values across their
// range, and tables nested in them, from builders and from bytes, to a
// bounded depth.
//
// Tables follow the rules of the format and the builder's documentation: a
// version fits in its slot, a table nested with SetTable has no header, and
// a table and the tables in it fit in their offset types.

// depth is how deeply tables nest: the root's fields can hold tables whose
// fields hold tables
const depth = 2

// OffsetTypes are every offset type.
var OffsetTypes = []tafla.OffsetType{tafla.Uint8, tafla.Uint16, tafla.Uint32}

// Tables generates tables of any layout.
func Tables() *rapid.Generator[Table] {
	return TablesOf(Layouts())
}

// TablesOf generates tables of the layouts that layouts generates.
func TablesOf(layouts *rapid.Generator[Layout]) *rapid.Generator[Table] {
	return rapid.Custom(func(t *rapid.T) Table {
		return drawTable(t, layouts, depth, false)
	})
}

// Layouts generates layouts of any header and offset type.
func Layouts() *rapid.Generator[Layout] {
	return LayoutsOf(rapid.SampledFrom(OffsetTypes))
}

// LayoutsOf generates layouts of any header, of the offset types that
// offsetTypes generates.
func LayoutsOf(offsetTypes *rapid.Generator[tafla.OffsetType]) *rapid.Generator[Layout] {
	return rapid.Custom(func(t *rapid.T) Layout {
		return Layout{
			Sized:      rapid.Bool().Draw(t, "sized"),
			Versioned:  rapid.Bool().Draw(t, "versioned"),
			OffsetType: offsetTypes.Draw(t, "offsetType"),
		}
	})
}

// limits are the most fields, array elements, characters, structs in an
// array and vector items that a table holds. A table of uint8 offsets, or
// nested in one, holds few enough to fit in its 255 bytes more often than
// not, and others hold enough for uint16 lengths and offsets past 255.
type limits struct {
	fields, elements, characters, structs, items int
}

func limitsOf(small bool) limits {
	if small {
		return limits{fields: 5, elements: 6, characters: 6, structs: 2, items: 2}
	}

	return limits{fields: 8, elements: 24, characters: 16, structs: 6, items: 4}
}

// drawTable draws a table of layouts whose fields hold tables depth deep, and
// hold few values when small
func drawTable(t *rapid.T, layouts *rapid.Generator[Layout], depth int, small bool) Table {
	layout := layouts.Draw(t, "layout")
	small = small || layout.OffsetType == tafla.Uint8

	table := Table{Layout: layout}
	if layout.Versioned {
		maxVersion := layout.OffsetType.MaxValue()
		table.Version = weighted(t, "version", []choice[uint32]{
			{1, func(*rapid.T) uint32 { return 0 }},
			{1, func(*rapid.T) uint32 { return maxVersion }},
			{1, func(t *rapid.T) uint32 { return rapid.Uint32Range(0, maxVersion).Draw(t, "version") }},
		})
	}

	count := rapid.IntRange(0, limitsOf(small).fields).Draw(t, "fields")
	for range count {
		table.Fields = append(table.Fields, drawField(t, layout.OffsetType, depth, small))
	}

	return fitted(table)
}

// drawField draws a field of any type, or a table nested depth deep
func drawField(t *rapid.T, offsetType tafla.OffsetType, depth int, small bool) Field {
	if depth == 0 {
		return drawValue(t, limitsOf(small))
	}

	return weighted(t, "field", []choice[Field]{
		{8, func(t *rapid.T) Field { return drawValue(t, limitsOf(small)) }},
		{1, func(t *rapid.T) Field { return drawNestedTable(t, offsetType, depth-1, small) }},
		{1, func(t *rapid.T) Field { return drawVector(t, depth-1, small) }},
	})
}

// drawValue draws a scalar, array, string or struct, which hold values rather
// than tables
func drawValue(t *rapid.T, limits limits) Field {
	elements := limits.elements
	choices := make([]choice[Field], 0, 23)
	for _, scalar := range ScalarTypes {
		choices = append(choices, choice[Field]{1, func(t *rapid.T) Field { return drawScalar(t, scalar) }})
	}

	choices = append(choices,
		choice[Field]{1, func(t *rapid.T) Field { return Int8Array(drawSlice(t, elements, int8s)) }},
		choice[Field]{1, func(t *rapid.T) Field { return Uint8Array(drawSlice(t, elements, uint8s)) }},
		choice[Field]{1, func(t *rapid.T) Field { return Int16Array(drawSlice(t, elements, int16s)) }},
		choice[Field]{1, func(t *rapid.T) Field { return Uint16Array(drawSlice(t, elements, uint16s)) }},
		choice[Field]{1, func(t *rapid.T) Field { return Int32Array(drawSlice(t, elements, int32s)) }},
		choice[Field]{1, func(t *rapid.T) Field { return Uint32Array(drawSlice(t, elements, uint32s)) }},
		choice[Field]{1, func(t *rapid.T) Field { return Int64Array(drawSlice(t, elements, int64s)) }},
		choice[Field]{1, func(t *rapid.T) Field { return Uint64Array(drawSlice(t, elements, uint64s)) }},
		choice[Field]{1, func(t *rapid.T) Field { return Float32Array(drawSlice(t, elements, float32s)) }},
		choice[Field]{1, func(t *rapid.T) Field { return Float64Array(drawSlice(t, elements, float64s)) }},
		// Each as likely as three types of number
		choice[Field]{3, func(t *rapid.T) Field { return String(drawString(t, limits.characters)) }},
		choice[Field]{3, func(t *rapid.T) Field { return Struct(drawScalars(t, drawStructTypes(t))) }},
		choice[Field]{3, func(t *rapid.T) Field { return drawStructArray(t, limits.structs) }},
	)

	return weighted(t, "value", choices)
}

// DrawScalar draws a scalar field of a type.
func DrawScalar(t *rapid.T, scalar ScalarType) Field {
	return drawScalar(t, scalar)
}

func drawScalar(t *rapid.T, scalar ScalarType) Field {
	switch scalar {
	case Int8Type:
		return Int8(int8s(t))
	case Uint8Type:
		return Uint8(uint8s(t))
	case Int16Type:
		return Int16(int16s(t))
	case Uint16Type:
		return Uint16(uint16s(t))
	case Int32Type:
		return Int32(int32s(t))
	case Uint32Type:
		return Uint32(uint32s(t))
	case Int64Type:
		return Int64(int64s(t))
	case Uint64Type:
		return Uint64(uint64s(t))
	case Float32Type:
		return Float32(float32s(t))
	default:
		return Float64(float64s(t))
	}
}

// drawStructTypes draws the types of the scalars of a struct, of one to four
func drawStructTypes(t *rapid.T) []ScalarType {
	return rapid.SliceOfN(rapid.SampledFrom(ScalarTypes), 1, 4).Draw(t, "types")
}

// drawScalars draws the scalars of a struct of types
func drawScalars(t *rapid.T, types []ScalarType) []Field {
	scalars := make([]Field, len(types))
	for index, scalar := range types {
		scalars[index] = drawScalar(t, scalar)
	}

	return scalars
}

// drawStructArray draws an array of up to structs structs of the same
// scalars, empty too
func drawStructArray(t *rapid.T, structs int) Field {
	types := drawStructTypes(t)
	count := rapid.IntRange(0, structs).Draw(t, "structs")

	array := StructArray{Types: types, Items: make([][]Field, count)}
	for index := range array.Items {
		array.Items[index] = drawScalars(t, types)
	}

	return array
}

func drawSlice[T any](t *rapid.T, most int, element func(*rapid.T) T) []T {
	values := make([]T, rapid.IntRange(0, most).Draw(t, "elements"))
	for index := range values {
		values[index] = element(t)
	}

	return values
}

// drawInteger draws integers across their range, and their limits and 0 more
// often than chance
func drawInteger[T any](t *rapid.T, lowest, highest T, any *rapid.Generator[T]) T {
	return weighted(t, "integer", []choice[T]{
		{1, func(t *rapid.T) T { return rapid.SampledFrom([]T{lowest, *new(T), highest}).Draw(t, "limit") }},
		{3, func(t *rapid.T) T { return any.Draw(t, "integer") }},
	})
}

func int8s(t *rapid.T) int8   { return drawInteger(t, math.MinInt8, math.MaxInt8, rapid.Int8()) }
func uint8s(t *rapid.T) uint8 { return drawInteger(t, 0, math.MaxUint8, rapid.Uint8()) }
func int16s(t *rapid.T) int16 { return drawInteger(t, math.MinInt16, math.MaxInt16, rapid.Int16()) }
func uint16s(t *rapid.T) uint16 {
	return drawInteger(t, 0, math.MaxUint16, rapid.Uint16())
}
func int32s(t *rapid.T) int32 { return drawInteger(t, math.MinInt32, math.MaxInt32, rapid.Int32()) }
func uint32s(t *rapid.T) uint32 {
	return drawInteger(t, 0, math.MaxUint32, rapid.Uint32())
}
func int64s(t *rapid.T) int64 { return drawInteger(t, math.MinInt64, math.MaxInt64, rapid.Int64()) }
func uint64s(t *rapid.T) uint64 {
	return drawInteger(t, 0, math.MaxUint64, rapid.Uint64())
}

// float32s draws floats of every class and sign, NaNs with any payload among
// them, from any bits, and -0, NaN, the infinities and the limits more often
// than chance
func float32s(t *rapid.T) float32 {
	special := []uint32{
		0x8000_0000,        // -0
		QuietNaN32,         // NaN
		QuietNaN32 | 1<<31, // NaN of the other sign
		math.Float32bits(float32(math.Inf(1))),
		math.Float32bits(float32(math.Inf(-1))),
		math.Float32bits(-math.MaxFloat32),
		math.Float32bits(math.MaxFloat32),
		0x0080_0000, // the smallest normal
		1,           // the smallest subnormal
	}

	bits := weighted(t, "float32", []choice[uint32]{
		{1, func(t *rapid.T) uint32 { return rapid.SampledFrom(special).Draw(t, "special") }},
		{3, func(t *rapid.T) uint32 { return rapid.Uint32().Draw(t, "bits") }},
	})

	return math.Float32frombits(bits)
}

// float64s draws floats like float32s, as float64s
func float64s(t *rapid.T) float64 {
	special := []uint64{
		1 << 63,
		QuietNaN64,
		QuietNaN64 | 1<<63,
		math.Float64bits(math.Inf(1)),
		math.Float64bits(math.Inf(-1)),
		math.Float64bits(-math.MaxFloat64),
		math.Float64bits(math.MaxFloat64),
		0x0010_0000_0000_0000,
		1,
	}

	bits := weighted(t, "float64", []choice[uint64]{
		{1, func(t *rapid.T) uint64 { return rapid.SampledFrom(special).Draw(t, "special") }},
		{3, func(t *rapid.T) uint64 { return rapid.Uint64().Draw(t, "bits") }},
	})

	return math.Float64frombits(bits)
}

// DrawString draws a string of up to most characters, as string fields hold
// them.
func DrawString(t *rapid.T, most int) string {
	return drawString(t, most)
}

// drawString draws strings of ASCII, of characters of two, three and four
// bytes in UTF-8, the last outside the Basic Multilingual Plane, and of
// U+FEFF, which isn't a byte order mark within a table, leading or not
func drawString(t *rapid.T, most int) string {
	runes := drawSlice(t, most, func(t *rapid.T) rune {
		return weighted(t, "character", []choice[rune]{
			{3, func(t *rapid.T) rune { return rapid.Int32Range('a', 'z').Draw(t, "ascii") }},
			{1, func(*rapid.T) rune { return '\ufeff' }},
			{1, func(t *rapid.T) rune { return rapid.Int32Range(0x80, 0x7ff).Draw(t, "two") }},
			{1, func(t *rapid.T) rune {
				// Of three bytes, but the surrogates, which aren't characters
				character := rapid.Int32Range(0x800, 0xffff-0x800).Draw(t, "three")
				if character >= 0xd800 {
					character += 0x800
				}
				return character
			}},
			{1, func(t *rapid.T) rune { return rapid.Int32Range(0x1_0000, 0x10_ffff).Draw(t, "four") }},
		})
	})

	return string(runes)
}

// drawNestedTable draws a table nested with SetTable, which has no header,
// from its builder or bytes, aligned by default or explicitly. It's read in
// place when its offsets are as wide as its parent's, as they are more often
// than not.
func drawNestedTable(t *rapid.T, parent tafla.OffsetType, depth int, small bool) Field {
	offsetType := weighted(t, "offsetType", []choice[tafla.OffsetType]{
		{2, func(*rapid.T) tafla.OffsetType { return parent }},
		{1, func(t *rapid.T) tafla.OffsetType { return rapid.SampledFrom(OffsetTypes).Draw(t, "offsetType") }},
	})
	layouts := rapid.Just(Headerless(offsetType))

	alignments := []tafla.Alignment{0, tafla.Align1, tafla.Align2, tafla.Align4, tafla.Align8}
	return TableField{
		Nested:    drawNested(t, layouts, depth, small),
		Alignment: rapid.SampledFrom(alignments).Draw(t, "alignment"),
	}
}

// drawVector draws a vector of tables of any layout, from their builders or
// bytes
func drawVector(t *rapid.T, depth int, small bool) Field {
	count := rapid.IntRange(0, limitsOf(small).items).Draw(t, "items")
	vector := make(Vector, count)
	for index := range vector {
		vector[index] = drawNested(t, Layouts(), depth, small)
	}

	return vector
}

func drawNested(t *rapid.T, layouts *rapid.Generator[Layout], depth int, small bool) Nested {
	return Nested{
		From:  rapid.SampledFrom([]Source{FromBuilder, FromBytes}).Draw(t, "from"),
		Table: drawTable(t, layouts, depth, small),
	}
}

// choice is a way to draw a value, with its weight among the others
type choice[T any] struct {
	weight int
	draw   func(*rapid.T) T
}

// weighted draws a value in one of the ways, as often as their weights say
func weighted[T any](t *rapid.T, label string, choices []choice[T]) T {
	total := 0
	for _, choice := range choices {
		total += choice.weight
	}

	drawn := rapid.IntRange(0, total-1).Draw(t, label)
	for _, choice := range choices {
		if drawn < choice.weight {
			return choice.draw(t)
		}
		drawn -= choice.weight
	}

	panic("the weights add up")
}

// fitted keeps the table's first fields that surely fit in its offset type,
// as many as its header and their longest lengths leave room for. The tables
// nested in it were fitted to theirs.
func fitted(table Table) Table {
	width := uint64(table.Layout.OffsetType.Size())
	maxLength := uint64(table.Layout.OffsetType.MaxValue())

	length := uint64(table.Layout.HeaderSize())
	for index, field := range table.Fields {
		length += width + longest(field, width)
		if length > maxLength {
			table.Fields = table.Fields[:index]
			break
		}
	}

	return table
}

func longestTable(table Table) uint64 {
	width := uint64(table.Layout.OffsetType.Size())
	length := uint64(table.Layout.HeaderSize())
	for _, field := range table.Fields {
		length += width + longest(field, width)
	}

	return length
}

// longest returns the longest that a field can be in a table of width-byte
// offsets, with the most padding that it can need
func longest(field Field, width uint64) uint64 {
	array := func(count int, size uint64) uint64 {
		return size - 1 + width + uint64(count)*size
	}

	switch value := field.(type) {
	case Int8, Uint8, Bool:
		return 1
	case Int16, Uint16:
		return 2
	case Int32, Uint32, Float32:
		return 4
	case Int64, Uint64, Float64:
		return 8
	case Int8Array:
		return array(len(value), 1)
	case Uint8Array:
		return array(len(value), 1)
	case Int16Array:
		return array(len(value), 2)
	case Uint16Array:
		return array(len(value), 2)
	case Int32Array:
		return array(len(value), 4)
	case Uint32Array:
		return array(len(value), 4)
	case Float32Array:
		return array(len(value), 4)
	case Int64Array:
		return array(len(value), 8)
	case Uint64Array:
		return array(len(value), 8)
	case Float64Array:
		return array(len(value), 8)
	case String:
		return width + uint64(len(value))
	case Struct:
		return uint64(StructSize(StructTypes(value)))
	case StructArray:
		size := uint64(StructSize(value.Types))
		alignment := uint64(StructAlignment(value.Types))
		return alignment - 1 + width + uint64(len(value.Items))*size
	case TableField:
		return 7 + longestTable(value.Nested.Table)
	case Vector:
		length := width
		for _, item := range value {
			length += width + longestTable(item.Table)
		}
		return length
	default:
		panic("unknown field")
	}
}
