package tafla_test

// A builder copies each value into its buffer when it's set, Bytes returns
// that buffer, and Reset starts the next table after the bytes of a slice
// of its caller's. These tests check, through the public API, that a table
// is written alike into any bytes, after any table before it, and that the
// caller's values and bytes are its own once a setter or Reset returns.

import (
	"bytes"
	"errors"
	"strings"
	"testing"

	tafla "github.com/snovader/tafla/go"
	"github.com/snovader/tafla/go/tests/internal/tables"
	"pgregory.net/rapid"
)

// Any table is written alike by a builder from NewTableBuilder and by one
// reset after the bytes of a slice, whose capacity past them holds other
// bytes, whether the builder is new or held another table, finished or not,
// and whether a reset failed before, with its nested tables from builders or
// in place. The bytes before the table stay as they were, and writing the
// table again into the bytes that Bytes returned writes it alike.
func TestATableIsWrittenAlikeIntoAnyBytes(t *testing.T) {
	rapid.Check(t, func(t *rapid.T) {
		table := rapid.OneOf(tables.Tables(), tablesOfShortOffsets()).Draw(t, "table")
		if rapid.Bool().Draw(t, "nested in place") {
			table = table.NestedInPlace()
		}
		inPlace := rapid.Bool().Draw(t, "written in place")
		expected, expectedErr := table.Bytes()

		var builder tafla.TableBuilder
		before := rapid.OneOf(tables.Tables(), tablesOfShortOffsets()).Draw(t, "before")
		if err := builder.Reset(nil, len(before.Fields), before.TableLayout()); err == nil {
			set := rapid.IntRange(0, len(before.Fields)).Draw(t, "fields set before")
			_ = setFields(&builder, before.Fields[:set])
			if set == len(before.Fields) {
				_, _ = builder.Bytes()
			}
		}
		if rapid.Bool().Draw(t, "a reset fails") {
			layout := tafla.TableLayout{}.WithOffsetType(tafla.Uint8).WithVersion(256)
			expectBuildError(t, builder.Reset(nil, 1, layout), tafla.VersionTooLarge)
		}

		prefix := rapid.SliceOfN(rapid.Byte(), 0, 16).Draw(t, "prefix")
		dst := make([]byte, len(prefix), len(prefix)+rapid.IntRange(0, 600).Draw(t, "room"))
		copy(dst, prefix)
		stale := dst[len(dst):cap(dst)]
		for index := range stale {
			stale[index] = 0xa5
		}

		for round := range 2 {
			written, err := writeInto(&builder, dst, table, inPlace)
			if !equalErrors(err, expectedErr) {
				t.Fatalf("failed with %v rather than %v", err, expectedErr)
			}
			if err != nil {
				return
			}
			if !bytes.Equal(written[:len(prefix)], prefix) || !bytes.Equal(written[len(prefix):], expected) {
				t.Fatalf("wrote % x after % x\nrather than % x, in round %d", written[len(prefix):], prefix, expected, round)
			}

			// Given back, from the table's start
			dst = written[:len(prefix)]
		}
	})
}

// writeInto writes the table with builder, reset after the bytes of dst,
// with the tables nested in it in place when inPlace, and returns dst with
// the table appended, or the first error
func writeInto(builder *tafla.TableBuilder, dst []byte, table tables.Table, inPlace bool) ([]byte, error) {
	if err := builder.Reset(dst, len(table.Fields), table.TableLayout()); err != nil {
		return nil, err
	}
	if err := table.SetFields(builder, inPlace); err != nil {
		return nil, err
	}

	return builder.Bytes()
}

func setFields(builder *tafla.TableBuilder, fields []tables.Field) error {
	for _, field := range fields {
		if err := tables.Set(builder, field); err != nil {
			return err
		}
	}

	return nil
}

func expectBuildError(t *rapid.T, err error, kind tafla.BuildErrorKind) {
	var buildError *tafla.BuildError
	if !errors.As(err, &buildError) || buildError.Kind != kind {
		t.Fatalf("failed with %v rather than %v", err, kind)
	}
}

// A setter copies its value, and SetTable and SetVector copy the table, so
// that changing a slice, the bytes of a table or a nested builder once its
// setter returns doesn't change the table
func TestSettersCopyTheirValues(t *testing.T) {
	write := func(change bool) []byte {
		integers := []uint32{1, 2, 3}
		structs := []byte{4, 5, 6, 7}
		names := []string{"first", "second"}
		nestedBytes := tafla.TableBytes{4, 0, 0, 0, 8}

		nested := must(tafla.NewTableBuilder(1, tafla.TableLayout{}))
		check(t, nested.SetString("nested"))
		item := must(tafla.NewTableBuilder(1, tafla.TableLayout{}))
		check(t, item.SetUint16(9))

		builder := must(tafla.NewTableBuilder(7, tafla.TableLayout{}))
		check(t, builder.SetUint32Array(integers))
		check(t, builder.SetStructArray(structs, 2, tafla.Align2))
		check(t, builder.SetStringVector(names))
		check(t, builder.SetStruct(structs))
		check(t, builder.SetTable(nested))
		check(t, builder.SetTable(nestedBytes))
		check(t, builder.SetVector(item, nestedBytes))

		if change {
			integers[0], structs[0], names[0], nestedBytes[4] = 10, 11, "changed", 12
			check(t, nested.Reset(nil, 1, tafla.TableLayout{}))
			check(t, nested.SetString("another table"))
			check(t, item.Reset(make([]byte, 0, 64), 2, tafla.TableLayout{}))
		}

		return must(builder.Bytes())
	}

	if changed, unchanged := write(true), write(false); !bytes.Equal(changed, unchanged) {
		t.Errorf("wrote % x\nrather than % x", changed, unchanged)
	}
}

