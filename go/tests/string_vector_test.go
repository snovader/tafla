package tafla_test

// A vector of strings reads back every string, at once and each from its
// ends alone, in the bytes of a vector whose items are the strings' UTF-8.
// Reading every string reads what reading each string reads, and fails where
// any of them fails, whatever the bytes hold. Writers reject a vector that
// makes its table too long, as they reject any field. The examples document
// the errors.

import (
	"bytes"
	"encoding/binary"
	"errors"
	"slices"
	"strconv"
	"strings"
	"testing"
	"unsafe"

	tafla "github.com/snovader/tafla/go"
	"github.com/snovader/tafla/go/tests/internal/tables"
	"pgregory.net/rapid"
)

// tableOf returns a table of layout whose only field is a vector of values
func tableOf(values []string, layout tafla.TableLayout) ([]byte, error) {
	builder, err := tafla.NewTableBuilder(1, layout)
	if err != nil {
		return nil, err
	}
	if err := builder.SetStringVector(values); err != nil {
		return nil, err
	}

	return builder.Bytes()
}

// viewOf returns the vector of strings that is the only field of a table of
// layout
func viewOf(table []byte, layout tafla.TableLayout) (tafla.StringVector, error) {
	reader := tafla.NewTableReader(table, layout)
	position, err := reader.RootOffset(0)
	if err != nil {
		return tafla.StringVector{}, err
	}

	return reader.GetStringVector(position)
}

// stringVectors generates vectors of up to 8 strings of up to 16 characters,
// empty too, and of empty strings
func stringVectors() *rapid.Generator[[]string] {
	return rapid.Custom(func(t *rapid.T) []string {
		return tables.DrawStringVector(t, 8, 16)
	})
}

// eachString reads every string of a view from its ends alone, or returns
// the error of the first that fails
func eachString(view tafla.StringVector) ([]string, error) {
	values := make([]string, view.Len())
	for index := range values {
		value, err := view.Get(index)
		if err != nil {
			return nil, err
		}
		values[index] = value
	}

	return values, nil
}

func TestAVectorOfStringsReadsBackEveryStringAtOnceAndEachFromItsEnds(t *testing.T) {
	rapid.Check(t, func(t *rapid.T) {
		values := stringVectors().Draw(t, "values")
		byteOffset := rapid.IntRange(0, 7).Draw(t, "byteOffset")

		table := tables.AtByteOffset(must(tableOf(values, tafla.TableLayout{})), byteOffset)
		view := must(viewOf(table, tafla.TableLayout{}))

		if view.Len() != len(values) {
			t.Fatalf("a view of %d strings", view.Len())
		}
		if all, err := view.Strings(); err != nil || !slices.Equal(all, values) {
			t.Fatalf("read %q, %v", all, err)
		}
		if each, err := eachString(view); err != nil || !slices.Equal(each, values) {
			t.Fatalf("read %q, %v from their ends", each, err)
		}
	})
}

func TestAVectorOfStringsTakesTheBytesOfAVectorWhoseItemsAreTheStringsUTF8(t *testing.T) {
	rapid.Check(t, func(t *rapid.T) {
		values := stringVectors().Draw(t, "values")

		items := make([]tafla.NestedTable, len(values))
		for index, value := range values {
			items[index] = tafla.TableBytes(value)
		}
		builder := must(tafla.NewTableBuilder(1, tafla.TableLayout{}))
		check2(t, builder.SetVector(items...))

		if table := must(tableOf(values, tafla.TableLayout{})); !bytes.Equal(table, must(builder.Bytes())) {
			t.Fatalf("wrote % x\nrather than % x", table, must(builder.Bytes()))
		}
	})
}

