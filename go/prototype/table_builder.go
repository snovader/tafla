// Package prototype holds a prototype of [tafla.TableBuilder] that writes
// each field's bytes when it's set, rather than recording the field and
// writing every field in Bytes, as tafla.TableBuilder does. It has the same
// API, writes the same bytes and fails with the same errors, so that the two
// can be compared. It's an evaluation, not a supported package: see
// README.md.
package prototype

import (
	"encoding/binary"
	"math"
	"slices"
	"strconv"
	"unsafe"

	tafla "github.com/snovader/tafla/go"
)

// TableBuilder builds a table, which a [tafla.ByteBuffer] reads back, as
// [tafla.TableBuilder] does.
//
// Setting a field writes its offset and its bytes at once, into a buffer that
// holds inlineBytes within the builder and then moves to the heap. Runs of
// segmentMin bytes or more, such as a column, are kept by reference as
// segments instead, and [TableBuilder.Bytes] copies them between the
// buffer's bytes into the table, which is new bytes of its exact length. So,
// as with tafla.TableBuilder, arrays, strings and the bytes of structs and
// tables mustn't change until the table is written.
//
// A table nested in place with [TableBuilder.SetTableFunc] is written where
// its parent's bytes end, without padding, since its alignment is known only
// once its fields are set. It's then moved forward by its padding. Its
// offsets count from its own start, so none of them changes. A nested
// builder is copied in as its bytes and its segments.
//
// A setter that fails leaves the builder as it was: it checks the field
// before writing it, and a table or vector item that fails within its fill
// function is undone by truncating the buffer and the segments back to where
// they were.
type TableBuilder struct {
	// heap holds the table's bytes once they outgrow inline, at its full
	// length, and is nil until then. The builder doesn't point into itself, so
	// that a builder that no setter keeps can stay on the stack.
	heap []byte
	// The bytes written to the buffer: those of the fields set so far, but for
	// the segments
	written int

	// The runs kept by reference, in order: the first ones within the builder,
	// and the others in more
	segments [inlineSegments]segment
	more     []segment
	count    int

	// The counters of the table whose fields are being set: the builder's own,
	// or those of a table nested in place, which counts its fields as a
	// builder of its own would
	counters
	// The longest table that the offset type addresses, and an int holds
	limit  uint32
	layout tafla.TableLayout

	inline [inlineBytes]byte
}

// inlineSegments is the number of segments kept within the builder before
// the others go to the heap
const inlineSegments = 4

// counters are those of the table whose fields are being set, which a table
// nested in place sets aside
type counters struct {
	// The number of the table's fields, and of those set
	fields, set uint32
	// The table's length so far, which is where its next field goes from its
	// start, segments included
	length uint32
	// Where the table's first offset is in the buffer
	slots int
	// The largest alignment that a field needs from the table's start, which a
	// parent pads this table to when it nests it
	alignment tafla.Alignment
}

// checkpoint is what a failed table or vector nested in place truncates the
// builder back to
type checkpoint struct {
	counters
	written, count int
}

// segment is a run of bytes kept by reference, whose bytes come before the
// buffer's byte at
type segment struct {
	at     int
	data   unsafe.Pointer
	length int
}

func (s *segment) bytes() []byte {
	return unsafe.Slice((*byte)(s.data), s.length)
}

// NewTableBuilder starts a table of fields fields in layout, as
// [tafla.NewTableBuilder] does.
func NewTableBuilder(fields int, layout tafla.TableLayout) (*TableBuilder, error) {
	// Allocated here, where it's inlined, so that it stays on the caller's
	// stack unless a setter keeps it
	return new(TableBuilder).start(fields, layout, unversioned)
}

// NewVersionedTableBuilder starts a table of fields fields in layout, with
// version in its header, as [tafla.NewVersionedTableBuilder] does.
func NewVersionedTableBuilder(fields int, layout tafla.TableLayout, version uint32) (*TableBuilder, error) {
	return new(TableBuilder).start(fields, layout, int64(version))
}

