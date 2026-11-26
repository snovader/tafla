package generated_test

// The code generated from typescript/core/test/presence.tafla: optional
// fields, which its writers leave out with an offset of 0, and fields that a
// later version of a message or a later revision of a table appends, which
// its readers find missing from the tables that don't hold them. A scalar's
// getter returns its default for a field that the table doesn't hold, and
// its Has<Field>() tells whether it does; any other getter returns false
// after the zero value.
//
// The tests read a record through its getters into the conformance suite's
// description of its table, where a field that the reader finds missing is
// absent, so that it compares with the description of the fields that wrote
// it, which TableBuilder builds.

import (
	"bytes"
	"encoding/binary"
	"errors"
	"fmt"
	"math"
	"slices"
	"testing"

	tafla "github.com/snovader/tafla/go"
	"github.com/snovader/tafla/go/tests/generated/presence"
	"github.com/snovader/tafla/go/tests/internal/tables"
	"pgregory.net/rapid"
)

// The defaults that the schema gives the scalars that may be missing
const (
	flagDefault   = true
	totalDefault  = -1
	zDefault      = -0.5
	weightDefault = float32(0.1)
	countDefault  = 7
)

// absent is the description of a field that a table doesn't hold
var absent = tables.Absent{}

func drawOptional[T any](t *rapid.T, label string, draw func(*rapid.T) T) tafla.Optional[T] {
	// Absent as often as not
	if rapid.Bool().Draw(t, label) {
		return tafla.Optional[T]{}
	}

	return tafla.Some(draw(t))
}

func float64s(t *rapid.T) float64 {
	return float64(tables.DrawScalar(t, tables.Float64Type).(tables.Float64))
}

func float32s(t *rapid.T) float32 {
	return float32(tables.DrawScalar(t, tables.Float32Type).(tables.Float32))
}

func text(t *rapid.T) string {
	return tables.DrawString(t, 8)
}

func drawPoint(t *rapid.T) presence.PointFields {
	return presence.PointFields{X: float64s(t), Y: float64s(t), Z: float64s(t)}
}

func drawEntry(t *rapid.T) presence.EntryFields {
	return presence.EntryFields{
		Key:    rapid.Uint16().Draw(t, "key"),
		Note:   drawOptional(t, "note", text),
		Weight: float32s(t),
	}
}

func drawRecord(t *rapid.T) presence.RecordFields {
	return presence.RecordFields{
		Id:   rapid.Uint32().Draw(t, "id"),
		Name: drawOptional(t, "name", text),
		Values: drawOptional(t, "values", func(t *rapid.T) []float64 {
			return rapid.SliceOfN(rapid.Custom(float64s), 0, 4).Draw(t, "values")
		}),
		Origin: drawOptional(t, "origin", drawPoint),
		Flag: drawOptional(t, "flag", func(t *rapid.T) bool {
			return rapid.Bool().Draw(t, "flag")
		}),
		Entries: drawOptional(t, "entries", func(t *rapid.T) []presence.EntryFields {
			return rapid.SliceOfN(rapid.Custom(drawEntry), 0, 3).Draw(t, "entries")
		}),
		Total: drawOptional(t, "total", func(t *rapid.T) int64 {
			return rapid.Int64().Draw(t, "total")
		}),
		Label: text(t),
		Extent: drawOptional(t, "extent", func(t *rapid.T) presence.Extent {
			return presence.Extent{Width: float32s(t), Height: float32s(t)}
		}),
	}
}

// firstVersionOf returns the fields of a record's first version, without
// those that the second added
func firstVersionOf(fields presence.RecordFields) presence.RecordV1Fields {
	first := presence.RecordV1Fields{
		Id:     fields.Id,
		Name:   fields.Name,
		Values: fields.Values,
		Flag:   fields.Flag,
		Total:  fields.Total,
	}
	if origin, ok := fields.Origin.Get(); ok {
		first.Origin = tafla.Some(presence.PointV1Fields{X: origin.X, Y: origin.Y})
	}
	if entries, ok := fields.Entries.Get(); ok {
		items := make([]presence.EntryV1Fields, len(entries))
		for index, entry := range entries {
			items[index] = presence.EntryV1Fields{Key: entry.Key, Note: entry.Note}
		}
		first.Entries = tafla.Some(items)
	}

	return first
}

