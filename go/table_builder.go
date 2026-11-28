package tafla

import (
	"encoding/binary"
	"math"
	"strconv"
	"unsafe"
)

// TableBuilder builds a table, which a [TableReader] reads back.
//
// A table is a header with one offset per field, after the table's total
// length and version when its layout has them, followed by the fields in the
// order they're set. Arrays, strings and vectors start with their count, and
// a vector then holds the end of each item. Arrays of scalars and of structs
// are padded so their elements are aligned from the start of the table.
// Fields are read back by index with [TableReader.RootOffset], or
// [TableReader.FieldOffset] for a table nested in another.
//
// Setting a field copies its value into the builder's buffer, after the
// fields set before it, and writes its offset, so the builder keeps nothing
// of its caller's: a slice can change, and a nested builder be reset, once
// the setter returns. The buffer then holds the table, which
// [TableBuilder.Bytes] returns without copying it.
//
// [TableBuilder.Reset] starts the next table after the bytes of a slice of
// its caller's, into its capacity when it has room, as append does. So a
// loop that hands each table's bytes back to Reset allocates once, and a
// slice made with a capacity gives a first large table its room. The zero
// TableBuilder holds a table of no fields until it's reset, so a builder can
// be a value of its own, on the stack or within a struct, rather than the
// pointer that [NewTableBuilder] returns.
//
// A setter returns a *[BuildError], and leaves the builder as it was, when
// the table already holds all its fields or the field would make it too long
// for the layout's offset type: longer than 255 bytes, 64 KiB or 4 GiB, or
// than an int holds after the bytes before it, on a 32-bit host.
//
// [TableBuilder.SetTableFunc] and [TableBuilder.SetVectorFunc] write a
// nested table, or each item of a vector, in place, among the builder's own
// fields, through the [Item] that their function sets the fields on, rather
// than from a builder of its own, which would be copied in.
//
// A builder that has started a table isn't copied, since the copy would
// share its buffer.
type TableBuilder struct {
	// The bytes before the table, and the table so far: the slice that Reset
	// was given, and then new bytes, once its capacity has no room for a field
	buffer []byte
	// The counters of the table whose fields are being set: the builder's own,
	// or those of a table nested in place, which counts its fields as a
	// builder of its own would
	counters
	// The longest table that the offset type addresses, and an int holds
	// after the bytes before the table
	limit  uint32
	layout TableLayout
}

// counters are those of the table whose fields are being set, which a table
// nested in place sets aside
type counters struct {
	// Where the table starts in the buffer, and where its first offset is
	start, slots int
	// The number of the table's fields, and of those set
	fields, set uint32
	// The largest alignment that a field needs from the table's start, which a
	// parent pads this table to when it nests it
	alignment Alignment
}

// checkpoint is what a table nested in place that fails cuts the builder
// back to: its counters, and the buffer's length
type checkpoint struct {
	counters
	written int
}

// NewTableBuilder starts a table of fields fields in layout, which its
// readers must use too, with the layout's version in its header when it has
// one, as [TableBuilder.Reset] starts one into new bytes. Nested tables use
// the default layout, the zero [TableLayout]. It fails with VersionTooLarge
// if the layout's version doesn't fit in its offset type, or with
// HeaderTooLong if the header alone is too long for it, and panics if fields
// is negative.
func NewTableBuilder(fields int, layout TableLayout) (*TableBuilder, error) {
	// Allocated here, where it's inlined, so that it stays on the caller's
	// stack unless the caller keeps it
	return new(TableBuilder).reset(nil, fields, layout)
}

// Reset starts a table of fields fields in layout, as [NewTableBuilder]
// does, after the bytes of dst, and drops the table that the builder held,
// finished or not. The table is written into dst's capacity while it has
// room, and into new bytes otherwise, as append writes, which start with
// room for 256 bytes, or for 64 bytes a field after the header, up to
// 64 KiB, if that's more. [TableBuilder.Bytes] then returns dst with the
// table appended.
//
// So passing the bytes that Bytes returned, from their start, writes the
// next table into them, and a dst made with a capacity gives a table that
// room. The bytes in dst's capacity past its length are overwritten, so they
// mustn't be in use: bytes that the builder returned, the caller's own, or
// those of another builder. The bytes before the table aren't changed.
//
// The table's arrays are aligned from its start, so they're aligned in
// memory only when it starts at a multiple of 8 from an aligned address, as
// it does after an empty dst. Otherwise, readers copy them.
//
// It fails as NewTableBuilder does, leaving the builder as it was.
func (b *TableBuilder) Reset(dst []byte, fields int, layout TableLayout) error {
	_, err := b.reset(dst, fields, layout)
	return err
}

