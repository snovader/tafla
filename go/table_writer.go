package tafla

import (
	"encoding/binary"
	"math"
	"math/bits"
)

// TableSizer counts the bytes of a table, so that a [TableWriter] can write
// it into bytes of exactly that length.
//
// It's for code that knows a table's fields before it writes them, such as
// the code that tafla-gen generates, which writes tables without recording
// their fields as [TableBuilder] does.
//
// The fields are added in order, each with its type's method, which takes
// what decides its length: nothing for a scalar, the count of an array, the
// string itself. A nested table is added with [TableSizer.AddTable], and a
// vector with [TableSizer.AddVector], and the fields of each such table then
// follow between [TableSizer.StartTable] and [TableSizer.EndTable]. The
// sizer pads arrays and nested tables as TableBuilder does, so a writer that
// takes the same fields in the same order writes each where the sizer
// counted it.
//
// Adding a field doesn't fail: [TableSizer.Writer] returns the error of a
// table too long for its layout's offset type, or of an array of 0-byte
// structs, as TableBuilder's setter of the field would have.
type TableSizer struct {
	// The table's length so far, which is where the next field goes. It's in
	// uint64, so that it holds lengths past any table, and past any int on a
	// 32-bit host, up to past.
	position uint64
	// The start of the table being sized, which arrays are aligned from: 0
	// for the root, and past its header for the tables nested in it
	table uint64
	// The number of the root's fields
	fields int
	// The root's fields added so far, the last of which a table too long for
	// its offsets names
	rootFields int
	// The longest table that the offset type addresses, and an int holds
	maxLength uint64
	// The first failure, which ByteLength reports: an array of 0-byte
	// structs, or the root field at failedIndex that made the table
	// failedLength bytes, too long. It's kept as values, so that the methods
	// that add fields, which can fail, are cheap enough to inline.
	failure      BuildErrorKind
	failedIndex  int
	failedLength uint64
	version      uint32
	layout       TableLayout
}

// SizerParent is the table that a nested table or vector item was started
// in, which [TableSizer.EndTable] takes back.
type SizerParent struct {
	table uint64
}

// NewTableSizer starts sizing a table of fields fields in layout, which its
// readers must use too. Nested tables use the default layout. It fails with
// HeaderTooLong, as [NewTableBuilder] does, and panics, as it does, if the
// layout is versioned or fields is negative.
func NewTableSizer(fields int, layout TableLayout) (TableSizer, error) {
	return newTableSizer(fields, layout, false, 0)
}

// NewVersionedTableSizer starts sizing a table of fields fields in layout,
// whose writer writes version in its header. It fails with VersionTooLarge
// or HeaderTooLong, as [NewVersionedTableBuilder] does, and panics, as it
// does, if the layout isn't versioned or fields is negative.
func NewVersionedTableSizer(fields int, layout TableLayout, version uint32) (TableSizer, error) {
	return newTableSizer(fields, layout, true, version)
}

func newTableSizer(fields int, layout TableLayout, versioned bool, version uint32) (TableSizer, error) {
	if layout.versioned != versioned || version > layout.offsetType.MaxValue() ||
		uint64(fields) > layout.maxFields() {
		return TableSizer{}, layout.sizerFailure(fields, versioned, version)
	}

	// Built in place, rather than built and then copied
	return TableSizer{
		position:  uint64(layout.HeaderSize() + fields*layout.offsetType.Size()),
		fields:    fields,
		maxLength: layout.maxLength(),
		version:   version,
		layout:    layout,
	}, nil
}

// sizerFailure panics for a layout that's versioned other than the
// constructor or a negative number of fields, and returns the error of a
// version or header that doesn't fit
func (l TableLayout) sizerFailure(fields int, versioned bool, version uint32) error {
	if err := l.checkVersion(versioned, version); err != nil {
		return err
	}
	_, err := l.headerLength(fields)

	return err
}

// ByteLength returns the byte length of the table, with every field added.
//
// It fails with TooLong if the table is too long for its layout's offset
// type: longer than 255 bytes, 64 KiB or 4 GiB, or than an int on a 32-bit
// host. The error names the root field that made it too long, and the length
// that the root's fields up to it make the table, as TableBuilder's setter
// of the field returns. It fails with NotWholeStructs if an array of 0-byte
// structs was added, unless a field before it made the table too long.
func (s *TableSizer) ByteLength() (int, error) {
	if s.failure == 0 && s.position <= s.maxLength {
		return int(s.position), nil
	}

	return 0, s.failed()
}