// Bytes returns the slice that Reset was given, with the table appended
// after its bytes, which stay as they were, into its capacity while it has
// room: here a table of a string after a frame's header
func TestBytesReturnsTheTableAfterTheBytesOfDst(t *testing.T) {
	frame := make([]byte, 0, 64)
	frame = append(frame, "frame"...)

	var builder tafla.TableBuilder
	check(t, builder.Reset(frame, 1, tafla.TableLayout{}))
	check(t, builder.SetString("value"))
	written := must(builder.Bytes())

	alone := must(tafla.NewTableBuilder(1, tafla.TableLayout{}))
	check(t, alone.SetString("value"))
	if !bytes.Equal(written, append([]byte("frame"), must(alone.Bytes())...)) {
		t.Errorf("wrote % x", written)
	}
	if &written[0] != &frame[0] {
		t.Error("wrote the table into new bytes, although the frame had room for it")
	}
}

// A slice whose capacity has no room for the table's header is grown as
// append grows it, into new bytes, leaving its own as they were
func TestResetIntoBytesWithoutRoomLeavesThemAsTheyWere(t *testing.T) {
	frame := append(make([]byte, 0, 8), "frame\xee\xee\xee"...)[:5]

	var builder tafla.TableBuilder
	check(t, builder.Reset(frame, 1, tafla.TableLayout{}))
	check(t, builder.SetString(strings.Repeat("a", 1000)))
	written := must(builder.Bytes())

	if string(frame[:8]) != "frame\xee\xee\xee" {
		t.Errorf("left % x", frame[:8])
	}
	if string(written[:5]) != "frame" || len(written) != 5+4+4+1000 {
		t.Errorf("wrote %d bytes, after %q", len(written), written[:5])
	}
}

// The zero TableBuilder holds a table of no fields in the default layout,
// which it writes, and nests, as a builder of it from NewTableBuilder does
func TestTheZeroBuilderHoldsATableOfNoFields(t *testing.T) {
	var zero tafla.TableBuilder
	if table := must(zero.Bytes()); table == nil || len(table) != 0 {
		t.Errorf("wrote % x, nil: %t", table, table == nil)
	}
	expectError(t, zero.SetUint8(1), &tafla.BuildError{Kind: tafla.TooManyFields, Fields: 0})

	empty := must(tafla.NewTableBuilder(0, tafla.TableLayout{}))
	nest := func(table *tafla.TableBuilder) []byte {
		parent := must(tafla.NewTableBuilder(3, tafla.TableLayout{}))
		check(t, parent.SetUint8(1))
		check(t, parent.SetTable(table))
		check(t, parent.SetVector(table, table))
		return must(parent.Bytes())
	}
	if fromZero, fromNew := nest(&zero), nest(empty); !bytes.Equal(fromZero, fromNew) {
		t.Errorf("nested % x\nrather than % x", fromZero, fromNew)
	}
}

// A reset that fails leaves the builder as it was, holding its table
func TestAResetThatFailsLeavesTheBuilderAsItWas(t *testing.T) {
	builder := must(tafla.NewTableBuilder(2, tafla.TableLayout{}))
	check(t, builder.SetString("first"))

	err := builder.Reset(nil, 1, tafla.TableLayout{}.WithOffsetType(tafla.Uint8).WithVersion(256))
	expectError(t, err, &tafla.BuildError{Kind: tafla.VersionTooLarge, Version: 256, OffsetType: tafla.Uint8})
	err = builder.Reset(nil, 300, tafla.TableLayout{}.WithOffsetType(tafla.Uint8))
	expectError(t, err, &tafla.BuildError{Kind: tafla.HeaderTooLong, Fields: 300, OffsetType: tafla.Uint8})

	check(t, builder.SetString("second"))
	expected := must(tafla.NewTableBuilder(2, tafla.TableLayout{}))
	check(t, expected.SetString("first"))
	check(t, expected.SetString("second"))
	if table := must(builder.Bytes()); !bytes.Equal(table, must(expected.Bytes())) {
		t.Errorf("wrote % x", table)
	}
}

