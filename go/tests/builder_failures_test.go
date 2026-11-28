package tafla_test

// A setter that fails leaves the builder as it was, wherever it fails: a
// field too long or one too many, in any table of any offset type, and a
// table or vector item written in place whose fill function fails, or that
// makes its parent too long. Set again, the field fails as it failed, and
// once fill succeeds, the table is the one written without the failure.
//
// The build tag tafla_builder_stress makes the builder keep every array,
// string and nested table's bytes rather than copy them, and grow its buffer
// from a byte, so that these tests run those paths on small tables too.

import (
	"bytes"
	"errors"
	"math"
	"slices"
	"testing"

	tafla "github.com/snovader/tafla/go"
	"github.com/snovader/tafla/go/tests/internal/tables"
	"pgregory.net/rapid"
)

// Any table, with a long field anywhere, in a builder of a field fewer, as
// many or a field more, with its nested tables written in place or not: each
// setter that fails fails again when it's set again. Most tables nest only
// tables written in place, and a fill function among those that the table
// calls fails, however it fails, after any of its fields: it fails its
// setter with its error, and once that's set again, each setter's error and
// the table are as they are without it.
func TestASetterThatFailsLeavesTheBuilderAsItWas(t *testing.T) {
	rapid.Check(t, checkSettersThatFail)
}

// FuzzTableBuilder checks the property of
// TestASetterThatFailsLeavesTheBuilderAsItWas on the fuzzer's bytes.
func FuzzTableBuilder(f *testing.F) {
	f.Fuzz(rapid.MakeFuzz(checkSettersThatFail))
}

func checkSettersThatFail(t *rapid.T) {
	table := tables.Tables().Draw(t, "table")
	inPlace := rapid.IntRange(0, 3).Draw(t, "in place") > 0
	if inPlace && rapid.IntRange(0, 2).Draw(t, "only in place") > 0 {
		table = writtenInPlace(table.NestedInPlace())
	}
	if rapid.Bool().Draw(t, "long") {
		table = withLongField(t, table)
	}
	fields := max(len(table.Fields)+rapid.IntRange(-1, 1).Draw(t, "more fields"), 0)

	none := &fillFailure{at: -1}
	expected := buildSettingAgain(table, fields, inPlace, none)
	for index, errs := range expected.setters {
		if len(errs) == 2 && !sameError(errs[0], errs[1]) {
			t.Fatalf("field %d failed with %v, and then with %v", index, errs[0], errs[1])
		}
	}
	if none.calls == 0 {
		return
	}

	failure := &fillFailure{
		at:    rapid.IntRange(0, none.calls-1).Draw(t, "failing fill"),
		after: rapid.IntRange(0, 4).Draw(t, "fields before failing"),
		how:   rapid.SampledFrom([]failing{returningAnError, leavingFieldsUnset, settingAFieldMore}).Draw(t, "how"),
	}
	built := buildSettingAgain(table, fields, inPlace, failure)
	if failure.err != nil && !sameError(failure.failedWith, failure.err) {
		t.Fatalf("field %d failed with %v rather than %v", failure.field, failure.failedWith, failure.err)
	}
	sameErrors := func(a, b []error) bool { return slices.EqualFunc(a, b, sameError) }
	if !slices.EqualFunc(built.setters, expected.setters, sameErrors) {
		t.Fatalf("setters failed with %v\nrather than %v", built.setters, expected.setters)
	}
	if !bytes.Equal(built.bytes, expected.bytes) || !sameError(built.err, expected.err) {
		t.Fatalf("wrote % x, %v\nrather than % x, %v", built.bytes, built.err, expected.bytes, expected.err)
	}
}

// withLongField inserts a field anywhere in the table that's up to as long
// as fits in its offset type and a unit or two more, often near that, or up
// to 2,000 units in a table of uint32 offsets, long enough for the builder
// to keep rather than copy. Its nested tables have the table's offset type
// as often as not, so that they're written in place, and can make it too
// long there.
func withLongField(t *rapid.T, table tables.Table) tables.Table {
	long := rapid.SampledFrom(longFields).Draw(t, "long field")
	offsetType := table.Layout.OffsetType
	unit := long.unit(offsetType.Size())
	most := min(int(offsetType.MaxValue())/unit+2, 2_000)
	units := rapid.IntRange(0, most).Draw(t, "units")
	if rapid.Bool().Draw(t, "near the limit") {
		units = max(most-rapid.IntRange(0, 16).Draw(t, "units under the most"), 0)
	}
	field := long.field(units)
	if rapid.Bool().Draw(t, "nested in the table's offset type") {
		field = withOffsetType(field, offsetType)
	}

	at := rapid.IntRange(0, len(table.Fields)).Draw(t, "long field at")
	table.Fields = slices.Insert(slices.Clone(table.Fields), at, field)

	return table
}

