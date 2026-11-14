package tafla_test

// Readers decode untrusted bytes: any bytes, and valid tables cut short or
// overwritten, read as values or fail with a DecodeError, and never panic.
// The examples document the errors.

import (
	"bytes"
	"encoding/binary"
	"errors"
	"math"
	"math/bits"
	"reflect"
	"slices"
	"strconv"
	"testing"

	tafla "github.com/snovader/tafla/go"
	"github.com/snovader/tafla/go/tests/internal/tables"
	"pgregory.net/rapid"
)

// A field that lies before the cut reads the value that the table was built
// with, and any other fails with OutOfRange
func TestTruncatedTablesReadTheirValuesOrOutOfRange(t *testing.T) {
	rapid.Check(t, func(t *rapid.T) {
		table := tables.Tables().Draw(t, "table")
		built := must(table.Bytes())
		if len(built) == 0 {
			t.Skip("an empty table can't be cut")
		}

		cuts := rapid.SliceOfN(rapid.IntRange(0, len(built)-1), 1, 16).Draw(t, "cuts")
		for _, length := range cuts {
			// A copy, so that no read can reach the bytes after the cut
			truncated := slices.Clone(built[:length])

			for index, field := range table.Fields {
				read, err := tables.ReadRootField(truncated, table.Layout, index, field)
				var decodeError *tafla.DecodeError
				switch {
				case err == nil:
					if !tables.EqualFields(read, field) {
						t.Fatalf("field %d cut at %d read %+v rather than %+v", index, length, read, field)
					}
				case !errors.As(err, &decodeError) || decodeError.Kind != tafla.OutOfRange:
					t.Fatalf("field %d cut at %d: %v", index, length, err)
				}
			}
		}
	})
}

// Overwritten bytes can hold other values, offsets, counts and item ends,
// which read as other values, as vectors of another number of items or as a
// size other than the table's length, or fail with a DecodeError. Finding a
// vector item from its ends finds what iterating finds, whatever they hold.
func TestOverwrittenTablesReadValuesOrDecodeErrors(t *testing.T) {
	rapid.Check(t, func(t *rapid.T) {
		table := tables.Tables().Draw(t, "table")
		built := must(table.Bytes())
		if len(built) == 0 {
			t.Skip("an empty table has no bytes to overwrite")
		}

		overwrites := rapid.IntRange(1, 4).Draw(t, "overwrites")
		for range overwrites {
			built[rapid.IntRange(0, len(built)-1).Draw(t, "at")] = drawByte(t)
		}

		errs := []error{second(tables.ReadTable(built, table))}
		for index, field := range table.Fields {
			errs = append(errs, second(tables.ReadRootField(built, table.Layout, index, field)))
		}

		for _, err := range errs {
			checkReadError(t, err)
		}
	})
}

// Every getter, at the root offsets of a table of any layout, in the tables
// nested in it and in its vectors, and at any position and field offset, and
// structs of any size, returns a value or a DecodeError. Bytes read the same
// within larger bytes as on their own, at any address, and an OutOfRange
// error describes a read that would leave them. Finding a vector item from
// its ends finds what iterating finds.
func TestArbitraryBytesReadAsValuesOrDecodeErrors(t *testing.T) {
	rapid.Check(t, func(t *rapid.T) {
		drawn := drawBytes(t, 64, "bytes")
		before := drawBytes(t, 8, "before")
		after := drawBytes(t, 8, "after")
		byteOffset := rapid.IntRange(0, 7).Draw(t, "byteOffset")
		table := tables.Tables().Draw(t, "table")
		position := drawLarge(t, 80, "position")
		index := drawCount(t, 12, "index")
		size := drawCount(t, 16, "size")

		backing := slices.Concat(before, drawn, after)
		within := backing[len(before) : len(before)+len(drawn)]
		copied := tables.AtByteOffset(drawn, byteOffset)

		read := readsOf(copied, table, position, index, size)
		if readWithin := readsOf(within, table, position, index, size); !read.equal(readWithin) {
			t.Fatalf("the bytes read\n%+v\non their own and\n%+v\nwithin others", read, readWithin)
		}

		for _, err := range read.errors() {
			checkReadError(t, err)

			var decodeError *tafla.DecodeError
			if errors.As(err, &decodeError) && decodeError.Kind == tafla.OutOfRange {
				end := float64(decodeError.Position) + float64(decodeError.ByteLength)
				leaves := decodeError.Position < 0 || end > float64(decodeError.Available)
				if !leaves || decodeError.Available > len(drawn) {
					t.Fatalf("%+v doesn't leave the %d bytes", decodeError, len(drawn))
				}
			}
		}
	})
}