// unversioned is the version that start takes for a table without one, in
// one argument with the version, so that the constructors are cheap enough
// to inline
const unversioned = -1

// start checks the layout's version, or unversioned, and header, writes the
// header, and returns the builder
func (b *TableBuilder) start(fields int, layout tafla.TableLayout, versionOrNot int64) (*TableBuilder, error) {
	versioned := versionOrNot != unversioned
	version := uint32(versionOrNot)
	if err := checkVersion(layout, versioned, version); err != nil {
		return nil, err
	}
	header, err := headerLength(layout, fields)
	if err != nil {
		return nil, err
	}

	b.layout = layout
	b.limit = uint32(min(uint64(layout.OffsetType().MaxValue()), math.MaxInt))
	b.counters = counters{fields: uint32(fields), length: header, slots: layout.HeaderSize(), alignment: tafla.Align1}

	// The offsets are written as the fields are set, and the length by Bytes
	buffer, _ := b.reserve(int(header))
	if versioned {
		width := layout.OffsetSize()
		putSlot(buffer, layout.HeaderSize()-width, width, version)
	}

	return b, nil
}

// Bytes writes the table, at its exact size, into new bytes, as
// [tafla.TableBuilder.Bytes] does: the buffer's bytes, with the segments
// copied in between them.
func (b *TableBuilder) Bytes() ([]byte, error) {
	if err := b.checkComplete(); err != nil {
		return nil, err
	}

	if bytesWithoutCopy && b.count == 0 && b.heap != nil {
		table := b.heap[:b.written:b.written]
		b.putLength(table)
		return table, nil
	}

	table := make([]byte, b.length)
	b.copyTo(table)
	b.putLength(table)

	return table, nil
}

// copyTo copies the table into bytes of its length: the buffer's bytes, and
// each segment before the byte that it comes before
func (b *TableBuilder) copyTo(table []byte) {
	buffer := b.buffer()[:b.written]
	if b.count == 0 {
		copy(table, buffer)
		return
	}

	from, to := 0, 0
	for index := range b.count {
		segment := b.segment(index)
		to += copy(table[to:], buffer[from:segment.at])
		to += copy(table[to:], segment.bytes())
		from = segment.at
	}
	copy(table[to:], buffer[from:])
}

// putLength writes the length of a sized table into its header
func (b *TableBuilder) putLength(table []byte) {
	if b.layout.IsSized() {
		putSlot(table, 0, b.layout.OffsetSize(), uint32(len(table)))
	}
}

// SetInt8 sets an int8 field, as [tafla.TableBuilder.SetInt8] does.
func (b *TableBuilder) SetInt8(value int8) error { return b.setScalar(uint64(uint8(value)), 1) }

// SetUint8 sets a uint8 field.
func (b *TableBuilder) SetUint8(value uint8) error { return b.setScalar(uint64(value), 1) }

// SetInt16 sets an int16 field.
func (b *TableBuilder) SetInt16(value int16) error { return b.setScalar(uint64(uint16(value)), 2) }

// SetUint16 sets a uint16 field.
func (b *TableBuilder) SetUint16(value uint16) error { return b.setScalar(uint64(value), 2) }

// SetInt32 sets an int32 field.
func (b *TableBuilder) SetInt32(value int32) error { return b.setScalar(uint64(uint32(value)), 4) }

// SetUint32 sets a uint32 field.
func (b *TableBuilder) SetUint32(value uint32) error { return b.setScalar(uint64(value), 4) }

// SetInt64 sets an int64 field.
func (b *TableBuilder) SetInt64(value int64) error { return b.setScalar(uint64(value), 8) }

// SetUint64 sets a uint64 field.
func (b *TableBuilder) SetUint64(value uint64) error { return b.setScalar(value, 8) }

// SetFloat32 sets a float32 field.
func (b *TableBuilder) SetFloat32(value float32) error {
	return b.setScalar(uint64(math.Float32bits(value)), 4)
}

