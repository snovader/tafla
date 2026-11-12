package tafla_test

// Tables read back the values that they were built from. The examples show
// builders keeping their values, ArrayView reading arrays in place or
// copying them, structs read from their bytes, and vector items found from
// their ends.

import (
	"bytes"
	"encoding/binary"
	"fmt"
	"math"
	"slices"
	"testing"
	"unsafe"

	tafla "github.com/snovader/tafla/go"
	"github.com/snovader/tafla/go/tests/internal/tables"
	"pgregory.net/rapid"
)

// Floats compare by their bits, so that -0 and NaN count. The table is read
// at any byte offset from an aligned address, so that its arrays are read in
// place when they're aligned in memory and copied when they aren't.
func TestTablesReadBackTheValuesTheyWereBuiltFrom(t *testing.T) {
	rapid.Check(t, func(t *rapid.T) {
		table := tables.Tables().Draw(t, "table")
		byteOffset := rapid.IntRange(0, 7).Draw(t, "byteOffset")

		built, err := table.Bytes()
		if err != nil {
			t.Fatal(err)
		}

		read, err := tables.ReadTable(tables.AtByteOffset(built, byteOffset), table)
		if err != nil || !tables.Equal(read, table) {
			t.Fatalf("read %+v, %v\nrather than %+v", read, err, table)
		}
	})
}

func TestRoundTripsValuesThatChangeAfterTheTableIsWritten(t *testing.T) {
	floats := []float64{1.5, -2.5}
	name := []byte("owned")

	builder := must(tafla.NewTableBuilder(3, tafla.TableLayout{}))
	check(t, builder.SetFloat64Array(floats))
	check(t, builder.SetString(string(name)))
	check(t, builder.SetVector(tafla.TableBytes{1, 2}, tafla.TableBytes{3}))

	table := must(builder.Bytes())
	floats[0] = 0
	name[0] = 'X'

	buffer := tafla.NewByteBuffer(table, tafla.TableLayout{})
	read := must(buffer.GetFloat64Array(must(buffer.RootOffset(0))))
	if got := read.Slice(); !slices.Equal(got, []float64{1.5, -2.5}) {
		t.Errorf("read %v", got)
	}
	if got := must(buffer.GetString(must(buffer.RootOffset(1)))); got != "owned" {
		t.Errorf("read %q", got)
	}

	vector := must(buffer.GetVector(must(buffer.RootOffset(2))))
	var items [][]byte
	for item, err := range vector.Items() {
		check(t, err)
		items = append(items, item)
	}
	if !slices.EqualFunc(items, [][]byte{{1, 2}, {3}}, bytes.Equal) {
		t.Errorf("read %v", items)
	}
}

func TestFindsVectorItemsFromTheirEnds(t *testing.T) {
	builder := must(tafla.NewTableBuilder(1, tafla.TableLayout{}))
	check(t, builder.SetVector(tafla.TableBytes{1}, tafla.TableBytes{}, tafla.TableBytes{2, 3}))

	table := must(builder.Bytes())
	buffer := tafla.NewByteBuffer(table, tafla.TableLayout{})
	position := must(buffer.RootOffset(0))

	// The count and the three ends, at 4, then the items, from 20
	if !bytes.Equal(table[position:20], []byte{3, 0, 0, 0, 17, 0, 0, 0, 17, 0, 0, 0, 19, 0, 0, 0}) {
		t.Errorf("wrote % x", table[position:20])
	}
	if !bytes.Equal(table[20:], []byte{1, 2, 3}) {
		t.Errorf("wrote % x", table[20:])
	}

	vector := must(buffer.GetVector(position))
	if vector.Len() != 3 {
		t.Errorf("read %d items", vector.Len())
	}
	if item := must(vector.Get(2)); !bytes.Equal(item, []byte{2, 3}) {
		t.Errorf("read % x", item)
	}
	if item := must(vector.Get(1)); len(item) != 0 {
		t.Errorf("read % x", item)
	}
	if item := must(vector.GetInPlace(1)); !bytes.Equal(item, []byte{2, 3}) {
		t.Errorf("read % x in place, to the end of the bytes", item)
	}
	expectPanic(t, "an index past the items", func() { _, _ = vector.Get(3) })
	expectPanic(t, "a negative index", func() { _, _ = vector.GetInPlace(-1) })
}

