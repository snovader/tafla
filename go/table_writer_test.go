package tafla_test

// A TableSizer counts each table as TableBuilder counts it, padding
// included, and its TableWriter writes the same bytes from the tests' calls
// as from the builder's, which writes its tables through a TableWriter too,
// with nested tables written in place. They fail as the builder fails: for a
// table too long for its offsets, a header too long and a version too large.
// A writer given other fields than the sizer added fails, or panics for a
// write past the table, and never writes past it.
//
// The bytes themselves are checked against the format's rules in
// format_test.go, against the conformance suite, and against hand-written
// bytes here, since the builder that those tests build with writes through a
// TableWriter.

import (
	"bytes"
	"errors"
	"math"
	"runtime"
	"slices"
	"strings"
	"testing"

	tafla "github.com/snovader/tafla/go"
	"github.com/snovader/tafla/go/internal/tables"
	"pgregory.net/rapid"
)

func TestSizesAnyTableAsTableBuilderCountsItAndWritesTheSameBytes(t *testing.T) {
	rapid.Check(t, func(t *rapid.T) {
		table := inPlaceTables(tables.Layouts()).Draw(t, "table")
		built, err := table.Bytes()
		if err != nil {
			t.Skip("the table doesn't fit")
		}

		if written, err := table.Write(); err != nil || !bytes.Equal(written, built) {
			t.Fatalf("wrote % x, %v\nrather than % x", written, err, built)
		}

		// After other bytes, which the writer leaves as they are, and into
		// bytes that held others, which it overwrites, padding too
		dst := bytes.Repeat([]byte{0xa5}, 3+len(built)+5)[:3]
		appended, err := table.WriteAs(table, dst)
		if err != nil || !bytes.Equal(appended, slices.Concat(dst, built)) || &appended[0] != &dst[0] {
			t.Fatalf("appended % x, %v", appended, err)
		}
	})
}

// TableBuilder reports the error of a nested table too long for its
// offsets, which the sizer counts as part of the root, so only the errors'
// kinds are the same, and the field of the root that fails
func TestFailsAsTableBuilderFailsForAnyTableTooLongForItsOffsets(t *testing.T) {
	rapid.Check(t, func(t *rapid.T) {
		longer := inPlaceTables(shortLayouts()).Draw(t, "table")
		length := rapid.IntRange(0, 300).Draw(t, "length")
		longer.Fields = append(longer.Fields, tables.Uint8Array(make([]byte, length)))

		written, writeErr := longer.Write()
		built, buildErr := longer.Bytes()
		switch {
		case writeErr == nil && buildErr == nil:
			if !bytes.Equal(written, built) {
				t.Fatalf("wrote % x\nrather than % x", written, built)
			}
		case writeErr != nil && buildErr != nil:
			var writeError, buildError *tafla.BuildError
			if !errors.As(writeErr, &writeError) || !errors.As(buildErr, &buildError) || writeError.Kind != buildError.Kind {
				t.Fatalf("%v rather than %v", writeErr, buildErr)
			}
			if writeError.Kind == tafla.TooLong && writeError.Index != failingField(longer) {
				t.Fatalf("%v names field %d rather than %d", writeErr, writeError.Index, failingField(longer))
			}
		default:
			t.Fatalf("%v rather than %v", writeErr, buildErr)
		}
	})
}

