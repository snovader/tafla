package tafla_test

// The prototype of a TableBuilder that writes each field when it's set, in
// tafla/go/prototype, writes what TableBuilder writes and fails as it fails:
// on every conformance case, and on generated tables in every offset type,
// with fields too long, too many or too few, and fill functions that fail.
// The build tag tafla_prototype_stress makes every run of the prototype's a
// segment, and its buffer grow from a byte.

import (
	"bytes"
	"errors"
	"math"
	"slices"
	"testing"

	tafla "github.com/snovader/tafla/go"
	"github.com/snovader/tafla/go/prototype"
	"github.com/snovader/tafla/go/tests/internal/tables"
	"pgregory.net/rapid"
)

func TestThePrototypeRebuildsEveryValidTableByteForByte(t *testing.T) {
	for _, valid := range loadSuite(t).Valid {
		for _, inPlace := range []bool{false, true} {
			built, err := buildBoth(valid.Table, len(valid.Table.Fields), inPlace, noFailure).prototype.result()
			if err != nil {
				t.Errorf("%s, in place %v: %v", valid.Name, inPlace, err)
			} else if !bytes.Equal(built, valid.Bytes) {
				t.Errorf("%s, in place %v: built % x\nrather than % x", valid.Name, inPlace, built, valid.Bytes)
			}
		}
	}
}

// Any table, with a long field anywhere, in a builder of a field fewer, as
// many or a field more, with its nested tables written in place or not, and
// with a fill function that fails somehow, gets the same error from each
// setter, which is set again once after it fails, and the same bytes or
// error from Bytes
func TestThePrototypeWritesAndFailsAsTableBuilderDoes(t *testing.T) {
	rapid.Check(t, checkPrototype)
}

// FuzzPrototype runs the property of
// TestThePrototypeWritesAndFailsAsTableBuilderDoes on the fuzzer's bytes.
func FuzzPrototype(f *testing.F) {
	f.Fuzz(rapid.MakeFuzz(checkPrototype))
}

func checkPrototype(t *rapid.T) {
	table := tables.Tables().Draw(t, "table")
	if rapid.Bool().Draw(t, "long") {
		table = withLongField(t, table)
	}
	fields := max(len(table.Fields)+rapid.IntRange(-1, 1).Draw(t, "more fields"), 0)
	inPlace := rapid.Bool().Draw(t, "in place")
	failure := fillFailure{
		at:    rapid.IntRange(-1, 5).Draw(t, "failing fill"),
		after: rapid.IntRange(0, 4).Draw(t, "fields before failing"),
		how:   rapid.IntRange(0, 2).Draw(t, "how it fails"),
	}

	both := buildBoth(table, fields, inPlace, failure)
	if !slices.EqualFunc(both.prototype.errors, both.builder.errors, sameError) {
		t.Fatalf("setters failed with %v\nrather than %v", both.prototype.errors, both.builder.errors)
	}

	built, err := both.prototype.result()
	expected, expectedErr := both.builder.result()
	if !bytes.Equal(built, expected) || !sameError(err, expectedErr) {
		t.Fatalf("wrote % x, %v\nrather than % x, %v", built, err, expected, expectedErr)
	}
}

// withLongField inserts a field anywhere in the table that's up to as long
// as fits in its offset type and a unit or two more, often near that, or up
// to 2,000 units in a
// table of uint32 offsets, which holds runs that the prototype keeps as
// segments. Its nested tables have the table's offset type as often as not,
// so that they're written in place, and can make it too long there.
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

// fillFailure makes the fill function of the call at, counting every call
// of the table's, fail after it sets after fields: by returning errFill, by
// returning nil, or by setting one more field
type fillFailure struct {
	at, after, how int
	calls          *int
}

var noFailure = fillFailure{at: -1}

var errFill = errors.New("the fill function failed")

// build is a table built by a builder: each setter's error, and Bytes
type build struct {
	errors []error
	bytes  []byte
	err    error
}