// Uint32FieldOffset, with which the generated code reads uint32 offsets,
// reads what FieldOffset reads of a nested table at any position, and the
// positions that RootOffset reads of a root of uint32 offsets after its
// header, from any bytes. A root's offset past the largest int, which only
// a 32-bit int can't hold, fails with OutOfRange from either, at the table's
// position or at the offset's.
func TestUint32FieldOffsetReadsTheOffsetsThatTheBufferReads(t *testing.T) {
	rapid.Check(t, func(t *rapid.T) {
		bytes := drawBytes(t, 64, "bytes")
		table := max(drawLarge(t, 80, "table"), 0)
		index := rapid.IntRange(0, 20).Draw(t, "index")

		nested := tafla.NewByteBuffer(bytes, tafla.TableLayout{})
		position, err := tafla.Uint32FieldOffset(bytes, table, table+4*index)
		expected, expectedErr := nested.FieldOffset(table, index)
		if position != expected || !reflect.DeepEqual(err, expectedErr) {
			t.Fatalf("read %d, %v rather than %d, %v", position, err, expected, expectedErr)
		}

		for _, layout := range []tafla.TableLayout{
			{}, tafla.TableLayout{}.Sized(), tafla.TableLayout{}.Versioned(), tafla.TableLayout{}.Sized().Versioned(),
		} {
			root := tafla.NewByteBuffer(bytes, layout)
			position, err := tafla.Uint32FieldOffset(bytes, 0, layout.HeaderSize()+4*index)
			expected, expectedErr := root.RootOffset(index)

			var decodeError, expectedDecodeError *tafla.DecodeError
			bothFail := errors.As(err, &decodeError) && errors.As(expectedErr, &expectedDecodeError) &&
				decodeError.Kind == tafla.OutOfRange && expectedDecodeError.Kind == tafla.OutOfRange
			if !bothFail && (position != expected || err != nil || expectedErr != nil) {
				t.Fatalf("%+v: read %d, %v rather than %d, %v", layout, position, err, expected, expectedErr)
			}
		}
	})
}

// checkReadError fails unless err is nil, a decode error or a mismatch of
// the values read, which the description of other bytes reads
func checkReadError(t *rapid.T, err error) {
	var decodeError *tafla.DecodeError
	if err != nil && !errors.As(err, &decodeError) && !errors.Is(err, tables.ErrMismatch) {
		t.Fatalf("%v", err)
	}

	if errors.Is(err, tables.ErrMismatch) && bytes.Contains([]byte(err.Error()), []byte("Get finds")) {
		t.Fatalf("%v", err)
	}
}

// drawByte draws bytes, mostly small, so that offsets, counts and lengths
// often lie within them, and some 0xff
func drawByte(t *rapid.T) byte {
	switch choice := rapid.IntRange(0, 9).Draw(t, "kind"); {
	case choice < 4:
		return 0
	case choice < 8:
		return rapid.ByteRange(1, 15).Draw(t, "small")
	case choice < 9:
		return 0xff
	default:
		return rapid.Byte().Draw(t, "byte")
	}
}

func drawBytes(t *rapid.T, most int, label string) []byte {
	drawn := make([]byte, rapid.IntRange(0, most).Draw(t, label))
	for index := range drawn {
		drawn[index] = drawByte(t)
	}

	return drawn
}

// drawLarge draws positions, indexes and sizes, mostly below small, so that
// they lie within small bytes or tables, and up to the largest int, either
// sign
func drawLarge(t *rapid.T, small int, label string) int {
	switch choice := rapid.IntRange(0, 9).Draw(t, label+"Kind"); {
	case choice < 8:
		return rapid.IntRange(0, small-1).Draw(t, label)
	case choice < 9:
		return rapid.SampledFrom([]int{math.MaxInt, math.MinInt, -1}).Draw(t, label)
	default:
		return rapid.Int().Draw(t, label)
	}
}