// SetFloat64 sets a float64 field.
func (b *TableBuilder) SetFloat64(value float64) error {
	return b.setScalar(math.Float64bits(value), 8)
}

// SetInt8Array sets an array field of int8s, as
// [tafla.TableBuilder.SetInt8Array] does.
func (b *TableBuilder) SetInt8Array(values []int8) error { return b.setArray(elementBytes(values), 1) }

// SetUint8Array sets an array field of uint8s.
func (b *TableBuilder) SetUint8Array(values []uint8) error { return b.setArray(values, 1) }

// SetInt16Array sets an array field of int16s.
func (b *TableBuilder) SetInt16Array(values []int16) error {
	return b.setArray(elementBytes(values), 2)
}

// SetUint16Array sets an array field of uint16s.
func (b *TableBuilder) SetUint16Array(values []uint16) error {
	return b.setArray(elementBytes(values), 2)
}

// SetInt32Array sets an array field of int32s.
func (b *TableBuilder) SetInt32Array(values []int32) error {
	return b.setArray(elementBytes(values), 4)
}

// SetUint32Array sets an array field of uint32s.
func (b *TableBuilder) SetUint32Array(values []uint32) error {
	return b.setArray(elementBytes(values), 4)
}

// SetInt64Array sets an array field of int64s.
func (b *TableBuilder) SetInt64Array(values []int64) error {
	return b.setArray(elementBytes(values), 8)
}

// SetUint64Array sets an array field of uint64s.
func (b *TableBuilder) SetUint64Array(values []uint64) error {
	return b.setArray(elementBytes(values), 8)
}

// SetFloat32Array sets an array field of float32s.
func (b *TableBuilder) SetFloat32Array(values []float32) error {
	return b.setArray(elementBytes(values), 4)
}

// SetFloat64Array sets an array field of float64s.
func (b *TableBuilder) SetFloat64Array(values []float64) error {
	return b.setArray(elementBytes(values), 8)
}

// SetString sets a string field, of its bytes, as
// [tafla.TableBuilder.SetString] does.
func (b *TableBuilder) SetString(value string) error {
	return b.setRun(unsafe.Slice(unsafe.StringData(value), len(value)), 1, true, tafla.Align1)
}

// SetStruct sets a struct field from its bytes, as
// [tafla.TableBuilder.SetStruct] does. They're copied, however long.
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
	b.setField(length, tafla.Align1)

	return nil
}