// The table's one offset, the count and the ends, of 4 bytes each, and the
// strings' UTF-8
func TestAVectorOfStringsTakesTheEndOfEachStringAndACountBesideTheStringsUTF8(t *testing.T) {
	rapid.Check(t, func(t *rapid.T) {
		values := stringVectors().Draw(t, "values")

		utf8 := 0
		for _, value := range values {
			utf8 += len(value)
		}
		if table := must(tableOf(values, tafla.TableLayout{})); len(table) != 4+4+4*len(values)+utf8 {
			t.Fatalf("%d bytes for %d strings of %d bytes", len(table), len(values), utf8)
		}
	})
}

// In every offset type, and recorded in place in a nested table, through
// the Item that SetTableFunc gives
func TestASizerAndItsWriterWriteAVectorOfStringsAsABuilderDoes(t *testing.T) {
	rapid.Check(t, func(t *rapid.T) {
		values := tables.DrawStringVector(t, 6, 6)
		offsetType := rapid.SampledFrom(tables.OffsetTypes).Draw(t, "offsetType")
		layout := tafla.TableLayout{}.WithOffsetType(offsetType)

		built := must(tableOf(values, layout))
		sizer := must(tafla.NewTableSizer(1, layout))
		sizer.AddStringVector(values)
		writer := must(sizer.Writer())
		writer.SetStringVector(0, values)
		if written := must(writer.Bytes()); !bytes.Equal(written, built) {
			t.Fatalf("wrote % x\nrather than % x", written, built)
		}

		nested := must(tafla.NewTableBuilder(1, layout))
		check2(t, nested.SetStringVector(values))
		fromBuilder := must(tafla.NewTableBuilder(1, layout))
		check2(t, fromBuilder.SetTable(nested))
		inPlace := must(tafla.NewTableBuilder(1, layout))
		check2(t, inPlace.SetTableFunc(1, func(table tafla.Item) error {
			return table.SetStringVector(values)
		}))
		if recorded := must(inPlace.Bytes()); !bytes.Equal(recorded, must(fromBuilder.Bytes())) {
			t.Fatalf("recorded % x\nrather than % x", recorded, must(fromBuilder.Bytes()))
		}
	})
}

// Go's strings can hold any bytes, which the writers write as they are, as
// SetString does, and readers reject
func TestWritesAStringThatIsntUTF8AsItsBytesWhichFailsThatStringOnly(t *testing.T) {
	table := must(tableOf([]string{"a", "b\xff", "🌍"}, tafla.TableLayout{}))
	view := must(viewOf(table, tafla.TableLayout{}))

	// After the offset, the count, three ends and "a"
	expected := &tafla.DecodeError{Kind: tafla.InvalidUTF8, Position: 21}
	if value := must(view.Get(0)); value != "a" {
		t.Errorf("read %q", value)
	}
	if _, err := view.Get(1); !sameError(err, expected) {
		t.Errorf("%v rather than %v", err, expected)
	}
	if value := must(view.Get(2)); value != "🌍" {
		t.Errorf("read %q", value)
	}
	if values, err := view.Strings(); values != nil || !sameError(err, expected) {
		t.Errorf("read %q, %v rather than %v", values, err, expected)
	}
}

func TestReadsLongStringsOfEveryLengthOfCharacter(t *testing.T) {
	values := []string{strings.Repeat("a", 33), strings.Repeat("é", 40), strings.Repeat("東", 300), strings.Repeat("🌍", 4096)}
	view := must(viewOf(must(tableOf(values, tafla.TableLayout{})), tafla.TableLayout{}))

	if all := must(view.Strings()); !slices.Equal(all, values) {
		t.Errorf("read %q", all)
	}
	if value := must(view.Get(2)); value != values[2] {
		t.Errorf("read %q", value)
	}
}

// The strings are the table's bytes, as GetString returns a string, so they
// change when the bytes do
func TestReadsStringsThatShareTheBytes(t *testing.T) {
	table := must(tableOf([]string{"first", "", "second"}, tafla.TableLayout{}))
	view := must(viewOf(table, tafla.TableLayout{}))

	all := must(view.Strings())
	second := must(view.Get(2))
	// After the offset, the count, three ends and "first"
	if unsafe.StringData(all[0]) != &table[20] || unsafe.StringData(all[2]) != &table[25] ||
		unsafe.StringData(second) != &table[25] {
		t.Error("the strings are copies")
	}

	table[25] = 'S'
	if all[2] != "Second" || second != "Second" {
		t.Errorf("read %q and %q", all[2], second)
	}
}