// reset is Reset, returning the builder, or nil if it fails, in one call
// that NewTableBuilder inlines
func (b *TableBuilder) reset(dst []byte, fields int, layout TableLayout) (*TableBuilder, error) {
	if err := layout.checkVersion(); err != nil {
		return nil, err
	}
	header, err := layout.headerLength(fields)
	if err != nil {
		return nil, err
	}

	b.begin(dst, fields, header, layout)
	return b, nil
}

// begin starts a table of fields fields after dst, whose header of header
// bytes its layout checked, with the header written but for its length,
// which Bytes writes, and its offsets, which each setter writes
func (b *TableBuilder) begin(dst []byte, fields int, header uint32, layout TableLayout) {
	at := len(dst)
	b.buffer = dst
	if int(header) > cap(dst)-at {
		b.grow(firstRoom(header, fields))
	}
	b.buffer = b.buffer[:at+int(header)]

	if layout.versioned {
		width := layout.offsetType.Size()
		putSlot(b.buffer, at+layout.HeaderSize()-width, width, layout.version)
	}

	// The header check keeps fields within a uint32
	b.counters = counters{start: at, slots: at + layout.HeaderSize(), fields: uint32(fields), alignment: Align1}
	b.limit = uint32(min(layout.maxLength(), uint64(math.MaxInt-at)))
	b.layout = layout
}

// firstRoom returns the room that new bytes start with for a table of fields
// fields after its header of header bytes
func firstRoom(header uint32, fields int) int {
	// The header check keeps the header and its fields' room within an int
	room := min(uint64(fields)*roomForAField, mostRoomForFields)
	return max(firstCapacity, int(uint64(header)+room))
}

