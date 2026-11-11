package tafla_test

// Builders write any table that fits in its offset type, and reject what
// they can't write with a BuildError from the setter, leaving the builder as
// it was. The examples document the errors.

import (
	"bytes"
	"errors"
	"fmt"
	"math"
	"strconv"
	"strings"
	"testing"

	tafla "github.com/snovader/tafla/go"
	"github.com/snovader/tafla/go/internal/tables"
	"pgregory.net/rapid"
)

// After any fields, a field as long as fits is written and reads back, and a
// unit longer is rejected with the length that it would make the table
func TestTheLongestFieldThatFitsIsWrittenAndAUnitMoreIsRejected(t *testing.T) {
	rapid.Check(t, func(t *rapid.T) {
		table := tablesOfShortOffsets().Draw(t, "table")
		long := rapid.SampledFrom(longFields).Draw(t, "long")

		offsetType := table.Layout.OffsetType
		maxLength := int(offsetType.MaxValue())
		unit := long.unit(offsetType.Size())

		// The fields can leave no room for even the empty field
		empty := withField(table, long.field(0))
		emptyBytes, err := empty.Bytes()
		if err != nil {
			t.Skip("no room for the field")
		}

		units := (maxLength - len(emptyBytes)) / unit
		longest := withField(table, long.field(units))
		built := must(longest.Bytes())
		if len(built) <= maxLength-unit || len(built) > maxLength {
			t.Fatalf("%d bytes", len(built))
		}

		read, err := tables.ReadTable(built, longest)
		if err != nil || !tables.Equal(read, longest) {
			t.Fatalf("read %+v, %v", read, err)
		}

		builder := must(empty.Builder())
		for _, field := range table.Fields {
			check2(t, tables.Set(builder, field))
		}

		length := len(emptyBytes) + (units+1)*unit
		expected := &tafla.BuildError{Kind: tafla.TooLong, Index: len(table.Fields), Length: uint64(length), OffsetType: offsetType}
		if err := tables.Set(builder, long.field(units+1)); !equalErrors(err, expected) {
			t.Fatalf("%#v rather than %#v", err, expected)
		}

		check2(t, tables.Set(builder, empty.Fields[len(empty.Fields)-1]))
		if rebuilt := must(builder.Bytes()); !bytes.Equal(rebuilt, emptyBytes) {
			t.Fatalf("wrote % x rather than % x", rebuilt, emptyBytes)
		}
	})
}

// A field longer than the offsets address on its own, set after any of the
// fields, is rejected, and the builder writes the same table once the other
// fields are set
func TestAFieldTooLongForTheOffsetTypeIsRejectedAndLeavesTheBuilderAsItWas(t *testing.T) {
	rapid.Check(t, func(t *rapid.T) {
		table := tablesOfShortOffsets().Draw(t, "table")
		long := rapid.SampledFrom(longFields).Draw(t, "long")
		if len(table.Fields) == 0 {
			t.Skip("no fields to set it after")
		}
		at := rapid.IntRange(0, len(table.Fields)-1).Draw(t, "at")

		offsetType := table.Layout.OffsetType
		maxLength := int(offsetType.MaxValue())
		tooLong := long.field(maxLength/long.unit(offsetType.Size()) + 1)

		builder := must(table.Builder())
		for _, field := range table.Fields[:at] {
			check2(t, tables.Set(builder, field))
		}

		var buildError *tafla.BuildError
		err := tables.Set(builder, tooLong)
		if !errors.As(err, &buildError) || buildError.Kind != tafla.TooLong || buildError.Index != at ||
			buildError.Length <= uint64(maxLength) || buildError.OffsetType != offsetType {
			t.Fatalf("%#v", err)
		}

		for _, field := range table.Fields[at:] {
			check2(t, tables.Set(builder, field))
		}

		if built := must(builder.Bytes()); !bytes.Equal(built, must(table.Bytes())) {
			t.Fatalf("wrote % x rather than % x", built, must(table.Bytes()))
		}
	})
}