// The fields' lengths don't depend on where they start after the bytes, so
// that the bytes can make the table exactly as long as its offsets address
func TestWritesATableAsLongAsItsOffsetsAddressAndFailsForOneAByteLonger(t *testing.T) {
	rapid.Check(t, func(t *rapid.T) {
		table := inPlaceTables(shortLayouts()).Draw(t, "table")
		last := unpaddedField(t)

		offsetType := table.Layout.OffsetType
		maxLength := int(offsetType.MaxValue())
		withBytes := func(length int) tables.Table {
			fields := append(slices.Clone(table.Fields), tables.Uint8Array(make([]byte, length)), last)
			return tables.Table{Layout: table.Layout, Version: table.Version, Fields: fields}.NestedInPlace()
		}

		shortest, err := withBytes(0).Bytes()
		if err != nil {
			t.Skip("the table doesn't fit")
		}
		room := maxLength - len(shortest)

		longest := withBytes(room)
		written := must(longest.Write())
		if len(written) != maxLength || !bytes.Equal(written, must(longest.Bytes())) {
			t.Fatalf("wrote %d bytes", len(written))
		}

		expected := &tafla.BuildError{Kind: tafla.TooLong, Index: len(table.Fields) + 1, Length: uint64(maxLength) + 1, OffsetType: offsetType}
		tooLong := withBytes(room + 1)
		if _, err := tooLong.Write(); !equalErrors(err, expected) {
			t.Fatalf("%#v", err)
		}
		if _, err := tooLong.Bytes(); !equalErrors(err, expected) {
			t.Fatalf("%#v", err)
		}
	})
}

// A writer given a field of another length than the sizer added fails, or
// panics for a write past the table, without writing past it, and one given
// a field of the same length writes it as TableBuilder does, since the bytes
// come from the fields written
func TestAWriterOfOtherFieldsThanThoseSizedFailsOrWritesThemAsTableBuilderDoes(t *testing.T) {
	rapid.Check(t, func(t *rapid.T) {
		table := inPlaceTables(tables.Layouts()).Draw(t, "table")
		replacements := tables.Tables().Draw(t, "replacements")
		if len(table.Fields) == 0 || len(replacements.Fields) == 0 {
			t.Skip("no fields to replace")
		}
		built, err := table.Bytes()
		if err != nil {
			t.Skip("the table doesn't fit")
		}

		at := rapid.IntRange(0, len(table.Fields)-1).Draw(t, "at")
		with := rapid.IntRange(0, len(replacements.Fields)-1).Draw(t, "with")
		written := tables.Table{Layout: table.Layout, Version: table.Version, Fields: slices.Clone(table.Fields)}
		written.Fields[at] = replacements.Fields[with]
		written = written.NestedInPlace()

		writtenBytes, err, panicked := writeWithin(table, written, len(built))
		var buildError *tafla.BuildError
		switch {
		case panicked:
		case err == nil:
			if !bytes.Equal(writtenBytes, must(written.Bytes())) {
				t.Fatalf("wrote % x\nrather than % x", writtenBytes, must(written.Bytes()))
			}
		case !errors.As(err, &buildError) || buildError.Kind != tafla.NotAsSized || buildError.Length != uint64(len(built)):
			t.Fatalf("%#v", err)
		}
	})
}

// Calls in any order, with any indices, counts and items, write a table,
// fail with an error or panic for a write past the table, and never write
// past it
func TestNoCallsMakeAWriterWritePastTheTable(t *testing.T) {
	rapid.Check(t, func(t *rapid.T) {
		table := tables.Tables().Draw(t, "table")
		sizer, err := sized(table)
		if err != nil {
			t.Skip("the table doesn't fit")
		}
		length, err := sizer.ByteLength()
		if err != nil {
			t.Skip("the table doesn't fit")
		}
		calls := rapid.SliceOfN(rapid.Custom(drawCall), 0, 24).Draw(t, "calls")

		backing := bytes.Repeat([]byte{0xa5}, length+16)
		writer := must(sizer.AppendWriter(backing[:0]))
		written, err, panicked := catchWritePastTheTable(func() ([]byte, error) {
			var parents []tafla.WriterParent
			var vectors []tafla.VectorPosition
			for _, call := range calls {
				call(&writer, &parents, &vectors)
			}

			return writer.Bytes()
		})

		if !bytes.Equal(backing[length:], bytes.Repeat([]byte{0xa5}, 16)) {
			t.Fatalf("wrote past the table: % x", backing[length:])
		}
		if !panicked && err == nil && len(written) != length {
			t.Fatalf("wrote %d bytes of a table of %d", len(written), length)
		}
	})
}