// failed returns the first failure that ByteLength reports, or the error of
// the last field, which made the table too long
func (s *TableSizer) failed() error {
	switch s.failure {
	case NotWholeStructs:
		return &BuildError{Kind: NotWholeStructs}
	case TooLong:
		return s.tooLong(s.failedIndex, s.failedLength)
	default:
		return s.tooLong(max(s.rootFields-1, 0), s.position)
	}
}

func (s *TableSizer) tooLong(index int, length uint64) error {
	return &BuildError{Kind: TooLong, Index: index, Length: length, OffsetType: s.layout.offsetType}
}

// Writer returns a writer of the table into new bytes of its length, which
// takes its fields in the order that they were added. It fails with the
// errors of [TableSizer.ByteLength], before allocating the bytes.
func (s *TableSizer) Writer() (TableWriter, error) {
	return s.AppendWriter(nil)
}

// AppendWriter returns a writer of the table after the bytes of dst, into its
// capacity when it has room and into new bytes otherwise, as append does.
// [TableWriter.Bytes] returns dst with the table appended. It fails with the
// errors of [TableSizer.ByteLength], before writing or allocating any bytes.
//
// The table's arrays are aligned from its start, so they're aligned in
// memory only when it starts at a multiple of 8 from an aligned address, as
// it does after an empty dst. Otherwise, readers copy them.
func (s *TableSizer) AppendWriter(dst []byte) (TableWriter, error) {
	length, err := s.ByteLength()
	if err != nil {
		return TableWriter{}, err
	}

	return newTableWriter(dst, length, s.fields, s.layout, s.version), nil
}

// StartTable starts the fields of a nested table or vector item of fields
// fields, after its parent's [TableSizer.AddTable] or
// [TableSizer.AddVector].
func (s *TableSizer) StartTable(fields int) SizerParent {
	parent := SizerParent{table: s.table}
	s.table = s.position
	s.position = min(s.position+elements(fields)*s.width(), past)

	return parent
}

// EndTable ends the fields of the table that [TableSizer.StartTable]
// started.
func (s *TableSizer) EndTable(parent SizerParent) {
	s.table = parent.table
}

// AddInt8 adds an int8 field.
func (s *TableSizer) AddInt8() { s.add(1) }

// AddUint8 adds a uint8 field.
func (s *TableSizer) AddUint8() { s.add(1) }

// AddInt16 adds an int16 field.
func (s *TableSizer) AddInt16() { s.add(2) }

// AddUint16 adds a uint16 field.
func (s *TableSizer) AddUint16() { s.add(2) }

// AddInt32 adds an int32 field.
func (s *TableSizer) AddInt32() { s.add(4) }

// AddUint32 adds a uint32 field.
func (s *TableSizer) AddUint32() { s.add(4) }

// AddInt64 adds an int64 field.
func (s *TableSizer) AddInt64() { s.add(8) }

// AddUint64 adds a uint64 field.
func (s *TableSizer) AddUint64() { s.add(8) }

// AddFloat32 adds a float32 field.
func (s *TableSizer) AddFloat32() { s.add(4) }

// AddFloat64 adds a float64 field.
func (s *TableSizer) AddFloat64() { s.add(8) }

// AddInt8Array adds an array field of count int8s.
func (s *TableSizer) AddInt8Array(count int) { s.addArray(elements(count), Align1) }

// AddUint8Array adds an array field of count uint8s.
func (s *TableSizer) AddUint8Array(count int) { s.addArray(elements(count), Align1) }

// AddInt16Array adds an array field of count int16s.
func (s *TableSizer) AddInt16Array(count int) { s.addArray(elements(count)*2, Align2) }

// AddUint16Array adds an array field of count uint16s.
func (s *TableSizer) AddUint16Array(count int) { s.addArray(elements(count)*2, Align2) }

// AddInt32Array adds an array field of count int32s.
func (s *TableSizer) AddInt32Array(count int) { s.addArray(elements(count)*4, Align4) }

// AddUint32Array adds an array field of count uint32s.
func (s *TableSizer) AddUint32Array(count int) { s.addArray(elements(count)*4, Align4) }

// AddInt64Array adds an array field of count int64s.
func (s *TableSizer) AddInt64Array(count int) { s.addArray(elements(count)*8, Align8) }