// drawCount draws indexes and sizes, which callers never make negative,
// mostly below small, and up to the largest int
func drawCount(t *rapid.T, small int, label string) int {
	switch choice := rapid.IntRange(0, 9).Draw(t, label+"Kind"); {
	case choice < 8:
		return rapid.IntRange(0, small-1).Draw(t, label)
	case choice < 9:
		return math.MaxInt
	default:
		return rapid.IntRange(0, math.MaxInt).Draw(t, label)
	}
}

// reads is everything that a test reads from arbitrary bytes, to compare
// between copies of them
type reads struct {
	table       tables.Table
	tableErr    error
	fields      []tables.Field
	fieldErrs   []error
	size        int
	sizeErr     error
	version     uint32
	versionErr  error
	fieldOffset int
	offsetErr   error
	structBytes []byte
	structErr   error
	structArray []byte
	arrayErr    error
}

// readsOf reads bytes as table, and reads each of its root fields at its own
// root offset, at root offset index and at position, and a struct and an
// array of structs of size bytes at position
func readsOf(bytes []byte, table tables.Table, position, index, size int) reads {
	layout := table.Layout
	var read reads
	for rootIndex, field := range table.Fields {
		for _, read2 := range []func() (tables.Field, error){
			func() (tables.Field, error) { return tables.ReadRootField(bytes, layout, rootIndex, field) },
			func() (tables.Field, error) { return tables.ReadRootField(bytes, layout, index, field) },
			func() (tables.Field, error) { return tables.ReadField(bytes, layout, position, field) },
		} {
			value, err := read2()
			read.fields = append(read.fields, value)
			read.fieldErrs = append(read.fieldErrs, err)
		}
	}

	unversioned := layout
	unversioned.Versioned = false
	buffer := tafla.NewByteBuffer(bytes, unversioned.TableLayout())
	versioned := tafla.NewByteBuffer(bytes, unversioned.TableLayout().Versioned())

	read.table, read.tableErr = tables.ReadTable(bytes, table)
	read.size, read.sizeErr = buffer.Size()
	read.version, read.versionErr = versioned.Version()
	read.fieldOffset, read.offsetErr = buffer.FieldOffset(position, index)
	read.structBytes, read.structErr = buffer.GetStruct(position, size)
	read.structArray, read.arrayErr = buffer.GetStructArray(position, size)

	return read
}

func (r reads) errors() []error {
	errs := append([]error{r.tableErr, r.sizeErr, r.versionErr, r.offsetErr, r.structErr, r.arrayErr}, r.fieldErrs...)
	return slices.DeleteFunc(errs, func(err error) bool { return err == nil })
}

func (r reads) equal(other reads) bool {
	return tables.Equal(r.table, other.table) && sameError(r.tableErr, other.tableErr) &&
		slices.EqualFunc(r.fields, other.fields, equalField) &&
		slices.EqualFunc(r.fieldErrs, other.fieldErrs, sameError) &&
		r.size == other.size && sameError(r.sizeErr, other.sizeErr) &&
		r.version == other.version && sameError(r.versionErr, other.versionErr) &&
		r.fieldOffset == other.fieldOffset && sameError(r.offsetErr, other.offsetErr) &&
		bytes.Equal(r.structBytes, other.structBytes) && sameError(r.structErr, other.structErr) &&
		bytes.Equal(r.structArray, other.structArray) && sameError(r.arrayErr, other.arrayErr)
}

func equalField(a, b tables.Field) bool {
	return (a == nil && b == nil) || (a != nil && b != nil && tables.EqualFields(a, b))
}

// sameError reports whether two errors are both nil, or the same
func sameError(a, b error) bool {
	if a == nil || b == nil {
		return a == b
	}

	return reflect.DeepEqual(a, b) || a.Error() == b.Error()
}

func second[T any](_ T, err error) error {
	return err
}