// writeWithin sizes table, and writes the fields of written into bytes with
// room after the table's length, which it checks aren't written. It reports
// whether the writer panicked for a write past the table.
func writeWithin(table, written tables.Table, length int) ([]byte, error, bool) {
	backing := bytes.Repeat([]byte{0xa5}, length+16)
	result, err, panicked := catchWritePastTheTable(func() ([]byte, error) {
		return table.WriteAs(written, backing[:0])
	})

	if !bytes.Equal(backing[length:], bytes.Repeat([]byte{0xa5}, 16)) {
		panic("the writer wrote past the table")
	}

	return result, err, panicked
}

// catchWritePastTheTable runs write, and reports whether it panicked with a
// runtime error, as for a write past the table. Any other panic isn't
// caught.
func catchWritePastTheTable(write func() ([]byte, error)) (written []byte, err error, panicked bool) {
	defer func() {
		if recovered := recover(); recovered != nil {
			if _, ok := recovered.(runtime.Error); !ok {
				panic(recovered)
			}
			panicked = true
		}
	}()

	written, err = write()
	return written, err, false
}

type call func(writer *tafla.TableWriter, parents *[]tafla.WriterParent, vectors *[]tafla.VectorPosition)

// drawCall draws a call of a writer, with any index, count or item
func drawCall(t *rapid.T) call {
	// Mostly small, so that they fit, and sometimes any
	small := func(label string) int {
		if rapid.IntRange(0, 4).Draw(t, label+"Kind") < 4 {
			return rapid.IntRange(0, 7).Draw(t, label)
		}
		return rapid.IntRange(0, math.MaxInt32).Draw(t, label)
	}
	alignment := rapid.SampledFrom([]tafla.Alignment{tafla.Align1, tafla.Align2, tafla.Align4, tafla.Align8})

	switch rapid.IntRange(0, 9).Draw(t, "call") {
	case 0:
		index := small("index")
		return func(w *tafla.TableWriter, _ *[]tafla.WriterParent, _ *[]tafla.VectorPosition) { w.SetUint8(index, 1) }
	case 1:
		index, count := small("index"), rapid.IntRange(0, 7).Draw(t, "count")
		return func(w *tafla.TableWriter, _ *[]tafla.WriterParent, _ *[]tafla.VectorPosition) {
			w.SetFloat64Array(index, make([]float64, count))
		}
	case 2:
		index, length := small("index"), rapid.IntRange(0, 7).Draw(t, "length")
		return func(w *tafla.TableWriter, _ *[]tafla.WriterParent, _ *[]tafla.VectorPosition) {
			w.SetString(index, strings.Repeat("🦀", length))
		}
	case 3:
		index, length, size := small("index"), rapid.IntRange(0, 11).Draw(t, "length"), rapid.IntRange(0, 3).Draw(t, "size")
		return func(w *tafla.TableWriter, _ *[]tafla.WriterParent, _ *[]tafla.VectorPosition) {
			w.SetStructArray(index, bytes.Repeat([]byte{1}, length), size, tafla.Align2)
		}
	case 4:
		index, count := small("index"), rapid.IntRange(0, 7).Draw(t, "count")
		return func(w *tafla.TableWriter, _ *[]tafla.WriterParent, _ *[]tafla.VectorPosition) {
			structs := w.ReserveStructArray(index, count, 2, tafla.Align2)
			for position := range structs {
				structs[position] = 0xff
			}
		}
	case 5:
		index, alignment := small("index"), alignment.Draw(t, "alignment")
		return func(w *tafla.TableWriter, _ *[]tafla.WriterParent, _ *[]tafla.VectorPosition) {
			w.SetTable(index, alignment)
		}
	case 6:
		fields := small("fields")
		return func(w *tafla.TableWriter, parents *[]tafla.WriterParent, _ *[]tafla.VectorPosition) {
			*parents = append(*parents, w.StartTable(fields))
		}
	case 7:
		return func(w *tafla.TableWriter, parents *[]tafla.WriterParent, _ *[]tafla.VectorPosition) {
			if len(*parents) > 0 {
				w.EndTable((*parents)[len(*parents)-1])
				*parents = (*parents)[:len(*parents)-1]
			}
		}
	case 8:
		index, count := small("index"), small("count")
		return func(w *tafla.TableWriter, _ *[]tafla.WriterParent, vectors *[]tafla.VectorPosition) {
			*vectors = append(*vectors, w.SetVector(index, count))
		}
	default:
		item := small("item")
		return func(w *tafla.TableWriter, _ *[]tafla.WriterParent, vectors *[]tafla.VectorPosition) {
			if len(*vectors) > 0 {
				w.SetItemEnd((*vectors)[len(*vectors)-1], item)
			}
		}
	}
}