// AddUint64Array adds an array field of count uint64s.
func (s *TableSizer) AddUint64Array(count int) { s.addArray(elements(count)*8, Align8) }

// AddFloat32Array adds an array field of count float32s.
func (s *TableSizer) AddFloat32Array(count int) { s.addArray(elements(count)*4, Align4) }

// AddFloat64Array adds an array field of count float64s.
func (s *TableSizer) AddFloat64Array(count int) { s.addArray(elements(count)*8, Align8) }

// AddString adds a string field, of its bytes, which are UTF-8 if it is.
func (s *TableSizer) AddString(value string) {
	s.add(min(s.width()+uint64(len(value)), past))
}

// AddStruct adds a struct field of size bytes.
func (s *TableSizer) AddStruct(size int) {
	s.add(min(uint64(size), past))
}

// AddStructArray adds an array of count structs of size bytes each, padded
// so that they start at a multiple of alignment from the start of the
// table. [TableSizer.Writer] fails with NotWholeStructs if size is 0, as
// [TableBuilder.SetStructArray] does.
func (s *TableSizer) AddStructArray(count, size int, alignment Alignment) {
	high, byteLength := bits.Mul64(uint64(count), uint64(size))
	if high != 0 {
		byteLength = past
	}
	s.addArray(min(byteLength, past), alignment)

	// After addArray, which reports a field before this one that made the
	// table too long, as TableBuilder's setter of that field does first
	if size == 0 && s.failure == 0 {
		s.failure = NotWholeStructs
	}
}

// AddTable adds a nested table, padded to start at a multiple of alignment,
// whose fields then follow between [TableSizer.StartTable] and
// [TableSizer.EndTable].
func (s *TableSizer) AddTable(alignment Alignment) {
	s.startField()
	s.position = min(s.position+alignment.padding(s.position-s.table), past)
}

// AddVector adds a vector of count tables, whose fields then follow, each
// between [TableSizer.StartTable] and [TableSizer.EndTable].
func (s *TableSizer) AddVector(count int) {
	// The count, and then the end of each item
	s.add((elements(count) + 1) * s.width())
}

// add adds a field of byteLength bytes, without padding
func (s *TableSizer) add(byteLength uint64) {
	s.startField()
	s.position = min(s.position+byteLength, past)
}

// addArray adds an array of byteLength bytes after its count, padded so
// that its elements are aligned
func (s *TableSizer) addArray(byteLength uint64, alignment Alignment) {
	s.startField()

	start := s.position + s.width()
	s.position = min(start+alignment.padding(start-s.table)+byteLength, past)
}

// startField counts a field of the root, after checking the length that the
// root's fields before it make the table. The length is checked only
// between the root's fields, so that it's the length after the whole field
// that made the table too long, as TableBuilder reports it.
func (s *TableSizer) startField() {
	// Only the root starts at 0, since a nested table follows the offset of
	// the field that holds it
	if s.table != 0 {
		return
	}

	// The root's first field starts after the header, which fits, so a
	// field before this one made the table too long
	if s.position > s.maxLength && s.failure == 0 {
		s.failure, s.failedIndex, s.failedLength = TooLong, s.rootFields-1, s.position
	}
	s.rootFields++
}

// width returns the offset size, the width of offsets, lengths and header
// slots
func (s *TableSizer) width() uint64 {
	return uint64(s.layout.offsetType.Size())
}

// past is a length past any table, which the sizer counts lengths up to, so
// that they neither wrap nor saturate at the limit of a uint64: a count or
// length that isn't a slice's, such as a negative one, counts as past, and
// a sum of two lengths up to past doesn't wrap. Counting up to it rather than
// saturating each sum keeps the methods that add fields cheap enough to
// inline.
const past = 1 << 62

// elements returns a count of elements of up to 8 bytes, as a number whose
// product with their size is at most past
func elements(count int) uint64 {
	return min(uint64(count), past/8)
}