// withOffsetType returns a long field whose nested tables have offsetType
func withOffsetType(field tables.Field, offsetType tafla.OffsetType) tables.Field {
	nested := func(nested tables.Nested) tables.Nested {
		nested.Table.Layout = tables.Headerless(offsetType)
		return nested
	}

	switch value := field.(type) {
	case tables.TableField:
		value.Nested = nested(value.Nested)
		return value
	case tables.Vector:
		items := make(tables.Vector, len(value))
		for index, item := range value {
			items[index] = nested(item)
		}
		return items
	default:
		return field
	}
}

// failing is how a fill function fails
type failing uint8

const (
	// It returns an error of its own, after some of its fields
	returningAnError failing = iota
	// It returns nil before it has set all its fields
	leavingFieldsUnset
	// It sets a field more than its table holds
	settingAFieldMore
)

// fillFailure makes the fill function of the call at fail, counting every
// call of the table's, after it sets after fields, or none when at is -1.
// Once it has, err is what it expects the setter of the root's field index
// field to fail with, and failedWith what that setter failed with.
type fillFailure struct {
	at, after int
	how       failing

	calls, field    int
	err, failedWith error
}

var errFill = errors.New("the fill function failed")

// built is a table built by setting each field again once after it fails:
// the error of each setter of each field, and Bytes's bytes or error
type built struct {
	setters [][]error
	bytes   []byte
	err     error
}

// buildSettingAgain builds the table in a builder of fields fields, setting
// each field again once after it fails, and after a fill function fails it.
// The tables nested from builders without a header and of the table's offset
// type are written in place when inPlace, as tables.Table.BytesInPlace writes
// them, and fill functions fail as failure says.
func buildSettingAgain(table tables.Table, fields int, inPlace bool, failure *fillFailure) built {
	failure.calls, failure.field, failure.err, failure.failedWith = 0, -1, nil, nil
	root, err := startTable(table, fields)
	if err != nil {
		return built{err: err}
	}

	setter := setter{offsetType: table.Layout.OffsetType, inPlace: inPlace, failure: failure}
	setters := make([][]error, len(table.Fields))
	for index, field := range table.Fields {
		setters[index] = setter.setAgain(root, index, field)
	}

	written, err := root.Bytes()
	return built{setters: setters, bytes: written, err: err}
}

// setAgain sets field index of the root, and again once if it fails, and
// returns their errors. When a fill function fails, it sets the field again,
// without counting that setter, whose error the failure keeps.
func (s setter) setAgain(root *tafla.TableBuilder, index int, field tables.Field) []error {
	var errs []error
	for len(errs) < 2 {
		failed := s.failure.err != nil
		err := s.set(root, field)
		if !failed && s.failure.err != nil {
			s.failure.field, s.failure.failedWith = index, err
			continue
		}

		errs = append(errs, err)
		if err == nil {
			break
		}
	}

	return errs
}

// startTable starts a builder of the table's layout and version, of fields
// fields
func startTable(table tables.Table, fields int) (*tafla.TableBuilder, error) {
	return tafla.NewTableBuilder(fields, table.TableLayout())
}

// setter sets fields on a builder or an item, with the tables nested from
// builders without a header and of the root's offset type written in place
// when inPlace, and fill functions failing as failure says
type setter struct {
	offsetType tafla.OffsetType
	inPlace    bool
	failure    *fillFailure
}

func (s setter) set(on tables.Target, field tables.Field) error {
	switch value := field.(type) {
	case tables.TableField:
		if value.Alignment == 0 && s.writesInPlace(value.Nested) {
			fields := value.Nested.Table.Fields
			return on.SetTableFunc(len(fields), func(table tafla.Item) error { return s.fill(table, fields) })
		}
	case tables.Vector:
		if s.vectorInPlace(value) {
			return on.SetVectorFunc(len(value), len(value[0].Table.Fields), func(index int, item tafla.Item) error {
				return s.fill(item, value[index].Table.Fields)
			})
		}
	}

	return tables.Set(on, field)
}

func (s setter) writesInPlace(nested tables.Nested) bool {
	layout := nested.Table.Layout
	return s.inPlace && nested.From == tables.FromBuilder && !layout.Sized && !layout.Versioned &&
		layout.OffsetType == s.offsetType
}