func TestReturnsAnErrorOutsideTheBytes(t *testing.T) {
	backing := make([]byte, 32)
	buffer := tafla.NewByteBuffer(backing[:8], tafla.TableLayout{})

	_, err := buffer.GetFloat64(4)
	expectError(t, err, &tafla.DecodeError{Kind: tafla.OutOfRange, Position: 4, ByteLength: 8, Available: 8})

	_, err = buffer.GetInt64(math.MaxInt)
	expectMessage(t, err, "tafla: read exceeds the supplied bytes: 8 bytes at "+strconv.Itoa(math.MaxInt)+" of 8")
	_, err = buffer.GetInt64(-1)
	expectError(t, err, &tafla.DecodeError{Kind: tafla.OutOfRange, Position: -1, ByteLength: 8, Available: 8})

	for _, read := range []func() error{
		func() error { return second(buffer.GetUint32(5)) },
		func() error { return second(buffer.GetInt8(8)) },
		func() error { return second(buffer.RootOffset(math.MaxInt)) },
		func() error { return second(buffer.FieldOffset(math.MaxInt, math.MaxInt)) },
		func() error { return second(buffer.FieldOffset(-4, 1)) },
		func() error { return second(buffer.FieldOffset(4, 1)) },
	} {
		var decodeError *tafla.DecodeError
		if err := read(); !errors.As(err, &decodeError) {
			t.Errorf("%v rather than a decode error", err)
		}
	}

	if value, err := buffer.GetUint32(4); value != 0 || err != nil {
		t.Errorf("read %d, %v", value, err)
	}

	_, err = buffer.GetStruct(4, 5)
	expectError(t, err, outside(4, 5))
	if read := must(buffer.GetStruct(4, 4)); !bytes.Equal(read, []byte{0, 0, 0, 0}) {
		t.Errorf("read % x", read)
	}
}

// sliceOf returns a backing array whose 8 bytes from 4 hold uint32s,
// followed by bytes that a read past their end would return
func sliceOf(uint32s ...uint32) []byte {
	backing := bytes.Repeat([]byte{65}, 32)
	for index, value := range uint32s {
		binary.LittleEndian.PutUint32(backing[4+index*4:], value)
	}

	return backing
}

// outside returns the error of a read of byteLength bytes at position of the
// 8 bytes of sliceOf
func outside(position int, byteLength uint64) error {
	return &tafla.DecodeError{Kind: tafla.OutOfRange, Position: position, ByteLength: byteLength, Available: 8}
}

// The bytes hold a vector's count of 1 and its item's end at 13, which is
// also read as the count of an array, string or array of structs
func TestPanicsForANegativeFieldIndexOrStructSize(t *testing.T) {
	buffer := tafla.NewByteBuffer(sliceOf(8, 0), tafla.TableLayout{})

	expectPanic(t, "a negative root index", func() { _, _ = buffer.RootOffset(-1) })
	expectPanic(t, "a negative nested index", func() { _, _ = buffer.FieldOffset(0, -1) })
	expectPanic(t, "a negative struct size", func() { _, _ = buffer.GetStruct(0, -1) })
	expectPanic(t, "a negative struct array size", func() { _, _ = buffer.GetStructArray(4, -1) })
	expectPanic(t, "the smallest struct size", func() { _, _ = buffer.GetStructArray(4, math.MinInt) })
}

func TestRejectsAnArrayStringStructAndVectorItemOutsideTheBytes(t *testing.T) {
	backing := sliceOf(1, 13)
	buffer := tafla.NewByteBuffer(backing[4:12], tafla.TableLayout{})

	expectError(t, second(buffer.GetUint8Array(4)), outside(8, 13))
	expectError(t, second(buffer.GetString(4)), outside(8, 13))
	expectError(t, second(buffer.GetFloat32Array(4)), outside(8, 52))
	expectError(t, second(buffer.GetStructArray(4, 2)), outside(8, 26))

	// The vector's end lies within the bytes, and its item doesn't
	vector := must(buffer.GetVector(0))
	expectError(t, second(vector.GetInPlace(0)), outside(8, 5))
	expectError(t, second(vector.Get(0)), outside(8, 5))
	for _, err := range vector.ItemsInPlace() {
		expectError(t, err, outside(8, 5))
	}
	for _, err := range vector.Items() {
		expectError(t, err, outside(8, 5))
	}
}

func TestRejectsAVectorWhoseEndsLieOutsideTheBytes(t *testing.T) {
	backing := sliceOf(3)
	buffer := tafla.NewByteBuffer(backing[4:12], tafla.TableLayout{})

	// The count of 3, at 0, has three ends, of 12 bytes from 4, which is
	// checked before any item is found
	expectError(t, second(buffer.GetVector(0)), outside(4, 12))
}