// describeOptional describes an optional field by describe, or as absent
func describeOptional[T any](value tafla.Optional[T], describe func(T) tables.Field) tables.Field {
	if value, ok := value.Get(); ok {
		return describe(value)
	}

	return absent
}

// nested describes a table nested in a record from its fields
func nested(fields ...tables.Field) tables.Nested {
	return tables.Nested{From: tables.FromBuilder, Table: tables.Table{Layout: tables.Headerless(tafla.Uint32), Fields: fields}}
}

func describeString(value string) tables.Field { return tables.String(value) }

func describeExtent(extent presence.Extent) tables.Field {
	return tables.Struct{tables.Float32(extent.Width), tables.Float32(extent.Height)}
}

// recordLayout is the layout of Record and RecordV1
var recordLayout = tables.Layout{Sized: true, Versioned: true, OffsetType: tafla.Uint32}

// describeRecord describes the table that a record's fields write
func describeRecord(fields presence.RecordFields) tables.Table {
	describeEntries := func(entries []presence.EntryFields) tables.Field {
		vector := tables.Vector{}
		for _, entry := range entries {
			note := describeOptional(entry.Note, describeString)
			vector = append(vector, nested(tables.Uint16(entry.Key), note, tables.Float32(entry.Weight)))
		}
		return vector
	}

	return tables.Table{Layout: recordLayout, Version: presence.RecordFormatVersion, Fields: []tables.Field{
		tables.Uint32(fields.Id),
		describeOptional(fields.Name, describeString),
		describeOptional(fields.Values, func(values []float64) tables.Field { return tables.Float64Array(values) }),
		describeOptional(fields.Origin, func(origin presence.PointFields) tables.Field {
			return tables.TableField{Nested: nested(tables.Float64(origin.X), tables.Float64(origin.Y), tables.Float64(origin.Z))}
		}),
		describeOptional(fields.Flag, func(flag bool) tables.Field { return tables.Bool(flag) }),
		describeOptional(fields.Entries, describeEntries),
		describeOptional(fields.Total, func(total int64) tables.Field { return tables.Int64(total) }),
		tables.String(fields.Label),
		describeOptional(fields.Extent, describeExtent),
	}}
}

// describeFromFirstVersion describes the fields of a record's first version
// as a reader of the second reads them: those that the second version or
// revision added are missing
func describeFromFirstVersion(fields presence.RecordV1Fields) []tables.Field {
	describeEntries := func(entries []presence.EntryV1Fields) tables.Field {
		vector := tables.Vector{}
		for _, entry := range entries {
			vector = append(vector, nested(tables.Uint16(entry.Key), describeOptional(entry.Note, describeString), absent))
		}
		return vector
	}

	return []tables.Field{
		tables.Uint32(fields.Id),
		describeOptional(fields.Name, describeString),
		describeOptional(fields.Values, func(values []float64) tables.Field { return tables.Float64Array(values) }),
		describeOptional(fields.Origin, func(origin presence.PointV1Fields) tables.Field {
			return tables.TableField{Nested: nested(tables.Float64(origin.X), tables.Float64(origin.Y), absent)}
		}),
		describeOptional(fields.Flag, func(flag bool) tables.Field { return tables.Bool(flag) }),
		describeOptional(fields.Entries, describeEntries),
		describeOptional(fields.Total, func(total int64) tables.Field { return tables.Int64(total) }),
		absent,
		absent,
	}
}

// readOptional reads a field that may be missing through its getter, which
// returns whether the table holds it, and describes it by describe, or as
// absent
func readOptional[T any](get func() (T, bool, error), describe func(T) (tables.Field, error)) (tables.Field, error) {
	value, ok, err := get()
	switch {
	case err != nil:
		return nil, err
	case !ok:
		return absent, nil
	default:
		return describe(value)
	}
}

