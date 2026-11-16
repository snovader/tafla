package tafla

import (
	"encoding/binary"
	"math"
	"strconv"
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
// Setting a field writes its offset and its bytes at once, into a buffer of
// 128 bytes within the builder, and then into one on the heap, which doubles
// as it grows. [TableBuilder.Bytes] copies them into bytes of the table's
// exact length. Runs of 64 bytes or more, the elements of an array, a
// string, an array of structs or a table nested as bytes, are kept rather
// than copied, and Bytes copies them in between the buffer's bytes. So
// arrays, strings and the bytes of structs and tables must not change until
// the table is written. A table nested from its builder is copied in when
// it's set, with the runs that it keeps, unless that would copy 128 bytes or
// more, counting 16 for each run: then the builder keeps a copy of the nested
// builder, and Bytes writes its table, so that nesting builders copies fewer
// than 128 bytes per builder, however deep they're nested.
//
// A setter returns a *[BuildError], and leaves the builder as it was, when
// the table already holds all its fields or the field would make it too long
// for the layout's offset type: longer than 255 bytes, 64 KiB or 4 GiB, or
// than an int on a 32-bit host. The buffer keeps the capacity that it grew
// to, which the builder holds until it's dropped.
//
// [TableBuilder.SetTableFunc] and [TableBuilder.SetVectorFunc] write a
// nested table, or each item of a vector, in place, straight into the
// builder's own bytes, through the [Item] that their function sets the
// fields on, rather than from a builder of its own. They write the bytes
// that nested builders write, without a builder per table, nor copying it.
//
// A builder is used through the pointer that its constructor returns, and
// isn't copied, since it holds a small table's bytes within itself.
type TableBuilder struct {
	// heap holds the table's bytes once they outgrow inline, at its full
	// length, and is nil until then. The builder doesn't point into itself, so
	// that a builder that no setter keeps can stay on its caller's stack.
	heap []byte
	// The bytes written to the buffer: those of the fields set so far, but for
	// the runs kept
	written int

	// The runs kept, in order: the first ones within the builder, and the
	// others in more
	runs  [inlineRuns]run
	more  []run
	count int

	// The counters of the table whose fields are being set: the builder's own,
	// or those of a table nested in place, which counts its fields as a
	// builder of its own would
	counters
	// The longest table that the offset type addresses, and an int holds
	limit  uint32
	layout TableLayout

	inline [inlineBytes]byte
}

// inlineRuns is the number of runs kept within the builder before the others
// go to the heap
const inlineRuns = 4

// counters are those of the table whose fields are being set, which a table
// nested in place sets aside
type counters struct {
	// The number of the table's fields, and of those set
	fields, set uint32
	// The table's length so far, which is where its next field goes from its
	// start, runs included
	length uint32
	// Where the table's first offset is in the buffer
	slots int
	// The largest alignment that a field needs from the table's start, which a
	// parent pads this table to when it nests it
	alignment Alignment
}

// checkpoint is what a table or vector nested in place that fails truncates
// the builder back to
type checkpoint struct {
	counters
	written, count int
}

// run is a run of bytes that the builder keeps rather than copying, which
// comes before the buffer's byte at: the elements of an array, a string or
// bytes, or a nested table. data points into the caller's slice or string, or
// to a copy of the nested table's builder, so that the garbage collector keeps
// it while the builder does. It's a pointer and two uint32s, which a table's
// positions fit in, rather than a slice, whose capacity would make the
// builder 32 bytes longer: with slices, the benchmarks' vector of 100 nested
// builders took 12% longer.
type run struct {
	data unsafe.Pointer
	at   uint32
	// The run's length, or 0 for a nested table, whose builder holds its
	// length. Bytes are kept only when there are some.
	length uint32
}

// copyTo copies the run into the start of bytes, and returns its length
func (r *run) copyTo(bytes []byte) int {
	if r.length > 0 {
		return copy(bytes, unsafe.Slice((*byte)(r.data), r.length))
	}

	nested := (*TableBuilder)(r.data)
	table := bytes[:nested.length]
	nested.copyTo(table)
	nested.putLength(table)

	return len(table)
}

// NewTableBuilder starts a table of fields fields in layout, which its
// readers must use too. Nested tables use the default layout, the zero
// [TableLayout]. It fails with HeaderTooLong if the header alone is too long
// for the layout's offset type, and panics if the layout is versioned, which
// [NewVersionedTableBuilder] builds, or fields is negative.
func NewTableBuilder(fields int, layout TableLayout) (*TableBuilder, error) {
	// Allocated here, where it's inlined, so that it stays on the caller's
	// stack unless a setter keeps it
	return new(TableBuilder).start(fields, layout, unversioned)
}

// NewVersionedTableBuilder starts a table of fields fields in layout, with
// version in its header. Readers can check it with [ByteBuffer.Version]
// before reading any field. It fails with VersionTooLarge if the version
// doesn't fit in the layout's offset type, or with HeaderTooLong if the
// header alone is too long for it, and panics if the layout isn't versioned
// or fields is negative.
func NewVersionedTableBuilder(fields int, layout TableLayout, version uint32) (*TableBuilder, error) {
	return new(TableBuilder).start(fields, layout, int64(version))
}

// unversioned is the version that start takes for a table without one, in
// one argument with the version, so that the constructors are cheap enough
// to inline
const unversioned = -1

// start checks the layout's version, or unversioned, and header, writes the
// header but the length, which Bytes writes, and returns the builder
func (b *TableBuilder) start(fields int, layout TableLayout, versionOrNot int64) (*TableBuilder, error) {
	versioned := versionOrNot != unversioned
	version := uint32(versionOrNot)
	if err := layout.checkVersion(versioned, version); err != nil {
		return nil, err
	}
	header, err := layout.headerLength(fields)
	if err != nil {
		return nil, err
	}

	// The header check keeps fields within a uint32
	b.layout = layout
	b.limit = uint32(layout.maxLength())
	b.counters = counters{fields: uint32(fields), length: header, slots: layout.HeaderSize(), alignment: Align1}

	buffer, _ := b.reserve(int(header))
	if versioned {
		width := layout.offsetType.Size()
		putSlot(buffer, layout.HeaderSize()-width, width, version)
	}

	return b, nil
}

// Bytes writes the table, at its exact size, into new bytes: the buffer's
// bytes, with the runs kept copied in between them. It fails with Incomplete
// if fewer fields were set than the table holds.
func (b *TableBuilder) Bytes() ([]byte, error) {
	if err := b.checkComplete(); err != nil {
		return nil, err
	}

	table := make([]byte, b.length)
	b.copyTo(table)
	b.putLength(table)

	return table, nil
}

// copyTo copies the table into bytes of its length: the buffer's bytes, and
// each run before the byte that it comes before
func (b *TableBuilder) copyTo(table []byte) {
	buffer := b.buffer()[:b.written]
	if b.count == 0 {
		copy(table, buffer)
		return
	}

	from, to := 0, 0
	for index := range b.count {
		run := b.run(index)
		to += copy(table[to:], buffer[from:run.at])
		to += run.copyTo(table[to:])
		from = int(run.at)
	}
	copy(table[to:], buffer[from:])
}

// putLength writes the length of a sized table into its header
func (b *TableBuilder) putLength(table []byte) {
	if b.layout.sized {
		putSlot(table, 0, b.layout.offsetType.Size(), uint32(len(table)))
	}
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

// SetInt8Array sets an array field of int8s, which the builder may keep
// until the table is written. It fails as [TableBuilder.SetInt8] does.
func (b *TableBuilder) SetInt8Array(values []int8) error {
	return b.setArray(elementBytes(values), 1)
}

// SetUint8Array sets an array field of uint8s, which the builder may keep
// until the table is written. It fails as [TableBuilder.SetInt8] does.
func (b *TableBuilder) SetUint8Array(values []uint8) error { return b.setArray(values, 1) }

// SetInt16Array sets an array field of int16s, which the builder may keep
// until the table is written. It fails as [TableBuilder.SetInt8] does.
func (b *TableBuilder) SetInt16Array(values []int16) error {
	return b.setArray(elementBytes(values), 2)
}

// SetUint16Array sets an array field of uint16s, which the builder may keep
// until the table is written. It fails as [TableBuilder.SetInt8] does.
func (b *TableBuilder) SetUint16Array(values []uint16) error {
	return b.setArray(elementBytes(values), 2)
}

// SetInt32Array sets an array field of int32s, which the builder may keep
// until the table is written. It fails as [TableBuilder.SetInt8] does.
func (b *TableBuilder) SetInt32Array(values []int32) error {
	return b.setArray(elementBytes(values), 4)
}

// SetUint32Array sets an array field of uint32s, which the builder may keep
// until the table is written. It fails as [TableBuilder.SetInt8] does.
func (b *TableBuilder) SetUint32Array(values []uint32) error {
	return b.setArray(elementBytes(values), 4)
}

// SetInt64Array sets an array field of int64s, which the builder may keep
// until the table is written. It fails as [TableBuilder.SetInt8] does.
func (b *TableBuilder) SetInt64Array(values []int64) error {
	return b.setArray(elementBytes(values), 8)
}

// SetUint64Array sets an array field of uint64s, which the builder may keep
// until the table is written. It fails as [TableBuilder.SetInt8] does.
func (b *TableBuilder) SetUint64Array(values []uint64) error {
	return b.setArray(elementBytes(values), 8)
}

// SetFloat32Array sets an array field of float32s, which the builder may
// keep until the table is written. It fails as [TableBuilder.SetInt8] does.
func (b *TableBuilder) SetFloat32Array(values []float32) error {
	return b.setArray(elementBytes(values), 4)
}

// SetFloat64Array sets an array field of float64s, which the builder may
// keep until the table is written. It fails as [TableBuilder.SetInt8] does.
func (b *TableBuilder) SetFloat64Array(values []float64) error {
	return b.setArray(elementBytes(values), 8)
}

// SetString sets a string field, of its bytes, which are UTF-8 if it is. It
// fails as [TableBuilder.SetInt8] does.
func (b *TableBuilder) SetString(value string) error {
	return b.setRun(unsafe.Slice(unsafe.StringData(value), len(value)), len(value), Align1)
}

// SetStruct sets a struct field from its bytes: its scalars, little-endian,
// one after another, which are written as they are, without padding. It
// fails as [TableBuilder.SetInt8] does.
func (b *TableBuilder) SetStruct(bytes []byte) error {
	if b.set == b.fields {
		return b.tooManyFields()
	}
	length := uint64(b.length) + uint64(len(bytes))
	if length > uint64(b.limit) {
		return b.tooLong(length)
	}

	buffer, at := b.reserve(len(bytes))
	b.putOffset(buffer, b.length)
	copy(buffer[at:], bytes)
	b.setField(length, Align1)

	return nil
}

// SetStructArray sets an array of structs of size bytes each, from their
// bytes one after another, which it counts. It's padded so that the structs
// start at a multiple of alignment from the start of the table, which is the
// size of the structs' largest scalar. The builder may keep the bytes until
// the table is written. It fails with NotWholeStructs if size isn't positive
// or the bytes aren't a whole number of structs, or as
// [TableBuilder.SetInt8] does, and panics if alignment isn't 1, 2, 4 or 8.
func (b *TableBuilder) SetStructArray(bytes []byte, size int, alignment Alignment) error {
	alignment.mustBeValid()
	if size <= 0 || len(bytes)%size != 0 {
		return &BuildError{Kind: NotWholeStructs, Length: uint64(len(bytes)), Size: size}
	}

	return b.setRun(bytes, len(bytes)/size, alignment)
}

// setScalar writes a scalar field of size bytes, its bits' lowest
func (b *TableBuilder) setScalar(bits uint64, size int) error {
	if b.set == b.fields {
		return b.tooManyFields()
	}
	length := uint64(b.length) + uint64(size)
	if length > uint64(b.limit) {
		return b.tooLong(length)
	}

	// reserve's work, written out, since a call to it doesn't inline. With
	// the calls to reserve here and in startInPlace, the benchmarks' record
	// took 7% longer, and their vector of 100 nested tables 10% longer.
	at := b.written
	buffer := b.buffer()
	if size > len(buffer)-at {
		buffer = b.grow(at + size)
	}
	b.written = at + size

	b.putOffset(buffer, b.length)
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
	b.length = uint32(length)
	b.set++

	return nil
}

// setArray writes an array field from its elements' bytes in the host's
// byte order, as elements of size bytes. It isn't generic, so that the
// compiler sees that it doesn't keep the builder.
func (b *TableBuilder) setArray(elements []byte, size int) error {
	// Kept as they are only in the table's byte order
	if hostLittleEndian || size == 1 {
		return b.setRun(elements, len(elements)/size, Alignment(size))
	}

	if b.set == b.fields {
		return b.tooManyFields()
	}
	width := uint64(b.layout.offsetType.Size())
	padding := Alignment(size).padding(uint64(b.length) + width)
	length := uint64(b.length) + padding + width + uint64(len(elements))
	if length > uint64(b.limit) {
		return b.tooLong(length)
	}

	buffer, at := b.reserve(int(padding) + int(width) + len(elements))
	at = b.startArray(buffer, at, padding, len(elements)/size)
	putLittleEndian(buffer[at:at+len(elements)], elements, size)
	b.setField(length, Alignment(size))

	return nil
}

// setRun writes a field of bytes that start with their count: an array, a
// string or an array of structs, padded so that they start at a multiple of
// alignment from the table's start. A run of runMin bytes or more is kept
// rather than copied.
func (b *TableBuilder) setRun(bytes []byte, count int, alignment Alignment) error {
	if b.set == b.fields {
		return b.tooManyFields()
	}
	width := uint64(b.layout.offsetType.Size())
	padding := alignment.padding(uint64(b.length) + width)
	length := uint64(b.length) + padding + width + uint64(len(bytes))
	if length > uint64(b.limit) {
		return b.tooLong(length)
	}

	if len(bytes) >= runMin {
		buffer, at := b.reserve(int(padding) + int(width))
		b.startArray(buffer, at, padding, count)
		b.keepBytes(bytes)
	} else {
		buffer, at := b.reserve(int(padding) + int(width) + len(bytes))
		at = b.startArray(buffer, at, padding, count)
		copy(buffer[at:], bytes)
	}
	b.setField(length, alignment)

	return nil
}

// startArray writes an array's padding, its offset and its count from at,
// and returns where its elements go
func (b *TableBuilder) startArray(buffer []byte, at int, padding uint64, count int) int {
	clear(buffer[at : at+int(padding)])
	at += int(padding)
	b.putOffset(buffer, b.length+uint32(padding))

	width := b.layout.offsetType.Size()
	putSlot(buffer, at, width, uint32(count))

	return at + width
}

// setField counts a field that was written, which makes the table length
// bytes, and needs alignment from its start
func (b *TableBuilder) setField(length uint64, alignment Alignment) {
	b.length = uint32(length)
	b.alignment = max(b.alignment, alignment)
	b.set++
}

// NestedTable is a table to nest with [TableBuilder.SetTable] or in
// [TableBuilder.SetVector]: a *[TableBuilder], or [TableBytes].
//
// A builder is copied into its parent when it's set, with the runs that it
// keeps, which its parent keeps in turn, or, when they're long, kept as a
// copy of the builder, whose table is written with its parent's. Bytes are
// copied, or kept as a run when they're long enough.
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
// A builder must have all its fields set. It's written as its own Bytes
// would write it, in its own layout, but without new bytes of its own:
// copied in, or kept until the table is written when it's long, as the runs
// that it keeps and long bytes are.
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

func (b *TableBuilder) setTable(table NestedTable, alignment Alignment) error {
	if err := checkNested(table); err != nil {
		return err
	}
	if b.set == b.fields {
		return b.tooManyFields()
	}
	padding := alignment.padding(uint64(b.length))
	length := uint64(b.length) + padding + nestedLength(table)
	if length > uint64(b.limit) {
		return b.tooLong(length)
	}

	buffer, at := b.reserve(int(padding))
	clear(buffer[at : at+int(padding)])
	b.putOffset(buffer, b.length+uint32(padding))
	b.appendNested(table)
	b.setField(length, alignment)

	return nil
}

// SetVector writes a vector of tables, from their builders or their bytes,
// after the end of each, which lets readers find any item without reading
// the others. Builders must have all their fields set, and are written as
// [TableBuilder.SetTable] writes them. Items aren't padded, so their arrays
// are read in place only when they happen to be aligned. The builder doesn't
// keep the slice of items.
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
	width := uint64(b.layout.offsetType.Size())
	length := uint64(b.length) + width
	for _, item := range items {
		length += width + nestedLength(item)
	}
	if length > uint64(b.limit) {
		return b.tooLong(length)
	}

	buffer, vector := b.reserve((len(items) + 1) * int(width))
	b.putOffset(buffer, b.length)
	putSlot(buffer, vector, int(width), uint32(len(items)))

	// Each end counts from the vector's count, after the ends
	end := uint64(len(items)+1) * width
	for index, item := range items {
		b.appendNested(item)
		end += nestedLength(item)
		putSlot(b.buffer(), vector+(index+1)*int(width), int(width), uint32(end))
	}
	b.setField(length, Align1)

	return nil
}

// SetTableFunc writes a table of fields fields nested in this one, in
// place, as [TableBuilder.SetTable] nests a builder of them in the default
// layout of this table's offset type, but without a builder of its own: fill
// sets its fields, in order, on the [Item] that it's given, and each is
// written straight into this builder's bytes. The table is padded to start
// at a multiple of its largest alignment, as SetTable pads a builder, so the
// bytes are those that SetTable writes. It's written unpadded, and then moved
// forward by its padding once its fields give its alignment.
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
	if b.set == b.fields {
		return b.tooManyFields()
	}
	header, err := b.nestedHeader(fields)
	if err != nil {
		return err
	}

	saved := b.checkpoint()
	b.startInPlace(fields, header)
	err = fill(Item{builder: b})
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
	padding := table.alignment.padding(uint64(b.length))
	length := uint64(b.length) + padding + uint64(table.length)
	if length > uint64(b.limit) {
		b.rollBack(saved)
		return b.tooLong(length)
	}

	if padding > 0 {
		b.moveForward(saved, int(padding))
	}
	b.putOffset(b.buffer(), b.length+uint32(padding))
	b.setField(length, table.alignment)

	return nil
}