// A table nested from its builder writes the same parent as from its bytes:
// the builder is written in place when it has the parent's offset type and
// no header, and in its own layout otherwise. The table starts at the
// alignment it's set with and holds its bytes, and the vector's items read
// back as its bytes.
func TestATableNestedFromItsBuilderWritesTheSameParentAsFromItsBytes(t *testing.T) {
	rapid.Check(t, func(t *rapid.T) {
		table := tables.Tables().Draw(t, "table")
		before := rapid.IntRange(0, 7).Draw(t, "before")

		built, err := table.Bytes()
		if err != nil {
			t.Skip("the table doesn't fit")
		}

		// Of the table's offset type when the parent fits in it, so that a
		// builder without a header is written in place whatever its offset
		// type, and otherwise of uint32 offsets
		offsetType := table.Layout.OffsetType
		if longestParent(offsetType, before, len(built)) > int(offsetType.MaxValue()) {
			offsetType = tafla.Uint32
		}
		layout := tafla.TableLayout{}.WithOffsetType(offsetType)

		nested := must(table.Build())
		fromBuilder := must(parentOf(func() tafla.NestedTable { return nested }, before, layout))
		fromBytes := must(parentOf(func() tafla.NestedTable { return tafla.TableBytes(built) }, before, layout))
		if !bytes.Equal(fromBuilder, fromBytes) {
			t.Fatalf("wrote % x\nfrom the builder and % x\nfrom bytes", fromBuilder, fromBytes)
		}

		buffer := tafla.NewByteBuffer(fromBytes, layout)
		position := must(buffer.RootOffset(1))
		if position%8 != 0 || !bytes.Equal(fromBytes[position:position+len(built)], built) {
			t.Fatalf("the table at %d holds % x", position, fromBytes[position:])
		}

		vector := must(buffer.GetVector(must(buffer.RootOffset(2))))
		for item, err := range vector.Items() {
			if err != nil || !bytes.Equal(item, built) {
				t.Fatalf("read % x, %v", item, err)
			}
		}
	})
}

// parentOf writes a parent of before bytes in a uint8[], the table at
// alignment 8, and a vector of the table twice
func parentOf(table func() tafla.NestedTable, before int, layout tafla.TableLayout) ([]byte, error) {
	parent, err := tafla.NewTableBuilder(3, layout)
	if err != nil {
		return nil, err
	}

	err = errors.Join(
		parent.SetUint8Array(make([]byte, before)),
		parent.SetTableAligned(table(), tafla.Align8),
		parent.SetVector(table(), table()),
	)
	if err != nil {
		return nil, err
	}

	return parent.Bytes()
}

// longestParent returns the most bytes that parentOf writes for a table of
// length bytes, with offsetType offsets
func longestParent(offsetType tafla.OffsetType, before, length int) int {
	width := offsetType.Size()
	offsets := 3 * width
	array := width + before
	table := (8 - 1) + length
	vector := 3*width + 2*length

	return offsets + array + table + vector
}

// tablesOfShortOffsets generates tables of uint8 and uint16 offsets, whose
// limits a test can reach
func tablesOfShortOffsets() *rapid.Generator[tables.Table] {
	return tables.TablesOf(tables.LayoutsOf(rapid.SampledFrom([]tafla.OffsetType{tafla.Uint8, tafla.Uint16})))
}

func withField(table tables.Table, field tables.Field) tables.Table {
	table.Fields = append(append([]tables.Field(nil), table.Fields...), field)
	return table
}

// longField is a field whose byte length grows by a unit with each of its
// units
type longField uint8

const (
	longBytes longField = iota
	longFloats
	longString
	// An array of structs of a uint16 and a uint8, of 3 bytes aligned to 2
	longStructs
	// A vector of empty tables, each an end of its own
	longEmptyItems
	// A vector of a table of bytes, from its builder
	longItem
	// A table of bytes, nested from its bytes
	longNestedBytes
	// A table of bytes, nested from its builder
	longNestedBuilder
)

var longFields = []longField{
	longBytes, longFloats, longString, longStructs, longEmptyItems, longItem, longNestedBytes, longNestedBuilder,
}