func TestRejectsAVectorItemThatEndsBeforeItStarts(t *testing.T) {
	// A count of 2 puts the first item after the ends, at 12, but it ends at
	// 11
	table := []byte{2, 0, 0, 0, 11, 0, 0, 0, 12, 0, 0, 0}
	vector := must(tafla.NewByteBuffer(table, tafla.TableLayout{}).GetVector(0))

	expected := &tafla.DecodeError{Kind: tafla.ItemEndsBeforeStart, Position: 12, End: 11}
	expectError(t, second(vector.GetInPlace(0)), expected)
	expectError(t, second(vector.Get(0)), expected)
	for _, err := range vector.ItemsInPlace() {
		expectError(t, err, expected)
		break
	}
	for _, err := range vector.Items() {
		expectError(t, err, expected)
		break
	}
	expectMessage(t, expected, "tafla: the vector item at 12 ends before it starts, at 11")
	expectPanic(t, "an index past the items", func() { _, _ = vector.Get(2) })
}

func TestRejectsAnArrayWhoseByteLengthOverflows(t *testing.T) {
	table := []byte{255, 255, 255, 255, 0, 0, 0, 0}
	buffer := tafla.NewByteBuffer(table, tafla.TableLayout{})

	expected := &tafla.DecodeError{Kind: tafla.OutOfRange, Position: 4, ByteLength: 8 * 0xffff_ffff, Available: 8}
	expectError(t, second(buffer.GetFloat64Array(0)), expected)

	// Structs as large as an int make byte lengths past a uint64 on 64-bit
	// hosts, which saturate
	byteLength := uint64(math.MaxUint64)
	if high, low := bits.Mul64(0xffff_ffff, math.MaxInt); high == 0 {
		byteLength = low
	}
	expected = &tafla.DecodeError{Kind: tafla.OutOfRange, Position: 4, ByteLength: byteLength, Available: 8}
	expectError(t, second(buffer.GetStructArray(0, math.MaxInt)), expected)
}

func TestRejectsInvalidUTF8(t *testing.T) {
	cases := []struct {
		name string
		utf8 []byte
	}{
		{"a lone continuation byte", []byte{0x80}},
		{"a truncated sequence", []byte{0xe2, 0x82}},
		{"an overlong encoding", []byte{0xc0, 0xaf}},
		{"an encoded surrogate", []byte{0xed, 0xa0, 0x80}},
	}

	for _, c := range cases {
		table := binary.LittleEndian.AppendUint32(nil, uint32(len(c.utf8)))
		table = append(table, c.utf8...)

		_, err := tafla.NewByteBuffer(table, tafla.TableLayout{}).GetString(0)
		expectError(t, err, &tafla.DecodeError{Kind: tafla.InvalidUTF8, Position: 0})
		expectMessage(t, err, "tafla: the string at 0 isn't valid UTF-8")
	}
}

func TestDescribesAnUnsupportedVersion(t *testing.T) {
	layout := tafla.TableLayout{}.Sized().Versioned()
	builder := must(tafla.NewVersionedTableBuilder(1, layout, 2))
	check(t, builder.SetUint8(42))
	table := must(builder.Bytes())

	version := must(tafla.NewByteBuffer(table, layout).Version())
	err := tafla.NewUnsupportedVersionError(version, 1)
	expectError(t, err, &tafla.DecodeError{Kind: tafla.UnsupportedVersion, Version: 2, Supported: 1})
	expectMessage(t, err, "tafla: unsupported format version 2; the reader reads version 1")
}

func TestPanicsForTheVersionOfALayoutWithoutOne(t *testing.T) {
	buffer := tafla.NewByteBuffer([]byte{1, 0, 0, 0}, tafla.TableLayout{}.Sized())
	expectPanic(t, "Version", func() { _, _ = buffer.Version() })
}

func expectError(t *testing.T, err, expected error) {
	t.Helper()

	if !reflect.DeepEqual(err, expected) {
		t.Errorf("%#v rather than %#v", err, expected)
	}
}

func expectMessage(t *testing.T, err error, expected string) {
	t.Helper()

	if err == nil || err.Error() != expected {
		t.Errorf("%v rather than %q", err, expected)
	}
}