// inPlaceTables generates tables of the layouts that layouts generates,
// whose nested tables a TableWriter writes in place
func inPlaceTables(layouts *rapid.Generator[tables.Layout]) *rapid.Generator[tables.Table] {
	return rapid.Map(tables.TablesOf(layouts), tables.Table.NestedInPlace)
}

// shortLayouts generates layouts of uint8 and uint16 offsets, whose limits a
// test can reach
func shortLayouts() *rapid.Generator[tables.Layout] {
	return tables.LayoutsOf(rapid.SampledFrom([]tafla.OffsetType{tafla.Uint8, tafla.Uint16}))
}

// unpaddedField draws a field after which a table's length doesn't depend
// on where it starts: one that isn't padded, from a table of uint8 offsets,
// which holds few enough values to fit after others more often than not
func unpaddedField(t *rapid.T) tables.Field {
	for {
		scalar := rapid.SampledFrom(tables.ScalarTypes).Draw(t, "scalar")
		field := tables.DrawScalar(t, scalar)
		if rapid.Bool().Draw(t, "string") {
			field = tables.String(strings.Repeat("a", rapid.IntRange(0, 6).Draw(t, "length")))
		}
		if tables.FieldAlignment(field) == tafla.Align1 {
			return field
		}
	}
}

// failingField returns the index of the root field that TableBuilder fails
// to set, whether the root or a table nested in that field is too long
func failingField(table tables.Table) int {
	builder, err := table.Builder()
	if err != nil {
		return -1
	}

	for index, field := range table.Fields {
		if tables.Set(builder, field) != nil {
			return index
		}
	}

	return -1
}

// sized returns a sizer of the table with every field added
func sized(table tables.Table) (tafla.TableSizer, error) {
	sizer, err := table.Sizer()
	if err != nil {
		return sizer, err
	}

	for _, field := range table.Fields {
		tables.Add(&sizer, field)
	}

	return sizer, nil
}

func TestPadsAStructArrayANestedTableAndAVectorAsTableBuilderCountsTheirPadding(t *testing.T) {
	// A uint8 leaves the uint32[] 3 bytes short of its alignment, the array
	// leaves the structs 2 bytes short of theirs, and the structs leave the
	// nested table of float64s 6 bytes short
	nested := func(fields ...tables.Field) tables.Nested {
		return tables.Nested{From: tables.FromBuilder, Table: tables.Table{Layout: tables.Headerless(tafla.Uint16), Fields: fields}}
	}
	table := tables.Table{
		Layout:  tables.Layout{Sized: true, Versioned: true, OffsetType: tafla.Uint16},
		Version: 7,
		Fields: []tables.Field{
			tables.Uint8(1),
			tables.Uint32Array{2, 3},
			tables.StructArray{
				Types: []tables.ScalarType{tables.Uint16Type, tables.Float64Type},
				Items: [][]tables.Field{{tables.Uint16(4), tables.Float64(5)}},
			},
			tables.TableField{Nested: nested(tables.Float64Array{6})},
			tables.Vector{nested(tables.String("🦀"))},
		},
	}

	expected := []byte{
		// The length, the version and the offsets of the fields
		76, 0, 7, 0, 14, 0, 18, 0, 30, 0, 48, 0, 64, 0,
		// The uint8, the padding, the count and the uint32s
		1, 0, 0, 0, 2, 0, 2, 0, 0, 0, 3, 0, 0, 0,
		// The padding, the count and the struct of a uint16 and a float64
		0, 0, 1, 0, 4, 0, 0, 0, 0, 0, 0, 0, 0x14, 0x40,
		// The padding, and the nested table: its offset, its padding, the
		// count and the float64
		0, 0, 0, 0, 0, 0,
		6, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0x18, 0x40,
		// The vector's count and end, and its item: its offset, and the
		// string's length and bytes
		1, 0, 12, 0, 2, 0, 4, 0, 0xf0, 0x9f, 0xa6, 0x80,
	}

	if written := must(table.Write()); !bytes.Equal(written, expected) {
		t.Errorf("wrote % x", written)
	}
	if built := must(table.Bytes()); !bytes.Equal(built, expected) {
		t.Errorf("built % x", built)
	}
	if read, err := tables.ReadTable(expected, table); err != nil || !tables.Equal(read, table) {
		t.Errorf("read %+v, %v", read, err)
	}
}