func (l longField) field(units int) tables.Field {
	bytesTable := func() []tables.Field { return []tables.Field{tables.Uint8Array(make([]byte, units))} }

	switch l {
	case longBytes:
		return tables.Uint8Array(make([]byte, units))
	case longFloats:
		return tables.Float64Array(make([]float64, units))
	case longString:
		return tables.String(strings.Repeat("a", units))
	case longStructs:
		items := make([][]tables.Field, units)
		for index := range items {
			items[index] = []tables.Field{tables.Uint16(1), tables.Uint8(2)}
		}
		return tables.StructArray{Types: []tables.ScalarType{tables.Uint16Type, tables.Uint8Type}, Items: items}
	case longEmptyItems:
		items := make(tables.Vector, units)
		for index := range items {
			items[index] = nestedOf(tables.FromBytes, nil)
		}
		return items
	case longItem:
		return tables.Vector{nestedOf(tables.FromBuilder, bytesTable())}
	case longNestedBytes:
		return tables.TableField{Nested: nestedOf(tables.FromBytes, bytesTable())}
	default:
		return tables.TableField{Nested: nestedOf(tables.FromBuilder, bytesTable())}
	}
}

// unit returns the bytes that each unit adds in a table of width-byte
// offsets
func (l longField) unit(width int) int {
	switch l {
	case longFloats:
		return 8
	case longStructs:
		return 3
	case longEmptyItems:
		return width
	default:
		return 1
	}
}

// nestedOf returns a table of fields with no header and uint32 offsets,
// which fits in any parent
func nestedOf(from tables.Source, fields []tables.Field) tables.Nested {
	return tables.Nested{From: from, Table: tables.Table{Layout: tables.Headerless(tafla.Uint32), Fields: fields}}
}

func TestRejectsAFieldBeyondTheCount(t *testing.T) {
	builder := must(tafla.NewTableBuilder(1, tafla.TableLayout{}))
	check(t, builder.SetUint8(1))
	table := must(builder.Bytes())

	err := builder.SetUint8(2)
	expectError(t, err, &tafla.BuildError{Kind: tafla.TooManyFields, Fields: 1})
	expectMessage(t, err, "tafla: can't add field 2 to a table of 1")
	if !bytes.Equal(must(builder.Bytes()), table) {
		t.Error("the rejected field changed the table")
	}
}

func TestRejectsWritingBeforeEveryFieldIsSet(t *testing.T) {
	for set := range 5 {
		builder := must(tafla.NewTableBuilder(set+1, tafla.TableLayout{}))
		for range set {
			check(t, builder.SetUint8(1))
		}

		_, err := builder.Bytes()
		expectError(t, err, &tafla.BuildError{Kind: tafla.Incomplete, Set: set, Fields: set + 1})
		expectMessage(t, err, fmt.Sprintf("tafla: only %d of the table's %d fields were set", set, set+1))
	}
}

func TestRejectsAFieldThatMakesTheTableTooLong(t *testing.T) {
	for _, offsetType := range []tafla.OffsetType{tafla.Uint8, tafla.Uint16} {
		layout := tafla.TableLayout{}.WithOffsetType(offsetType)
		width := offsetType.Size()
		maxLength := int(offsetType.MaxValue())

		// Two offsets, the array's length prefix and bytes, and then one byte
		fits := must(tafla.NewTableBuilder(2, layout))
		check(t, fits.SetUint8Array(make([]byte, maxLength-3*width-1)))
		check(t, fits.SetUint8(7))

		table := must(fits.Bytes())
		buffer := tafla.NewByteBuffer(table, layout)
		if len(table) != maxLength || must(buffer.GetUint8(must(buffer.RootOffset(1)))) != 7 {
			t.Errorf("wrote %d bytes", len(table))
		}

		tooLong := must(tafla.NewTableBuilder(2, layout))
		check(t, tooLong.SetUint8Array(make([]byte, maxLength-3*width)))

		err := tooLong.SetUint8(7)
		expectError(t, err, &tafla.BuildError{Kind: tafla.TooLong, Index: 1, Length: uint64(maxLength) + 1, OffsetType: offsetType})
		expectMessage(t, err, fmt.Sprintf("tafla: field 2 makes the table %d bytes, too long for %v offsets", maxLength+1, offsetType))
	}
}