// SetStructArray sets an array of structs of size bytes each, from their
// bytes, as [tafla.TableBuilder.SetStructArray] does.
func (b *TableBuilder) SetStructArray(bytes []byte, size int, alignment tafla.Alignment) error {
	mustBeValid(alignment)
	if size <= 0 || len(bytes)%size != 0 {
		return &tafla.BuildError{Kind: tafla.NotWholeStructs, Length: uint64(len(bytes)), Size: size}
	}

	return b.setRun(bytes, size, false, alignment)
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

	buffer, at := b.reserve(size)
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
// byte order, as elements of size bytes
func (b *TableBuilder) setArray(elements []byte, size int) error {
	// Kept as they are only in the table's byte order
	if hostLittleEndian || size == 1 {
		return b.setRun(elements, size, false, tafla.Alignment(size))
	}

	if b.set == b.fields {
		return b.tooManyFields()
	}
	width := uint64(b.layout.OffsetSize())
	padding := padding(tafla.Alignment(size), uint64(b.length)+width)
	length := uint64(b.length) + padding + width + uint64(len(elements))
	if length > uint64(b.limit) {
		return b.tooLong(length)
	}

	buffer, at := b.reserve(int(padding) + int(width) + len(elements))
	at = b.startArray(buffer, at, padding, len(elements)/size)
	putElements(buffer[at:at+len(elements)], elements, size)
	b.setField(length, tafla.Alignment(size))

	return nil
}

// setRun writes a field of bytes that start with their count: an array, a
// string or an array of structs, of size bytes each, padded so that they
// start at a multiple of alignment from the table's start. A run of
// segmentMin bytes or more is kept by reference, as a segment.
func (b *TableBuilder) setRun(bytes []byte, size int, isString bool, alignment tafla.Alignment) error {
	if b.set == b.fields {
		return b.tooManyFields()
	}
	width := uint64(b.layout.OffsetSize())
	padding := padding(alignment, uint64(b.length)+width)
	length := uint64(b.length) + padding + width + uint64(len(bytes))
	if length > uint64(b.limit) {
		return b.tooLong(length)
	}

	count := len(bytes) / size
	if isString {
		count = len(bytes)
	}

	if len(bytes) >= segmentMin {
		buffer, at := b.reserve(int(padding) + int(width))
		b.startArray(buffer, at, padding, count)
		b.addSegment(b.written, bytes)
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

	width := b.layout.OffsetSize()
	putSlot(buffer, at, width, uint32(count))

	return at + width
}

// setField counts a field that was written, which makes the table length
// bytes, and needs alignment from its start
func (b *TableBuilder) setField(length uint64, alignment tafla.Alignment) {
	b.length = uint32(length)
	b.alignment = max(b.alignment, alignment)
	b.set++
}

// NestedTable is a table to nest with [TableBuilder.SetTable] or in
// [TableBuilder.SetVector]: a *[TableBuilder], or [TableBytes].
type NestedTable interface {
	nestedTable()
}

func (*TableBuilder) nestedTable() {}

// TableBytes are a table's bytes, as its builder wrote them, to nest in
// another table.
type TableBytes []byte

func (TableBytes) nestedTable() {}

// SetTable embeds a table, from its builder or its bytes, as
// [tafla.TableBuilder.SetTable] does. A builder is copied in, as its bytes
// and its segments.
func (b *TableBuilder) SetTable(table NestedTable) error {
	alignment := tafla.Align4
	if builder, ok := table.(*TableBuilder); ok && builder != nil {
		alignment = builder.alignment
	}

	return b.setTable(table, alignment)
}

// SetTableAligned embeds a table padded to start at a multiple of alignment,
// as [tafla.TableBuilder.SetTableAligned] does.
func (b *TableBuilder) SetTableAligned(table NestedTable, alignment tafla.Alignment) error {
	mustBeValid(alignment)
	return b.setTable(table, alignment)
}

func (b *TableBuilder) setTable(table NestedTable, alignment tafla.Alignment) error {
	if err := checkNested(table); err != nil {
		return err
	}
	if b.set == b.fields {
		return b.tooManyFields()
	}
	padding := padding(alignment, uint64(b.length))
	length := uint64(b.length) + padding + nestedLength(table)
	if length > uint64(b.limit) {
		return b.tooLong(length)
	}

	buffer, at := b.reserve(int(padding))
	clear(buffer[at:])
	b.putOffset(buffer, b.length+uint32(padding))
	b.appendNested(table)
	b.setField(length, alignment)

	return nil
}

// SetVector writes a vector of tables, from their builders or their bytes,
// as [tafla.TableBuilder.SetVector] does. Builders are copied in, as their
// bytes and their segments.
func (b *TableBuilder) SetVector(items ...NestedTable) error {
	for _, item := range items {
		if err := checkNested(item); err != nil {
			return err
		}
	}
	if b.set == b.fields {
		return b.tooManyFields()
	}
	width := uint64(b.layout.OffsetSize())
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
	b.setField(length, tafla.Align1)

	return nil
}

// SetTableFunc writes a table of fields fields nested in this one, in place,
// as [tafla.TableBuilder.SetTableFunc] records it: fill sets its fields on
// the [Item] that it's given, and each is written straight into this
// builder's bytes. The table is written unpadded, and then moved forward by
// its padding, once its fields give its alignment.
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
	padding := padding(table.alignment, uint64(b.length))
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
// place, as [tafla.TableBuilder.SetVectorFunc] records them: fill(index,
// item) sets the fields of item index, for each in order. Once the first item
// is written, the buffer makes room for the others, each as long as it.
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
	width := uint64(b.layout.OffsetSize())
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
		start := b.written
		b.startInPlace(fields, header)
		err := fill(index, Item{builder: b})
		if err == nil {
			err = b.checkComplete()
		}
		if err != nil {
			b.rollBack(saved)
			return err
		}

		end += uint64(b.length)
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
	b.setField(length, tafla.Align1)

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
// bytes and segments, with its length in its header when it's sized, or
// bytes, which are a segment when they're long enough
func (b *TableBuilder) appendNested(table NestedTable) {
	nested, ok := table.(*TableBuilder)
	if !ok {
		bytes := table.(TableBytes)
		if len(bytes) >= segmentMin {
			b.addSegment(b.written, bytes)
			return
		}

		buffer, at := b.reserve(len(bytes))
		copy(buffer[at:], bytes)
		return
	}

	buffer, at := b.reserve(nested.written)
	copy(buffer[at:], nested.buffer()[:nested.written])
	if nested.layout.IsSized() {
		putSlot(buffer, at, nested.layout.OffsetSize(), nested.length)
	}

	for index := range nested.count {
		segment := nested.segment(index)
		b.addSegment(at+segment.at, segment.bytes())
	}
}

// nestedHeader returns the length of the offsets of a table of fields fields
// nested in this one in place, in the default layout of its offset type
func (b *TableBuilder) nestedHeader(fields int) (uint32, error) {
	return headerLength(tafla.TableLayout{}.WithOffsetType(b.layout.OffsetType()), fields)
}

// startInPlace starts the fields of a table of fields fields nested in
// place, after its offsets, of header bytes
func (b *TableBuilder) startInPlace(fields int, header uint32) {
	_, at := b.reserve(int(header))
	b.counters = counters{fields: uint32(fields), length: header, slots: at, alignment: tafla.Align1}
}

// moveForward moves the bytes written since saved forward by padding bytes,
// and the segments kept since, and zeroes the padding before them
func (b *TableBuilder) moveForward(saved checkpoint, padding int) {
	start := saved.written
	end := b.written
	buffer, _ := b.reserve(padding)
	copy(buffer[start+padding:], buffer[start:end])
	clear(buffer[start : start+padding])

	for index := saved.count; index < b.count; index++ {
		b.segment(index).at += padding
	}
}

// reserveItems makes room for count more items of a vector of bytes bytes
// each in the buffer, as its first item, so that writing them grows it
// once, unless they're longer. It never makes room for more than a table
// that fits in the offset type.
func (b *TableBuilder) reserveItems(count, bytes int) {
	room := min(uint64(count)*uint64(bytes), uint64(b.limit))
	if end := b.written + int(room); end > len(b.buffer()) {
		b.grow(end)
	}
}

func (b *TableBuilder) checkpoint() checkpoint {
	return checkpoint{counters: b.counters, written: b.written, count: b.count}
}

// rollBack truncates the buffer and the segments back to saved, and counts
// the fields of its table again, leaving the builder as it was
func (b *TableBuilder) rollBack(saved checkpoint) {
	// Cleared, so that the builder doesn't keep their values
	for index := saved.count; index < b.count; index++ {
		*b.segment(index) = segment{}
	}
	if b.count > inlineSegments {
		b.more = b.more[:max(saved.count-inlineSegments, 0)]
	}

	b.count = saved.count
	b.written = saved.written
	b.counters = saved.counters
}

func (b *TableBuilder) checkComplete() error {
	if b.set != b.fields {
		return &tafla.BuildError{Kind: tafla.Incomplete, Set: int(b.set), Fields: int(b.fields)}
	}

	return nil
}

func (b *TableBuilder) tooManyFields() error {
	return &tafla.BuildError{Kind: tafla.TooManyFields, Fields: int(b.fields)}
}

// tooLong returns the error of the next field, which would make the table
// length bytes
func (b *TableBuilder) tooLong(length uint64) error {
	return &tafla.BuildError{Kind: tafla.TooLong, Index: int(b.set), Length: length, OffsetType: b.layout.OffsetType()}
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
// more, and returns it
func (b *TableBuilder) grow(end int) []byte {
	if growByAppend && b.heap != nil {
		heap := slices.Grow(b.heap[:b.written], end-b.written)
		b.heap = heap[:cap(heap)]
		return b.heap
	}

	// Copied rather than appended, so that the builder never points into
	// itself, even where the compiler can't tell
	heap := make([]byte, max(end, firstHeapBytes, 2*len(b.buffer())))
	copy(heap, b.buffer()[:b.written])
	b.heap = heap

	return heap
}

// addSegment keeps bytes by reference, before the buffer's byte at
func (b *TableBuilder) addSegment(at int, bytes []byte) {
	segment := segment{at: at, data: unsafe.Pointer(unsafe.SliceData(bytes)), length: len(bytes)}
	if b.count < inlineSegments {
		b.segments[b.count] = segment
	} else {
		b.more = append(b.more, segment)
	}
	b.count++
}

func (b *TableBuilder) segment(index int) *segment {
	if index < inlineSegments {
		return &b.segments[index]
	}

	return &b.more[index-inlineSegments]
}

// putOffset writes the offset of the next field of the table being set
func (b *TableBuilder) putOffset(buffer []byte, offset uint32) {
	width := b.layout.OffsetSize()
	putSlot(buffer, b.slots+int(b.set)*width, width, offset)
}

// putSlot writes value at position in bytes, in width bytes
func putSlot(bytes []byte, position, width int, value uint32) {
	switch width {
	case 1:
		bytes[position] = uint8(value)
	case 2:
		binary.LittleEndian.PutUint16(bytes[position:], uint16(value))
	default:
		binary.LittleEndian.PutUint32(bytes[position:], value)
	}
}

// padding returns the bytes that bring offset to a multiple of alignment
func padding(alignment tafla.Alignment, offset uint64) uint64 {
	return -offset & uint64(alignment-1)
}

// mustBeValid panics unless the alignment is 1, 2, 4 or 8
func mustBeValid(alignment tafla.Alignment) {
	if alignment != tafla.Align1 && alignment != tafla.Align2 && alignment != tafla.Align4 && alignment != tafla.Align8 {
		panic("tafla: an alignment is 1, 2, 4 or 8 bytes, not " + strconv.Itoa(int(alignment)))
	}
}

// headerLength returns the byte length of the header of a table of fields
// fields in layout, as tafla's builder checks it
func headerLength(layout tafla.TableLayout, fields int) (uint32, error) {
	if fields < 0 {
		panic("tafla: a table can't hold " + strconv.Itoa(fields) + " fields")
	}

	size := uint64(layout.OffsetSize())
	slots := uint64(fields) + uint64(layout.HeaderSize())/size
	if slots > uint64(layout.OffsetType().MaxValue())/size {
		return 0, &tafla.BuildError{Kind: tafla.HeaderTooLong, Fields: fields, OffsetType: layout.OffsetType()}
	}

	return uint32(slots * size), nil
}

// checkVersion checks the layout's version, as tafla's builder checks it
func checkVersion(layout tafla.TableLayout, versioned bool, version uint32) error {
	if layout.IsVersioned() != versioned {
		if layout.IsVersioned() {
			panic("tafla: a versioned layout needs a version")
		}
		panic("tafla: the layout has no version slot")
	}

	if versioned && version > layout.OffsetType().MaxValue() {
		return &tafla.BuildError{Kind: tafla.VersionTooLarge, Version: version, OffsetType: layout.OffsetType()}
	}

	return nil
}

// elementBytes returns the bytes of values in place, in the host's byte
// order
func elementBytes[T tafla.Element](values []T) []byte {
	var zero T
	return unsafe.Slice((*byte)(unsafe.Pointer(unsafe.SliceData(values))), uintptr(len(values))*unsafe.Sizeof(zero))
}

// putElements writes elements of size bytes from their bytes in the host's
// byte order into bytes, little-endian
func putElements(bytes, elements []byte, size int) {
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