func TestNamesTheRootFieldThatMakesTheTableTooLongWithTheLengthAfterIt(t *testing.T) {
	// A vector of three items of 102 bytes, which is too long for uint8
	// offsets after its second item, and makes the table 314 bytes
	item := tables.Nested{
		From:  tables.FromBuilder,
		Table: tables.Table{Layout: tables.Headerless(tafla.Uint8), Fields: []tables.Field{tables.Uint8Array(make([]byte, 100))}},
	}
	table := tables.Table{
		Layout: tables.Headerless(tafla.Uint8),
		Fields: []tables.Field{tables.Uint8(1), tables.Vector{item, item, item}, tables.Uint8(2)},
	}

	expected := &tafla.BuildError{Kind: tafla.TooLong, Index: 1, Length: 314, OffsetType: tafla.Uint8}
	expectError(t, second(table.Write()), expected)
	expectError(t, second(table.Bytes()), expected)
}

func TestRejectsATableOfUint32OffsetsPast4GiBBeforeAllocatingIt(t *testing.T) {
	// The table would hold its offset, its count and 2^30 uint32s
	sizer := must(tafla.NewTableSizer(1, tafla.TableLayout{}))
	sizer.AddUint32Array(1 << 30)

	expected := &tafla.BuildError{Kind: tafla.TooLong, Index: 0, Length: 4_294_967_304, OffsetType: tafla.Uint32}
	expectError(t, second(sizer.ByteLength()), expected)
	expectError(t, second(sizer.Writer()), expected)
	expectError(t, third(tafla.AppendTable(nil, 1, tafla.TableLayout{}, 4_294_967_304)), expected)
	expectMessage(t, expected, "tafla: field 1 makes the table 4294967304 bytes, too long for uint32 offsets")
}

func TestCountsLengthsPastAnyIntWithoutOverflowing(t *testing.T) {
	sizer := must(tafla.NewTableSizer(3, tafla.TableLayout{}))
	sizer.AddUint8()
	sizer.AddFloat64Array(math.MaxInt)
	sizer.AddVector(math.MaxInt)
	for range 3 {
		parent := sizer.StartTable(math.MaxInt)
		sizer.AddStructArray(math.MaxInt, math.MaxInt, tafla.Align8)
		sizer.AddTable(tafla.Align8)
		sizer.EndTable(parent)
	}

	var buildError *tafla.BuildError
	if _, err := sizer.Writer(); !errors.As(err, &buildError) || buildError.Kind != tafla.TooLong ||
		buildError.Index != 1 || buildError.OffsetType != tafla.Uint32 {
		t.Errorf("%#v", err)
	}
}