func TestTheZeroStringVectorIsEmpty(t *testing.T) {
	var view tafla.StringVector

	if values, err := view.Strings(); view.Len() != 0 || len(values) != 0 || err != nil {
		t.Errorf("a view of %d strings reads %q, %v", view.Len(), values, err)
	}
	expectPanic(t, "string 0", func() { _, _ = view.Get(0) })
}

// asciiVectors generates vectors of one to 12 strings of up to 40
// characters of ASCII
func asciiVectors() *rapid.Generator[[]string] {
	return rapid.SliceOfN(rapid.StringOfN(rapid.Int32Range(0, 0x7f), 0, 40, -1), 1, 12)
}

// longVectors generates vectors of many strings, mostly ASCII, of the
// strings' indices padded to a length, with a few others among them
func longVectors() *rapid.Generator[[]string] {
	return rapid.Custom(func(t *rapid.T) []string {
		count := rapid.IntRange(100, 1500).Draw(t, "count")
		length := rapid.IntRange(1, 60).Draw(t, "length")
		values := make([]string, count)
		for index := range values {
			number := strconv.Itoa(index)
			values[index] = strings.Repeat("x", max(length-len(number), 0)) + number
		}

		others := rapid.IntRange(0, 8).Draw(t, "others")
		for range others {
			values[rapid.IntRange(0, count-1).Draw(t, "at")] = tables.DrawString(t, 16)
		}

		return values
	})
}

func TestReadingEveryStringReadsListsOfASCIIAndOthersAsEachStringOnItsOwn(t *testing.T) {
	rapid.Check(t, func(t *rapid.T) {
		values := rapid.OneOf(asciiVectors(), stringVectors(), longVectors()).Draw(t, "values")
		view := must(viewOf(must(tableOf(values, tafla.TableLayout{})), tafla.TableLayout{}))

		if all, err := view.Strings(); err != nil || !slices.Equal(all, values) {
			t.Fatalf("read %q, %v", all, err)
		}
	})
}

// Overwritten bytes, past the table's offset, so that the vector is found,
// and ends moved by a few bytes, which can split a character or lead before
// the end before them, read as a view or fail with the decode error, and the
// view reads every string as it reads each, or fails with the error of the
// first that fails
func TestReadingEveryStringReadsWhatReadingEachReadsAndFailsWhereAnyFails(t *testing.T) {
	rapid.Check(t, func(t *rapid.T) {
		values := rapid.OneOf(asciiVectors(), stringVectors(), longVectors()).Draw(t, "values")
		table := must(tableOf(values, tafla.TableLayout{}))
		overwrites := rapid.IntRange(0, 4).Draw(t, "overwrites")
		for range overwrites {
			table[rapid.IntRange(4, len(table)-1).Draw(t, "at")] = rapid.Byte().Draw(t, "byte")
		}
		moves := rapid.IntRange(0, min(len(values), 2)).Draw(t, "moves")
		for range moves {
			// The end of a string, after the table's offset and the count
			slot := 8 + 4*rapid.IntRange(0, len(values)-1).Draw(t, "string")
			end := int(binary.LittleEndian.Uint32(table[slot:])) + rapid.IntRange(-3, 3).Draw(t, "by")
			binary.LittleEndian.PutUint32(table[slot:], uint32(max(end, 0)))
		}

		view, err := viewOf(table, tafla.TableLayout{})
		var decodeError *tafla.DecodeError
		if err != nil {
			if !errors.As(err, &decodeError) {
				t.Fatalf("%v rather than a decode error", err)
			}
			return
		}

		each, eachErr := eachString(view)
		all, allErr := view.Strings()
		if !sameError(allErr, eachErr) || !slices.Equal(all, each) {
			t.Fatalf("read %q, %v rather than %q, %v", all, allErr, each, eachErr)
		}
		if eachErr != nil && !errors.As(eachErr, &decodeError) {
			t.Fatalf("%v rather than a decode error", eachErr)
		}
	})
}

