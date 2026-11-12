package tafla

import (
	"math"
	"unsafe"
)

// TableBuilder builds a table, which a [ByteBuffer] reads back.
//
// A table is a header with one offset per field, after the table's total
// length and version when its layout has them, followed by the fields in the
// order they're set. Arrays, strings and vectors start with their count, and
// a vector then holds the end of each item. Arrays of scalars and of structs
// are padded so their elements are aligned from the start of the table.
// Fields are read back by index with [ByteBuffer.RootOffset], or
// [ByteBuffer.FieldOffset] for a table nested in another.
//
// Setting a field records it, keeping its value, and [TableBuilder.Bytes]
// then writes every field through a [TableWriter], into bytes of the table's
// exact length. Arrays, strings and the bytes of structs and tables aren't
// copied until then, so their elements must not change in between. Tables
// nested from their builders are written by the same writer, straight into
// this one, when they have its offset type and no header.
//
// A setter returns a *[BuildError], and leaves the builder as it was, when
// the table already holds all its fields or the field would make it too long
// for the layout's offset type: longer than 255 bytes, 64 KiB or 4 GiB, or
// than an int on a 32-bit host.
//
// A builder is used through the pointer that its constructor returns, and
// isn't copied, since it holds the fields of a small table within itself.
type TableBuilder struct {
	// The fields set so far, which a small table holds in the allocation of
	// its builder
	values []field
	// The number of fields that the table holds
	fields int
	// The table's length so far, which is where the next field goes
	length  uint32
	version uint32
	layout  TableLayout
	// The largest alignment that a field needs from the table's start, which
	// a parent pads this table to when it nests it
	alignment Alignment
}

// NewTableBuilder starts a table of fields fields in layout, which its
// readers must use too. Nested tables use the default layout, the zero
// [TableLayout]. It fails with HeaderTooLong if the header alone is too long
// for the layout's offset type, and panics if the layout is versioned, which
// [NewVersionedTableBuilder] builds, or fields is negative.
func NewTableBuilder(fields int, layout TableLayout) (*TableBuilder, error) {
	return newTableBuilder(fields, layout, false, 0)
}

// NewVersionedTableBuilder starts a table of fields fields in layout, with
// version in its header. Readers can check it with [ByteBuffer.Version]
// before reading any field. It fails with VersionTooLarge if the version
// doesn't fit in the layout's offset type, or with HeaderTooLong if the
// header alone is too long for it, and panics if the layout isn't versioned
// or fields is negative.
func NewVersionedTableBuilder(fields int, layout TableLayout, version uint32) (*TableBuilder, error) {
	return newTableBuilder(fields, layout, true, version)
}

func newTableBuilder(fields int, layout TableLayout, versioned bool, version uint32) (*TableBuilder, error) {
	if err := layout.checkVersion(versioned, version); err != nil {
		return nil, err
	}

	header, err := layout.headerLength(fields)
	if err != nil {
		return nil, err
	}

	builder := allocateBuilder(fields)
	builder.fields = fields
	builder.length = header
	builder.version = version
	builder.layout = layout
	builder.alignment = Align1

	return builder, nil
}

// allocateBuilder allocates a builder of fields fields, in one allocation of
// the builder and its fields for a table of up to 4, as each of a vector's
// nested tables is. Against a builder with room for 4 fields, a vector of 100
// tables of 2 fields took 16% less time and 30% fewer bytes, and a table of 4
// fields as long; against room for 2, the table of 4 fields took 17% less
// time, in one allocation rather than two.
func allocateBuilder(fields int) *TableBuilder {
	switch fields {
	case 1:
		with := new(builderWith[[1]field])
		with.builder.values = with.values[:0]
		return &with.builder
	case 2:
		with := new(builderWith[[2]field])
		with.builder.values = with.values[:0]
		return &with.builder
	case 3:
		with := new(builderWith[[3]field])
		with.builder.values = with.values[:0]
		return &with.builder
	case 4:
		with := new(builderWith[[4]field])
		with.builder.values = with.values[:0]
		return &with.builder
	default:
		return &TableBuilder{values: make([]field, 0, fields)}
	}
}

// builderWith is a builder and the array of its fields, which it points into
type builderWith[Fields any] struct {
	builder TableBuilder
	values  Fields
}

// Bytes writes the table, at its exact size, into new bytes. It fails with
// Incomplete if fewer fields were set than the table holds.
func (b *TableBuilder) Bytes() ([]byte, error) {
	if err := b.checkComplete(); err != nil {
		return nil, err
	}

	// The constructor checked the header and the version, and each setter
	// the table's length, so the writer writes the fields as counted
	writer := newTableWriter(nil, int(b.length), b.fields, b.layout, b.version)
	b.writeFields(&writer)

	return writer.Bytes()
}