func TestRejectsBytesThatArentWholeStructs(t *testing.T) {
	cases := []struct {
		length, size int
		message      string
	}{
		{5, 2, "tafla: 5 bytes aren't a whole number of 2-byte structs"},
		{4, 0, "tafla: a struct can't be 0 bytes"},
		{0, 0, "tafla: a struct can't be 0 bytes"},
	}

	for _, c := range cases {
		builder := must(tafla.NewTableBuilder(1, tafla.TableLayout{}))
		err := builder.SetStructArray(make([]byte, c.length), c.size, tafla.Align1)
		expectError(t, err, &tafla.BuildError{Kind: tafla.NotWholeStructs, Length: uint64(c.length), Size: c.size})
		expectMessage(t, err, c.message)

		// The builder was left as it was
		check(t, builder.SetStructArray([]byte{1, 2, 3, 4}, 2, tafla.Align2))
		if table := must(builder.Bytes()); !bytes.Equal(table, []byte{4, 0, 0, 0, 2, 0, 0, 0, 1, 2, 3, 4}) {
			t.Errorf("wrote % x", table)
		}
	}
}

func TestRejectsAHeaderTooLong(t *testing.T) {
	for _, offsetType := range []tafla.OffsetType{tafla.Uint8, tafla.Uint16} {
		width := offsetType.Size()

		for _, sized := range []bool{false, true} {
			layout := tafla.TableLayout{}.WithOffsetType(offsetType)
			fields := int(offsetType.MaxValue()) / width
			if sized {
				layout = layout.Sized()
				fields--
			}

			if _, err := tafla.NewTableBuilder(fields, layout); err != nil {
				t.Errorf("%v: %v", layout, err)
			}
			_, err := tafla.NewTableBuilder(fields+1, layout)
			expectError(t, err, &tafla.BuildError{Kind: tafla.HeaderTooLong, Fields: fields + 1, OffsetType: offsetType})
		}
	}

	_, err := tafla.NewTableBuilder(math.MaxInt, tafla.TableLayout{})
	expectError(t, err, &tafla.BuildError{Kind: tafla.HeaderTooLong, Fields: math.MaxInt, OffsetType: tafla.Uint32})
	expectPanic(t, "a negative number of fields", func() { _, _ = tafla.NewTableBuilder(-1, tafla.TableLayout{}) })
}

// A table of 4 GiB can't be allocated in a test, but a vector that nests a
// builder of 64 MiB 64 times is that long without allocating more
func TestRejectsATableOfUint32OffsetsPast4GiBBeforeWritingIt(t *testing.T) {
	nested := must(tafla.NewTableBuilder(1, tafla.TableLayout{}))
	check(t, nested.SetUint8Array(make([]byte, 1<<26)))

	items := make([]tafla.NestedTable, 64)
	for index := range items {
		items[index] = nested
	}

	builder := must(tafla.NewTableBuilder(1, tafla.TableLayout{}))
	var buildError *tafla.BuildError
	if err := builder.SetVector(items...); !errors.As(err, &buildError) || buildError.Kind != tafla.TooLong ||
		buildError.Index != 0 || buildError.Length <= 1<<32 {
		t.Errorf("%#v", err)
	}

	// A 32-bit int doesn't hold the length of 63, which is too long there
	err := builder.SetVector(items[:63]...)
	if strconv.IntSize == 64 {
		check(t, err)
	} else if !errors.As(err, &buildError) || buildError.Kind != tafla.TooLong {
		t.Errorf("%#v", err)
	}
}

