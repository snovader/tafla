package prototype

import tafla "github.com/snovader/tafla/go"

// Item is a table that a [TableBuilder] writes in place, as [tafla.Item] is
// one that tafla.TableBuilder records in place. Its setters are the
// builder's, and write each field straight into the builder's bytes.
//
// An Item is valid only while the function that it's given to runs, and the
// builder itself mustn't be used within it.
type Item struct {
	builder *TableBuilder
}

func (i Item) SetInt8(value int8) error       { return i.builder.SetInt8(value) }
func (i Item) SetUint8(value uint8) error     { return i.builder.SetUint8(value) }
func (i Item) SetInt16(value int16) error     { return i.builder.SetInt16(value) }
func (i Item) SetUint16(value uint16) error   { return i.builder.SetUint16(value) }
func (i Item) SetInt32(value int32) error     { return i.builder.SetInt32(value) }
func (i Item) SetUint32(value uint32) error   { return i.builder.SetUint32(value) }
func (i Item) SetInt64(value int64) error     { return i.builder.SetInt64(value) }
func (i Item) SetUint64(value uint64) error   { return i.builder.SetUint64(value) }
func (i Item) SetFloat32(value float32) error { return i.builder.SetFloat32(value) }
func (i Item) SetFloat64(value float64) error { return i.builder.SetFloat64(value) }

func (i Item) SetInt8Array(values []int8) error       { return i.builder.SetInt8Array(values) }
func (i Item) SetUint8Array(values []uint8) error     { return i.builder.SetUint8Array(values) }
func (i Item) SetInt16Array(values []int16) error     { return i.builder.SetInt16Array(values) }
func (i Item) SetUint16Array(values []uint16) error   { return i.builder.SetUint16Array(values) }
func (i Item) SetInt32Array(values []int32) error     { return i.builder.SetInt32Array(values) }
func (i Item) SetUint32Array(values []uint32) error   { return i.builder.SetUint32Array(values) }
func (i Item) SetInt64Array(values []int64) error     { return i.builder.SetInt64Array(values) }
func (i Item) SetUint64Array(values []uint64) error   { return i.builder.SetUint64Array(values) }
func (i Item) SetFloat32Array(values []float32) error { return i.builder.SetFloat32Array(values) }
func (i Item) SetFloat64Array(values []float64) error { return i.builder.SetFloat64Array(values) }

func (i Item) SetString(value string) error { return i.builder.SetString(value) }
func (i Item) SetStruct(bytes []byte) error { return i.builder.SetStruct(bytes) }

func (i Item) SetStructArray(bytes []byte, size int, alignment tafla.Alignment) error {
	return i.builder.SetStructArray(bytes, size, alignment)
}

func (i Item) SetTable(table NestedTable) error { return i.builder.SetTable(table) }

func (i Item) SetTableAligned(table NestedTable, alignment tafla.Alignment) error {
	return i.builder.SetTableAligned(table, alignment)
}

func (i Item) SetVector(items ...NestedTable) error { return i.builder.SetVector(items...) }

func (i Item) SetTableFunc(fields int, fill func(table Item) error) error {
	return i.builder.SetTableFunc(fields, fill)
}

func (i Item) SetVectorFunc(count, fields int, fill func(index int, item Item) error) error {
	return i.builder.SetVectorFunc(count, fields, fill)
}