// SetInt8 sets an int8 field. It fails with TooManyFields if the table
// holds all its fields already, or with TooLong if the field would make it
// too long for its offset type.
func (b *TableBuilder) SetInt8(value int8) error { return b.setScalar(uint64(uint8(value)), 1) }

// SetUint8 sets a uint8 field. It fails as [TableBuilder.SetInt8] does.
func (b *TableBuilder) SetUint8(value uint8) error { return b.setScalar(uint64(value), 1) }

// SetInt16 sets an int16 field. It fails as [TableBuilder.SetInt8] does.
func (b *TableBuilder) SetInt16(value int16) error { return b.setScalar(uint64(uint16(value)), 2) }

// SetUint16 sets a uint16 field. It fails as [TableBuilder.SetInt8] does.
func (b *TableBuilder) SetUint16(value uint16) error { return b.setScalar(uint64(value), 2) }

// SetInt32 sets an int32 field. It fails as [TableBuilder.SetInt8] does.
func (b *TableBuilder) SetInt32(value int32) error { return b.setScalar(uint64(uint32(value)), 4) }

// SetUint32 sets a uint32 field. It fails as [TableBuilder.SetInt8] does.
func (b *TableBuilder) SetUint32(value uint32) error { return b.setScalar(uint64(value), 4) }

// SetInt64 sets an int64 field. It fails as [TableBuilder.SetInt8] does.
func (b *TableBuilder) SetInt64(value int64) error { return b.setScalar(uint64(value), 8) }

// SetUint64 sets a uint64 field. It fails as [TableBuilder.SetInt8] does.
func (b *TableBuilder) SetUint64(value uint64) error { return b.setScalar(value, 8) }

// SetFloat32 sets a float32 field. It fails as [TableBuilder.SetInt8] does.
func (b *TableBuilder) SetFloat32(value float32) error {
	return b.setScalar(uint64(math.Float32bits(value)), 4)
}

// SetFloat64 sets a float64 field. It fails as [TableBuilder.SetInt8] does.
func (b *TableBuilder) SetFloat64(value float64) error {
	return b.setScalar(math.Float64bits(value), 8)
}

// SetInt8Array sets an array field of int8s, which the builder keeps until
// the table is written. It fails as [TableBuilder.SetInt8] does.
func (b *TableBuilder) SetInt8Array(values []int8) error { return setArray(b, values) }

// SetUint8Array sets an array field of uint8s, which the builder keeps until
// the table is written. It fails as [TableBuilder.SetInt8] does.
func (b *TableBuilder) SetUint8Array(values []uint8) error { return setArray(b, values) }

// SetInt16Array sets an array field of int16s, which the builder keeps until
// the table is written. It fails as [TableBuilder.SetInt8] does.
func (b *TableBuilder) SetInt16Array(values []int16) error { return setArray(b, values) }

// SetUint16Array sets an array field of uint16s, which the builder keeps
// until the table is written. It fails as [TableBuilder.SetInt8] does.
func (b *TableBuilder) SetUint16Array(values []uint16) error { return setArray(b, values) }

// SetInt32Array sets an array field of int32s, which the builder keeps until
// the table is written. It fails as [TableBuilder.SetInt8] does.
func (b *TableBuilder) SetInt32Array(values []int32) error { return setArray(b, values) }

// SetUint32Array sets an array field of uint32s, which the builder keeps
// until the table is written. It fails as [TableBuilder.SetInt8] does.
func (b *TableBuilder) SetUint32Array(values []uint32) error { return setArray(b, values) }

// SetInt64Array sets an array field of int64s, which the builder keeps until
// the table is written. It fails as [TableBuilder.SetInt8] does.
func (b *TableBuilder) SetInt64Array(values []int64) error { return setArray(b, values) }

// SetUint64Array sets an array field of uint64s, which the builder keeps
// until the table is written. It fails as [TableBuilder.SetInt8] does.
func (b *TableBuilder) SetUint64Array(values []uint64) error { return setArray(b, values) }

// SetFloat32Array sets an array field of float32s, which the builder keeps
// until the table is written. It fails as [TableBuilder.SetInt8] does.
func (b *TableBuilder) SetFloat32Array(values []float32) error { return setArray(b, values) }

