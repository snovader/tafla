package tafla_test

// Native fuzz targets, which mirror rust/fuzz. Their seeds are the
// conformance suite's tables, so that the fuzzer mutates tables that the
// descriptions read deep into, and `go test` runs them as tests. A panic is
// a finding, and so is a read that disagrees with SPEC.md, or an error other
// than the decode error.
//
//	go test -run '^$' -fuzz FuzzByteBuffer -fuzztime 5m
//	go test -run '^$' -fuzz FuzzConformance -fuzztime 5m

import (
	"encoding/binary"
	"errors"
	"math"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"unicode/utf8"
	"unsafe"

	tafla "github.com/snovader/tafla/go"
	"github.com/snovader/tafla/go/tests/internal/tables"
)

// checked is the number of elements of an array, and items of a vector, at
// its start and end that are read and compared with the bytes
const checked = 8

// farPositions lie past any bytes, where a position plus a length overflows,
// or before them
var farPositions = []int{math.MaxInt, math.MaxInt - 1, math.MaxInt - 3, math.MaxInt - 7, -1, -8, math.MinInt}

// addSeeds adds the conformance suite's tables to the corpus
func addSeeds(f *testing.F) {
	for _, directory := range []string{"valid", "equivalent", "invalid"} {
		files, err := filepath.Glob(filepath.Join(tables.Conformance(), directory, "*.bin"))
		if err != nil {
			f.Fatal(err)
		}

		for _, file := range files {
			bytes, err := os.ReadFile(file)
			if err != nil {
				f.Fatal(err)
			}
			f.Add(bytes)
		}
	}
}

// FuzzByteBuffer reads arbitrary bytes with every getter of ByteBuffer, at
// every position and past the end, in every offset type and header, and
// reads every array, struct array and vector that it finds there. Each read
// must return the bytes or value that lie where the format puts them, from
// the input, exactly when they lie within it, and the decode error otherwise.
func FuzzByteBuffer(f *testing.F) {
	addSeeds(f)

	f.Fuzz(func(t *testing.T, bytes []byte) {
		for _, offsetType := range tables.OffsetTypes {
			layout := tafla.TableLayout{}.WithOffsetType(offsetType)

			checkHeader(t, tafla.NewByteBuffer(bytes, layout))
			checkHeader(t, tafla.NewByteBuffer(bytes, layout.Sized()))
			checkVersion(t, tafla.NewByteBuffer(bytes, layout.Versioned()))
			checkVersion(t, tafla.NewByteBuffer(bytes, layout.Sized().Versioned()))

			buffer := tafla.NewByteBuffer(bytes, layout)
			for position := range len(bytes) + 9 {
				checkPosition(t, buffer, position)
			}
			for _, position := range farPositions {
				checkPosition(t, buffer, position)
			}
		}
	})
}

// FuzzConformance reads mutated tables as every valid table of the
// conformance suite describes them, following their offsets into nested
// tables, vectors, arrays, strings and structs, as the conformance test
// reads them. Reads may fail, but only with the decode error, or read values
// other than described, and finding a vector's item by index must find what
// iterating finds.
func FuzzConformance(f *testing.F) {
	addSeeds(f)
	suite, err := tables.LoadSuite()
	if err != nil {
		f.Fatal(err)
	}

	f.Fuzz(func(t *testing.T, bytes []byte) {
		for _, valid := range suite.Valid {
			read, err := tables.ReadTable(bytes, valid.Table)
			var decodeError *tafla.DecodeError
			switch {
			case err == nil:
				// The bytes hold values of the described types where it
				// reads them, which may differ from the description's
				if len(read.Fields) != len(valid.Table.Fields) {
					t.Fatalf("%s: read %d fields", valid.Name, len(read.Fields))
				}
			case errors.As(err, &decodeError):
				if err.Error() == "" {
					t.Fatal("an empty message")
				}
			case errors.Is(err, tables.ErrMismatch):
				// The bytes read, but with a size or a number of items other
				// than described
				if strings.Contains(err.Error(), "Get finds") {
					t.Fatalf("%s: %v", valid.Name, err)
				}
			default:
				t.Fatalf("%s: %v", valid.Name, err)
			}
		}
	})
}