func TestRejectsAHeaderTooLongForItsOffsetsAsTableBuilderDoes(t *testing.T) {
	for _, offsetType := range []tafla.OffsetType{tafla.Uint8, tafla.Uint16} {
		width := offsetType.Size()

		for _, sized := range []bool{false, true} {
			layout := tafla.TableLayout{}.WithOffsetType(offsetType)
			fields := int(offsetType.MaxValue()) / width
			if sized {
				layout = layout.Sized()
				fields--
			}

			// The longest header, which is the whole table
			length := uint64(offsetType.MaxValue()) / uint64(width) * uint64(width)
			if _, err := tafla.NewTableSizer(fields, layout); err != nil {
				t.Error(err)
			}
			if _, _, err := tafla.AppendTable(nil, fields, layout, length); err != nil {
				t.Error(err)
			}
			tooLong := second(tafla.NewTableBuilder(fields+1, layout))
			expectError(t, second(tafla.NewTableSizer(fields+1, layout)), tooLong)
			expectError(t, third(tafla.AppendTable(nil, fields+1, layout, length)), tooLong)

			versioned := layout.Versioned()
			if _, err := tafla.NewVersionedTableSizer(fields-1, versioned, 0); err != nil {
				t.Error(err)
			}
			if _, _, err := tafla.AppendVersionedTable(nil, fields-1, versioned, 0, length); err != nil {
				t.Error(err)
			}
			tooLong = second(tafla.NewVersionedTableBuilder(fields, versioned, 0))
			expectError(t, second(tafla.NewVersionedTableSizer(fields, versioned, 0)), tooLong)
			expectError(t, third(tafla.AppendVersionedTable(nil, fields, versioned, 0, length)), tooLong)
		}
	}

	expectError(t, second(tafla.NewTableSizer(math.MaxInt, tafla.TableLayout{})),
		second(tafla.NewTableBuilder(math.MaxInt, tafla.TableLayout{})))
}

func TestRejectsAVersionTooLargeForItsSlotAsTableBuilderDoes(t *testing.T) {
	cases := []struct {
		offsetType tafla.OffsetType
		version    uint32
	}{{tafla.Uint8, 0x100}, {tafla.Uint16, 0x1_0000}}

	for _, c := range cases {
		layout := tafla.TableLayout{}.WithOffsetType(c.offsetType).Versioned()

		expected := &tafla.BuildError{Kind: tafla.VersionTooLarge, Version: c.version, OffsetType: c.offsetType}
		expectError(t, second(tafla.NewVersionedTableSizer(1, layout, c.version)), expected)
		expectError(t, third(tafla.AppendVersionedTable(nil, 1, layout, c.version, 3*uint64(c.offsetType.Size()))), expected)
		expectError(t, second(tafla.NewVersionedTableBuilder(1, layout, c.version)), expected)
		if _, err := tafla.NewVersionedTableSizer(1, layout, c.version-1); err != nil {
			t.Error(err)
		}
	}
}

func TestPanicsForAStringLongerThanTheOneSized(t *testing.T) {
	sizer := must(tafla.NewTableSizer(1, tafla.TableLayout{}))
	sizer.AddString("a")
	writer := must(sizer.Writer())

	expectPanic(t, "a longer string", func() { writer.SetString(0, "ab") })
}

func TestFailsForFieldsShorterThanThoseSized(t *testing.T) {
	sizer := must(tafla.NewTableSizer(1, tafla.TableLayout{}))
	sizer.AddString("ab")
	writer := must(sizer.Writer())
	writer.SetString(0, "a")

	_, err := writer.Bytes()
	expectError(t, err, &tafla.BuildError{Kind: tafla.NotAsSized, Written: 9, Length: 10})
	expectMessage(t, err, "tafla: the fields wrote 9 of the table's 10 bytes")
}

func TestPanicsForAnItemEndPastTheTable(t *testing.T) {
	sizer := must(tafla.NewTableSizer(1, tafla.TableLayout{}))
	sizer.AddVector(1)
	parent := sizer.StartTable(0)
	sizer.EndTable(parent)

	writer := must(sizer.Writer())
	vector := writer.SetVector(0, 1)
	writer.EndTable(writer.StartTable(0))

	// Item 1's end would follow item 0's, where the table ends
	expectPanic(t, "an item past the vector", func() { writer.SetItemEnd(vector, 1) })
}