// SetFloat64Array sets an array field of float64s, which the builder keeps
// until the table is written. It fails as [TableBuilder.SetInt8] does.
func (b *TableBuilder) SetFloat64Array(values []float64) error { return setArray(b, values) }

// SetString sets a string field, of its bytes, which are UTF-8 if it is. It
// fails as [TableBuilder.SetInt8] does.
func (b *TableBuilder) SetString(value string) error {
	return b.addField(field{
		data:   unsafe.Pointer(unsafe.StringData(value)),
		length: len(value),
		kind:   stringField,
	})
}

// SetStruct sets a struct field from its bytes: its scalars, little-endian,
// one after another, which are written as they are, without padding. The
// builder keeps the bytes until the table is written. It fails as
// [TableBuilder.SetInt8] does.
func (b *TableBuilder) SetStruct(bytes []byte) error {
	return b.addField(field{data: bytesData(bytes), length: len(bytes), kind: structField})
}

// SetStructArray sets an array of structs of size bytes each, from their
// bytes one after another, which it counts. It's padded so that the structs
// start at a multiple of alignment from the start of the table, which is the
// size of the structs' largest scalar. The builder keeps the bytes until the
// table is written. It fails with NotWholeStructs if size isn't positive or
// the bytes aren't a whole number of structs, or as [TableBuilder.SetInt8]
// does, and panics if alignment isn't 1, 2, 4 or 8.
func (b *TableBuilder) SetStructArray(bytes []byte, size int, alignment Alignment) error {
	alignment.mustBeValid()
	if size <= 0 || len(bytes)%size != 0 {
		return &BuildError{Kind: NotWholeStructs, Length: uint64(len(bytes)), Size: size}
	}

	return b.addField(field{
		data:      bytesData(bytes),
		length:    len(bytes),
		bits:      uint64(size),
		kind:      structArrayField,
		alignment: alignment,
	})
}

// NestedTable is a table to nest with [TableBuilder.SetTable] or in
// [TableBuilder.SetVector]: a *[TableBuilder], or [TableBytes].
//
// A builder with its parent's offset type and no header is written straight
// into its parent. Bytes, and other builders, are copied.
type NestedTable interface {
	nestedTable()
}

func (*TableBuilder) nestedTable() {}

// TableBytes are a table's bytes, as its builder wrote them, to nest in
// another table.
type TableBytes []byte

func (TableBytes) nestedTable() {}

// SetTable embeds a table, from its builder or its bytes, without a length
// prefix. Readers find its fields with [ByteBuffer.FieldOffset] from the
// field's position, which reads offsets of the parent's offset type, so the
// table should have no header and the parent's offset type: that's the
// default layout in a parent of uint32 offsets.
//
// A builder must have all its fields set. Without a header and with this
// table's offset type, it's written straight into this table when this
// table is written, without bytes of its own. Otherwise it's written in its
// own layout, and copied. The builder and the bytes are kept until the table
// is written.
//
// The nested table's arrays are aligned from its own start, so it's padded
// to start at a multiple of an alignment, which they need to be read in
// place. For a builder, that's the largest alignment of its arrays, of
// scalars and of structs, and of its nested tables, or 1 if it has neither.
// For bytes, it's 4, which suits tables of 8-, 16- and 32-bit elements;
// [TableBuilder.SetTableAligned] takes another.
//
// It fails with Incomplete if the builder has fields left to set, or as
// [TableBuilder.SetInt8] does, and panics if the table is nil.
func (b *TableBuilder) SetTable(table NestedTable) error {
	alignment := Align4
	if builder, ok := table.(*TableBuilder); ok && builder != nil {
		alignment = builder.alignment
	}

	return b.setTable(table, alignment)
}

// SetTableAligned embeds a table like [TableBuilder.SetTable], padded to
// start at a multiple of alignment, such as [Align8] for a table of 64-bit
// arrays given as bytes. It fails as SetTable does, and panics if the
// alignment isn't 1, 2, 4 or 8.
func (b *TableBuilder) SetTableAligned(table NestedTable, alignment Alignment) error {
	alignment.mustBeValid()
	return b.setTable(table, alignment)
}