// checkHeader reads the table's size and its root fields' offsets
func checkHeader(t *testing.T, buffer tafla.ByteBuffer) {
	bytes := buffer.Bytes()
	layout := buffer.Layout()
	width := layout.OffsetSize()

	size, err := buffer.Size()
	if layout.IsSized() {
		expected, ok := slotAt(bytes, 0, layout.OffsetType())
		checkResult(t, bytes, size, err, int(expected), ok && fitsInt(uint64(expected)))
	} else if size != len(bytes) || err != nil {
		t.Fatalf("the size of %d bytes is %d, %v", len(bytes), size, err)
	}

	// Indices are never negative, which would panic
	indices := []int{math.MaxInt, math.MaxInt - 1, math.MaxInt - 3, math.MaxInt - 7}
	for index := range len(bytes)/width + 2 {
		indices = append(indices, index)
	}
	for _, index := range indices {
		var expected uint32
		ok := index <= len(bytes)
		if ok {
			expected, ok = slotAt(bytes, layout.HeaderSize()+index*width, layout.OffsetType())
		}

		offset, err := buffer.RootOffset(index)
		checkResult(t, bytes, offset, err, int(expected), ok && fitsInt(uint64(expected)))
	}
}

func checkVersion(t *testing.T, buffer tafla.ByteBuffer) {
	bytes := buffer.Bytes()
	layout := buffer.Layout()
	position := 0
	if layout.IsSized() {
		position = layout.OffsetSize()
	}

	expected, ok := slotAt(bytes, position, layout.OffsetType())
	version, err := buffer.Version()
	checkResult(t, bytes, version, err, expected, ok)
	checkHeader(t, buffer)
}

// checkPosition reads every type at position
func checkPosition(t *testing.T, buffer tafla.ByteBuffer, position int) {
	bytes := buffer.Bytes()
	offsetType := buffer.Layout().OffsetType()

	checkScalars(t, buffer, position)
	checkArrays(t, buffer, position)

	stringBytes, ok := arrayAt(bytes, offsetType, position, 1)
	read, err := buffer.GetUint8Array(position)
	checkIn(t, bytes, read, err, stringBytes, ok)
	text, err := buffer.GetString(position)
	if valid := ok && utf8.Valid(stringBytes); valid {
		checkResult(t, bytes, text, err, string(stringBytes), true)
	} else {
		checkError(t, bytes, err)
	}

	remaining := max(len(bytes)-max(position, 0), 0)
	for _, size := range []int{0, 1, 2, 3, 8, 13, remaining, remaining + 1, math.MaxInt} {
		expected, ok := rangeOf(bytes, position, uint64(size))
		read, err := buffer.GetStruct(position, size)
		checkIn(t, bytes, read, err, expected, ok)

		expected, ok = arrayAt(bytes, offsetType, position, uint64(size))
		read, err = buffer.GetStructArray(position, size)
		checkIn(t, bytes, read, err, expected, ok)
	}

	for _, index := range []int{0, 1, 2, math.MaxInt} {
		var expected uint32
		ok := position >= 0 && index <= len(bytes)
		if ok {
			expected, ok = slotAt(bytes, position+index*offsetType.Size(), offsetType)
		}

		offset, err := buffer.FieldOffset(position, index)
		ok = ok && fitsInt(uint64(position)+uint64(expected))
		checkResult(t, bytes, offset, err, position+int(expected), ok)
	}

	checkVector(t, buffer, position)
}