func TestRejectsAVersionTooLargeForItsSlot(t *testing.T) {
	cases := []struct {
		offsetType tafla.OffsetType
		version    uint32
	}{{tafla.Uint8, 0x100}, {tafla.Uint16, 0x1_0000}}

	for _, c := range cases {
		layout := tafla.TableLayout{}.WithOffsetType(c.offsetType).Versioned()

		_, err := tafla.NewVersionedTableBuilder(1, layout, c.version)
		expectError(t, err, &tafla.BuildError{Kind: tafla.VersionTooLarge, Version: c.version, OffsetType: c.offsetType})
		expectMessage(t, err, fmt.Sprintf("tafla: version %d doesn't fit in a %v slot", c.version, c.offsetType))
		if _, err := tafla.NewVersionedTableBuilder(1, layout, c.version-1); err != nil {
			t.Error(err)
		}
	}
}

func TestPanicsForAVersionThatTheLayoutDoesntHave(t *testing.T) {
	expectPanic(t, "a versioned layout without a version", func() {
		_, _ = tafla.NewTableBuilder(1, tafla.TableLayout{}.Versioned())
	})
	expectPanic(t, "a version without a slot", func() {
		_, _ = tafla.NewVersionedTableBuilder(1, tafla.TableLayout{}, 1)
	})
}

func TestRejectsAnIncompleteNestedBuilder(t *testing.T) {
	incomplete := must(tafla.NewTableBuilder(2, tafla.TableLayout{}))
	check(t, incomplete.SetUint8(1))

	parent := must(tafla.NewTableBuilder(2, tafla.TableLayout{}))
	expected := &tafla.BuildError{Kind: tafla.Incomplete, Set: 1, Fields: 2}
	expectError(t, parent.SetTable(incomplete), expected)
	expectError(t, parent.SetVector(tafla.TableBytes{0}, incomplete), expected)

	// The parent was left as it was
	nested := must(tafla.NewTableBuilder(2, tafla.TableLayout{}))
	check(t, nested.SetUint8(1))
	check(t, nested.SetUint8(2))
	check(t, parent.SetTable(nested))
	check(t, parent.SetVector(nested))

	table := must(parent.Bytes())
	buffer := tafla.NewByteBuffer(table, tafla.TableLayout{})
	position := must(buffer.RootOffset(0))
	if position != 8 || must(buffer.GetUint8(must(buffer.FieldOffset(position, 1)))) != 2 {
		t.Errorf("read the nested table at %d", position)
	}

	vector := must(buffer.GetVector(must(buffer.RootOffset(1))))
	if item := must(vector.Get(0)); vector.Len() != 1 || !bytes.Equal(item, must(nested.Bytes())) {
		t.Errorf("read % x", item)
	}
}

func TestPanicsForANilOrInvalidNestedTableOrAlignment(t *testing.T) {
	builder := must(tafla.NewTableBuilder(1, tafla.TableLayout{}))
	var nilBuilder *tafla.TableBuilder

	expectPanic(t, "a nil table", func() { _ = builder.SetTable(nil) })
	expectPanic(t, "a nil builder", func() { _ = builder.SetTable(nilBuilder) })
	expectPanic(t, "a nil item", func() { _ = builder.SetVector(nil) })
	expectPanic(t, "an alignment of 3", func() { _ = builder.SetTableAligned(tafla.TableBytes{}, 3) })
	expectPanic(t, "an alignment of 0", func() { _ = builder.SetStructArray(nil, 1, 0) })
	expectPanic(t, "an offset type of 3", func() { tafla.TableLayout{}.WithOffsetType(3) })
}

// The builder copies the slice of a vector's items, so that it writes the
// items that it was given
func TestWritesTheItemsOfAVectorThatItWasGiven(t *testing.T) {
	items := []tafla.NestedTable{tafla.TableBytes{1}}
	builder := must(tafla.NewTableBuilder(1, tafla.TableLayout{}))
	check(t, builder.SetVector(items...))
	items[0] = tafla.TableBytes{2, 3, 4}

	if table := must(builder.Bytes()); !bytes.Equal(table, []byte{4, 0, 0, 0, 1, 0, 0, 0, 9, 0, 0, 0, 1}) {
		t.Errorf("wrote % x", table)
	}
}

func check2(t *rapid.T, err error) {
	if err != nil {
		t.Fatal(err)
	}
}

func equalErrors(err error, expected error) bool {
	return sameError(err, expected)
}