// TableWriter writes a table forward into bytes of exactly its length, which
// a [TableSizer] counted.
//
// It doesn't record the fields first, as [TableBuilder] does before it writes
// them through a writer of its own. It comes from [TableSizer.Writer] or
// [TableSizer.AppendWriter], and takes the fields that the sizer added, in
// the same order, each with its index in its table.
//
// Each field's offset is written as the writer reaches the field. A nested
// table follows its parent's [TableWriter.SetTable], between
// [TableWriter.StartTable] and [TableWriter.EndTable]; the items of a vector
// follow its [TableWriter.SetVector] the same way, each followed by
// [TableWriter.SetItemEnd], which writes where it ends. Arrays and nested
// tables are padded so that their elements are aligned from their table's
// start, as the sizer counts them. The writer writes every byte of a table
// whose fields are as sized, its padding's zeros too, so the bytes that
// AppendWriter writes into needn't be zero.
//
// It's the target of the code that tafla-gen generates, whose calls follow
// from the schema, so it doesn't check them again: fields are set in index
// order, each once, as the sizer added them, and each vector item is
// followed by its [TableWriter.SetItemEnd]. A field index outside its table,
// or an item outside its vector, writes an offset or item end over another
// slot or field, so the table's bytes are undefined, though never outside
// the table's bytes: a write past them panics, as indexing a slice past its
// length does. Hand-written tables use TableBuilder, which checks every
// field.
//
// Fields that differ from those that the sizer added, such as values that
// changed in between, never write outside the table's bytes either. A field
// that would end past them panics, and [TableWriter.Bytes] fails with
// NotAsSized for fields that didn't fill the table.
type TableWriter struct {
	// The table's bytes, of its length exactly
	bytes []byte
	// The bytes that Bytes returns, which end with the table's
	whole []byte
	// Where the next field goes, from the table's start
	position int
	// The start of the table being written, which its offsets count from: 0
	// for the root
	table int
	// The position of that table's first offset
	slots int
	// The byte length of the root's header slots, which its offsets follow
	headerSize int
	// The offset size, the width of offsets, counts, item ends and header
	// slots
	width int
	// The first error of an array of structs given bytes that aren't whole
	// structs, which Bytes returns
	err *BuildError
}

// WriterParent is the table that a nested table or vector item was started
// in, which [TableWriter.EndTable] takes back.
type WriterParent struct {
	table int
}

// VectorPosition is the position of a vector that [TableWriter.SetVector]
// wrote, which [TableWriter.SetItemEnd] takes.
type VectorPosition struct {
	position int
}

// newTableWriter writes the header of the root table of fields fields after
// dst, into length bytes that it appends, which the sizer or TableBuilder
// checked are at least the header and at most what the offset type
// addresses
func newTableWriter(dst []byte, length, fields int, layout TableLayout, version uint32) TableWriter {
	start := len(dst)
	var whole []byte
	switch {
	case dst == nil:
		// Exactly the table's length
		whole = make([]byte, length)
	case cap(dst)-start >= length:
		whole = dst[:start+length]
	default:
		whole = append(dst, make([]byte, length)...)
	}
	table := whole[start : start+length : start+length]

	width := layout.offsetType.Size()
	headerSize := layout.HeaderSize()
	if layout.sized {
		putSlot(table, 0, width, uint32(length))
	}
	if layout.versioned {
		putSlot(table, headerSize-width, width, version)
	}

	// Built in place, rather than built and then copied
	return TableWriter{
		bytes:      table,
		whole:      whole,
		position:   headerSize + fields*width,
		slots:      headerSize,
		headerSize: headerSize,
		width:      width,
	}
}

// Bytes returns the bytes that the writer wrote the table into, rather than a
// copy: the table's own, from [TableSizer.Writer], or dst with the table
// appended, from [TableSizer.AppendWriter]. It fails with NotAsSized if the
// fields didn't fill the table, as when they differ from those that the
// sizer added, and with NotWholeStructs if an array of structs was given
// bytes that aren't whole structs.
func (w *TableWriter) Bytes() ([]byte, error) {
	if w.err != nil {
		return nil, w.err
	}

	if w.position != len(w.bytes) {
		return nil, &BuildError{Kind: NotAsSized, Written: w.position, Length: uint64(len(w.bytes))}
	}

	return w.whole, nil
}

// StartTable starts the fields of a nested table or vector item of fields
// fields, after its parent's [TableWriter.SetTable] or
// [TableWriter.SetVector].
func (w *TableWriter) StartTable(fields int) WriterParent {
	parent := WriterParent{table: w.table}

	w.table = w.position
	w.slots = w.position
	w.position += fields * w.width

	return parent
}

// EndTable ends the fields of the table that [TableWriter.StartTable]
// started.
func (w *TableWriter) EndTable(parent WriterParent) {
	w.table = parent.table
	// Only the root starts at 0, and only its offsets follow a header
	if parent.table == 0 {
		w.slots = w.headerSize
	} else {
		w.slots = parent.table
	}
}

