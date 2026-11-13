package tables

import (
	tafla "github.com/snovader/tafla/go"
)

// Builder starts a builder of the table's layout, version and number of
// fields, with no field set.
func (t Table) Builder() (*tafla.TableBuilder, error) {
	layout := t.Layout.TableLayout()
	if t.Layout.Versioned {
		return tafla.NewVersionedTableBuilder(len(t.Fields), layout, t.Version)
	}

	return tafla.NewTableBuilder(len(t.Fields), layout)
}

// Build builds the table, which keeps its values.
func (t Table) Build() (*tafla.TableBuilder, error) {
	builder, err := t.Builder()
	if err != nil {
		return nil, err
	}

	for _, field := range t.Fields {
		if err := Set(builder, field); err != nil {
			return nil, err
		}
	}

	return builder, nil
}

// Bytes writes the table with a TableBuilder.
func (t Table) Bytes() ([]byte, error) {
	builder, err := t.Build()
	if err != nil {
		return nil, err
	}

	return builder.Bytes()
}

// BytesInPlace writes the table with a TableBuilder, as Bytes does, but
// records each table nested from a builder that it writes in place with
// SetTableFunc, and each vector of them whose items hold as many fields with
// SetVectorFunc, rather than from their builders. Its bytes and errors are
// those of Bytes.
func (t Table) BytesInPlace() ([]byte, error) {
	builder, err := t.Builder()
	if err != nil {
		return nil, err
	}

	// The tables that its builder would write in place: without a header,
	// and with this table's offset type
	inPlace := func(nested Nested) bool {
		layout := nested.Table.Layout
		return nested.From == FromBuilder && !layout.Sized && !layout.Versioned &&
			layout.OffsetType == t.Layout.OffsetType
	}
	for _, field := range t.Fields {
		if err := set(builder, field, inPlace); err != nil {
			return nil, err
		}
	}

	return builder.Bytes()
}

// Target is what a field is set on: a *tafla.TableBuilder, or a tafla.Item
// of a table that one records in place.
type Target interface {
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
	SetTable(table tafla.NestedTable) error
	SetTableAligned(table tafla.NestedTable, alignment tafla.Alignment) error
	SetVector(items ...tafla.NestedTable) error
	SetTableFunc(fields int, fill func(table tafla.Item) error) error
	SetVectorFunc(count, fields int, fill func(index int, item tafla.Item) error) error
}

var (
	_ Target = (*tafla.TableBuilder)(nil)
	_ Target = tafla.Item{}
)

// Set sets a field on builder, which keeps its value, or the bytes of
// structs.
func Set(builder *tafla.TableBuilder, field Field) error {
	return set(builder, field, nil)
}

// set sets a field on target, as Set does, and records the tables nested in
// it that inPlace accepts in place, as BytesInPlace does
func set(target Target, field Field, inPlace func(Nested) bool) error {
	switch value := field.(type) {
	case Int8:
		return target.SetInt8(int8(value))
	case Uint8:
		return target.SetUint8(uint8(value))
	case Int16:
		return target.SetInt16(int16(value))
	case Uint16:
		return target.SetUint16(uint16(value))
	case Int32:
		return target.SetInt32(int32(value))
	case Uint32:
		return target.SetUint32(uint32(value))
	case Int64:
		return target.SetInt64(int64(value))
	case Uint64:
		return target.SetUint64(uint64(value))
	case Float32:
		return target.SetFloat32(float32(value))
	case Float64:
		return target.SetFloat64(float64(value))
	case Bool:
		return target.SetUint8(boolByte(bool(value)))
	case Int8Array:
		return target.SetInt8Array(value)
	case Uint8Array:
		return target.SetUint8Array(value)
	case Int16Array:
		return target.SetInt16Array(value)
	case Uint16Array:
		return target.SetUint16Array(value)
	case Int32Array:
		return target.SetInt32Array(value)
	case Uint32Array:
		return target.SetUint32Array(value)
	case Int64Array:
		return target.SetInt64Array(value)
	case Uint64Array:
		return target.SetUint64Array(value)
	case Float32Array:
		return target.SetFloat32Array(value)
	case Float64Array:
		return target.SetFloat64Array(value)
	case String:
		return target.SetString(string(value))
	case Struct:
		return target.SetStruct(StructBytes(value))
	case StructArray:
		var bytes []byte
		for _, item := range value.Items {
			bytes = append(bytes, StructBytes(item)...)
		}
		return target.SetStructArray(bytes, StructSize(value.Types), StructAlignment(value.Types))
	case TableField:
		if value.Alignment == 0 && inPlace != nil && inPlace(value.Nested) {
			fields := value.Nested.Table.Fields
			return target.SetTableFunc(len(fields), func(table tafla.Item) error {
				return setAll(table, fields, inPlace)
			})
		}

		table, err := value.Nested.Build()
		if err != nil {
			return err
		}
		if value.Alignment != 0 {
			return target.SetTableAligned(table, value.Alignment)
		}
		return target.SetTable(table)
	case Vector:
		if inPlace != nil && value.allInPlace(inPlace) {
			return target.SetVectorFunc(len(value), value.itemFields(), func(index int, item tafla.Item) error {
				return setAll(item, value[index].Table.Fields, inPlace)
			})
		}

		items := make([]tafla.NestedTable, 0, len(value))
		for _, nested := range value {
			item, err := nested.Build()
			if err != nil {
				return err
			}
			items = append(items, item)
		}
		return target.SetVector(items...)
	default:
		panic("unknown field")
	}
}

// setAll sets the fields on target, as set does
func setAll(target Target, fields []Field, inPlace func(Nested) bool) error {
	for _, field := range fields {
		if err := set(target, field, inPlace); err != nil {
			return err
		}
	}

	return nil
}

// allInPlace returns whether inPlace accepts each item, and they hold as
// many fields, so that SetVectorFunc records them
func (v Vector) allInPlace(inPlace func(Nested) bool) bool {
	for _, item := range v {
		if !inPlace(item) || len(item.Table.Fields) != v.itemFields() {
			return false
		}
	}

	return true
}

// itemFields returns the number of the first item's fields, or 0 for no
// items
func (v Vector) itemFields() int {
	if len(v) == 0 {
		return 0
	}

	return len(v[0].Table.Fields)
}

// Build builds the table to nest, as its builder or its bytes.
func (n Nested) Build() (tafla.NestedTable, error) {
	builder, err := n.Table.Build()
	if err != nil {
		return nil, err
	}

	if n.From == FromBuilder {
		return builder, nil
	}

	bytes, err := builder.Bytes()
	return tafla.TableBytes(bytes), err
}

func boolByte(value bool) uint8 {
	if value {
		return 1
	}

	return 0
}