// SetVectorFunc writes a vector of count tables of fields fields each, in
// place, as [TableBuilder.SetTableFunc] writes one: fill(index, item) sets
// the fields of item index, for each in order. The vector's bytes are those
// that [TableBuilder.SetVector] writes for builders of the items. Items
// aren't padded, as SetVector doesn't pad them.
//
// It fails with TooLong before writing any item if the vector's count and
// the ends of its items alone make the table too long, or after writing them
// if the items do. It fails otherwise as SetTableFunc does, for each item,
// and returns what fill returns, leaving the builder as it was in each case.
// It panics if count or fields is negative. If fill panics, the builder is
// left writing the item, and mustn't be used again.
func (b *TableBuilder) SetVectorFunc(count, fields int, fill func(index int, item Item) error) error {
	if count < 0 {
		panic("tafla: a vector can't hold " + strconv.Itoa(count) + " items")
	}
	if b.set == b.fields {
		return b.tooManyFields()
	}

	// The table's length with the count and the ends of the items, which
	// fail before any item is written, as does a count past what any table
	// holds
	width := uint64(b.layout.offsetType.Size())
	length := uint64(math.MaxUint64)
	if uint64(count) < math.MaxUint32 {
		length = uint64(b.length) + (uint64(count)+1)*width
	}
	if length > uint64(b.limit) {
		return b.tooLong(length)
	}
	// Only an item fails for its header, as its builder would
	header, err := b.nestedHeader(fields)
	if err != nil && count > 0 {
		return err
	}

	saved := b.checkpoint()
	buffer, vector := b.reserve((count + 1) * int(width))
	putSlot(buffer, vector, int(width), uint32(count))

	// Each end counts from the vector's count, after the ends
	end := (uint64(count) + 1) * width
	for index := range count {
		start, kept := b.written, b.count
		b.startInPlace(fields, header)
		err := fill(index, Item{builder: b})
		if err == nil {
			err = b.checkComplete()
		}
		if err != nil {
			b.rollBack(saved)
			return err
		}

		// Once the items make the table too long, which fails once they're
		// all written, unless one fails first, they aren't kept, so that the
		// buffer holds no more than a table that fits
		end += uint64(b.length)
		if uint64(saved.length)+end > uint64(b.limit) {
			b.truncate(start, kept)
			continue
		}

		putSlot(b.buffer(), vector+(index+1)*int(width), int(width), uint32(end))
		if index == 0 {
			b.reserveItems(count-1, b.written-start)
		}
	}

	b.counters = saved.counters
	length = uint64(b.length) + end
	if length > uint64(b.limit) {
		b.rollBack(saved)
		return b.tooLong(length)
	}

	b.putOffset(b.buffer(), b.length)
	b.setField(length, Align1)

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

// nestedLength returns the byte length of a nested table
func nestedLength(table NestedTable) uint64 {
	if builder, ok := table.(*TableBuilder); ok {
		return uint64(builder.length)
	}

	return uint64(len(table.(TableBytes)))
}

// appendNested writes a nested table after the buffer's bytes: a builder's
// bytes and runs, with its length in its header when it's sized, unless
// copying them would copy tableRunMin bytes or more, when it keeps the
// builder, or bytes, which are kept as a run when they're long enough
func (b *TableBuilder) appendNested(table NestedTable) {
	nested, ok := table.(*TableBuilder)
	if !ok {
		bytes := table.(TableBytes)
		if len(bytes) >= runMin {
			b.keepBytes(bytes)
			return
		}

		buffer, at := b.reserve(len(bytes))
		copy(buffer[at:], bytes)
		return
	}
	if nested.written+nested.count*int(unsafe.Sizeof(run{})) >= tableRunMin {
		b.keepTable(nested)
		return
	}

	buffer, at := b.reserve(nested.written)
	copy(buffer[at:], nested.buffer()[:nested.written])
	if nested.layout.sized {
		putSlot(buffer, at, nested.layout.offsetType.Size(), nested.length)
	}

	for index := range nested.count {
		kept := *nested.run(index)
		kept.at += uint32(at)
		b.keep(kept)
	}
}

// keepTable keeps a nested builder's table as a run after the buffer's
// bytes, which Bytes copies in, rather than copying it now, so that a table
// nested in a tree of builders is copied once rather than at each level. The
// run holds a copy of the builder, whose fields are all set, so that the
// builder itself can stay on its caller's stack.
func (b *TableBuilder) keepTable(nested *TableBuilder) {
	kept := new(TableBuilder)
	*kept = *nested
	b.keep(run{data: unsafe.Pointer(kept), at: uint32(b.written)})
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
	at := b.written
	if int(header) > len(b.buffer())-at {
		b.grow(at + int(header))
	}
	b.written = at + int(header)

	b.counters = counters{fields: uint32(fields), length: header, slots: at, alignment: Align1}
}

// moveForward moves the bytes written since saved forward by padding bytes,
// and the runs kept since, and zeroes the padding before them. The table's
// offsets count from its own start, so none of them changes.
func (b *TableBuilder) moveForward(saved checkpoint, padding int) {
	start := saved.written
	end := b.written
	buffer, _ := b.reserve(padding)
	copy(buffer[start+padding:], buffer[start:end])
	clear(buffer[start : start+padding])

	for index := saved.count; index < b.count; index++ {
		b.run(index).at += uint32(padding)
	}
}

// reserveItems makes room in the buffer for count more items of a vector of
// bytes bytes each, as its first item, so that writing them grows it once,
// unless they're longer. Growing the buffer as they were written took the
// benchmarks' vector of 100 nested tables 15% longer, in 6 allocations of
// 11.5 KB rather than 4 of 7.8 KB. It makes room for at most itemRoom bytes,
// so that a long first item doesn't make room for many short ones, and never
// for more than a table that fits in the offset type.
func (b *TableBuilder) reserveItems(count, bytes int) {
	left := uint64(b.limit) - min(uint64(b.written), uint64(b.limit))
	room := min(uint64(count)*uint64(bytes), itemRoom, left)
	if end := b.written + int(room); end > len(b.buffer()) {
		b.grow(end)
	}
}

func (b *TableBuilder) checkpoint() checkpoint {
	return checkpoint{counters: b.counters, written: b.written, count: b.count}
}

// rollBack truncates the buffer and the runs back to saved, and counts the
// fields of its table again, leaving the builder as it was, but for the
// buffer's capacity
func (b *TableBuilder) rollBack(saved checkpoint) {
	b.truncate(saved.written, saved.count)
	b.counters = saved.counters
}

// truncate truncates the buffer to written bytes, and the runs to count
func (b *TableBuilder) truncate(written, count int) {
	// Cleared, so that the builder doesn't keep their bytes
	for index := count; index < b.count; index++ {
		*b.run(index) = run{}
	}
	if b.count > inlineRuns {
		b.more = b.more[:max(count-inlineRuns, 0)]
	}

	b.count = count
	b.written = written
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

// buffer returns the buffer, which the builder's bytes start, at its full
// capacity
func (b *TableBuilder) buffer() []byte {
	if b.heap != nil {
		return b.heap
	}

	return b.inline[:]
}

// reserve makes room for count bytes after the buffer's bytes, which the
// caller writes every one of, and returns the buffer and where they start
func (b *TableBuilder) reserve(count int) ([]byte, int) {
	at := b.written
	end := at + count
	buffer := b.buffer()
	if end > len(buffer) {
		buffer = b.grow(end)
	}
	b.written = end

	return buffer, at
}

// grow moves the buffer's bytes to a buffer on the heap of end bytes or
// more, at least twice as long, and returns it. It copies them rather than
// appending them, so that the builder never points into itself, even where
// the compiler can't tell; append's growth took as long. Growing by half
// rather than doubling took most tables as long or longer, and a vector of
// 100,000 items of three fields 11% longer, allocating 41% more, though trees
// of nested builders took 3-5% less time.
func (b *TableBuilder) grow(end int) []byte {
	heap := make([]byte, max(end, firstHeapBytes, 2*len(b.buffer())))
	copy(heap, b.buffer()[:b.written])
	b.heap = heap

	return heap
}

// keepBytes keeps bytes as a run after the buffer's bytes
func (b *TableBuilder) keepBytes(bytes []byte) {
	b.keep(run{data: unsafe.Pointer(unsafe.SliceData(bytes)), at: uint32(b.written), length: uint32(len(bytes))})
}

// keep adds a run after those kept
func (b *TableBuilder) keep(kept run) {
	if b.count < inlineRuns {
		b.runs[b.count] = kept
	} else {
		if b.more == nil {
			b.more = make([]run, 0, moreRuns)
		}
		b.more = append(b.more, kept)
	}
	b.count++
}

func (b *TableBuilder) run(index int) *run {
	if index < inlineRuns {
		return &b.runs[index]
	}

	return &b.more[index-inlineRuns]
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