// Each string is checked on its own, so an end within a character fails the
// strings before and after it, whose bytes together are valid UTF-8
func TestAnEndThatSplitsACharacterFailsTheStringsOnEitherSideAndReadsTheOthers(t *testing.T) {
	table := must(tableOf([]string{"a", "東", "京", "b"}, tafla.TableLayout{}))
	// The end of the second string, after the table's offset, the count and
	// the first end, is 24, within 東, at 25
	table[12]--
	view := must(viewOf(table, tafla.TableLayout{}))

	expected := &tafla.DecodeError{Kind: tafla.InvalidUTF8, Position: 25}
	if value := must(view.Get(0)); value != "a" {
		t.Errorf("read %q", value)
	}
	if _, err := view.Get(1); !sameError(err, expected) {
		t.Errorf("%v rather than %v", err, expected)
	}
	if _, err := view.Get(2); !sameError(err, &tafla.DecodeError{Kind: tafla.InvalidUTF8, Position: 27}) {
		t.Errorf("%v rather than an error at 27", err)
	}
	if value := must(view.Get(3)); value != "b" {
		t.Errorf("read %q", value)
	}
	if _, err := view.Strings(); !sameError(err, expected) {
		t.Errorf("%v rather than %v", err, expected)
	}
}

func TestNamesThePositionOfAStringThatIsntUTF8(t *testing.T) {
	for _, length := range []int{3, 40} {
		table := must(tableOf([]string{"a", strings.Repeat("b", length)}, tafla.TableLayout{}))
		// The second string starts after the offset, the count, two ends and
		// the first string
		table[17] = 0xff
		view := must(viewOf(table, tafla.TableLayout{}))

		_, err := view.Get(1)
		expectError(t, err, &tafla.DecodeError{Kind: tafla.InvalidUTF8, Position: 17})
		expectMessage(t, err, "tafla: the string at 17 isn't valid UTF-8")
		_, err = view.Strings()
		expectError(t, err, &tafla.DecodeError{Kind: tafla.InvalidUTF8, Position: 17})
	}
}

func TestRejectsAStringThatEndsBeforeItStarts(t *testing.T) {
	table := must(tableOf([]string{"ab", "c", "d"}, tafla.TableLayout{}))
	// The end of the second string, at 12, is that of the first, 18, less
	// one, so that it starts at 22 and ends at 21
	binary.LittleEndian.PutUint32(table[12:], 17)
	view := must(viewOf(table, tafla.TableLayout{}))

	expected := &tafla.DecodeError{Kind: tafla.ItemEndsBeforeStart, Position: 22, End: 21}
	_, err := view.Get(1)
	expectError(t, err, expected)
	_, err = view.Strings()
	expectError(t, err, expected)
	expectMessage(t, err, "tafla: the vector item at 22 ends before it starts, at 21")
}

func TestRejectsAStringThatEndsPastTheBytes(t *testing.T) {
	table := must(tableOf([]string{"ab", "c"}, tafla.TableLayout{}))
	// The end of the last string, from the vector's position, 4, is a byte
	// past the table's 19
	binary.LittleEndian.PutUint32(table[12:], 16)
	view := must(viewOf(table, tafla.TableLayout{}))

	expected := &tafla.DecodeError{Kind: tafla.OutOfRange, Position: 18, ByteLength: 2, Available: 19}
	_, err := view.Get(1)
	expectError(t, err, expected)
	_, err = view.Strings()
	expectError(t, err, expected)
}

