package tafla

// Item is a table that a [TableBuilder] records in place, as one of its
// fields with [TableBuilder.SetTableFunc] or as each item of a vector with
// [TableBuilder.SetVectorFunc], which give it to the function that sets its
// fields. Its setters are the builder's, taking the same values and failing
// as they fail, and record each field straight among the builder's own, so
// the table needs no builder of its own. The fields are set in order, and
// counted against the table's own number of fields and its own length, as
// a builder of it would count them.
//
// An Item is valid only while that function runs, and the builder itself
// mustn't be used within it.
type Item struct {
	builder *TableBuilder
}

// SetInt8 sets an int8 field, as [TableBuilder.SetInt8] does.
func (i Item) SetInt8(value int8) error { return i.builder.SetInt8(value) }

// SetUint8 sets a uint8 field, as [TableBuilder.SetUint8] does.
func (i Item) SetUint8(value uint8) error { return i.builder.SetUint8(value) }

// SetInt16 sets an int16 field, as [TableBuilder.SetInt16] does.
func (i Item) SetInt16(value int16) error { return i.builder.SetInt16(value) }

// SetUint16 sets a uint16 field, as [TableBuilder.SetUint16] does.
func (i Item) SetUint16(value uint16) error { return i.builder.SetUint16(value) }

// SetInt32 sets an int32 field, as [TableBuilder.SetInt32] does.
func (i Item) SetInt32(value int32) error { return i.builder.SetInt32(value) }

// SetUint32 sets a uint32 field, as [TableBuilder.SetUint32] does.
func (i Item) SetUint32(value uint32) error { return i.builder.SetUint32(value) }

// SetInt64 sets an int64 field, as [TableBuilder.SetInt64] does.
func (i Item) SetInt64(value int64) error { return i.builder.SetInt64(value) }

// SetUint64 sets a uint64 field, as [TableBuilder.SetUint64] does.
func (i Item) SetUint64(value uint64) error { return i.builder.SetUint64(value) }

// SetFloat32 sets a float32 field, as [TableBuilder.SetFloat32] does.
func (i Item) SetFloat32(value float32) error { return i.builder.SetFloat32(value) }

// SetFloat64 sets a float64 field, as [TableBuilder.SetFloat64] does.
func (i Item) SetFloat64(value float64) error { return i.builder.SetFloat64(value) }

// SetInt8Array sets an array field of int8s, as
// [TableBuilder.SetInt8Array] does.
func (i Item) SetInt8Array(values []int8) error { return i.builder.SetInt8Array(values) }

// SetUint8Array sets an array field of uint8s, as
// [TableBuilder.SetUint8Array] does.
func (i Item) SetUint8Array(values []uint8) error { return i.builder.SetUint8Array(values) }

// SetInt16Array sets an array field of int16s, as
// [TableBuilder.SetInt16Array] does.
func (i Item) SetInt16Array(values []int16) error { return i.builder.SetInt16Array(values) }

// SetUint16Array sets an array field of uint16s, as
// [TableBuilder.SetUint16Array] does.
func (i Item) SetUint16Array(values []uint16) error { return i.builder.SetUint16Array(values) }

// SetInt32Array sets an array field of int32s, as
// [TableBuilder.SetInt32Array] does.
func (i Item) SetInt32Array(values []int32) error { return i.builder.SetInt32Array(values) }

// SetUint32Array sets an array field of uint32s, as
// [TableBuilder.SetUint32Array] does.
func (i Item) SetUint32Array(values []uint32) error { return i.builder.SetUint32Array(values) }

// SetInt64Array sets an array field of int64s, as
// [TableBuilder.SetInt64Array] does.
func (i Item) SetInt64Array(values []int64) error { return i.builder.SetInt64Array(values) }

// SetUint64Array sets an array field of uint64s, as
// [TableBuilder.SetUint64Array] does.
func (i Item) SetUint64Array(values []uint64) error { return i.builder.SetUint64Array(values) }

// SetFloat32Array sets an array field of float32s, as
// [TableBuilder.SetFloat32Array] does.
func (i Item) SetFloat32Array(values []float32) error { return i.builder.SetFloat32Array(values) }

// SetFloat64Array sets an array field of float64s, as
// [TableBuilder.SetFloat64Array] does.
func (i Item) SetFloat64Array(values []float64) error { return i.builder.SetFloat64Array(values) }

// SetString sets a string field, as [TableBuilder.SetString] does.
func (i Item) SetString(value string) error { return i.builder.SetString(value) }

// SetStruct sets a struct field from its bytes, as [TableBuilder.SetStruct]
// does.
func (i Item) SetStruct(bytes []byte) error { return i.builder.SetStruct(bytes) }

// SetStructArray sets an array of structs from their bytes, as
// [TableBuilder.SetStructArray] does.
func (i Item) SetStructArray(bytes []byte, size int, alignment Alignment) error {
	return i.builder.SetStructArray(bytes, size, alignment)
}

// SetTable nests a table from its builder or its bytes, as
// [TableBuilder.SetTable] does.
func (i Item) SetTable(table NestedTable) error { return i.builder.SetTable(table) }

// SetTableAligned nests a table padded to start at a multiple of alignment,
// as [TableBuilder.SetTableAligned] does.
func (i Item) SetTableAligned(table NestedTable, alignment Alignment) error {
	return i.builder.SetTableAligned(table, alignment)
}

// SetVector sets a vector of tables from their builders or their bytes, as
// [TableBuilder.SetVector] does.
func (i Item) SetVector(items ...NestedTable) error { return i.builder.SetVector(items...) }

// SetTableFunc records a table nested in this one in place, as
// [TableBuilder.SetTableFunc] does.
func (i Item) SetTableFunc(fields int, fill func(table Item) error) error {
	return i.builder.SetTableFunc(fields, fill)
}

// SetVectorFunc records a vector of tables in place, as
// [TableBuilder.SetVectorFunc] does.
func (i Item) SetVectorFunc(count, fields int, fill func(index int, item Item) error) error {
	return i.builder.SetVectorFunc(count, fields, fill)
}