// SetVector writes a vector of tables, from their builders or their bytes,
// after the end of each, which lets readers find any item without reading
// the others. Builders must have all their fields set, and are written as
// [TableBuilder.SetTable] writes them: straight into this table, or in their
// own layout, and copied. Items aren't padded, so their arrays are read in
// place only when they happen to be aligned. The builder keeps the items
// until the table is written, but not the slice of them.
//
// It fails with Incomplete if a builder has fields left to set, or as
// [TableBuilder.SetInt8] does, and panics if an item is nil.
func (b *TableBuilder) SetVector(items ...NestedTable) error {
	for _, item := range items {
		if err := checkNested(item); err != nil {
			return err
		}
	}

	// A copy, so that the caller's slice can change
	items = append([]NestedTable(nil), items...)
	return b.addField(field{data: unsafe.Pointer(unsafe.SliceData(items)), length: len(items), kind: vectorField})
}

func (b *TableBuilder) setTable(table NestedTable, alignment Alignment) error {
	if err := checkNested(table); err != nil {
		return err
	}

	switch table := table.(type) {
	case *TableBuilder:
		return b.addField(field{data: unsafe.Pointer(table), kind: builderField, alignment: alignment})
	default:
		bytes := table.(TableBytes)
		return b.addField(field{data: bytesData(bytes), length: len(bytes), kind: bytesField, alignment: alignment})
	}
}

// checkNested checks that a nested table is a complete builder or bytes
func checkNested(table NestedTable) error {
	switch table := table.(type) {
	case *TableBuilder:
		if table == nil {
			panic("tafla: a nested table is nil")
		}
		return table.checkComplete()
	case TableBytes:
		return nil
	default:
		panic("tafla: a nested table is nil")
	}
}

func (b *TableBuilder) setScalar(bits uint64, size uint8) error {
	return b.addField(field{bits: bits, kind: scalarField, size: size})
}

func setArray[T Element](b *TableBuilder, values []T) error {
	elements := elementBytes(values)
	return b.addField(field{
		data:   bytesData(elements),
		length: len(elements),
		kind:   arrayField,
		size:   uint8(sizeOf[T]()),
	})
}

// addField records a field after the padding that it needs. Checking the
// table's length here, rather than when writing it, reports a field too long
// for the layout from its own setter and leaves the builder as it was. Every
// offset, count, length and item end is at most the table's length, so
// that's the one value to check.
func (b *TableBuilder) addField(f field) error {
	index := len(b.values)
	if index == b.fields {
		return &BuildError{Kind: TooManyFields, Fields: b.fields}
	}

	width := uint64(b.layout.offsetType.Size())
	length := uint64(b.length) + f.padding(uint64(b.length), width) + f.byteLength(width)
	if length > uint64(b.layout.offsetType.MaxValue()) || length > math.MaxInt {
		return &BuildError{Kind: TooLong, Index: index, Length: length, OffsetType: b.layout.offsetType}
	}

	b.length = uint32(length)
	b.alignment = max(b.alignment, f.alignmentFromStart())
	b.values = append(b.values, f)

	return nil
}

func (b *TableBuilder) checkComplete() error {
	if len(b.values) != b.fields {
		return &BuildError{Kind: Incomplete, Set: len(b.values), Fields: b.fields}
	}

	return nil
}

// writeFields writes the fields into the table that writer is writing, each
// at its index
func (b *TableBuilder) writeFields(writer *TableWriter) {
	for index := range b.values {
		b.values[index].write(writer, index, b.layout.offsetType)
	}
}

// writeNested writes the table as a field or vector item of a table of
// offsetType offsets. It's written in place when the writer's StartTable
// writes it as it is, with that offset type and without a header. Otherwise
// it's written by a writer of its own into the bytes that it takes in its
// parent, rather than into bytes of its own that are then copied.
func (b *TableBuilder) writeNested(writer *TableWriter, offsetType OffsetType) {
	if b.layout.offsetType == offsetType && !b.layout.sized && !b.layout.versioned {
		parent := writer.StartTable(b.fields)
		b.writeFields(writer)
		writer.EndTable(parent)
		return
	}

	start := writer.position
	end := start + int(b.length)
	nested := newTableWriter(writer.bytes[start:start:end], int(b.length), b.fields, b.layout, b.version)
	b.writeFields(&nested)
	writer.position = end
}

// writeNestedTable writes a table nested in a table of offsetType offsets,
// after the field's offset or before the item's end
func writeNestedTable(writer *TableWriter, table NestedTable, offsetType OffsetType) {
	switch table := table.(type) {
	case *TableBuilder:
		table.writeNested(writer, offsetType)
	case TableBytes:
		writer.copyTable(table)
	}
}

// bytesData returns the address of the first of bytes, which a field keeps
// with their length
func bytesData(bytes []byte) unsafe.Pointer {
	return unsafe.Pointer(unsafe.SliceData(bytes))
}

type fieldKind uint8