func (b build) result() ([]byte, error) {
	return b.bytes, b.err
}

// both are a table built by the prototype and by TableBuilder
type both struct {
	prototype, builder build
}

// buildBoth builds the table with each builder, of fields fields, setting
// each field again once after it fails
func buildBoth(table tables.Table, fields int, inPlace bool, failure fillFailure) both {
	start := func(fields int) (target, func() ([]byte, error), error) {
		builder, err := startPrototype(table, fields)
		if err != nil {
			return nil, nil, err
		}
		return prototypeTarget{builder}, builder.Bytes, nil
	}
	startBuilder := func(fields int) (target, func() ([]byte, error), error) {
		builder, err := startTableBuilder(table, fields)
		if err != nil {
			return nil, nil, err
		}
		return builderTarget{builder}, builder.Bytes, nil
	}

	return both{
		prototype: buildWith(start, table, fields, inPlace, failure),
		builder:   buildWith(startBuilder, table, fields, inPlace, failure),
	}
}

func buildWith(
	start func(fields int) (target, func() ([]byte, error), error),
	table tables.Table, fields int, inPlace bool, failure fillFailure,
) build {
	root, bytes, err := start(fields)
	if err != nil {
		return build{err: err}
	}

	calls := 0
	failure.calls = &calls
	setter := setter{offsetType: table.Layout.OffsetType, inPlace: inPlace, failure: failure}

	var errs []error
	for _, field := range table.Fields {
		err := setter.set(root, field)
		if err != nil {
			errs = append(errs, err, setter.set(root, field))
		}
	}

	written, err := bytes()
	return build{errors: errs, bytes: written, err: err}
}

func startPrototype(table tables.Table, fields int) (*prototype.TableBuilder, error) {
	layout := table.Layout.TableLayout()
	if table.Layout.Versioned {
		return prototype.NewVersionedTableBuilder(fields, layout, table.Version)
	}

	return prototype.NewTableBuilder(fields, layout)
}

func startTableBuilder(table tables.Table, fields int) (*tafla.TableBuilder, error) {
	layout := table.Layout.TableLayout()
	if table.Layout.Versioned {
		return tafla.NewVersionedTableBuilder(fields, layout, table.Version)
	}

	return tafla.NewTableBuilder(fields, layout)
}

// values are the setters of values, which both builders and their items have
type values interface {
	SetInt8(value int8) error
	SetUint8(value uint8) error
	SetInt16(value int16) error
	SetUint16(value uint16) error
	SetInt32(value int32) error
	SetUint32(value uint32) error
	SetInt64(value int64) error
	SetUint64(value uint64) error
	SetFloat32(value float32) error
	SetFloat64(value float64) error
	SetInt8Array(values []int8) error
	SetUint8Array(values []uint8) error
	SetInt16Array(values []int16) error
	SetUint16Array(values []uint16) error
	SetInt32Array(values []int32) error
	SetUint32Array(values []uint32) error
	SetInt64Array(values []int64) error
	SetUint64Array(values []uint64) error
	SetFloat32Array(values []float32) error
	SetFloat64Array(values []float64) error
	SetString(value string) error
	SetStruct(bytes []byte) error
	SetStructArray(bytes []byte, size int, alignment tafla.Alignment) error
}

// target is what a field is set on, a builder or an item of either kind,
// with its tables nested from builders of its own kind
type target interface {
	values
	setTable(nested tables.Nested, alignment tafla.Alignment) error
	setVector(vector tables.Vector) error
	setTableFunc(fields int, fill func(table target) error) error
	setVectorFunc(count, fields int, fill func(index int, item target) error) error
}

// prototypeTarget is a prototype's builder or item
type prototypeTarget struct {
	prototypeSetters
}

type prototypeSetters interface {
	values
	SetTable(table prototype.NestedTable) error
	SetTableAligned(table prototype.NestedTable, alignment tafla.Alignment) error
	SetVector(items ...prototype.NestedTable) error
	SetTableFunc(fields int, fill func(table prototype.Item) error) error
	SetVectorFunc(count, fields int, fill func(index int, item prototype.Item) error) error
}