// SetInt8 writes int8 field index.
func (w *TableWriter) SetInt8(index int, value int8) {
	w.SetUint8(index, uint8(value))
}

// SetUint8 writes uint8 field index.
func (w *TableWriter) SetUint8(index int, value uint8) {
	w.setOffset(index)
	w.bytes[w.position] = value
	w.position++
}

// SetInt16 writes int16 field index.
func (w *TableWriter) SetInt16(index int, value int16) {
	w.SetUint16(index, uint16(value))
}

// SetUint16 writes uint16 field index.
func (w *TableWriter) SetUint16(index int, value uint16) {
	w.setOffset(index)
	binary.LittleEndian.PutUint16(w.bytes[w.position:], value)
	w.position += 2
}

// SetInt32 writes int32 field index.
func (w *TableWriter) SetInt32(index int, value int32) {
	w.SetUint32(index, uint32(value))
}

// SetUint32 writes uint32 field index.
func (w *TableWriter) SetUint32(index int, value uint32) {
	w.setOffset(index)
	binary.LittleEndian.PutUint32(w.bytes[w.position:], value)
	w.position += 4
}

// SetInt64 writes int64 field index.
func (w *TableWriter) SetInt64(index int, value int64) {
	w.SetUint64(index, uint64(value))
}

// SetUint64 writes uint64 field index.
func (w *TableWriter) SetUint64(index int, value uint64) {
	w.setOffset(index)
	binary.LittleEndian.PutUint64(w.bytes[w.position:], value)
	w.position += 8
}

// SetFloat32 writes float32 field index.
func (w *TableWriter) SetFloat32(index int, value float32) {
	w.SetUint32(index, math.Float32bits(value))
}

// SetFloat64 writes float64 field index.
func (w *TableWriter) SetFloat64(index int, value float64) {
	w.SetUint64(index, math.Float64bits(value))
}

// SetInt8Array writes array field index of int8s.
func (w *TableWriter) SetInt8Array(index int, values []int8) { setElements(w, index, values) }

// SetUint8Array writes array field index of uint8s.
func (w *TableWriter) SetUint8Array(index int, values []uint8) { setElements(w, index, values) }

// SetInt16Array writes array field index of int16s.
func (w *TableWriter) SetInt16Array(index int, values []int16) { setElements(w, index, values) }

// SetUint16Array writes array field index of uint16s.
func (w *TableWriter) SetUint16Array(index int, values []uint16) { setElements(w, index, values) }

// SetInt32Array writes array field index of int32s.
func (w *TableWriter) SetInt32Array(index int, values []int32) { setElements(w, index, values) }

// SetUint32Array writes array field index of uint32s.
func (w *TableWriter) SetUint32Array(index int, values []uint32) { setElements(w, index, values) }

// SetInt64Array writes array field index of int64s.
func (w *TableWriter) SetInt64Array(index int, values []int64) { setElements(w, index, values) }

// SetUint64Array writes array field index of uint64s.
func (w *TableWriter) SetUint64Array(index int, values []uint64) { setElements(w, index, values) }

// SetFloat32Array writes array field index of float32s.
func (w *TableWriter) SetFloat32Array(index int, values []float32) { setElements(w, index, values) }

// SetFloat64Array writes array field index of float64s.
func (w *TableWriter) SetFloat64Array(index int, values []float64) { setElements(w, index, values) }

// SetString writes string field index, as its bytes, which are UTF-8 if it
// is.
func (w *TableWriter) SetString(index int, value string) {
	w.setOffset(index)
	w.putSlot(w.position, uint32(len(value)))
	w.position += w.width
	w.position += copy(w.bytes[w.position:w.position+len(value)], value)
}

// SetStruct writes struct field index from its bytes: its scalars,
// little-endian, one after another, which are written as they are, without
// padding.
func (w *TableWriter) SetStruct(index int, bytes []byte) {
	w.setOffset(index)
	w.position += copy(w.bytes[w.position:w.position+len(bytes)], bytes)
}

// ReserveStruct writes the offset of struct field index, of size bytes, and
// returns its bytes, which the caller writes the struct's scalars into,
// little-endian, before the next field, as the code that tafla-gen generates
// writes its structs. They aren't zeroed in bytes that
// [TableSizer.AppendWriter] reuses.
func (w *TableWriter) ReserveStruct(index, size int) []byte {
	w.setOffset(index)

	start := w.position
	w.position += size
	return w.bytes[start:w.position:w.position]
}