// vectorInPlace returns whether every item is written in place, and they
// hold as many fields, as SetVectorFunc writes them
func (s setter) vectorInPlace(vector tables.Vector) bool {
	if len(vector) == 0 {
		return false
	}
	for _, item := range vector {
		if !s.writesInPlace(item) || len(item.Table.Fields) != len(vector[0].Table.Fields) {
			return false
		}
	}

	return true
}

// fill sets the fields of a table written in place, and fails if it's the
// call that fails
func (s setter) fill(table tafla.Item, fields []tables.Field) error {
	failing := s.failure.calls == s.failure.at
	s.failure.calls++

	for index, field := range fields {
		if failing && index == s.failure.after && s.failure.how != settingAFieldMore {
			return s.failure.fail(table, len(fields))
		}
		if err := s.set(table, field); err != nil {
			return err
		}
	}
	if failing && s.failure.how != leavingFieldsUnset {
		return s.failure.fail(table, len(fields))
	}

	return nil
}

// fail fails the fill function of a table of fields fields, and keeps the
// error that its setter fails with
func (f *fillFailure) fail(table tafla.Item, fields int) error {
	switch f.how {
	case returningAnError:
		f.err = errFill
		return errFill
	case leavingFieldsUnset:
		f.err = &tafla.BuildError{Kind: tafla.Incomplete, Set: f.after, Fields: fields}
		return nil
	default:
		f.err = &tafla.BuildError{Kind: tafla.TooManyFields, Fields: fields}
		return table.SetUint8(1)
	}
}

// A table nested in place that fits in its own offsets, but makes its
// parent too long once it's padded, fails once its fields are set, as from
// its builder, and leaves the builder as it was
func TestATableWrittenInPlaceThatMakesItsParentTooLongLeavesTheBuilderAsItWas(t *testing.T) {
	layout := tafla.TableLayout{}.WithOffsetType(tafla.Uint8)
	nested := func(count int) func(table tafla.Item) error {
		return func(table tafla.Item) error { return table.SetUint16Array(make([]uint16, count)) }
	}
	start := func() *tafla.TableBuilder {
		builder := must(tafla.NewTableBuilder(3, layout))
		check(t, builder.SetUint8(1))
		check(t, builder.SetUint8(2))
		return builder
	}

	// 3 offsets and 2 bytes, then a byte of padding and the table: its offset,
	// the array's count and 248 bytes
	tooLong := &tafla.BuildError{Kind: tafla.TooLong, Index: 2, Length: 256, OffsetType: tafla.Uint8}
	fromBuilder := must(tafla.NewTableBuilder(1, layout))
	check(t, fromBuilder.SetUint16Array(make([]uint16, 124)))
	expectError(t, start().SetTable(fromBuilder), tooLong)

	builder := start()
	expectError(t, builder.SetTableFunc(1, nested(124)), tooLong)
	check(t, builder.SetTableFunc(1, nested(123)))

	expected := start()
	check(t, expected.SetTableFunc(1, nested(123)))
	if built, want := must(builder.Bytes()), must(expected.Bytes()); !bytes.Equal(built, want) {
		t.Errorf("wrote % x\nrather than % x", built, want)
	}
}

// Padding is zeroed where a table that failed was written, as it is in a
// table written without it
func TestPaddingIsZeroedWhereATableThatFailedWasWritten(t *testing.T) {
	ones := func(table tafla.Item) error {
		check(t, table.SetUint64(math.MaxUint64))
		return errFill
	}
	write := func(nestFailing func(builder *tafla.TableBuilder) error) []byte {
		builder := must(tafla.NewTableBuilder(3, tafla.TableLayout{}))
		check(t, builder.SetUint8(1))
		if nestFailing != nil {
			if err := nestFailing(builder); err != errFill {
				t.Fatalf("returned %v", err)
			}
		}
		check(t, builder.SetFloat64Array([]float64{2}))
		check(t, builder.SetTableAligned(tafla.TableBytes{3}, tafla.Align8))
		return must(builder.Bytes())
	}

	expected := write(nil)
	failedTable := write(func(builder *tafla.TableBuilder) error { return builder.SetTableFunc(1, ones) })
	failedItems := write(func(builder *tafla.TableBuilder) error {
		return builder.SetVectorFunc(3, 1, func(_ int, item tafla.Item) error { return ones(item) })
	})
	for _, got := range [][]byte{failedTable, failedItems} {
		if !bytes.Equal(got, expected) {
			t.Errorf("wrote % x\nrather than % x", got, expected)
		}
	}
}