const (
	// bits holds the scalar's bits, and size its size
	scalarField fieldKind = iota
	// data and length hold the elements' bytes, in the host's byte order,
	// and size the elements' size
	arrayField
	// data and length hold the string's bytes
	stringField
	// data and length hold the struct's bytes
	structField
	// data and length hold the structs' bytes, bits their size, and
	// alignment their alignment
	structArrayField
	// data holds the *TableBuilder, and alignment the table's alignment
	builderField
	// data and length hold the table's bytes, and alignment its alignment
	bytesField
	// data and length hold the items, a []NestedTable
	vectorField
)

// field is a field that a setter recorded, in 32 bytes: data points at its
// value, so that the garbage collector keeps it while the builder holds it,
// and the kind says what it points at
type field struct {
	data      unsafe.Pointer
	length    int
	bits      uint64
	kind      fieldKind
	size      uint8
	alignment Alignment
}

func (f *field) bytes() []byte {
	return unsafe.Slice((*byte)(f.data), f.length)
}

func (f *field) items() []NestedTable {
	return unsafe.Slice((*NestedTable)(f.data), f.length)
}

// alignmentFromStart returns the alignment that the field needs from the
// table's start
func (f *field) alignmentFromStart() Alignment {
	switch f.kind {
	case arrayField:
		return Alignment(f.size)
	case structArrayField, builderField, bytesField:
		return f.alignment
	default:
		return Align1
	}
}

// padding returns the padding before the field when it would start at
// offset, which aligns an array's elements after its count and a nested
// table's start
func (f *field) padding(offset, width uint64) uint64 {
	switch f.kind {
	case arrayField:
		return Alignment(f.size).padding(offset + width)
	case structArrayField:
		return f.alignment.padding(offset + width)
	case builderField, bytesField:
		return f.alignment.padding(offset)
	default:
		return 0
	}
}

// byteLength returns the field's byte length after its padding, which a
// vector that nests a table many times can make longer than any table
func (f *field) byteLength(width uint64) uint64 {
	switch f.kind {
	case scalarField:
		return uint64(f.size)
	case arrayField, stringField, structArrayField:
		return width + uint64(f.length)
	case builderField:
		return uint64((*TableBuilder)(f.data).length)
	case vectorField:
		length := width
		for _, item := range f.items() {
			length += width + nestedLength(item)
		}
		return length
	default:
		return uint64(f.length)
	}
}

// nestedLength returns the byte length of a nested table
func nestedLength(table NestedTable) uint64 {
	if builder, ok := table.(*TableBuilder); ok {
		return uint64(builder.length)
	}

	return uint64(len(table.(TableBytes)))
}

// write writes the field as field index of the table that writer is
// writing, of offsetType offsets
func (f *field) write(writer *TableWriter, index int, offsetType OffsetType) {
	switch f.kind {
	case scalarField:
		switch f.size {
		case 1:
			writer.SetUint8(index, uint8(f.bits))
		case 2:
			writer.SetUint16(index, uint16(f.bits))
		case 4:
			writer.SetUint32(index, uint32(f.bits))
		default:
			writer.SetUint64(index, f.bits)
		}
	case arrayField:
		f.writeArray(writer, index)
	case stringField:
		writer.SetString(index, unsafe.String((*byte)(f.data), f.length))
	case structField:
		writer.SetStruct(index, f.bytes())
	case structArrayField:
		size := int(f.bits)
		copy(writer.ReserveStructArray(index, f.length/size, size, f.alignment), f.bytes())
	case builderField:
		writer.SetTable(index, f.alignment)
		(*TableBuilder)(f.data).writeNested(writer, offsetType)
	case bytesField:
		writer.SetTable(index, f.alignment)
		writer.copyTable(f.bytes())
	case vectorField:
		items := f.items()
		vector := writer.SetVector(index, len(items))
		for item, table := range items {
			writeNestedTable(writer, table, offsetType)
			writer.SetItemEnd(vector, item)
		}
	}
}

// writeArray writes an array field from its elements' bytes in the host's
// byte order, as elements of their size, which are aligned to it in memory
func (f *field) writeArray(writer *TableWriter, index int) {
	count := f.length / int(f.size)
	switch f.size {
	case 1:
		setElements(writer, index, f.bytes())
	case 2:
		setElements(writer, index, unsafe.Slice((*uint16)(f.data), count))
	case 4:
		setElements(writer, index, unsafe.Slice((*uint32)(f.data), count))
	default:
		setElements(writer, index, unsafe.Slice((*uint64)(f.data), count))
	}
}
