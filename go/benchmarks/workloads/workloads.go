// Package workloads holds generic table shapes, with the TypeScript
// comparison's values.
//
// The shapes are sized to bracket how tafla is used: small tables of scalars
// and a string, documents whose header nests a table of numeric columns,
// vectors of small nested tables, arrays of structs, and fixed-size records.
// The values come from a port of the TypeScript comparison's seeded
// generator, so that the languages' numbers compare.
//
// Deserialized values share strings and columns with their bytes where a
// codec reads them in place, and own copies otherwise. Same compares floats
// by their bits, so that a codec reads back exactly what it wrote.
package workloads

import (
	"math"
	"slices"
)

// DocumentRows are the rows of the documents: a small one and a large one.
var DocumentRows = []int{100, 100_000}

// ListItems are the items of the vector of nested tables and of the array of
// structs.
const ListItems = 100

// Record is a small table: two float64s, an int64 and a 16-byte string.
//
// Its tags name its fields for MessagePack, as serde names them in Rust.
type Record struct {
	X    float64 `msgpack:"x"`
	Y    float64 `msgpack:"y"`
	ID   int64   `msgpack:"id"`
	Name string  `msgpack:"name"`
}

// Document is a sized, versioned table of two float64s and a string, with a
// nested table of an int64 and three columns of the same length.
type Document struct {
	X    float64   `msgpack:"x"`
	Y    float64   `msgpack:"y"`
	Name string    `msgpack:"name"`
	Base int64     `msgpack:"base"`
	Keys []uint32  `msgpack:"keys"`
	A    []float32 `msgpack:"a"`
	B    []float32 `msgpack:"b"`
}

// Item is a table of a float32 and a nested table of two float32s, First and
// Second, or a struct of the three float32s.
type Item struct {
	Value  float32 `msgpack:"value"`
	First  float32 `msgpack:"first"`
	Second float32 `msgpack:"second"`
}

// List is a table holding a vector of item tables, or an array of item
// structs.
type List struct {
	Items []Item `msgpack:"items"`
}

// Sample is a fixed-size record of a uint32 and two float32s.
type Sample struct {
	Key uint32  `msgpack:"key"`
	A   float32 `msgpack:"a"`
	B   float32 `msgpack:"b"`
}

// TheRecord is the record that every codec writes.
var TheRecord = Record{X: 59.3293, Y: 18.0686, ID: -631_152_000, Name: "tafla-benchmarks"}

// TheSample is the fixed-size record that every codec writes.
var TheSample = Sample{Key: 2_743_826_400, A: 21.75, B: 3.25}

// NewDocument returns the document of rows rows.
func NewDocument(rows int) Document {
	random := newXorshift(rows)

	// Keys below 2^24 take 4 bytes in protobuf's varints, like the floats
	keys := make([]uint32, rows)
	a := make([]float32, rows)
	b := make([]float32, rows)
	for row := range rows {
		keys[row] = uint32(math.Floor(random.next() * (1 << 24)))
		a[row] = float32((random.next() - 0.5) * 100)
		b[row] = float32((random.next() - 0.5) * 100)
	}

	return Document{
		X:    TheRecord.X,
		Y:    TheRecord.Y,
		Name: TheRecord.Name,
		Base: TheRecord.ID,
		Keys: keys,
		A:    a,
		B:    b,
	}
}

// NewList returns the list of items items.
func NewList(items int) List {
	random := newXorshift(items)

	// Float32 values, so that every format reads them back exactly. Fields
	// are evaluated in the order they're written, as in JavaScript.
	list := List{Items: make([]Item, items)}
	for index := range list.Items {
		value := float32(random.next() * 30)
		first := float32(random.next()*4 - 2)
		second := float32(random.next() * 50)
		list.Items[index] = Item{Value: value, First: first, Second: second}
	}

	return list
}

// SumColumn returns the sum of values in their order, in a float64, as the
// TypeScript comparison adds them to a number. Every codec sums its column
// with it, so that their sums have the same bits.
func SumColumn(values []float32) float64 {
	sum := 0.0
	for _, value := range values {
		sum += float64(value)
	}

	return sum
}

// xorshift is the TypeScript generator's xorshift, in [0, 1), so that every
// run sees the same values. Its state is a uint32, whose bits JavaScript's
// 32-bit shifts keep the same.
type xorshift struct {
	state uint32
}

// newXorshift seeds a generator, wrapping the seed to 32 bits, as
// JavaScript's ^ wraps its operands
func newXorshift(seed int) *xorshift {
	return &xorshift{state: 0x2545_f491 ^ uint32(seed)}
}

func (x *xorshift) next() float64 {
	x.state ^= x.state << 13
	x.state ^= x.state >> 17
	x.state ^= x.state << 5

	return float64(x.state) / 4_294_967_296
}

// Same reports whether two records are the same, with floats by their bits.
func (r Record) Same(other Record) bool {
	return sameFloat64(r.X, other.X) && sameFloat64(r.Y, other.Y) && r.ID == other.ID && r.Name == other.Name
}

// Same reports whether two documents are the same, with floats by their
// bits.
func (d Document) Same(other Document) bool {
	return sameFloat64(d.X, other.X) && sameFloat64(d.Y, other.Y) && d.Name == other.Name &&
		d.Base == other.Base && slices.Equal(d.Keys, other.Keys) &&
		sameFloat32s(d.A, other.A) && sameFloat32s(d.B, other.B)
}

// Same reports whether two lists are the same, with floats by their bits.
func (l List) Same(other List) bool {
	return slices.EqualFunc(l.Items, other.Items, func(a, b Item) bool {
		return sameFloat32(a.Value, b.Value) && sameFloat32(a.First, b.First) && sameFloat32(a.Second, b.Second)
	})
}

// Same reports whether two samples are the same, with floats by their bits.
func (s Sample) Same(other Sample) bool {
	return s.Key == other.Key && sameFloat32(s.A, other.A) && sameFloat32(s.B, other.B)
}

func sameFloat32(a, b float32) bool {
	return math.Float32bits(a) == math.Float32bits(b)
}

func sameFloat64(a, b float64) bool {
	return math.Float64bits(a) == math.Float64bits(b)
}

func sameFloat32s(a, b []float32) bool {
	return slices.EqualFunc(a, b, sameFloat32)
}
