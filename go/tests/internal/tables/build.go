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

// Set sets a field on builder, which keeps its value, or the bytes of
// structs.
func Set(builder *tafla.TableBuilder, field Field) error {
	switch value := field.(type) {
	case Int8:
		return builder.SetInt8(int8(value))
	case Uint8:
		return builder.SetUint8(uint8(value))
	case Int16:
		return builder.SetInt16(int16(value))
	case Uint16:
		return builder.SetUint16(uint16(value))
	case Int32:
		return builder.SetInt32(int32(value))
	case Uint32:
		return builder.SetUint32(uint32(value))
	case Int64:
		return builder.SetInt64(int64(value))
	case Uint64:
		return builder.SetUint64(uint64(value))
	case Float32:
		return builder.SetFloat32(float32(value))
	case Float64:
		return builder.SetFloat64(float64(value))
	case Bool:
		return builder.SetUint8(boolByte(bool(value)))
	case Int8Array:
		return builder.SetInt8Array(value)
	case Uint8Array:
		return builder.SetUint8Array(value)
	case Int16Array:
		return builder.SetInt16Array(value)
	case Uint16Array:
		return builder.SetUint16Array(value)
	case Int32Array:
		return builder.SetInt32Array(value)
	case Uint32Array:
		return builder.SetUint32Array(value)
	case Int64Array:
		return builder.SetInt64Array(value)
	case Uint64Array:
		return builder.SetUint64Array(value)
	case Float32Array:
		return builder.SetFloat32Array(value)
	case Float64Array:
		return builder.SetFloat64Array(value)
	case String:
		return builder.SetString(string(value))
	case Struct:
		return builder.SetStruct(StructBytes(value))
	case StructArray:
		var bytes []byte
		for _, item := range value.Items {
			bytes = append(bytes, StructBytes(item)...)
		}
		return builder.SetStructArray(bytes, StructSize(value.Types), StructAlignment(value.Types))
	case TableField:
		table, err := value.Nested.Build()
		if err != nil {
			return err
		}
		if value.Alignment != 0 {
			return builder.SetTableAligned(table, value.Alignment)
		}
		return builder.SetTable(table)
	case Vector:
		items := make([]tafla.NestedTable, 0, len(value))
		for _, nested := range value {
			item, err := nested.Build()
			if err != nil {
				return err
			}
			items = append(items, item)
		}
		return builder.SetVector(items...)
	default:
		panic("unknown field")
	}
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