// readDefault reads a scalar that may be missing through its getter and its
// Has<Field>(), and fails unless it reads as its default when the table
// doesn't hold it
func readDefault[T any](get func() (T, error), has func() (bool, error), missing T, describe func(T) tables.Field) (tables.Field, error) {
	value, err := get()
	if err != nil {
		return nil, err
	}
	holds, err := has()
	if err != nil {
		return nil, fmt.Errorf("Has<Field>() failed where the getter read %v: %w", value, err)
	}

	if holds {
		return describe(value), nil
	}
	// By its bits, so that the default -0.5 and the getter's agree
	if !tables.EqualFields(describe(value), describe(missing)) {
		return nil, fmt.Errorf("read %v rather than the default %v", value, missing)
	}
	return absent, nil
}

func justString(value string) (tables.Field, error) { return tables.String(value), nil }

// readPoint reads a point through its getters
func readPoint(point presence.Point) (tables.Field, error) {
	x, err := point.X()
	if err != nil {
		return nil, err
	}
	y, err := point.Y()
	if err != nil {
		return nil, err
	}
	z, err := readDefault(point.Z, point.HasZ, zDefault, func(z float64) tables.Field { return tables.Float64(z) })
	if err != nil {
		return nil, err
	}

	return tables.TableField{Nested: nested(tables.Float64(x), tables.Float64(y), z)}, nil
}

// readEntry reads an entry through its getters
func readEntry(entry presence.Entry) (tables.Nested, error) {
	key, err := entry.Key()
	if err != nil {
		return tables.Nested{}, err
	}
	note, err := readOptional(entry.Note, justString)
	if err != nil {
		return tables.Nested{}, err
	}
	weight, err := readDefault(entry.Weight, entry.HasWeight, weightDefault, func(weight float32) tables.Field { return tables.Float32(weight) })
	if err != nil {
		return tables.Nested{}, err
	}

	return nested(tables.Uint16(key), note, weight), nil
}

// readEntries reads the items of a vector of entries by index, and fails
// unless iterating reads the same
func readEntries(entries presence.EntryVector) (tables.Field, error) {
	byIndex := tables.Vector{}
	for index := range entries.Len() {
		item, err := entries.Get(index)
		if err != nil {
			return nil, err
		}
		read, err := readEntry(item)
		if err != nil {
			return nil, err
		}
		byIndex = append(byIndex, read)
	}

	iterated := tables.Vector{}
	for item, err := range entries.All() {
		if err != nil {
			return nil, err
		}
		read, err := readEntry(item)
		if err != nil {
			return nil, err
		}
		iterated = append(iterated, read)
	}

	if !tables.EqualFields(byIndex, iterated) {
		return nil, errors.New("a vector of entries iterates over other items than it reads by index")
	}
	return byIndex, nil
}

// readRecord reads every field of a record through its getters, as the
// description of its table, where a field that the record doesn't hold is
// absent, and each scalar that it doesn't hold must read as its default
func readRecord(record presence.Record) ([]tables.Field, error) {
	id, err := record.Id()
	if err != nil {
		return nil, err
	}

	reads := []func() (tables.Field, error){
		func() (tables.Field, error) { return readOptional(record.Name, justString) },
		func() (tables.Field, error) { return readOptional(record.Values, readValues) },
		func() (tables.Field, error) { return readOptional(record.Origin, readPoint) },
		func() (tables.Field, error) {
			return readDefault(record.Flag, record.HasFlag, flagDefault, func(flag bool) tables.Field { return tables.Bool(flag) })
		},
		func() (tables.Field, error) { return readOptional(record.Entries, readEntries) },
		func() (tables.Field, error) {
			return readDefault(record.Total, record.HasTotal, totalDefault, func(total int64) tables.Field { return tables.Int64(total) })
		},
		func() (tables.Field, error) { return readOptional(record.Label, justString) },
		func() (tables.Field, error) { return readOptional(record.Extent, readExtent) },
	}

	fields := []tables.Field{tables.Uint32(id)}
	for _, read := range reads {
		field, err := read()
		if err != nil {
			return nil, err
		}
		fields = append(fields, field)
	}

	return fields, nil
}

func readValues(values tafla.ArrayView[float64]) (tables.Field, error) {
	return tables.Float64Array(values.Slice()), nil
}