func TestRoundTripsStructs(t *testing.T) {
	// A struct of a uint16 and a float32, of 6 bytes, whose arrays are
	// aligned to 4
	point := func(x uint16, y float32) []byte {
		bytes := binary.LittleEndian.AppendUint16(nil, x)
		return binary.LittleEndian.AppendUint32(bytes, math.Float32bits(y))
	}
	points := slices.Concat(point(1, 1.5), point(2, -0.25))

	builder := must(tafla.NewTableBuilder(3, tafla.TableLayout{}))
	check(t, builder.SetUint8(7))
	check(t, builder.SetStruct(point(3, 2.5)))
	check(t, builder.SetStructArray(points, 6, tafla.Align4))

	table := must(builder.Bytes())
	buffer := tafla.NewByteBuffer(table, tafla.TableLayout{})

	// The struct follows the uint8 at 12 unpadded, at 13
	position := must(buffer.RootOffset(1))
	if position != 13 || !bytes.Equal(must(buffer.GetStruct(position, 6)), point(3, 2.5)) {
		t.Errorf("read the struct at %d", position)
	}

	// The array's count is padded from 19 to 20, so that its structs start
	// at 24
	position = must(buffer.RootOffset(2))
	if position != 20 || must(buffer.GetUint32(position)) != 2 {
		t.Errorf("read the array at %d", position)
	}

	structs := must(buffer.GetStructArray(position, 6))
	if !bytes.Equal(structs, points) {
		t.Errorf("read % x", structs)
	}
	var ys []float32
	for start := 0; start < len(structs); start += 6 {
		ys = append(ys, must(tafla.NewByteBuffer(structs[start:start+6], tafla.TableLayout{}).GetFloat32(2)))
	}
	if !slices.Equal(ys, []float32{1.5, -0.25}) {
		t.Errorf("read %v", ys)
	}

	// Structs of 0 bytes take none, whatever the count
	if empty := must(buffer.GetStructArray(position, 0)); len(empty) != 0 {
		t.Errorf("read % x", empty)
	}
}

func TestPanicsForAnIndexOutOfRangeWithTheIndexAndTheLength(t *testing.T) {
	builder := must(tafla.NewTableBuilder(2, tafla.TableLayout{}))
	check(t, builder.SetUint16Array([]uint16{1, 2, 3}))
	check(t, builder.SetVector(tafla.TableBytes{1}, tafla.TableBytes{2}))
	buffer := tafla.NewByteBuffer(must(builder.Bytes()), tafla.TableLayout{})
	array := must(buffer.GetUint16Array(must(buffer.RootOffset(0))))
	vector := must(buffer.GetVector(must(buffer.RootOffset(1))))

	expectPanicMessage(t, "tafla: index 3 out of range of an array of 3", func() { array.At(3) })
	expectPanicMessage(t, "tafla: index -1 out of range of an array of 3", func() { array.At(-1) })
	expectPanicMessage(t, "tafla: index 2 out of range of a vector of 2", func() { _, _ = vector.Get(2) })
	expectPanicMessage(t, "tafla: index -1 out of range of a vector of 2", func() { _, _ = vector.GetInPlace(-1) })
}

func TestReturnsAnEmptyArrayInPlaceAtAnyAddress(t *testing.T) {
	builder := must(tafla.NewTableBuilder(1, tafla.TableLayout{}))
	check(t, builder.SetFloat64Array(nil))
	table := must(builder.Bytes())

	for byteOffset := range 8 {
		buffer := tafla.NewByteBuffer(tables.AtByteOffset(table, byteOffset), tafla.TableLayout{})
		float64s := must(buffer.GetFloat64Array(must(buffer.RootOffset(0))))

		if values, ok := float64s.AsSlice(); !ok || len(values) != 0 {
			t.Errorf("at %d, read %v, %v", byteOffset, values, ok)
		}
	}
}

func TestReadsVectorItemsInPlaceWithoutTheCapacityPastTheBytes(t *testing.T) {
	builder := must(tafla.NewTableBuilder(1, tafla.TableLayout{}))
	check(t, builder.SetVector(tafla.TableBytes{1}, tafla.TableBytes{2, 3}))
	table := must(builder.Bytes())

	// The bytes past the table, within its slice's capacity, must stay as
	// they are when an item is appended to
	backing := append(slices.Clone(table), 0xaa, 0xaa, 0xaa, 0xaa)
	buffer := tafla.NewByteBuffer(backing[:len(table)], tafla.TableLayout{})
	vector := must(buffer.GetVector(must(buffer.RootOffset(0))))

	for index := range vector.Len() {
		_ = append(must(vector.GetInPlace(index)), 0)
	}
	for item, err := range vector.ItemsInPlace() {
		check(t, err)
		_ = append(item, 0)
	}
	if past := backing[len(table):]; !bytes.Equal(past, []byte{0xaa, 0xaa, 0xaa, 0xaa}) {
		t.Errorf("appending to an item wrote % x past the table", past)
	}
}

