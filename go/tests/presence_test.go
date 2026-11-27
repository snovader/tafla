package tafla_test

// The properties of absent fields, whose offsets are 0 and which take no
// bytes, and of the fields that a reader of a later revision of a table
// finds missing. The tables are those of the conformance suite's model, with
// fields left out; conformance_test.go reads the checked-in cases of
// presence.

import (
	"bytes"
	"errors"
	"fmt"
	"slices"
	"testing"

	tafla "github.com/snovader/tafla/go"
	"github.com/snovader/tafla/go/tests/internal/tables"
	"pgregory.net/rapid"
)

// presenceTables generates tables of the layouts that layouts generates,
// with fields left out at every depth
func presenceTables(layouts *rapid.Generator[tables.Layout]) *rapid.Generator[tables.Table] {
	return rapid.Custom(func(t *rapid.T) tables.Table {
		table := tables.TablesOf(layouts).Draw(t, "table")
		absences := rapid.SliceOfN(rapid.Bool(), 1, 24).Draw(t, "absences")
		return tables.WithAbsences(table, absences)
	})
}

// revisedTables generates tables with fields left out whose first field is
// present, as a table that later revisions append fields to holds it
func revisedTables() *rapid.Generator[tables.Table] {
	return rapid.Map(presenceTables(tables.Layouts()), func(table tables.Table) tables.Table {
		// A uint8 takes no more room than the field that it replaces, or
		// than an empty table leaves
		switch {
		case len(table.Fields) == 0:
			table.Fields = []tables.Field{tables.Uint8(1)}
		case isAbsent(table.Fields[0]):
			table.Fields = slices.Clone(table.Fields)
			table.Fields[0] = tables.Uint8(1)
		}
		return table
	})
}

func isAbsent(field tables.Field) bool {
	_, absent := field.(tables.Absent)
	return absent
}

func TestTablesWithAbsentFieldsReadBackTheirValuesWhereverTheirBytesLie(t *testing.T) {
	rapid.Check(t, func(t *rapid.T) {
		table := presenceTables(tables.Layouts()).Draw(t, "table")
		byteOffset := rapid.IntRange(0, 7).Draw(t, "byteOffset")

		built := must(table.Bytes())
		read, err := tables.ReadTable(tables.AtByteOffset(built, byteOffset), table)
		if err != nil || !tables.Equal(read, table) {
			t.Fatalf("read %+v, %v\nrather than %+v", read, err, table)
		}
	})
}

func TestAnAbsentFieldHasTheOffset0AndEveryOtherFollowsTheOffsets(t *testing.T) {
	rapid.Check(t, func(t *rapid.T) {
		table := presenceTables(tables.Layouts()).Draw(t, "table")
		reader := tafla.NewTableReader(must(table.Bytes()), table.Layout.TableLayout())
		firstValue := table.Layout.HeaderSize() + len(table.Fields)*table.Layout.OffsetType.Size()

		for index, field := range table.Fields {
			offset := must(reader.RootOffset(index))
			if absent := isAbsent(field); absent && offset != 0 || !absent && offset < firstValue {
				t.Fatalf("field %d, %T, has the offset %d", index, field, offset)
			}
		}
	})
}

// The sizer and the writer write the bytes that TableBuilder writes, which
// a builder that records its nested tables in place writes too, in new bytes
// and in reused ones, which aren't zero, so the writer writes each absent
// field's offset, and so does a writer that lays out fixed fields
func TestTableWriterWritesAbsentFieldsAsTableBuilderDoes(t *testing.T) {
	rapid.Check(t, func(t *rapid.T) {
		table := presenceTables(tables.Layouts()).Draw(t, "table").NestedInPlace()
		built, err := table.Bytes()
		if err != nil {
			t.Skip("the table doesn't fit")
		}

		if written, err := table.Write(); err != nil || !bytes.Equal(written, built) {
			t.Fatalf("wrote % x, %v\nrather than % x", written, err, built)
		}
		if recorded, err := table.BytesInPlace(); err != nil || !bytes.Equal(recorded, built) {
			t.Fatalf("recorded % x, %v\nrather than % x", recorded, err, built)
		}

		reused := bytes.Repeat([]byte{0xff}, len(built)+8)[:0]
		if written, err := table.WriteAs(table, reused); err != nil || !bytes.Equal(written, built) {
			t.Fatalf("wrote % x, %v into reused bytes\nrather than % x", written, err, built)
		}
		reused = bytes.Repeat([]byte{0xff}, len(built)+8)[:0]
		if laidOut, err := table.WriteLaidOut(reused); err != nil || !bytes.Equal(laidOut, built) {
			t.Fatalf("laid out % x, %v\nrather than % x", laidOut, err, built)
		}
	})
}