func readExtent(extent presence.Extent) (tables.Field, error) {
	return describeExtent(extent), nil
}

// sameFields reports whether two descriptions of a table's fields are the
// same, with floats compared by their bits
func sameFields(a, b []tables.Field) bool {
	return tables.Equal(tables.Table{Fields: a}, tables.Table{Fields: b})
}

func TestARecordReadsBackItsFieldsAndTheDefaultsOfAbsentScalarsWhereverItsBytesLie(t *testing.T) {
	rapid.Check(t, func(t *rapid.T) {
		fields := drawRecord(t)
		byteOffset := rapid.IntRange(0, 7).Draw(t, "byteOffset")

		written := must(fields.MarshalBinary())
		record, err := presence.NewRecord(tables.AtByteOffset(written, byteOffset))
		if err != nil {
			t.Fatal(err)
		}

		expected := describeRecord(fields).Fields
		if read, err := readRecord(record); err != nil || !sameFields(read, expected) {
			t.Fatalf("read %+v, %v\nrather than %+v", read, err, expected)
		}
	})
}

// A record is written as TableBuilder writes its description, an absent
// field with SetAbsent, into new bytes and into reused ones, which aren't
// zero
func TestARecordIsWrittenAsTableBuilderWritesItsAbsentFields(t *testing.T) {
	rapid.Check(t, func(t *rapid.T) {
		fields := drawRecord(t)
		built := must(describeRecord(fields).Bytes())

		if written, err := fields.MarshalBinary(); err != nil || !bytes.Equal(written, built) {
			t.Fatalf("wrote % x, %v\nrather than % x", written, err, built)
		}

		reused := bytes.Repeat([]byte{0xff}, len(built)+8)[:0]
		appended, err := fields.AppendBinary(reused)
		if err != nil || !bytes.Equal(appended, built) || &appended[:1][0] != &reused[:1][0] {
			t.Fatalf("appended % x, %v\nrather than % x", appended, err, built)
		}
	})
}

func TestARecordWithoutItsOptionalFieldsHasTheOffset0ForEach(t *testing.T) {
	written := must(presence.RecordFields{Id: 7, Label: "a"}.MarshalBinary())

	builder := must(tafla.NewTableBuilder(9, tafla.TableLayout{}.Sized().WithVersion(presence.RecordFormatVersion)))
	check := func(err error) {
		if err != nil {
			t.Fatal(err)
		}
	}
	check(builder.SetUint32(7))
	for range 6 {
		check(builder.SetAbsent())
	}
	check(builder.SetString("a"))
	check(builder.SetAbsent())

	if built := must(builder.Bytes()); !bytes.Equal(written, built) {
		t.Errorf("wrote % x\nrather than % x", written, built)
	}
}

// A reader of the second version reads a table of the first, and finds the
// fields that the second version, or the second revision of a nested table
// or a vector's item, added missing
func TestARecordOfTheFirstVersionReadsWithTheFieldsThatTheSecondAddedMissing(t *testing.T) {
	rapid.Check(t, func(t *rapid.T) {
		first := firstVersionOf(drawRecord(t))
		written := must(first.MarshalBinary())

		record, err := presence.NewRecord(written)
		if err != nil {
			t.Fatal(err)
		}

		expected := describeFromFirstVersion(first)
		if read, err := readRecord(record); err != nil || !sameFields(read, expected) {
			t.Fatalf("read %+v, %v\nrather than %+v", read, err, expected)
		}
	})
}

func TestARecordRejectsAVersionBeforeItsFirstOrAfterItsOwn(t *testing.T) {
	rapid.Check(t, func(t *rapid.T) {
		written := must(drawRecord(t).MarshalBinary())
		version := rapid.OneOf(rapid.SampledFrom([]uint32{0, 1, 2, 3, math.MaxUint32}), rapid.Uint32()).Draw(t, "version")
		binary.LittleEndian.PutUint32(written[4:], version)

		if read, err := presence.RecordVersionOf(written); read != version || err != nil {
			t.Fatalf("read the version %d, %v", read, err)
		}

		_, err := presence.NewRecord(written)
		reads := version >= presence.RecordFirstVersion && version <= presence.RecordFormatVersion
		expected := tafla.NewUnsupportedVersionError(version, presence.RecordFormatVersion)
		if reads && err != nil || !reads && !sameError(err, expected) {
			t.Fatalf("%v rather than %v", err, expected)
		}
	})
}