// Bytes returns the table, after the bytes of the slice that
// [TableBuilder.Reset] was given, or alone for a builder from
// [NewTableBuilder], with its length written into a sized header. They're
// the builder's buffer, rather than a copy, with the capacity that it has
// past the table, which the builder doesn't write to again unless they're
// given back to Reset: bytes.Clone copies a small table to keep it without
// that room. It fails with Incomplete if fewer fields were set than the
// table holds.
func (b *TableBuilder) Bytes() ([]byte, error) {
	if err := b.checkComplete(); err != nil {
		return nil, err
	}

	if b.layout.sized {
		putSlot(b.buffer, b.start, b.layout.offsetType.Size(), uint32(len(b.buffer)-b.start))
	}
	// A table of no bytes, after none, is empty rather than nil, as tables are
	if b.buffer == nil {
		return []byte{}, nil
	}

	return b.buffer, nil
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

// SetInt8Array sets an array field of int8s. It fails as
// [TableBuilder.SetInt8] does.
func (b *TableBuilder) SetInt8Array(values []int8) error {
	return b.setArray(elementBytes(values), 1)
}

// SetUint8Array sets an array field of uint8s. It fails as
// [TableBuilder.SetInt8] does.
func (b *TableBuilder) SetUint8Array(values []uint8) error { return b.setArray(values, 1) }

// SetInt16Array sets an array field of int16s. It fails as
// [TableBuilder.SetInt8] does.
func (b *TableBuilder) SetInt16Array(values []int16) error {
	return b.setArray(elementBytes(values), 2)
}

// SetUint16Array sets an array field of uint16s. It fails as
// [TableBuilder.SetInt8] does.
func (b *TableBuilder) SetUint16Array(values []uint16) error {
	return b.setArray(elementBytes(values), 2)
}

// SetInt32Array sets an array field of int32s. It fails as
// [TableBuilder.SetInt8] does.
func (b *TableBuilder) SetInt32Array(values []int32) error {
	return b.setArray(elementBytes(values), 4)
}

// SetUint32Array sets an array field of uint32s. It fails as
// [TableBuilder.SetInt8] does.
func (b *TableBuilder) SetUint32Array(values []uint32) error {
	return b.setArray(elementBytes(values), 4)
}

// SetInt64Array sets an array field of int64s. It fails as
// [TableBuilder.SetInt8] does.
func (b *TableBuilder) SetInt64Array(values []int64) error {
	return b.setArray(elementBytes(values), 8)
}

// SetUint64Array sets an array field of uint64s. It fails as
// [TableBuilder.SetInt8] does.
func (b *TableBuilder) SetUint64Array(values []uint64) error {
	return b.setArray(elementBytes(values), 8)
}

// SetFloat32Array sets an array field of float32s. It fails as
// [TableBuilder.SetInt8] does.
func (b *TableBuilder) SetFloat32Array(values []float32) error {
	return b.setArray(elementBytes(values), 4)
}

// SetFloat64Array sets an array field of float64s. It fails as
// [TableBuilder.SetInt8] does.
func (b *TableBuilder) SetFloat64Array(values []float64) error {
	return b.setArray(elementBytes(values), 8)
}

// SetBoolArray sets an array field of bools, written as a uint8 array of 1
// for each true value and 0 for each false one. It fails as
// [TableBuilder.SetInt8] does.
func (b *TableBuilder) SetBoolArray(values []bool) error { return b.setArray(boolBytes(values), 1) }

// SetString sets a string field, of its bytes, which are UTF-8 if it is. It
// fails as [TableBuilder.SetInt8] does.
func (b *TableBuilder) SetString(value string) error {
	return b.setCounted(unsafe.Slice(unsafe.StringData(value), len(value)), len(value), Align1)
}

// SetStringVector sets a vector of strings: the end of each string, which
// lets readers find any string without reading the others, and then each
// string's bytes, which are UTF-8 if it is. It takes no more bytes for each
// string than its end, where a vector of tables of one string takes each
// table's offset and the string's count too. It fails as
// [TableBuilder.SetInt8] does.
func (b *TableBuilder) SetStringVector(values []string) error {
	if b.set == b.fields {
		return b.tooManyFields()
	}
	// The strings' bytes, up to a length past any table, which fails
	strings := uint64(0)
	for _, value := range values {
		strings = min(strings+uint64(len(value)), past)
	}
	// The count, the end of each string, and the strings
	width := b.layout.offsetType.Size()
	position := len(b.buffer) - b.start
	length := uint64(position) + (uint64(len(values))+1)*uint64(width) + strings
	if length > uint64(b.limit) {
		return b.tooLong(length)
	}

	buffer, vector := b.reserve(int(length) - position)
	b.putOffset(buffer, uint32(position))
	putSlot(buffer, vector, width, uint32(len(values)))

	// Each end counts from the vector's count
	at := vector + (len(values)+1)*width
	for index, value := range values {
		at += copy(buffer[at:], value)
		putSlot(buffer, vector+(index+1)*width, width, uint32(at-vector))
	}
	b.setField(Align1)

	return nil
}

// SetStruct sets a struct field from its bytes: its scalars, little-endian,
// one after another, which are written as they are, without padding. It
// fails as [TableBuilder.SetInt8] does.
func (b *TableBuilder) SetStruct(bytes []byte) error {
	if b.set == b.fields {
		return b.tooManyFields()
	}
	position := len(b.buffer) - b.start
	length := uint64(position) + uint64(len(bytes))
	if length > uint64(b.limit) {
		return b.tooLong(length)
	}

	buffer, at := b.reserve(len(bytes))
	b.putOffset(buffer, uint32(position))
	copy(buffer[at:], bytes)
	b.setField(Align1)

	return nil
}

// SetStructArray sets an array of structs of size bytes each, from their
// bytes one after another, which it counts. It's padded so that the structs
// start at a multiple of alignment from the start of the table, which is the
// size of the structs' largest scalar. It fails with NotWholeStructs if size
// isn't positive or the bytes aren't a whole number of structs, or as
// [TableBuilder.SetInt8] does, and panics if alignment isn't 1, 2, 4 or 8.
func (b *TableBuilder) SetStructArray(bytes []byte, size int, alignment Alignment) error {
	alignment.mustBeValid()
	if size <= 0 || len(bytes)%size != 0 {
		return &BuildError{Kind: NotWholeStructs, Length: uint64(len(bytes)), Size: size}
	}

	return b.setCounted(bytes, len(bytes)/size, alignment)
}

// setScalar writes a scalar field of size bytes, its bits' lowest
func (b *TableBuilder) setScalar(bits uint64, size int) error {
	if b.set == b.fields {
		return b.tooManyFields()
	}
	at := len(b.buffer)
	position := at - b.start
	length := uint64(position) + uint64(size)
	if length > uint64(b.limit) {
		return b.tooLong(length)
	}

	// reserve's work, written out, since a call to it doesn't inline
	if size > cap(b.buffer)-at {
		b.grow(size)
	}
	buffer := b.buffer[:at+size]
	b.buffer = buffer

	b.putOffset(buffer, uint32(position))
	switch size {
	case 1:
		buffer[at] = uint8(bits)
	case 2:
		binary.LittleEndian.PutUint16(buffer[at:], uint16(bits))
	case 4:
		binary.LittleEndian.PutUint32(buffer[at:], uint32(bits))
	default:
		binary.LittleEndian.PutUint64(buffer[at:], bits)
	}
	b.set++

	return nil
}

// setArray writes an array field from its elements' bytes in the host's
// byte order, as elements of size bytes. It isn't generic, so that the
// compiler sees that it doesn't keep the builder.
func (b *TableBuilder) setArray(elements []byte, size int) error {
	// Copied as they are in the table's byte order
	if hostLittleEndian || size == 1 {
		return b.setCounted(elements, len(elements)/size, Alignment(size))
	}

	if b.set == b.fields {
		return b.tooManyFields()
	}
	position, padding, length := b.countedLength(len(elements), Alignment(size))
	if length > uint64(b.limit) {
		return b.tooLong(length)
	}

	buffer, at := b.reserve(int(length) - position)
	at = b.startCounted(buffer, at, position, padding, len(elements)/size)
	putLittleEndian(buffer[at:at+len(elements)], elements, size)
	b.setField(Alignment(size))

	return nil
}

// setCounted writes a field of bytes that start with their count: an array,
// a string or an array of structs, padded so that they start at a multiple
// of alignment from the table's start
func (b *TableBuilder) setCounted(bytes []byte, count int, alignment Alignment) error {
	if b.set == b.fields {
		return b.tooManyFields()
	}
	position, padding, length := b.countedLength(len(bytes), alignment)
	if length > uint64(b.limit) {
		return b.tooLong(length)
	}

	buffer, at := b.reserve(int(length) - position)
	at = b.startCounted(buffer, at, position, padding, count)
	copy(buffer[at:], bytes)
	b.setField(alignment)

	return nil
}

// countedLength returns the table's length so far, the padding of a field of
// bytes bytes after its count, aligned to alignment, and the table's length
// with it
func (b *TableBuilder) countedLength(bytes int, alignment Alignment) (position int, padding, length uint64) {
	width := uint64(b.layout.offsetType.Size())
	position = len(b.buffer) - b.start
	padding = alignment.padding(uint64(position) + width)

	return position, padding, uint64(position) + padding + width + uint64(bytes)
}

// startCounted writes the padding, the offset and the count of a field that
// starts at position in the table, and at at in the buffer, and returns
// where its bytes go
func (b *TableBuilder) startCounted(buffer []byte, at, position int, padding uint64, count int) int {
	clear(buffer[at : at+int(padding)])
	at += int(padding)
	b.putOffset(buffer, uint32(position)+uint32(padding))

	width := b.layout.offsetType.Size()
	putSlot(buffer, at, width, uint32(count))

	return at + width
}

// setField counts a field that was written, which needs alignment from the
// table's start
func (b *TableBuilder) setField(alignment Alignment) {
	b.alignment = max(b.alignment, alignment)
	b.set++
}

// NestedTable is a table to nest with [TableBuilder.SetTable] or in
// [TableBuilder.SetVector]: a *[TableBuilder], or [TableBytes]. Either is
// copied in when it's set.
type NestedTable interface {
	nestedTable()
}

func (*TableBuilder) nestedTable() {}

// TableBytes are a table's bytes, as its builder wrote them, to nest in
// another table.
type TableBytes []byte

func (TableBytes) nestedTable() {}

// SetTable embeds a table, from its builder or its bytes, without a length
// prefix. Readers find its fields with [TableReader.FieldOffset] from the
// field's position, which reads offsets of the parent's offset type, so the
// table should have no header and the parent's offset type: that's the
// default layout in a parent of uint32 offsets.
//
// A builder must have all its fields set, and its table is copied in as its
// [TableBuilder.Bytes] would return it, in its own layout. So nesting
// builders in builders copies each table once for each builder that it's
// nested in: [TableBuilder.SetTableFunc] writes a nested table in place,
// without copying it.
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
		// At least 1, for the zero TableBuilder
		alignment = max(builder.alignment, Align1)
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

func (b *TableBuilder) setTable(table NestedTable, alignment Alignment) error {
	if err := checkNested(table); err != nil {
		return err
	}
	if b.set == b.fields {
		return b.tooManyFields()
	}
	bytes := nestedBytes(table)
	position := len(b.buffer) - b.start
	padding := alignment.padding(uint64(position))
	length := uint64(position) + padding + uint64(len(bytes))
	if length > uint64(b.limit) {
		return b.tooLong(length)
	}

	buffer, at := b.reserve(int(length) - position)
	clear(buffer[at : at+int(padding)])
	at += int(padding)
	b.putOffset(buffer, uint32(position)+uint32(padding))
	putNested(buffer[at:at+len(bytes)], table, bytes)
	b.setField(alignment)

	return nil
}

// SetVector writes a vector of tables, from their builders or their bytes,
// after the end of each, which lets readers find any item without reading
// the others. Builders must have all their fields set, and are copied in as
// [TableBuilder.SetTable] copies them. Items aren't padded, so their arrays
// are read in place only when they happen to be aligned.
//
// It fails with Incomplete if a builder has fields left to set, or as
// [TableBuilder.SetInt8] does, and panics if an item is nil.
func (b *TableBuilder) SetVector(items ...NestedTable) error {
	for _, item := range items {
		if err := checkNested(item); err != nil {
			return err
		}
	}
	if b.set == b.fields {
		return b.tooManyFields()
	}
	width := b.layout.offsetType.Size()
	position := len(b.buffer) - b.start
	length := uint64(position) + uint64(width)
	for _, item := range items {
		length += uint64(width) + uint64(len(nestedBytes(item)))
	}
	if length > uint64(b.limit) {
		return b.tooLong(length)
	}

	buffer, vector := b.reserve(int(length) - position)
	b.putOffset(buffer, uint32(position))
	putSlot(buffer, vector, width, uint32(len(items)))

	// Each end counts from the vector's count, after the ends
	at := vector + (len(items)+1)*width
	for index, item := range items {
		bytes := nestedBytes(item)
		putNested(buffer[at:at+len(bytes)], item, bytes)
		at += len(bytes)
		putSlot(buffer, vector+(index+1)*width, width, uint32(at-vector))
	}
	b.setField(Align1)

	return nil
}

// SetTableFunc writes a table of fields fields nested in this one, in
// place, as [TableBuilder.SetTable] nests a builder of them in the default
// layout of this table's offset type, but without a builder of its own: fill
// sets its fields, in order, on the [Item] that it's given, and each is
// written straight into this builder's bytes. The table is padded to start
// at a multiple of its largest alignment, as SetTable pads a builder, so the
// bytes are those that SetTable writes.
//
// That alignment is known once its fields are set, so the table is written
// unpadded, and then moved forward by its padding, which copies it again.
// [TableBuilder.SetTableFuncAligned] pads it before its fields, for an
// alignment that its caller knows.
//
// fill sets the fields on the Item, not on this builder, and the Item is
// valid only while fill runs.
//
// It fails with TooManyFields if this table holds all its fields already,
// with HeaderTooLong if the nested table's offsets alone are too long for
// the offset type, with Incomplete if fill leaves fields of the nested table
// unset, with the error of a setter of the Item, or with TooLong if the
// nested table makes this one too long, and returns what fill returns,
// leaving the builder as it was in each case. It panics if fields is
// negative. If fill panics, the builder is left writing the nested table,
// and mustn't be used again.
func (b *TableBuilder) SetTableFunc(fields int, fill func(table Item) error) error {
	// In a function small enough to inline, so that where fill is a function
	// literal, the compiler sees what it does with the builder, and keeps a
	// builder that nothing else keeps on its caller's stack
	saved, err := b.startTable(fields, Align1)
	if err != nil {
		return err
	}

	return b.endTable(saved, Align1, fill(Item{builder: b}))
}

// SetTableFuncAligned writes a table nested in place, as
// [TableBuilder.SetTableFunc] does, padded to start at a multiple of
// alignment before fill sets its fields, so that it isn't moved after. If
// its fields need a larger alignment, it's moved forward to that, as
// SetTableFunc moves it, and the bytes are those of SetTableFunc. Otherwise
// they're those of [TableBuilder.SetTableAligned] with a builder of the
// table: [Align8] for a table that holds an array of 64-bit elements, or a
// table nested in place that does, always takes the bytes of SetTableFunc.
//
// It fails as SetTableFunc does, and panics if the alignment isn't 1, 2, 4
// or 8.
func (b *TableBuilder) SetTableFuncAligned(fields int, alignment Alignment, fill func(table Item) error) error {
	alignment.mustBeValid()
	saved, err := b.startTable(fields, alignment)
	if err != nil {
		return err
	}

	return b.endTable(saved, alignment, fill(Item{builder: b}))
}

// startTable starts a table of fields fields nested in place, after its
// padding to alignment, and returns what the builder was
func (b *TableBuilder) startTable(fields int, alignment Alignment) (checkpoint, error) {
	if b.set == b.fields {
		return checkpoint{}, b.tooManyFields()
	}
	header, err := b.nestedHeader(fields)
	if err != nil {
		return checkpoint{}, err
	}

	saved := b.checkpoint()
	padding := int(alignment.padding(uint64(saved.written - b.start)))
	buffer, at := b.reserve(padding)
	clear(buffer[at:])
	b.startInPlace(fields, header)

	return saved, nil
}

// endTable finishes the table nested in place since saved, whose fill
// returned err, padded to its fields' largest alignment, or to declared if
// that's larger, or cuts the builder back to saved if it fails
func (b *TableBuilder) endTable(saved checkpoint, declared Alignment, err error) error {
	if err == nil {
		err = b.checkComplete()
	}
	if err != nil {
		b.rollBack(saved)
		return err
	}

	// Padded from this table's length, as a nested builder is
	table := b.counters
	b.counters = saved.counters
	position := saved.written - b.start
	alignment := max(declared, table.alignment)
	padding := int(alignment.padding(uint64(position)))
	length := uint64(position) + uint64(padding) + uint64(len(b.buffer)-table.start)
	if length > uint64(b.limit) {
		b.rollBack(saved)
		return b.tooLong(length)
	}

	// The padding that its declared alignment gave it is at most this
	if moved := padding - (table.start - saved.written); moved > 0 {
		b.moveForward(table.start, moved)
	}
	b.putOffset(b.buffer, uint32(position+padding))
	b.setField(alignment)

	return nil
}

// SetVectorFunc writes a vector of count tables of fields fields each, in
// place, as [TableBuilder.SetTableFunc] writes one: fill(index, item) sets
// the fields of item index, for each in order. The vector's bytes are those
// that [TableBuilder.SetVector] writes for builders of the items. Items
// aren't padded, as SetVector doesn't pad them.
//
// It fails with TooLong before writing any item if the vector's count and
// the ends of its items alone make the table too long, with the length that
// they make it, or after writing them if the items do. It fails otherwise
// as SetTableFunc does, for each item, and returns what fill returns,
// leaving the builder as it was in each case. It panics if count or fields
// is negative. If fill panics, the builder is left writing the item, and
// mustn't be used again.
func (b *TableBuilder) SetVectorFunc(count, fields int, fill func(index int, item Item) error) error {
	// In a function small enough to inline, as SetTableFunc is
	vector, err := b.startVector(count, fields)
	if err != nil {
		return err
	}
	for index := range count {
		b.startInPlace(fields, vector.header)
		if err := b.endItem(&vector, index, fill(index, Item{builder: b})); err != nil {
			return err
		}
	}

	return b.endVector(&vector)
}

// vectorInPlace is a vector of tables that the builder writes in place
type vectorInPlace struct {
	// What the builder was before the vector
	saved checkpoint
	// Where the vector's count is in the buffer
	at int
	// The end of the items written so far, from the vector's count
	end uint64
	// The number of items, and the length of each one's header
	count  int
	header uint32
}

// startVector starts a vector in place of count tables of fields fields,
// with its count and room for the ends of its items
func (b *TableBuilder) startVector(count, fields int) (vectorInPlace, error) {
	if count < 0 {
		panic("tafla: a vector can't hold " + strconv.Itoa(count) + " items")
	}
	if b.set == b.fields {
		return vectorInPlace{}, b.tooManyFields()
	}

	// The table's length with the count and the ends of the items, which
	// fail before any item is written, as does a count past what any table
	// holds
	width := uint64(b.layout.offsetType.Size())
	position := uint64(len(b.buffer) - b.start)
	length := uint64(math.MaxUint64)
	if uint64(count) < math.MaxUint32 {
		length = position + (uint64(count)+1)*width
	}
	if length > uint64(b.limit) {
		return vectorInPlace{}, b.tooLong(length)
	}
	// Only an item fails for its header, as its builder would
	header, err := b.nestedHeader(fields)
	if err != nil && count > 0 {
		return vectorInPlace{}, err
	}

	saved := b.checkpoint()
	ends := (uint64(count) + 1) * width
	buffer, at := b.reserve(int(ends))
	putSlot(buffer, at, int(width), uint32(count))

	return vectorInPlace{saved: saved, at: at, end: ends, count: count, header: header}, nil
}

// endItem finishes item index of a vector in place, whose fill returned
// err, or cuts the builder back to before the vector if it fails
func (b *TableBuilder) endItem(vector *vectorInPlace, index int, err error) error {
	if err == nil {
		err = b.checkComplete()
	}
	if err != nil {
		b.rollBack(vector.saved)
		return err
	}

	// Once the items make the table too long, which fails once they're all
	// written, unless one fails first, they aren't kept, so that the buffer
	// holds no more than a table that fits
	item := len(b.buffer) - b.start
	vector.end += uint64(item)
	if uint64(vector.saved.written-vector.saved.start)+vector.end > uint64(b.limit) {
		b.buffer = b.buffer[:b.start]
		return nil
	}

	width := b.layout.offsetType.Size()
	putSlot(b.buffer, vector.at+(index+1)*width, width, uint32(vector.end))
	if index == 0 {
		b.reserveItems(vector.count-1, item)
	}

	return nil
}

// endVector finishes a vector in place once its items are written, or cuts
// the builder back to before it if they make the table too long
func (b *TableBuilder) endVector(vector *vectorInPlace) error {
	b.counters = vector.saved.counters
	position := vector.saved.written - b.start
	length := uint64(position) + vector.end
	if length > uint64(b.limit) {
		b.rollBack(vector.saved)
		return b.tooLong(length)
	}

	b.putOffset(b.buffer, uint32(position))
	b.setField(Align1)

	return nil
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

// nestedBytes returns the bytes of a nested table: a builder's table, but
// for the length of a sized one, which putNested writes, or the bytes
func nestedBytes(table NestedTable) []byte {
	if builder, ok := table.(*TableBuilder); ok {
		return builder.buffer[builder.start:]
	}

	return table.(TableBytes)
}

// putNested writes a nested table into bytes of its length: its bytes, and
// a builder's length, when it's sized, as its Bytes would write it
func putNested(into []byte, table NestedTable, bytes []byte) {
	copy(into, bytes)
	if builder, ok := table.(*TableBuilder); ok && builder.layout.sized {
		putSlot(into, 0, builder.layout.offsetType.Size(), uint32(len(bytes)))
	}
}

// nestedHeader returns the length of the offsets of a table of fields fields
// nested in this one in place, in the default layout of its offset type
func (b *TableBuilder) nestedHeader(fields int) (uint32, error) {
	return TableLayout{offsetType: b.layout.offsetType}.headerLength(fields)
}

// startInPlace starts the fields of a table of fields fields nested in
// place, after its offsets, of header bytes
func (b *TableBuilder) startInPlace(fields int, header uint32) {
	// reserve's work, written out, since a call to it doesn't inline
	at := len(b.buffer)
	if int(header) > cap(b.buffer)-at {
		b.grow(int(header))
	}
	b.buffer = b.buffer[:at+int(header)]

	b.counters = counters{start: at, slots: at, fields: uint32(fields), alignment: Align1}
}

// moveForward moves the bytes from the buffer's from to its end forward by
// padding bytes, and zeroes the padding before them. The table that they
// hold counts its offsets from its own start, so none of them changes.
func (b *TableBuilder) moveForward(from, padding int) {
	end := len(b.buffer)
	buffer, _ := b.reserve(padding)
	copy(buffer[from+padding:], buffer[from:end])
	clear(buffer[from : from+padding])
}

// reserveItems makes room in the buffer for count more items of a vector of
// bytes bytes each, as its first item, so that writing them grows it once,
// unless they're longer. Growing the buffer as they were written took the
// benchmarks' vector of 100 nested tables 15% longer, in 6 allocations of
// 11.5 KB rather than 4 of 7.8 KB. It makes room for at most itemRoom bytes,
// so that a long first item doesn't make room for many short ones, and never
// for more than a table that fits in the offset type.
func (b *TableBuilder) reserveItems(count, bytes int) {
	written := uint64(len(b.buffer))
	left := uint64(b.limit) - min(written, uint64(b.limit))
	room := int(min(uint64(count)*uint64(bytes), itemRoom, left))
	if room > cap(b.buffer)-len(b.buffer) {
		b.grow(room)
	}
}

func (b *TableBuilder) checkpoint() checkpoint {
	return checkpoint{counters: b.counters, written: len(b.buffer)}
}

// rollBack cuts the buffer back to saved, and counts the fields of its table
// again, leaving the builder as it was, but for the buffer's capacity
func (b *TableBuilder) rollBack(saved checkpoint) {
	b.buffer = b.buffer[:saved.written]
	b.counters = saved.counters
}

func (b *TableBuilder) checkComplete() error {
	if b.set != b.fields {
		return &BuildError{Kind: Incomplete, Set: int(b.set), Fields: int(b.fields)}
	}

	return nil
}

func (b *TableBuilder) tooManyFields() error {
	return &BuildError{Kind: TooManyFields, Fields: int(b.fields)}
}

// tooLong returns the error of the next field, which would make the table
// length bytes
func (b *TableBuilder) tooLong(length uint64) error {
	return &BuildError{Kind: TooLong, Index: int(b.set), Length: length, OffsetType: b.layout.offsetType}
}

// reserve makes room for count bytes after the buffer's bytes, which the
// caller writes every one of, and returns the buffer and where they start
func (b *TableBuilder) reserve(count int) ([]byte, int) {
	at := len(b.buffer)
	if count > cap(b.buffer)-at {
		b.grow(count)
	}
	b.buffer = b.buffer[:at+count]

	return b.buffer, at
}

// grow makes room for count more bytes, in new bytes of at least
// grownCapacity's capacity, with the buffer's bytes copied in
func (b *TableBuilder) grow(count int) {
	length := len(b.buffer)
	capacity := max(grownCapacity(cap(b.buffer)), length+count)

	// Appended, so that only the bytes past the buffer's are zeroed
	b.buffer = append(b.buffer, make([]byte, capacity-length)...)[:length]
}

// putOffset writes the offset of the next field of the table being set
func (b *TableBuilder) putOffset(buffer []byte, offset uint32) {
	width := b.layout.offsetType.Size()
	putSlot(buffer, b.slots+int(b.set)*width, width, offset)
}

// putLittleEndian writes elements of size bytes from their bytes in the
// host's byte order into bytes, little-endian, on a big-endian host
func putLittleEndian(bytes, elements []byte, size int) {
	for at := 0; at < len(elements); at += size {
		switch size {
		case 2:
			binary.LittleEndian.PutUint16(bytes[at:], binary.NativeEndian.Uint16(elements[at:]))
		case 4:
			binary.LittleEndian.PutUint32(bytes[at:], binary.NativeEndian.Uint32(elements[at:]))
		default:
			binary.LittleEndian.PutUint64(bytes[at:], binary.NativeEndian.Uint64(elements[at:]))
		}
	}
}