func (p prototypeTarget) setTable(nested tables.Nested, alignment tafla.Alignment) error {
	table, err := nestedPrototype(nested)
	if err != nil {
		return err
	}
	if alignment != 0 {
		return p.SetTableAligned(table, alignment)
	}

	return p.SetTable(table)
}

func (p prototypeTarget) setVector(vector tables.Vector) error {
	items := make([]prototype.NestedTable, 0, len(vector))
	for _, nested := range vector {
		item, err := nestedPrototype(nested)
		if err != nil {
			return err
		}
		items = append(items, item)
	}

	return p.SetVector(items...)
}

func (p prototypeTarget) setTableFunc(fields int, fill func(table target) error) error {
	return p.SetTableFunc(fields, func(table prototype.Item) error { return fill(prototypeTarget{table}) })
}

func (p prototypeTarget) setVectorFunc(count, fields int, fill func(index int, item target) error) error {
	return p.SetVectorFunc(count, fields, func(index int, item prototype.Item) error {
		return fill(index, prototypeTarget{item})
	})
}

// nestedPrototype builds a table to nest with the prototype, as its builder
// or its bytes, setting its fields as tables.Nested.Build does
func nestedPrototype(nested tables.Nested) (prototype.NestedTable, error) {
	builder, err := startPrototype(nested.Table, len(nested.Table.Fields))
	if err != nil {
		return nil, err
	}
	for _, field := range nested.Table.Fields {
		if err := (setter{}).set(prototypeTarget{builder}, field); err != nil {
			return nil, err
		}
	}

	if nested.From == tables.FromBuilder {
		return builder, nil
	}
	bytes, err := builder.Bytes()
	return prototype.TableBytes(bytes), err
}

// builderTarget is a TableBuilder or its item
type builderTarget struct {
	tables.Target
}

func (b builderTarget) setTable(nested tables.Nested, alignment tafla.Alignment) error {
	table, err := nested.Build()
	if err != nil {
		return err
	}
	if alignment != 0 {
		return b.SetTableAligned(table, alignment)
	}

	return b.SetTable(table)
}

func (b builderTarget) setVector(vector tables.Vector) error {
	items := make([]tafla.NestedTable, 0, len(vector))
	for _, nested := range vector {
		item, err := nested.Build()
		if err != nil {
			return err
		}
		items = append(items, item)
	}

	return b.SetVector(items...)
}

func (b builderTarget) setTableFunc(fields int, fill func(table target) error) error {
	return b.SetTableFunc(fields, func(table tafla.Item) error { return fill(builderTarget{table}) })
}

func (b builderTarget) setVectorFunc(count, fields int, fill func(index int, item target) error) error {
	return b.SetVectorFunc(count, fields, func(index int, item tafla.Item) error {
		return fill(index, builderTarget{item})
	})
}

// setter sets fields on either builder, with the tables nested from
// builders without a header and of the root's offset type written in place
// when inPlace, as tables.Table.BytesInPlace writes them, and fill functions
// failing as failure says
type setter struct {
	offsetType tafla.OffsetType
	inPlace    bool
	failure    fillFailure
}