// The sizer counts an absent field among the root's fields, so that it fails
// as TableBuilder fails for a table too long, naming the same field
func TestFailsAsTableBuilderFailsForATableWithAbsentFieldsTooLongForItsOffsets(t *testing.T) {
	rapid.Check(t, func(t *rapid.T) {
		longer := presenceTables(shortLayouts()).Draw(t, "table").NestedInPlace()
		length := rapid.IntRange(0, 300).Draw(t, "length")
		longer.Fields = append(longer.Fields, tables.Uint8Array(make([]byte, length)), tables.Absent{})

		written, writeErr := longer.Write()
		built, buildErr := longer.Bytes()
		var writeError, buildError *tafla.BuildError
		switch {
		case writeErr == nil && buildErr == nil:
			if !bytes.Equal(written, built) {
				t.Fatalf("wrote % x\nrather than % x", written, built)
			}
		case !errors.As(writeErr, &writeError) || !errors.As(buildErr, &buildError) || writeError.Kind != buildError.Kind:
			t.Fatalf("%v rather than %v", writeErr, buildErr)
		case writeError.Kind == tafla.TooLong && writeError.Index != failingField(longer):
			t.Fatalf("%v names field %d rather than %d", writeErr, writeError.Index, failingField(longer))
		}
	})
}

func TestNamesTheFieldThatMakesATableTooLongAfterAnAbsentOne(t *testing.T) {
	table := tables.Table{
		Layout: tables.Layout{OffsetType: tafla.Uint8},
		Fields: []tables.Field{tables.Absent{}, tables.Uint8Array(make([]byte, 300)), tables.Absent{}},
	}

	// The offsets, and then the array's count and bytes
	expected := &tafla.BuildError{Kind: tafla.TooLong, Index: 1, Length: 3 + 1 + 300, OffsetType: tafla.Uint8}
	if _, err := table.Write(); !equalErrors(err, expected) {
		t.Errorf("wrote %#v", err)
	}
	if _, err := table.Bytes(); !equalErrors(err, expected) {
		t.Errorf("built %#v", err)
	}
}

func TestAnAbsentFieldCountsAsAFieldOfItsTable(t *testing.T) {
	builder := must(tafla.NewTableBuilder(1, tafla.TableLayout{}))
	check(t, builder.SetAbsent())

	expected := &tafla.BuildError{Kind: tafla.TooManyFields, Fields: 1}
	if err := builder.SetAbsent(); !equalErrors(err, expected) {
		t.Errorf("%#v rather than %#v", err, expected)
	}
	if built := must(builder.Bytes()); !bytes.Equal(built, make([]byte, 4)) {
		t.Errorf("built % x", built)
	}

	incomplete := must(tafla.NewTableBuilder(2, tafla.TableLayout{}))
	check(t, incomplete.SetAbsent())
	expected = &tafla.BuildError{Kind: tafla.Incomplete, Set: 1, Fields: 2}
	if _, err := incomplete.Bytes(); !equalErrors(err, expected) {
		t.Errorf("%#v rather than %#v", err, expected)
	}

	// An item recorded in place sets one as the builder does
	parent := must(tafla.NewTableBuilder(1, tafla.TableLayout{}))
	check(t, parent.SetTableFunc(2, func(table tafla.Item) error {
		if err := table.SetAbsent(); err != nil {
			return err
		}
		return table.SetUint8(7)
	}))
	if built := must(parent.Bytes()); !bytes.Equal(built, []byte{4, 0, 0, 0, 0, 0, 0, 0, 8, 0, 0, 0, 7}) {
		t.Errorf("built % x", built)
	}
}