// The ends of a count of 2^32 - 1 strings don't fit in the bytes, which
// fails before anything allocates for them
func TestRejectsAnImpossibleCountBeforeAllocatingForIt(t *testing.T) {
	reader := tafla.NewTableReader([]byte{255, 255, 255, 255}, tafla.TableLayout{})

	_, err := reader.GetStringVector(0)
	expectError(t, err, &tafla.DecodeError{Kind: tafla.OutOfRange, Position: 4, ByteLength: 4 * 0xffff_ffff, Available: 4})
}

func TestPanicsForAStringOutOfRangeWithTheIndexAndTheLength(t *testing.T) {
	view := must(viewOf(must(tableOf([]string{"a", "b"}, tafla.TableLayout{})), tafla.TableLayout{}))

	expectPanicMessage(t, "tafla: index 2 out of range of a vector of 2", func() { _, _ = view.Get(2) })
	expectPanicMessage(t, "tafla: index -1 out of range of a vector of 2", func() { _, _ = view.Get(-1) })
}

// A builder of two fields, a uint8 and a vector of one string, fills a table
// of uint8 or uint16 offsets, and fails for a byte more, leaving the builder
// as it was, as a sizer fails
func TestFillsATableOfShortOffsetsAndRejectsAByteMore(t *testing.T) {
	for _, offsetType := range []tafla.OffsetType{tafla.Uint8, tafla.Uint16} {
		layout := tafla.TableLayout{}.WithOffsetType(offsetType)
		maxLength := int(offsetType.MaxValue())
		// Two offsets, the uint8, and the vector's count and its one end
		room := maxLength - 4*offsetType.Size() - 1
		tooLong := []string{strings.Repeat("a", room+1)}
		longest := []string{strings.Repeat("a", room)}

		builder := must(tafla.NewTableBuilder(2, layout))
		check(t, builder.SetUint8(7))
		expected := &tafla.BuildError{Kind: tafla.TooLong, Index: 1, Length: uint64(maxLength + 1), OffsetType: offsetType}
		expectError(t, builder.SetStringVector(tooLong), expected)

		sizer := must(tafla.NewTableSizer(2, layout))
		sizer.AddUint8()
		sizer.AddStringVector(tooLong)
		expectError(t, second(sizer.ByteLength()), expected)

		check(t, builder.SetStringVector(longest))
		table := must(builder.Bytes())
		reader := tafla.NewTableReader(table, layout)
		if len(table) != maxLength || must(reader.GetUint8(must(reader.RootOffset(0)))) != 7 {
			t.Errorf("%v: wrote % x", offsetType, table)
		}
		values := must(must(reader.GetStringVector(must(reader.RootOffset(1)))).Strings())
		if !slices.Equal(values, longest) {
			t.Errorf("%v: read %q", offsetType, values)
		}
	}
}

// A writer given longer strings than its sizer counted panics for a write
// past the table, without writing past it, and given shorter ones fails
func TestAWriterOfOtherStringsThanThoseSizedPanicsOrFails(t *testing.T) {
	sizer := must(tafla.NewTableSizer(1, tafla.TableLayout{}))
	sizer.AddStringVector([]string{"a", "b"})

	for _, values := range [][]string{{"a", "bc"}, {"a", "b", ""}} {
		backing := bytes.Repeat([]byte{0xa5}, 18+8)
		writer := must(sizer.AppendWriter(backing[:0]))
		expectPanic(t, "a write past the table", func() { writer.SetStringVector(0, values) })
		if !bytes.Equal(backing[18:], bytes.Repeat([]byte{0xa5}, 8)) {
			t.Errorf("wrote past the table: % x", backing[18:])
		}
	}

	sizer = must(tafla.NewTableSizer(1, tafla.TableLayout{}))
	sizer.AddStringVector([]string{"ab"})
	writer := must(sizer.Writer())
	writer.SetStringVector(0, []string{"a"})
	_, err := writer.Bytes()
	expectError(t, err, &tafla.BuildError{Kind: tafla.NotAsSized, Written: 13, Length: 14})
	expectMessage(t, err, "tafla: the fields wrote 13 of the table's 14 bytes")
}