func (s setter) set(on target, field tables.Field) error {
	switch value := field.(type) {
	case tables.TableField:
		if value.Alignment == 0 && s.writesInPlace(value.Nested) {
			fields := value.Nested.Table.Fields
			return on.setTableFunc(len(fields), func(table target) error { return s.fill(table, fields) })
		}
		return on.setTable(value.Nested, value.Alignment)
	case tables.Vector:
		if s.vectorInPlace(value) {
			return on.setVectorFunc(len(value), len(value[0].Table.Fields), func(index int, item target) error {
				return s.fill(item, value[index].Table.Fields)
			})
		}
		return on.setVector(value)
	default:
		return setValue(on, field)
	}
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

// fill sets the fields of a table written in place, unless it's the call
// that fails
func (s setter) fill(table target, fields []tables.Field) error {
	call := *s.failure.calls
	*s.failure.calls++
	failing := call == s.failure.at

	for index, field := range fields {
		if failing && index == s.failure.after {
			return s.failure.fail(table)
		}
		if err := s.set(table, field); err != nil {
			return err
		}
	}
	if failing {
		return s.failure.fail(table)
	}

	return nil
}

func (f fillFailure) fail(table target) error {
	switch f.how {
	case 0:
		return errFill
	case 1:
		return nil
	default:
		return table.SetUint8(1)
	}
}

// setValue sets a field that holds values rather than tables
func setValue(target values, field tables.Field) error {
	switch value := field.(type) {
	case tables.Int8:
		return target.SetInt8(int8(value))
	case tables.Uint8:
		return target.SetUint8(uint8(value))
	case tables.Int16:
		return target.SetInt16(int16(value))
	case tables.Uint16:
		return target.SetUint16(uint16(value))
	case tables.Int32:
		return target.SetInt32(int32(value))
	case tables.Uint32:
		return target.SetUint32(uint32(value))
	case tables.Int64:
		return target.SetInt64(int64(value))
	case tables.Uint64:
		return target.SetUint64(uint64(value))
	case tables.Float32:
		return target.SetFloat32(float32(value))
	case tables.Float64:
		return target.SetFloat64(float64(value))
	case tables.Bool:
		if value {
			return target.SetUint8(1)
		}
		return target.SetUint8(0)
	case tables.Int8Array:
		return target.SetInt8Array(value)
	case tables.Uint8Array:
		return target.SetUint8Array(value)
	case tables.Int16Array:
		return target.SetInt16Array(value)
	case tables.Uint16Array:
		return target.SetUint16Array(value)
	case tables.Int32Array:
		return target.SetInt32Array(value)
	case tables.Uint32Array:
		return target.SetUint32Array(value)
	case tables.Int64Array:
		return target.SetInt64Array(value)
	case tables.Uint64Array:
		return target.SetUint64Array(value)
	case tables.Float32Array:
		return target.SetFloat32Array(value)
	case tables.Float64Array:
		return target.SetFloat64Array(value)
	case tables.String:
		return target.SetString(string(value))
	case tables.Struct:
		return target.SetStruct(tables.StructBytes(value))
	case tables.StructArray:
		var bytes []byte
		for _, item := range value.Items {
			bytes = append(bytes, tables.StructBytes(item)...)
		}
		return target.SetStructArray(bytes, tables.StructSize(value.Types), tables.StructAlignment(value.Types))
	default:
		panic("unknown field")
	}
}

// The prototype checks a vector's length before it writes any item, as
// TableBuilder does, and keeps a nested builder's long arrays as segments,
// so that a vector that nests a builder of 64 MiB 63 times needs no more
func TestThePrototypeRejectsATableOfUint32OffsetsPast4GiBBeforeWritingIt(t *testing.T) {
	nested := must(prototype.NewTableBuilder(1, tafla.TableLayout{}))
	check(t, nested.SetUint8Array(make([]byte, 1<<26)))

	items := make([]prototype.NestedTable, 64)
	for index := range items {
		items[index] = nested
	}

	builder := must(prototype.NewTableBuilder(1, tafla.TableLayout{}))
	var buildError *tafla.BuildError
	if err := builder.SetVector(items...); !errors.As(err, &buildError) || buildError.Kind != tafla.TooLong ||
		buildError.Index != 0 || buildError.Length <= 1<<32 {
		t.Errorf("%#v", err)
	}

	err := builder.SetVector(items[:63]...)
	if math.MaxInt > math.MaxUint32 {
		check(t, err)
	} else if !errors.As(err, &buildError) || buildError.Kind != tafla.TooLong {
		t.Errorf("%#v", err)
	}
}

func TestThePrototypePanicsAsTableBuilderDoes(t *testing.T) {
	builder := must(prototype.NewTableBuilder(1, tafla.TableLayout{}))
	var nilBuilder *prototype.TableBuilder

	expectPanic(t, "a nil table", func() { _ = builder.SetTable(nil) })
	expectPanic(t, "a nil builder", func() { _ = builder.SetTable(nilBuilder) })
	expectPanic(t, "a nil item", func() { _ = builder.SetVector(nil) })
	expectPanic(t, "an alignment of 3", func() { _ = builder.SetTableAligned(prototype.TableBytes{}, 3) })
	expectPanic(t, "an alignment of 0", func() { _ = builder.SetStructArray(nil, 1, 0) })
	expectPanic(t, "a table of -1 fields", func() { _ = builder.SetTableFunc(-1, nil) })
	expectPanic(t, "a vector of -1 items", func() { _ = builder.SetVectorFunc(-1, 0, nil) })
	expectPanic(t, "items of -1 fields", func() { _ = builder.SetVectorFunc(0, -1, nil) })
	expectPanic(t, "a negative number of fields", func() { _, _ = prototype.NewTableBuilder(-1, tafla.TableLayout{}) })
	expectPanic(t, "a versioned layout without a version", func() {
		_, _ = prototype.NewTableBuilder(1, tafla.TableLayout{}.Versioned())
	})
	expectPanic(t, "a version without a slot", func() {
		_, _ = prototype.NewVersionedTableBuilder(1, tafla.TableLayout{}, 1)
	})
}

// The prototype, like TableBuilder, keeps arrays until the table is written,
// but not after: the table holds the values set
func TestThePrototypeWritesValuesThatChangeAfterTheTableIsWritten(t *testing.T) {
	floats := make([]float64, 100)
	floats[0] = 1.5
	builder := must(prototype.NewTableBuilder(1, tafla.TableLayout{}))
	check(t, builder.SetFloat64Array(floats))

	table := must(builder.Bytes())
	floats[0] = 0

	buffer := tafla.NewByteBuffer(table, tafla.TableLayout{})
	read := must(buffer.GetFloat64Array(must(buffer.RootOffset(0))))
	if read.At(0) != 1.5 || read.Len() != 100 {
		t.Errorf("read %v of %d", read.At(0), read.Len())
	}
}

// Padding is zeroed where a table that failed wrote, as TableBuilder's
// bytes are zero there
func TestThePrototypeZeroesPaddingWhereAFailedTableWrote(t *testing.T) {
	ones := func(table prototype.Item) error {
		check(t, table.SetUint64(math.MaxUint64))
		return errFill
	}
	write := func(nestFailed func(builder *prototype.TableBuilder) error) []byte {
		builder := must(prototype.NewTableBuilder(3, tafla.TableLayout{}))
		check(t, builder.SetUint8(1))
		if err := nestFailed(builder); err != errFill {
			t.Fatalf("returned %v", err)
		}
		check(t, builder.SetFloat64Array([]float64{2}))
		check(t, builder.SetTableAligned(prototype.TableBytes{3}, tafla.Align8))
		return must(builder.Bytes())
	}

	expected := must(tafla.NewTableBuilder(3, tafla.TableLayout{}))
	check(t, expected.SetUint8(1))
	check(t, expected.SetFloat64Array([]float64{2}))
	check(t, expected.SetTableAligned(tafla.TableBytes{3}, tafla.Align8))
	want := must(expected.Bytes())

	failedTable := write(func(builder *prototype.TableBuilder) error { return builder.SetTableFunc(1, ones) })
	failedItems := write(func(builder *prototype.TableBuilder) error {
		return builder.SetVectorFunc(3, 1, func(_ int, item prototype.Item) error { return ones(item) })
	})
	for _, got := range [][]byte{failedTable, failedItems} {
		if !bytes.Equal(got, want) {
			t.Errorf("wrote % x\nrather than % x", got, want)
		}
	}
}