// SetStructArray writes field index, an array of structs of size bytes each,
// from their bytes one after another, which it counts. It's padded so that
// the structs start at a multiple of alignment from the start of the table,
// which is the size of the structs' largest scalar. [TableWriter.Bytes]
// fails with NotWholeStructs if size is 0 or the bytes aren't a whole number
// of structs.
func (w *TableWriter) SetStructArray(index int, bytes []byte, size int, alignment Alignment) {
	if size <= 0 || len(bytes)%size != 0 {
		if w.err == nil {
			w.err = &BuildError{Kind: NotWholeStructs, Length: uint64(len(bytes)), Size: size}
		}
		return
	}

	copy(w.ReserveStructArray(index, len(bytes)/size, size, alignment), bytes)
}

// ReserveStructArray writes the offset and count of field index, an array of
// count structs of size bytes each, padded so that they start at a multiple
// of alignment from the start of the table, which is the size of the
// structs' largest scalar. It returns the bytes of the structs, count times
// size, which the caller writes each struct's scalars into, little-endian,
// before the next field, as the code that tafla-gen generates writes its
// structs without collecting their bytes first. They aren't zeroed in bytes
// that [TableSizer.AppendWriter] reuses.
func (w *TableWriter) ReserveStructArray(index, count, size int, alignment Alignment) []byte {
	w.startArray(index, count, alignment)

	start := w.position
	w.position += count * size
	return w.bytes[start:w.position:w.position]
}

// SetTable writes the offset of nested table field index, padded to start at
// a multiple of alignment, whose fields then follow between
// [TableWriter.StartTable] and [TableWriter.EndTable].
func (w *TableWriter) SetTable(index int, alignment Alignment) {
	w.pad(int(alignment.padding(uint64(w.position - w.table))))
	w.setOffset(index)
}

// SetVector writes the offset and count of vector field index, of count
// tables, whose items then follow, each between [TableWriter.StartTable] and
// [TableWriter.EndTable] and then [TableWriter.SetItemEnd].
func (w *TableWriter) SetVector(index, count int) VectorPosition {
	w.setOffset(index)

	// The count, and then the end of each item, which is written after the
	// item
	position := w.position
	w.putSlot(position, uint32(count))
	w.position += (count + 1) * w.width

	return VectorPosition{position: position}
}

// SetItemEnd writes the end of item item of the vector, which was just
// written.
func (w *TableWriter) SetItemEnd(vector VectorPosition, item int) {
	// Each end counts from the vector's position, which is its count's
	slot := vector.position + (item+1)*w.width
	w.putSlot(slot, uint32(w.position-vector.position))
}

// copyTable writes a finished table's bytes as they are, in place of the
// fields that StartTable and EndTable would enclose: after its parent's
// SetTable, or as a vector item before its SetItemEnd. It's for the tables
// that TableBuilder can't write in place: tables nested as bytes, and
// builders with a header or another offset type.
func (w *TableWriter) copyTable(table []byte) {
	w.position += copy(w.bytes[w.position:w.position+len(table)], table)
}

func setElements[T Element](w *TableWriter, index int, values []T) {
	w.startArray(index, len(values), Alignment(sizeOf[T]()))

	end := w.position + len(values)*sizeOf[T]()
	putElements(w.bytes[w.position:end], values)
	w.position = end
}

// startArray pads an array so that its elements, after its count, are
// aligned, and writes its offset and count
func (w *TableWriter) startArray(index, count int, alignment Alignment) {
	w.pad(int(alignment.padding(uint64(w.position + w.width - w.table))))
	w.setOffset(index)
	w.putSlot(w.position, uint32(count))
	w.position += w.width
}

// setOffset writes the offset of field index of the table being written,
// which is where the field starts
func (w *TableWriter) setOffset(index int) {
	w.putSlot(w.slots+index*w.width, uint32(w.position-w.table))
}

// pad writes count zeros of padding, at most 7
func (w *TableWriter) pad(count int) {
	padding := w.bytes[w.position : w.position+count]
	for index := range padding {
		padding[index] = 0
	}
	w.position += count
}

// putSlot writes an offset, count, length, item end or header slot at
// position. A value is at most the table's length, which fits in the
// offsets, unless the fields differ from those sized.
func (w *TableWriter) putSlot(position int, value uint32) {
	putSlot(w.bytes, position, w.width, value)
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