func TestRejectsAnArrayOf0ByteStructsFromTheSizerAsTableBuilderDoes(t *testing.T) {
	sizer := must(tafla.NewTableSizer(1, tafla.TableLayout{}))
	sizer.AddStructArray(2, 0, tafla.Align1)

	builder := must(tafla.NewTableBuilder(1, tafla.TableLayout{}))
	expected := builder.SetStructArray(nil, 0, tafla.Align1)
	expectError(t, second(sizer.ByteLength()), expected)
	expectError(t, second(sizer.Writer()), expected)
}

func TestSizesAndWritesAStructOf0BytesAtTheNextFieldsOffset(t *testing.T) {
	sizer := must(tafla.NewTableSizer(2, tafla.TableLayout{}))
	sizer.AddStruct(0)
	sizer.AddUint8()
	writer := must(sizer.Writer())
	writer.SetStruct(0, nil)
	writer.SetUint8(1, 7)

	builder := must(tafla.NewTableBuilder(2, tafla.TableLayout{}))
	check(t, builder.SetStruct(nil))
	check(t, builder.SetUint8(7))

	// Both fields' offsets are 8, after the two offsets
	expected := []byte{8, 0, 0, 0, 8, 0, 0, 0, 7}
	if written := must(writer.Bytes()); !bytes.Equal(written, expected) {
		t.Errorf("wrote % x", written)
	}
	if built := must(builder.Bytes()); !bytes.Equal(built, expected) {
		t.Errorf("built % x", built)
	}
}

func TestWritesStructsIntoTheirReservedBytesAsFromAllTheirBytes(t *testing.T) {
	items := []struct {
		name  string
		value uint16
	}{{"a", 1}, {"b", 2}}

	sizer := must(tafla.NewTableSizer(1, tafla.TableLayout{}))
	sizer.AddStructArray(len(items), 2, tafla.Align2)

	fromItems := must(sizer.Writer())
	structs := fromItems.ReserveStructArray(0, len(items), 2, tafla.Align2)
	for index, item := range items {
		structs[2*index] = byte(item.value)
		structs[2*index+1] = byte(item.value >> 8)
	}

	fromBytes := must(sizer.Writer())
	fromBytes.SetStructArray(0, []byte{1, 0, 2, 0}, 2, tafla.Align2)

	if a, b := must(fromItems.Bytes()), must(fromBytes.Bytes()); !bytes.Equal(a, b) {
		t.Errorf("wrote % x rather than % x", a, b)
	}
}

func TestFailsForBytesThatArentWholeStructsAsTableBuilderDoes(t *testing.T) {
	for _, c := range []struct{ length, size int }{{5, 2}, {4, 0}, {0, 0}} {
		sizer := must(tafla.NewTableSizer(1, tafla.TableLayout{}))
		sizer.AddStructArray(2, 2, tafla.Align1)
		writer := must(sizer.Writer())
		writer.SetStructArray(0, make([]byte, c.length), c.size, tafla.Align1)

		builder := must(tafla.NewTableBuilder(1, tafla.TableLayout{}))
		expected := builder.SetStructArray(make([]byte, c.length), c.size, tafla.Align1)
		expectError(t, second(writer.Bytes()), expected)
	}
}