func checkScalars(t *testing.T, buffer tafla.ByteBuffer, position int) {
	bytes := buffer.Bytes()
	scalar := func(size int) uint64 {
		var value uint64
		for index := size - 1; index >= 0; index-- {
			value = value<<8 | uint64(bytes[position+index])
		}
		return value
	}
	check := func(size int, read uint64, err error) {
		_, ok := rangeOf(bytes, position, uint64(size))
		if ok {
			checkResult(t, bytes, read, err, scalar(size), true)
		} else {
			checkError(t, bytes, err)
		}
	}

	// Compared by their bits, which are the bits of floats too
	int8Value, err := buffer.GetInt8(position)
	check(1, uint64(uint8(int8Value)), err)
	uint8Value, err := buffer.GetUint8(position)
	check(1, uint64(uint8Value), err)
	int16Value, err := buffer.GetInt16(position)
	check(2, uint64(uint16(int16Value)), err)
	uint16Value, err := buffer.GetUint16(position)
	check(2, uint64(uint16Value), err)
	int32Value, err := buffer.GetInt32(position)
	check(4, uint64(uint32(int32Value)), err)
	uint32Value, err := buffer.GetUint32(position)
	check(4, uint64(uint32Value), err)
	int64Value, err := buffer.GetInt64(position)
	check(8, uint64(int64Value), err)
	uint64Value, err := buffer.GetUint64(position)
	check(8, uint64Value, err)
	float32Value, err := buffer.GetFloat32(position)
	check(4, uint64(math.Float32bits(float32Value)), err)
	float64Value, err := buffer.GetFloat64(position)
	check(8, math.Float64bits(float64Value), err)
}

func checkArrays(t *testing.T, buffer tafla.ByteBuffer, position int) {
	checkArray(t, buffer, position, tafla.ByteBuffer.GetInt8Array, func(v int8) uint64 { return uint64(uint8(v)) })
	checkArray(t, buffer, position, tafla.ByteBuffer.GetInt16Array, func(v int16) uint64 { return uint64(uint16(v)) })
	checkArray(t, buffer, position, tafla.ByteBuffer.GetUint16Array, func(v uint16) uint64 { return uint64(v) })
	checkArray(t, buffer, position, tafla.ByteBuffer.GetInt32Array, func(v int32) uint64 { return uint64(uint32(v)) })
	checkArray(t, buffer, position, tafla.ByteBuffer.GetUint32Array, func(v uint32) uint64 { return uint64(v) })
	checkArray(t, buffer, position, tafla.ByteBuffer.GetInt64Array, func(v int64) uint64 { return uint64(v) })
	checkArray(t, buffer, position, tafla.ByteBuffer.GetUint64Array, func(v uint64) uint64 { return v })
	checkArray(t, buffer, position, tafla.ByteBuffer.GetFloat32Array, func(v float32) uint64 { return uint64(math.Float32bits(v)) })
	checkArray(t, buffer, position, tafla.ByteBuffer.GetFloat64Array, math.Float64bits)
}

// checkArray checks that every way of reading an array's elements reads the
// elements of its bytes, comparing them by their bits. Only the first and
// last checked elements are compared, since every position may hold an
// array as long as the bytes, and reading each in full would take time
// quadratic in their length.
func checkArray[T tafla.Element](
	t *testing.T,
	buffer tafla.ByteBuffer,
	position int,
	get func(tafla.ByteBuffer, int) (tafla.ArrayView[T], error),
	bitsOf func(T) uint64,
) {
	bytes := buffer.Bytes()
	var zero T
	size := len(binaryBytes(zero))
	expected, ok := arrayAt(bytes, buffer.Layout().OffsetType(), position, uint64(size))

	view, err := get(buffer, position)
	checkIn(t, bytes, view.Bytes(), err, expected, ok)
	if err != nil {
		return
	}

	elementBits := func(index int) uint64 {
		var value uint64
		for byteIndex := size - 1; byteIndex >= 0; byteIndex-- {
			value = value<<8 | uint64(expected[index*size+byteIndex])
		}
		return value
	}

	length := len(expected) / size
	if view.Len() != length {
		t.Fatalf("an array of %d elements has a length of %d", length, view.Len())
	}

	for index := range min(length, checked) {
		for _, at := range []int{index, length - 1 - index} {
			if bitsOf(view.At(at)) != elementBits(at) {
				t.Fatalf("element %d is %v", at, view.At(at))
			}
		}
	}

	iterated := 0
	for index, value := range view.All() {
		if iterated == checked {
			break
		}
		if index != iterated || bitsOf(value) != elementBits(index) {
			t.Fatalf("iterated over element %d, %v", index, value)
		}
		iterated++
	}

	if length <= checked {
		for index, value := range view.Slice() {
			if bitsOf(value) != elementBits(index) {
				t.Fatalf("copied element %d as %v", index, value)
			}
		}
	}

	if slice, ok := view.AsSlice(); ok && length > 0 {
		if len(slice) != length || unsafe.Pointer(&slice[0]) != unsafe.Pointer(&expected[0]) {
			t.Fatalf("a slice of %d elements of %d isn't the bytes", len(slice), length)
		}
	}
}