// A writer into bytes that held others writes 0 over the offset of an
// absent field, as SPEC.md's Writers asks
func TestWritesTheOffsetOfAnAbsentFieldOverReusedBytes(t *testing.T) {
	layout := tafla.TableLayout{}.WithOffsetType(tafla.Uint16)
	sizer := must(tafla.NewTableSizer(3, layout))
	sizer.AddUint8()
	sizer.AddAbsent()
	sizer.AddAbsent()

	reused := bytes.Repeat([]byte{0xa5}, 16)[:1]
	writer := must(sizer.AppendWriter(reused))
	writer.SetUint8(0, 9)
	writer.SetAbsent(1)
	writer.SetAbsent(2)
	if written := must(writer.Bytes()); !bytes.Equal(written, []byte{0xa5, 6, 0, 0, 0, 0, 0, 9}) {
		t.Errorf("wrote % x", written)
	}
}

// A reader that knows more fields than a table holds finds each of them
// missing, since their offsets would end after the first value starts, or
// lie in the padding before it
func TestFindsEveryFieldAfterTheTablesOwnMissing(t *testing.T) {
	rapid.Check(t, func(t *rapid.T) {
		table := revisedTables().Draw(t, "table")
		table.Missing = rapid.IntRange(1, 4).Draw(t, "missing")

		read, err := tables.ReadTable(must(table.Bytes()), table)
		if err != nil || !tables.Equal(read, table) {
			t.Fatalf("read %+v, %v\nrather than %+v", read, err, table)
		}
	})
}

// A reader of an earlier revision reads the fields that it knows from a
// table that holds more, and a later revision's reader of those fields finds
// only the absent ones missing
func TestReadsTheFieldsOfAnEarlierRevisionAndFindsOnlyTheAbsentOnesOfALaterOneMissing(t *testing.T) {
	rapid.Check(t, func(t *rapid.T) {
		table := revisedTables().Draw(t, "table")
		known := rapid.IntRange(1, len(table.Fields)).Draw(t, "known")
		later := table.Fields[known:]

		earlier := table
		earlier.Fields = table.Fields[:known]
		earlier.Missing = len(later)

		expected := earlier
		expected.Missing = 0
		for _, field := range later {
			if isAbsent(field) {
				expected.Missing++
			}
		}

		read, err := tables.ReadTable(must(table.Bytes()), earlier)
		if err != nil || !tables.Equal(read, expected) {
			t.Fatalf("read %+v, %v\nrather than %+v", read, err, expected)
		}
	})
}

// Overwritten or cut bytes read as values, absent and missing fields, or
// fail with a decode error, and read the same wherever they lie, within
// their own bytes
func TestCorruptedTablesWithAbsentAndMissingFieldsReadAsValuesOrDecodeErrors(t *testing.T) {
	rapid.Check(t, func(t *rapid.T) {
		table := revisedTables().Draw(t, "table")
		table.Missing = rapid.IntRange(0, 4).Draw(t, "missing")
		built := must(table.Bytes())

		overwrites := rapid.IntRange(1, 4).Draw(t, "overwrites")
		for range overwrites {
			built[rapid.IntRange(0, len(built)-1).Draw(t, "at")] = drawByte(t)
		}
		cut := built[:rapid.IntRange(0, len(built)).Draw(t, "cut")]
		byteOffset := rapid.IntRange(0, 7).Draw(t, "byteOffset")

		read, err := tables.ReadTable(slices.Clone(cut), table)
		checkReadError(t, err)
		elsewhere, errElsewhere := tables.ReadTable(tables.AtByteOffset(cut, byteOffset), table)
		if fmt.Sprint(err) != fmt.Sprint(errElsewhere) || err == nil && !tables.Equal(read, elsewhere) {
			t.Fatalf("read %+v, %v\nand elsewhere %+v, %v", read, err, elsewhere, errElsewhere)
		}
	})
}

func TestTheZeroOptionalIsAbsentAndSomeHoldsItsValue(t *testing.T) {
	if value, ok := (tafla.Optional[string]{}).Get(); ok || value != "" {
		t.Errorf("the zero Optional holds %q, %v", value, ok)
	}
	if value, ok := tafla.Some("").Get(); !ok || value != "" {
		t.Errorf("Some(\"\") holds %q, %v", value, ok)
	}

	// An empty slice is present, and keeps its elements' address
	empty := make([]float64, 0, 1)
	if values, ok := tafla.Some(empty).Get(); !ok || values == nil || cap(values) != 1 {
		t.Errorf("Some of an empty slice holds %v, %v", values, ok)
	}
	if values, ok := (tafla.Optional[[]float64]{}).Get(); ok || values != nil {
		t.Errorf("the zero Optional holds %v, %v", values, ok)
	}
}