// Tables whose fields of fixed size the caller lays out itself, as the code
// that tafla-gen generates does, are written as TableBuilder writes them,
// every byte of them: a table's first such fields with AddFields and
// ReserveFields, nested tables of only such fields with AddTableBytes and
// ReserveTableBytes, and a table of only such fields, and a string after
// them, with AppendTable
func TestWritesTablesWhoseFixedFieldsTheCallerLaysOutAsTableBuilderDoes(t *testing.T) {
	rapid.Check(t, func(t *rapid.T) {
		table := inPlaceTables(tables.Layouts()).Draw(t, "table")
		switch rapid.IntRange(0, 3).Draw(t, "fields") {
		case 1:
			table = table.FixedFields()
		case 2:
			table = table.FixedFields()
			table.Fields = append(table.Fields, tables.String(tables.DrawString(t, 8)))
		case 3:
			// A table of fields of fixed size, nested in a field and as the
			// items of a vector, which are all of the same length
			fixed := table.FixedFields()
			fixed.Layout, fixed.Version = tables.Headerless(table.Layout.OffsetType), 0
			nested := tables.Nested{From: tables.FromBuilder, Table: fixed}
			items := slices.Repeat([]tables.Nested{nested}, rapid.IntRange(0, 4).Draw(t, "items"))
			table.Fields = slices.Concat([]tables.Field{tables.TableField{Nested: nested}, tables.Vector(items)}, table.Fields)
		}
		built, err := table.Bytes()
		if err != nil {
			t.Skip("the table doesn't fit")
		}

		if written, err := table.WriteLaidOut(nil); err != nil || !bytes.Equal(written, built) {
			t.Fatalf("wrote % x, %v\nrather than % x", written, err, built)
		}

		dst := bytes.Repeat([]byte{0xa5}, 3+len(built)+5)[:3]
		appended, err := table.WriteLaidOut(dst)
		if err != nil || !bytes.Equal(appended, slices.Concat(dst, built)) || &appended[0] != &dst[0] {
			t.Fatalf("appended % x, %v", appended, err)
		}
	})
}

// Only the string after a table's fields of fixed size can make it too long,
// so AppendTable names its last field, as TableBuilder does
func TestAppendsATableAsLongAsItsOffsetsAddressAndFailsForAStringOneByteLonger(t *testing.T) {
	rapid.Check(t, func(t *rapid.T) {
		table := inPlaceTables(shortLayouts()).Draw(t, "table").FixedFields()
		offsetType := table.Layout.OffsetType
		maxLength := int(offsetType.MaxValue())
		withString := func(length int) tables.Table {
			fields := append(table.Fields, tables.String(strings.Repeat("a", length)))
			return tables.Table{Layout: table.Layout, Version: table.Version, Fields: fields}
		}

		shortest, err := withString(0).Bytes()
		if err != nil {
			t.Skip("the table doesn't fit")
		}
		room := maxLength - len(shortest)

		longest := withString(room)
		written := must(longest.WriteLaidOut(nil))
		if len(written) != maxLength || !bytes.Equal(written, must(longest.Bytes())) {
			t.Fatalf("wrote %d bytes", len(written))
		}

		expected := &tafla.BuildError{Kind: tafla.TooLong, Index: len(table.Fields), Length: uint64(maxLength) + 1, OffsetType: offsetType}
		tooLong := withString(room + 1)
		if _, err := tooLong.WriteLaidOut(nil); !equalErrors(err, expected) {
			t.Fatalf("%#v", err)
		}
		if _, err := tooLong.Bytes(); !equalErrors(err, expected) {
			t.Fatalf("%#v", err)
		}
	})
}

func TestPanicsForALayoutWithoutTheVersionGivenOrANegativeNumberOfFields(t *testing.T) {
	plain, versioned := tafla.TableLayout{}, tafla.TableLayout{}.Versioned()

	expectPanic(t, "a sizer of a versioned layout", func() { _, _ = tafla.NewTableSizer(1, versioned) })
	expectPanic(t, "a versioned sizer of a layout without a version slot", func() {
		_, _ = tafla.NewVersionedTableSizer(1, plain, 0)
	})
	expectPanic(t, "a sizer of a negative number of fields", func() { _, _ = tafla.NewTableSizer(-1, plain) })

	expectPanic(t, "a table of a versioned layout", func() { _, _, _ = tafla.AppendTable(nil, 1, versioned, 8) })
	expectPanic(t, "a versioned table of a layout without a version slot", func() {
		_, _, _ = tafla.AppendVersionedTable(nil, 1, plain, 0, 4)
	})
	expectPanic(t, "a table of a negative number of fields", func() { _, _, _ = tafla.AppendTable(nil, -1, plain, 0) })
}

// third returns the error of a call that returns two values before it
func third[T, U any](_ T, _ U, err error) error {
	return err
}