func TestReadsArraysInPlaceWhenAligned(t *testing.T) {
	values := []float64{1.5, math.Copysign(0, -1), math.Float64frombits(tables.QuietNaN64)}
	builder := must(tafla.NewTableBuilder(2, tafla.TableLayout{}))
	check(t, builder.SetUint8Array([]byte{1, 2, 3}))
	check(t, builder.SetFloat64Array(values))
	table := must(builder.Bytes())

	for byteOffset := range 8 {
		copied := tables.AtByteOffset(table, byteOffset)
		buffer := tafla.NewByteBuffer(copied, tafla.TableLayout{})

		position := must(buffer.RootOffset(0))
		uint8s := must(buffer.GetUint8Array(position))
		if &uint8s[0] != &copied[position+4] {
			t.Errorf("at %d, the bytes are a copy", byteOffset)
		}

		float64s := must(buffer.GetFloat64Array(must(buffer.RootOffset(1))))
		// The array lies a multiple of 8 bytes from the table's start, and
		// is in place where the host aligns float64 to the table's offset:
		// only at 0 on 64-bit hosts, and at 0 and 4 on 32-bit ones, which
		// align it to 4
		inPlace := readsInPlace && byteOffset%int(unsafe.Alignof(float64(0))) == 0

		inPlaceSlice, ok := float64s.AsSlice()
		if ok != inPlace {
			t.Errorf("at %d, in place: %v", byteOffset, ok)
		}
		if ok && &inPlaceSlice[0] != (*float64)(unsafe.Pointer(&float64s.Bytes()[0])) {
			t.Errorf("at %d, the slice is a copy", byteOffset)
		}

		slice := float64s.Slice()
		if !equalBits(slice, values) || !equalBits(float64s.Append(nil), values) {
			t.Errorf("at %d, read %v", byteOffset, slice)
		}
		if inPlace != (&slice[0] == (*float64)(unsafe.Pointer(&float64s.Bytes()[0]))) {
			t.Errorf("at %d, Slice() returns the bytes in place: %v", byteOffset, !inPlace)
		}

		var iterated []float64
		for index, value := range float64s.All() {
			if index != len(iterated) {
				t.Errorf("at %d, iterated over index %d", byteOffset, index)
			}
			iterated = append(iterated, value)
		}
		if !equalBits(iterated, values) || !equalBits(slices.Collect(float64s.Values()), values) {
			t.Errorf("at %d, iterated over %v", byteOffset, iterated)
		}

		if math.Float64bits(float64s.At(1)) != math.Float64bits(math.Copysign(0, -1)) {
			t.Errorf("at %d, read %v", byteOffset, float64s.At(1))
		}
		expectPanic(t, "an index past the elements", func() { float64s.At(3) })
	}
}

func TestNestsABuilderSeveralTimes(t *testing.T) {
	nested := must(tafla.NewTableBuilder(2, tafla.TableLayout{}))
	check(t, nested.SetFloat64Array([]float64{1, 2}))
	check(t, nested.SetString("nested"))
	own := must(nested.Bytes())

	fromBuilder := must(tafla.NewTableBuilder(2, tafla.TableLayout{}))
	check(t, fromBuilder.SetTable(nested))
	check(t, fromBuilder.SetVector(nested, nested))

	fromBytes := must(tafla.NewTableBuilder(2, tafla.TableLayout{}))
	check(t, fromBytes.SetTableAligned(tafla.TableBytes(own), tafla.Align8))
	check(t, fromBytes.SetVector(tafla.TableBytes(own), tafla.TableBytes(own)))

	table := must(fromBuilder.Bytes())
	if !bytes.Equal(table, must(fromBytes.Bytes())) {
		t.Errorf("wrote % x\nfrom the builder and % x\nfrom bytes", table, must(fromBytes.Bytes()))
	}

	// The builder is unchanged by nesting
	if !bytes.Equal(must(nested.Bytes()), own) {
		t.Error("nesting changed the builder")
	}

	buffer := tafla.NewByteBuffer(table, tafla.TableLayout{})
	vector := must(buffer.GetVector(must(buffer.RootOffset(1))))
	for item, err := range vector.Items() {
		if err != nil || !bytes.Equal(item, own) {
			t.Errorf("read % x, %v", item, err)
		}
	}
}

func must[T any](value T, err error) T {
	if err != nil {
		panic(err)
	}

	return value
}

func check(t *testing.T, err error) {
	t.Helper()

	if err != nil {
		t.Fatal(err)
	}
}

// expectPanic fails the test unless call panics.
func expectPanic(t *testing.T, name string, call func()) {
	t.Helper()

	defer func() {
		if recover() == nil {
			t.Errorf("%s didn't panic", name)
		}
	}()
	call()
}

// expectPanicMessage fails the test unless call panics with expected
func expectPanicMessage(t *testing.T, expected string, call func()) {
	t.Helper()

	defer func() {
		if message := fmt.Sprint(recover()); message != expected {
			t.Errorf("panicked with %q rather than %q", message, expected)
		}
	}()
	call()
}

func equalBits(a, b []float64) bool {
	return slices.EqualFunc(a, b, func(x, y float64) bool {
		return math.Float64bits(x) == math.Float64bits(y)
	})
}