// binaryBytes returns the little-endian bytes of an element
func binaryBytes(value any) []byte {
	bytes, err := binary.Append(nil, binary.LittleEndian, value)
	if err != nil {
		panic(err)
	}

	return bytes
}

// checkVector reads the vector at position and each of its items, by
// iterating, by index and in place
func checkVector(t *testing.T, buffer tafla.ByteBuffer, position int) {
	bytes := buffer.Bytes()
	offsetType := buffer.Layout().OffsetType()
	width := offsetType.Size()

	// A count, and an end for each item after it, within the bytes
	count, ok := slotAt(bytes, position, offsetType)
	ok = ok && uint64(position)+uint64(width)*(uint64(count)+1) <= uint64(len(bytes))

	vector, err := buffer.GetVector(position)
	if err != nil {
		if ok {
			t.Fatalf("a vector at %d: %v", position, err)
		}
		checkError(t, bytes, err)
		return
	}
	if !ok || vector.Len() != int(count) {
		t.Fatalf("a vector at %d of %d items", position, vector.Len())
	}

	checkItems(t, bytes, position, offsetType, vector)
}

// checkItems reads the first checked items, by iterating, by index and in
// place, and the last by index
func checkItems(t *testing.T, bytes []byte, position int, offsetType tafla.OffsetType, vector tafla.Vector) {
	length := vector.Len()

	var items, itemsInPlace []item
	for bytes, err := range vector.Items() {
		items = append(items, item{bytes, err})
	}
	for bytes, err := range vector.ItemsInPlace() {
		itemsInPlace = append(itemsInPlace, item{bytes, err})
	}
	if len(items) != length || len(itemsInPlace) != length {
		t.Fatalf("iterated over %d and %d of %d items", len(items), len(itemsInPlace), length)
	}

	indices := []int{}
	for index := range min(length, checked) {
		indices = append(indices, index)
	}
	if length > 0 {
		indices = append(indices, length-1)
	}

	for _, index := range indices {
		checkItem(t, bytes, position, offsetType, index, items[index], itemsInPlace[index])

		read, err := vector.Get(index)
		readInPlace, errInPlace := vector.GetInPlace(index)
		if !sameItemOf(item{read, err}, items[index]) || !sameItemOf(item{readInPlace, errInPlace}, itemsInPlace[index]) {
			t.Fatalf("item %d read differently by index than by iterating", index)
		}
	}
}

type item struct {
	bytes []byte
	err   error
}

func sameItemOf(a, b item) bool {
	if a.err != nil || b.err != nil {
		return sameError(a.err, b.err)
	}

	return len(a.bytes) == len(b.bytes) && (len(a.bytes) == 0 || &a.bytes[0] == &b.bytes[0])
}

// checkItem checks item index of the vector at position, which starts where
// the one before it ends, and the first after the count and the ends
func checkItem(t *testing.T, bytes []byte, position int, offsetType tafla.OffsetType, index int, read, readInPlace item) {
	width := offsetType.Size()
	count, _ := slotAt(bytes, position, offsetType)
	endOf := func(index int) uint64 {
		end, _ := slotAt(bytes, position+(index+1)*width, offsetType)
		return uint64(end)
	}

	// In uint64, which holds every end, past any 32-bit int too
	start := (uint64(count) + 1) * uint64(width)
	if index > 0 {
		start = endOf(index - 1)
	}
	end := endOf(index)
	rest := uint64(len(bytes) - position)
	ok := start <= end && end <= rest
	var expected []byte
	if ok {
		expected = bytes[position+int(start) : position+int(end)]
	}

	var decodeError *tafla.DecodeError
	switch {
	case read.err == nil:
		checkIn(t, bytes, read.bytes, nil, expected, ok)
	case errors.As(read.err, &decodeError) && decodeError.Kind == tafla.ItemEndsBeforeStart:
		if start <= end {
			t.Fatalf("item %d from %d to %d: %v", index, start, end, read.err)
		}
	default:
		if ok {
			t.Fatalf("item %d: %v", index, read.err)
		}
		checkError(t, bytes, read.err)
	}

	if readInPlace.err == nil {
		if !ok {
			t.Fatalf("item %d in place lies outside the bytes", index)
		}
		rest := bytes[position+int(start):]
		if len(readInPlace.bytes) != len(rest) || (len(rest) > 0 && &readInPlace.bytes[0] != &rest[0]) {
			t.Fatalf("item %d in place isn't the rest of the bytes", index)
		}
	} else if !sameError(readInPlace.err, read.err) {
		t.Fatalf("item %d failed in place with %v, and with %v", index, readInPlace.err, read.err)
	}
}