// A table nested in place with SetTableFuncAligned is padded for its
// alignment before its fields, and writes the bytes of SetTableAligned with
// its builder when its fields need no more, and those of SetTableFunc when
// they need more, to which it's moved: here after a parent's byte
func TestATableNestedInPlaceAtAnAlignmentWritesWhatItsBuilderWrites(t *testing.T) {
	cases := []struct {
		name      string
		alignment tafla.Alignment
		set       func(tables.Target) error
		// The alignment of its builder that writes the same bytes
		written tafla.Alignment
	}{
		{"a byte at 8", tafla.Align8, func(table tables.Target) error { return table.SetUint8(1) }, tafla.Align8},
		{"uint32s at 4", tafla.Align4, func(table tables.Target) error { return table.SetUint32Array([]uint32{2}) }, tafla.Align4},
		{"float64s at 4", tafla.Align4, func(table tables.Target) error { return table.SetFloat64Array([]float64{3}) }, tafla.Align8},
		{"float64s at 1", tafla.Align1, func(table tables.Target) error { return table.SetFloat64Array([]float64{3}) }, tafla.Align8},
		{"a byte at 1", tafla.Align1, func(table tables.Target) error { return table.SetUint8(1) }, tafla.Align1},
	}

	for _, c := range cases {
		nested := must(tafla.NewTableBuilder(1, tafla.TableLayout{}))
		check(t, c.set(nested))
		fromBuilder := must(tafla.NewTableBuilder(2, tafla.TableLayout{}))
		check(t, fromBuilder.SetUint8(9))
		check(t, fromBuilder.SetTableAligned(nested, c.written))

		inPlace := must(tafla.NewTableBuilder(2, tafla.TableLayout{}))
		check(t, inPlace.SetUint8(9))
		check(t, inPlace.SetTableFuncAligned(1, c.alignment, func(table tafla.Item) error { return c.set(table) }))

		if written, expected := must(inPlace.Bytes()), must(fromBuilder.Bytes()); !bytes.Equal(written, expected) {
			t.Errorf("%s: wrote % x\nrather than % x", c.name, written, expected)
		}
	}
}

// A table nested in place at an alignment fails as its builder fails when
// its padding makes its parent too long, leaving the builder as it was
func TestATableNestedInPlaceAtAnAlignmentFailsAsItsBuilderFails(t *testing.T) {
	layout := tafla.TableLayout{}.WithOffsetType(tafla.Uint8)
	start := func() *tafla.TableBuilder {
		builder := must(tafla.NewTableBuilder(3, layout))
		check(t, builder.SetUint8Array(make([]byte, 248)))
		check(t, builder.SetUint8(1))
		return builder
	}
	// 3 offsets, a count, 248 bytes and a byte make 253, which a table of an
	// offset and a byte, padded by 3 to 8, makes 258
	nested := must(tafla.NewTableBuilder(1, layout))
	check(t, nested.SetUint8(1))
	fromBuilder := start()
	expected := fromBuilder.SetTableAligned(nested, tafla.Align8)

	inPlace := start()
	err := inPlace.SetTableFuncAligned(1, tafla.Align8, func(table tafla.Item) error { return table.SetUint8(1) })
	expectError(t, err, expected)
	expectError(t, err, &tafla.BuildError{Kind: tafla.TooLong, Index: 2, Length: 258, OffsetType: tafla.Uint8})

	check(t, inPlace.SetUint8(2))
	check(t, fromBuilder.SetUint8(2))
	if written := must(inPlace.Bytes()); !bytes.Equal(written, must(fromBuilder.Bytes())) {
		t.Errorf("wrote % x", written)
	}
}

// Tables nested in place at any depth are written into the builder's
// buffer, and allocate in proportion to their table: here 2,000 tables, each
// of a string of 100 bytes and the table nested in it
func TestTablesNestedDeeplyInPlaceAllocateInProportionToTheirTable(t *testing.T) {
	const depth = 2_000
	value := strings.Repeat("a", 100)

	// The innermost table holds the string alone
	fields := func(depth int) int { return min(depth, 1) + 1 }
	var level func(table tafla.Item, depth int) error
	level = func(table tafla.Item, depth int) error {
		if err := table.SetString(value); err != nil || depth == 0 {
			return err
		}
		return table.SetTableFunc(fields(depth-1), func(inner tafla.Item) error { return level(inner, depth-1) })
	}

	var table []byte
	allocated := allocatedBy(func() {
		builder := must(tafla.NewTableBuilder(1, tafla.TableLayout{}))
		check(t, builder.SetTableFunc(fields(depth), func(outer tafla.Item) error { return level(outer, depth) }))
		table = must(builder.Bytes())
	})
	if allocated > 16*uint64(len(table)) {
		t.Errorf("allocated %d bytes for a table of %d", allocated, len(table))
	}

	reader := tafla.NewTableReader(table, tafla.TableLayout{})
	position := must(reader.RootOffset(0))
	for level := range depth + 1 {
		read := must(reader.GetString(must(reader.FieldOffset(position, 0))))
		if read != value {
			t.Fatalf("read %q at level %d", read, level)
		}
		if level < depth {
			position = must(reader.FieldOffset(position, 1))
		}
	}
}
