package generated_test

// The vectors of strings of all_types.tafla, whose getters return the
// runtime's StringVector and whose structs of fields hold []string. The
// tests of every message read them back, write them as TableBuilder does
// and read them corrupted; these document their views and their errors.

import (
	"bytes"
	"slices"
	"strings"
	"testing"

	tafla "github.com/snovader/tafla/go"
	alltypes "github.com/snovader/tafla/go/tests/generated/all_types"
)

func stringVectorsFields() alltypes.StringVectorsFields {
	return alltypes.StringVectorsFields{
		Id:     7,
		Tags:   []string{"first", "Zürich", "", "東京 🌍"},
		Labels: alltypes.LabelsFields{Values: []string{"label"}},
		Entries: []alltypes.EntryFields{
			{Key: "one", Aliases: []string{"uno", "ett"}},
			{Key: "two", Aliases: nil},
		},
	}
}

// The getters return views, which read each string from its ends, or every
// string at once, in a message, a nested table and the items of a vector
func TestReadsVectorsOfStringsThroughTheirViews(t *testing.T) {
	fields := stringVectorsFields()
	message := alltypes.NewStringVectors(must(fields.MarshalBinary()))

	tags := must(message.Tags())
	if tags.Len() != 4 || must(tags.Get(3)) != "東京 🌍" || !slices.Equal(must(tags.Strings()), fields.Tags) {
		t.Errorf("read %d tags, %q", tags.Len(), must(tags.Strings()))
	}

	labels := must(must(message.Labels()).Fields())
	if !slices.Equal(labels.Values, []string{"label"}) {
		t.Errorf("read the labels %q", labels.Values)
	}

	entries := must(message.Entries())
	for index, expected := range fields.Entries {
		entry := must(entries.Get(index))
		aliases := must(must(entry.Aliases()).Strings())
		if must(entry.Key()) != expected.Key || !slices.Equal(aliases, expected.Aliases) {
			t.Errorf("read entry %d as %q, %q", index, must(entry.Key()), aliases)
		}
	}
}

// Each string is checked on its own when it's read, so a tag of invalid
// UTF-8 fails when it's read, and the strings around it and the other fields
// read as they were written
func TestATagOfInvalidUTF8FailsThatTagOnly(t *testing.T) {
	fields := stringVectorsFields()
	written := must(fields.MarshalBinary())

	// "Zürich" is the only string with ü, whose first byte becomes a byte
	// that UTF-8 never holds
	at := bytes.Index(written, []byte("ü"))
	written[at] = 0xff
	message := alltypes.NewStringVectors(written)

	tags := must(message.Tags())
	expected := &tafla.DecodeError{Kind: tafla.InvalidUTF8, Position: at - 1}
	if must(tags.Get(0)) != "first" || must(tags.Get(2)) != "" || must(tags.Get(3)) != "東京 🌍" {
		t.Error("read other tags than were written")
	}
	if _, err := tags.Get(1); !sameError(err, expected) {
		t.Errorf("%v rather than %v", err, expected)
	}
	if values, err := tags.Strings(); values != nil || !sameError(err, expected) {
		t.Errorf("read %q, %v rather than %v", values, err, expected)
	}

	if must(message.Id()) != 7 || must(must(message.Labels()).Fields()).Values[0] != "label" {
		t.Error("read other fields than were written")
	}
	entry := must(must(message.Entries()).Get(0))
	if aliases := must(must(entry.Aliases()).Strings()); !slices.Equal(aliases, []string{"uno", "ett"}) {
		t.Errorf("read the aliases %q", aliases)
	}
}

// A message of uint16 offsets whose vector of strings makes it longer than
// they address fails as TableBuilder fails, naming the vector, with the
// length after it
func TestAVectorOfStringsTooLongForUint16OffsetsFailsAsTableBuilderFails(t *testing.T) {
	fields := alltypes.NarrowStringVectorsFields{Names: []string{"a", strings.Repeat("b", 65_530)}, Count: 2}

	// Two offsets, the count, two ends and the strings
	expected := &tafla.BuildError{Kind: tafla.TooLong, Index: 0, Length: 4 + 2 + 4 + 65_531, OffsetType: tafla.Uint16}
	if _, err := fields.MarshalBinary(); !sameError(err, expected) {
		t.Errorf("%v rather than %v", err, expected)
	}

	builder := must(tafla.NewTableBuilder(2, tafla.TableLayout{}.WithOffsetType(tafla.Uint16)))
	if err := builder.SetStringVector(fields.Names); !sameError(err, expected) {
		t.Errorf("TableBuilder failed with %v rather than %v", err, expected)
	}

	// A byte less than the longest that fits, with the uint8
	fields.Names[1] = strings.Repeat("b", 65_535-4-2-4-1-1-1)
	written := must(fields.MarshalBinary())
	read := must(alltypes.NewNarrowStringVectors(written).Fields())
	if len(written) != 65_534 || !slices.Equal(read.Names, fields.Names) || read.Count != 2 {
		t.Errorf("wrote %d bytes, which read %d names and %d", len(written), len(read.Names), read.Count)
	}
}