// fitsInt reports whether an int holds an offset or position, which a 32-bit
// int doesn't past 2 GiB, where readers fail with OutOfRange, since no bytes
// reach it
func fitsInt(value uint64) bool {
	return value <= math.MaxInt
}

// slotAt returns the value of the slot as wide as offsetType at position,
// or false when it doesn't lie within the bytes
func slotAt(bytes []byte, position int, offsetType tafla.OffsetType) (uint32, bool) {
	slot, ok := rangeOf(bytes, position, uint64(offsetType.Size()))
	if !ok {
		return 0, false
	}

	var value uint32
	for index := len(slot) - 1; index >= 0; index-- {
		value = value<<8 | uint32(slot[index])
	}

	return value, true
}

// arrayAt returns the bytes of the elements of elementSize bytes after the
// count at position, or false when the count or the elements don't lie
// within the bytes
func arrayAt(bytes []byte, offsetType tafla.OffsetType, position int, elementSize uint64) ([]byte, bool) {
	count, ok := slotAt(bytes, position, offsetType)
	if !ok {
		return nil, false
	}

	// A byte length past a uint64 lies past any bytes too
	if elementSize != 0 && uint64(count) > math.MaxUint64/elementSize {
		return nil, false
	}

	return rangeOf(bytes, position+offsetType.Size(), uint64(count)*elementSize)
}

func rangeOf(bytes []byte, position int, byteLength uint64) ([]byte, bool) {
	if position < 0 || position > len(bytes) || byteLength > uint64(len(bytes)-position) {
		return nil, false
	}

	return bytes[position : position+int(byteLength)], true
}

// checkResult checks that a read returns the expected value, or fails with
// the decode error when there's none
func checkResult[T comparable](t *testing.T, bytes []byte, read T, err error, expected T, ok bool) {
	t.Helper()

	switch {
	case err == nil && (!ok || read != expected):
		t.Fatalf("read %v rather than %v, %v", read, expected, ok)
	case err != nil && ok:
		t.Fatalf("failed with %v rather than reading %v", err, expected)
	case err != nil:
		checkError(t, bytes, err)
	}
}

// checkIn checks that a read is the expected bytes, from the input rather
// than copied
func checkIn(t *testing.T, bytes, read []byte, err error, expected []byte, ok bool) {
	t.Helper()

	switch {
	case err == nil && !ok:
		t.Fatalf("read % x outside the bytes", read)
	case err == nil:
		if len(read) != len(expected) || (len(read) > 0 && &read[0] != &expected[0]) {
			t.Fatalf("read % x rather than the bytes % x", read, expected)
		}
	case ok:
		t.Fatalf("failed with %v rather than reading % x", err, expected)
	default:
		checkError(t, bytes, err)
	}
}

// checkError checks that a read failed with an error that describes the
// bytes
func checkError(t *testing.T, bytes []byte, err error) {
	t.Helper()

	var decodeError *tafla.DecodeError
	if !errors.As(err, &decodeError) || err.Error() == "" {
		t.Fatalf("%v rather than a decode error", err)
	}

	switch decodeError.Kind {
	case tafla.OutOfRange:
		if decodeError.Available != len(bytes) {
			t.Fatalf("%v of %d bytes", err, len(bytes))
		}
	case tafla.InvalidUTF8:
		if decodeError.Position >= len(bytes) {
			t.Fatalf("%v of %d bytes", err, len(bytes))
		}
	case tafla.ItemEndsBeforeStart:
		if decodeError.End >= decodeError.Position {
			t.Fatal(err)
		}
	default:
		t.Fatalf("a read failed with %v", err)
	}
}