func TestARecordOfOneVersionRejectsTheOtherWithItsReaderOfOne(t *testing.T) {
	written := must(presence.RecordFields{Id: 1}.MarshalBinary())

	expected := tafla.NewUnsupportedVersionError(presence.RecordFormatVersion, presence.RecordV1FormatVersion)
	if _, err := presence.NewRecordV1(written); !sameError(err, expected) {
		t.Errorf("%v rather than %v", err, expected)
	}
}

// A message without a version finds a field that its second revision
// appended missing from a table of the first, from its first field's offset,
// and a reader of the first revision reads a table of the second
func TestASampleOfTheFirstRevisionReadsTheFieldThatTheSecondAppendedAsItsDefault(t *testing.T) {
	rapid.Check(t, func(t *rapid.T) {
		key := rapid.Uint32().Draw(t, "key")
		count := rapid.Uint16().Draw(t, "count")

		first := presence.NewSample(must(presence.SampleV1Fields{Key: key}.MarshalBinary()))
		if read, err := first.Key(); read != key || err != nil {
			t.Fatalf("read the key %d, %v", read, err)
		}
		if read, err := first.Count(); read != countDefault || err != nil {
			t.Fatalf("read the count %d, %v of the first revision", read, err)
		}
		if has, err := first.HasCount(); has || err != nil {
			t.Fatalf("the first revision holds the count: %v, %v", has, err)
		}

		written := must(presence.SampleFields{Key: key, Count: count}.MarshalBinary())
		second := presence.NewSample(written)
		if read, err := second.Count(); read != count || err != nil {
			t.Fatalf("read the count %d, %v", read, err)
		}
		if has, err := second.HasCount(); !has || err != nil {
			t.Fatalf("the second revision holds the count: %v, %v", has, err)
		}
		if read, err := presence.NewSampleV1(written).Key(); read != key || err != nil {
			t.Fatalf("the first revision's reader read the key %d, %v", read, err)
		}
	})
}

// Overwritten and cut bytes read as values, missing fields and defaults, or
// fail with the decode error, through every getter, and never panic
func TestACorruptedRecordReadsAsValuesMissingFieldsOrDecodeErrors(t *testing.T) {
	rapid.Check(t, func(t *rapid.T) {
		written := must(drawRecord(t).MarshalBinary())
		overwrites := rapid.IntRange(1, 4).Draw(t, "overwrites")
		for range overwrites {
			at := rapid.IntRange(0, len(written)-1).Draw(t, "at")
			written[at] = rapid.SampledFrom([]byte{0, 1, 2, 7, 8, 12, 0x80, 0xff}).Draw(t, "byte")
		}
		cut := rapid.IntRange(0, len(written)).Draw(t, "cut")

		for _, bytes := range [][]byte{written, slices.Clone(written[:cut])} {
			record, err := presence.NewRecord(bytes)
			if err == nil {
				_, err = readRecord(record)
			}

			var decodeError *tafla.DecodeError
			if err != nil && !errors.As(err, &decodeError) {
				t.Fatalf("%v rather than a decode error", err)
			}
		}
	})
}

func TestArbitraryBytesReadAsASampleOrDecodeErrors(t *testing.T) {
	rapid.Check(t, func(t *rapid.T) {
		bytes := rapid.SliceOfN(rapid.SampledFrom([]byte{0, 1, 2, 4, 6, 8, 12, 0x80, 0xff}), 0, 24).Draw(t, "bytes")
		sample := presence.NewSample(bytes)

		_, keyErr := sample.Key()
		count, err := readDefault(sample.Count, sample.HasCount, countDefault, func(count uint16) tables.Field { return tables.Uint16(count) })
		for _, err := range []error{keyErr, err} {
			var decodeError *tafla.DecodeError
			if err != nil && !errors.As(err, &decodeError) {
				t.Fatalf("read %v, %v rather than a decode error", count, err)
			}
		}
	})
}
