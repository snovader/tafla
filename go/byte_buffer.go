package tafla

import (
	"encoding/binary"
	"math"
	"strconv"
	"unicode/utf8"
	"unsafe"
)

// intIs64 says whether an int is 64 bits wide, so that it holds any offset
// added to any position, which a 32-bit int doesn't
const intIs64 = strconv.IntSize == 64

// ByteBuffer reads a table from the supplied bytes, in the layout it was
// written with.
//
// Fields are found by index with [ByteBuffer.RootOffset], or
// [ByteBuffer.FieldOffset] for a table nested in another, and then read at
// that position by their type's getter.
//
// Every read checks that it stays within the bytes and returns a
// *[DecodeError] otherwise, so no bytes, however corrupted, make a read
// panic or return values from outside them. Strings must be valid UTF-8.
//
// Arrays, strings, structs and vector items are read in place, sharing the
// bytes rather than copying them, so they change when the bytes do. A
// string shares them too, which Go's strings otherwise don't: the bytes
// must not change while it's in use.
//
// A ByteBuffer is a small value, which is cheap to copy and to create for
// each read. The zero ByteBuffer reads empty bytes in the default layout.
type ByteBuffer struct {
	bytes  []byte
	layout TableLayout
}

// NewByteBuffer returns a reader of the table in bytes, in layout.
func NewByteBuffer(bytes []byte, layout TableLayout) ByteBuffer {
	return ByteBuffer{bytes: bytes, layout: layout}
}

// Bytes returns the bytes that the buffer reads.
func (b ByteBuffer) Bytes() []byte {
	return b.bytes
}

// Layout returns the layout that the buffer reads the table in.
func (b ByteBuffer) Layout() TableLayout {
	return b.layout
}

// Size returns the table's byte length from its header when the layout is
// sized, and otherwise the length of the bytes. It fails with OutOfRange if
// the header doesn't lie within the bytes.
func (b ByteBuffer) Size() (int, error) {
	if !b.layout.sized {
		return len(b.bytes), nil
	}

	return b.getPosition(0)
}

// Version returns the version that the table's header carries after its
// length. It's as wide as the header's other slots. It fails with OutOfRange
// if the header doesn't lie within the bytes, and panics if the layout isn't
// versioned.
//
// The buffer doesn't compare versions: a reader of version supported
// returns [NewUnsupportedVersionError] when the version isn't its own.
func (b ByteBuffer) Version() (uint32, error) {
	if !b.layout.versioned {
		panic("tafla: the layout has no version")
	}

	position := 0
	if b.layout.sized {
		position = b.layout.offsetType.Size()
	}

	return b.getSlot(position)
}

// maxIndex is the largest index of a field whose slot lies at a position
// that an int holds, after any header. A larger index names no field, and
// fails as a read at the largest int.
const maxIndex = (math.MaxInt - 8) / 4

// RootOffset returns the position of field index of the root table. It
// fails with OutOfRange if the field's offset doesn't lie within the bytes,
// and panics if index is negative.
func (b ByteBuffer) RootOffset(index int) (int, error) {
	if index < 0 {
		panic("tafla: a negative field index")
	}

	size := b.layout.offsetType.Size()
	if index > maxIndex {
		return 0, outOfRange(math.MaxInt, uint64(size), len(b.bytes))
	}

	return b.getPosition(b.layout.headerSlots()*size + index*size)
}

// FieldOffset returns the position of field index of a nested table, which
// has no header and stores its field offsets from its own start.
// tablePosition is the nested table's position, which is the position of
// the field that holds it. It fails with OutOfRange if the field's offset
// doesn't lie within the bytes, and panics if index is negative.
func (b ByteBuffer) FieldOffset(tablePosition, index int) (int, error) {
	if index < 0 {
		panic("tafla: a negative field index")
	}

	size := b.layout.offsetType.Size()
	if index > maxIndex {
		return 0, outOfRange(math.MaxInt, uint64(size), len(b.bytes))
	}
	if tablePosition < 0 {
		return 0, outOfRange(tablePosition, uint64(size), len(b.bytes))
	}

	// A slot past the largest int wraps to a negative position, which the
	// read rejects
	offset, err := b.getSlot(tablePosition + index*size)
	if err != nil {
		return 0, err
	}

	// Only a 32-bit int can overflow, at a position past any bytes
	position := uint(tablePosition) + uint(offset)
	if !intIs64 && (position < uint(tablePosition) || position > math.MaxInt) {
		return 0, outOfRange(tablePosition, uint64(offset), len(b.bytes))
	}

	return int(position), nil
}

// Uint32FieldOffset returns the position of a field of the table at table in
// bytes, whose offsets are uint32s, from its offset at slot: table plus the
// offset. It fails with OutOfRange if the offset doesn't lie within the
// bytes.
//
// It's [ByteBuffer.FieldOffset] for the code that tafla-gen generates, which
// knows a table's offset type and the slot of each field, as a function
// small enough to inline into each getter: a nested table's slots follow
// its position, and a root's its header, from position 0. table is a
// position that a read returned, which isn't negative.
func Uint32FieldOffset(bytes []byte, table, slot int) (int, error) {
	if slot < 0 || slot > len(bytes)-4 {
		return 0, outOfRange(slot, 4, len(bytes))
	}

	// Only a 32-bit int can overflow, at a position past any bytes
	offset := binary.LittleEndian.Uint32(bytes[slot:])
	position := uint(table) + uint(offset)
	if !intIs64 && (position < uint(table) || position > math.MaxInt) {
		return 0, outOfRange(table, uint64(offset), len(bytes))
	}

	return int(position), nil
}

// GetInt8 reads the int8 at position. It fails with OutOfRange if the value
// doesn't lie within the bytes.
func (b ByteBuffer) GetInt8(position int) (int8, error) {
	value, err := b.GetUint8(position)
	return int8(value), err
}

// GetUint8 reads the uint8 at position. It fails with OutOfRange if the
// value doesn't lie within the bytes.
func (b ByteBuffer) GetUint8(position int) (uint8, error) {
	if uint(position) >= uint(len(b.bytes)) {
		return 0, outOfRange(position, 1, len(b.bytes))
	}

	return b.bytes[position], nil
}

// GetInt16 reads the int16 at position. It fails with OutOfRange if the
// value doesn't lie within the bytes.
func (b ByteBuffer) GetInt16(position int) (int16, error) {
	value, err := b.GetUint16(position)
	return int16(value), err
}

// GetUint16 reads the uint16 at position. It fails with OutOfRange if the
// value doesn't lie within the bytes.
func (b ByteBuffer) GetUint16(position int) (uint16, error) {
	if position < 0 || position > len(b.bytes)-2 {
		return 0, outOfRange(position, 2, len(b.bytes))
	}

	return binary.LittleEndian.Uint16(b.bytes[position:]), nil
}

// GetInt32 reads the int32 at position. It fails with OutOfRange if the
// value doesn't lie within the bytes.
func (b ByteBuffer) GetInt32(position int) (int32, error) {
	value, err := b.GetUint32(position)
	return int32(value), err
}

// GetUint32 reads the uint32 at position. It fails with OutOfRange if the
// value doesn't lie within the bytes.
func (b ByteBuffer) GetUint32(position int) (uint32, error) {
	if position < 0 || position > len(b.bytes)-4 {
		return 0, outOfRange(position, 4, len(b.bytes))
	}

	return binary.LittleEndian.Uint32(b.bytes[position:]), nil
}

// GetInt64 reads the int64 at position. It fails with OutOfRange if the
// value doesn't lie within the bytes.
func (b ByteBuffer) GetInt64(position int) (int64, error) {
	value, err := b.GetUint64(position)
	return int64(value), err
}

// GetUint64 reads the uint64 at position. It fails with OutOfRange if the
// value doesn't lie within the bytes.
func (b ByteBuffer) GetUint64(position int) (uint64, error) {
	if position < 0 || position > len(b.bytes)-8 {
		return 0, outOfRange(position, 8, len(b.bytes))
	}

	return binary.LittleEndian.Uint64(b.bytes[position:]), nil
}

// GetFloat32 reads the float32 at position. It fails with OutOfRange if the
// value doesn't lie within the bytes.
func (b ByteBuffer) GetFloat32(position int) (float32, error) {
	value, err := b.GetUint32(position)
	return math.Float32frombits(value), err
}

// GetFloat64 reads the float64 at position. It fails with OutOfRange if the
// value doesn't lie within the bytes.
func (b ByteBuffer) GetFloat64(position int) (float64, error) {
	value, err := b.GetUint64(position)
	return math.Float64frombits(value), err
}

// GetInt8Array reads the array of int8s at position. It fails with
// OutOfRange if the array doesn't lie within the bytes.
func (b ByteBuffer) GetInt8Array(position int) (ArrayView[int8], error) {
	return getArray[int8](b, position)
}

// GetUint8Array reads the array of bytes at position, which are the table's.
// It fails with OutOfRange if the array doesn't lie within the bytes.
func (b ByteBuffer) GetUint8Array(position int) ([]byte, error) {
	return b.arrayBytes(position, 1)
}

// GetInt16Array reads the array of int16s at position. It fails with
// OutOfRange if the array doesn't lie within the bytes.
func (b ByteBuffer) GetInt16Array(position int) (ArrayView[int16], error) {
	return getArray[int16](b, position)
}

// GetUint16Array reads the array of uint16s at position. It fails with
// OutOfRange if the array doesn't lie within the bytes.
func (b ByteBuffer) GetUint16Array(position int) (ArrayView[uint16], error) {
	return getArray[uint16](b, position)
}

// GetInt32Array reads the array of int32s at position. It fails with
// OutOfRange if the array doesn't lie within the bytes.
func (b ByteBuffer) GetInt32Array(position int) (ArrayView[int32], error) {
	return getArray[int32](b, position)
}

// GetUint32Array reads the array of uint32s at position. It fails with
// OutOfRange if the array doesn't lie within the bytes.
func (b ByteBuffer) GetUint32Array(position int) (ArrayView[uint32], error) {
	return getArray[uint32](b, position)
}

// GetInt64Array reads the array of int64s at position. It fails with
// OutOfRange if the array doesn't lie within the bytes.
func (b ByteBuffer) GetInt64Array(position int) (ArrayView[int64], error) {
	return getArray[int64](b, position)
}

// GetUint64Array reads the array of uint64s at position. It fails with
// OutOfRange if the array doesn't lie within the bytes.
func (b ByteBuffer) GetUint64Array(position int) (ArrayView[uint64], error) {
	return getArray[uint64](b, position)
}

// GetFloat32Array reads the array of float32s at position. It fails with
// OutOfRange if the array doesn't lie within the bytes.
func (b ByteBuffer) GetFloat32Array(position int) (ArrayView[float32], error) {
	return getArray[float32](b, position)
}

// GetFloat64Array reads the array of float64s at position. It fails with
// OutOfRange if the array doesn't lie within the bytes.
func (b ByteBuffer) GetFloat64Array(position int) (ArrayView[float64], error) {
	return getArray[float64](b, position)
}

// GetString reads the string at position, which must be valid UTF-8. A
// leading U+FEFF is part of the string rather than a byte order mark. It
// fails with OutOfRange if the string doesn't lie within the bytes, or with
// InvalidUTF8 if it isn't valid UTF-8.
//
// The string shares the table's bytes rather than copying them, so they
// must not change while it's in use, as for [unsafe.String]. Copy it, such
// as with strings.Clone, to keep it past them.
func (b ByteBuffer) GetString(position int) (string, error) {
	bytes, err := b.arrayBytes(position, 1)
	if err != nil {
		return "", err
	}

	if !utf8.Valid(bytes) {
		return "", &DecodeError{Kind: InvalidUTF8, Position: position}
	}

	return unsafe.String(unsafe.SliceData(bytes), len(bytes)), nil
}

// GetStruct reads the size bytes of the struct at position, whose scalars
// lie at fixed offsets from its start. They're the table's bytes. It fails
// with OutOfRange if the struct doesn't lie within the bytes, and panics if
// size is negative.
func (b ByteBuffer) GetStruct(position, size int) ([]byte, error) {
	if size < 0 {
		panic("tafla: a negative struct size")
	}

	return b.byteRange(position, uint64(size))
}

// GetStructArray reads the bytes of the structs of the array at position,
// each of size bytes, one after another after the count. They're the
// table's bytes, and their number is their length over size. Structs of 0
// bytes take none, so a size of 0 returns no bytes, whatever the count. It
// fails with OutOfRange if the array doesn't lie within the bytes, and
// panics if size is negative.
func (b ByteBuffer) GetStructArray(position, size int) ([]byte, error) {
	if size < 0 {
		panic("tafla: a negative struct size")
	}

	return b.arrayBytes(position, uint64(size))
}

// GetVector reads the vector at position, checking that its count and the
// ends of its items lie within the bytes. Each item is checked when it's
// read. It fails with OutOfRange if the count or the ends don't lie within
// the bytes.
func (b ByteBuffer) GetVector(position int) (Vector, error) {
	count, err := b.getPosition(position)
	if err != nil {
		return Vector{}, err
	}

	// Every item has an end, so this rejects an impossible count before any
	// item is read
	width := b.layout.offsetType.Size()
	if _, err := b.byteRange(position+width, uint64(count)*uint64(width)); err != nil {
		return Vector{}, err
	}

	// Without the capacity past the bytes, which an item read in place, up
	// to their end, would otherwise give its caller
	end := len(b.bytes)
	return Vector{
		bytes:      b.bytes[position:end:end],
		position:   position,
		length:     count,
		offsetType: b.layout.offsetType,
	}, nil
}

func getArray[T Element](b ByteBuffer, position int) (ArrayView[T], error) {
	bytes, err := b.arrayBytes(position, uint64(sizeOf[T]()))
	return newArrayView[T](bytes), err
}

// getPosition reads a length or offset at position, which an int holds
// on a 64-bit host. On a 32-bit host, a value past the largest int lies past
// any bytes, and fails with OutOfRange.
func (b ByteBuffer) getPosition(position int) (int, error) {
	value, err := b.getSlot(position)
	if !intIs64 && uint64(value) > math.MaxInt {
		return 0, outOfRange(position, uint64(value), len(b.bytes))
	}

	return int(value), err
}

// getSlot reads a length, offset or header slot at position, as wide as the
// offset size, after one check of its range
func (b ByteBuffer) getSlot(position int) (uint32, error) {
	size := b.layout.offsetType.Size()
	if position < 0 || position > len(b.bytes)-size {
		return 0, outOfRange(position, uint64(size), len(b.bytes))
	}

	slot := b.bytes[position:]
	switch size {
	case 1:
		return uint32(slot[0]), nil
	case 2:
		return uint32(binary.LittleEndian.Uint16(slot)), nil
	default:
		return binary.LittleEndian.Uint32(slot), nil
	}
}

// arrayBytes returns the bytes of the elements, each of elementSize bytes,
// after the count at position
func (b ByteBuffer) arrayBytes(position int, elementSize uint64) ([]byte, error) {
	count, err := b.getSlot(position)
	if err != nil {
		return nil, err
	}

	// The count lies within the bytes, so the start doesn't overflow. A
	// struct can be as large as an int, and a byte length past a uint64 lies
	// past any bytes, so the product saturates.
	start := position + b.layout.offsetType.Size()
	byteLength := uint64(count) * elementSize
	if elementSize != 0 && byteLength/elementSize != uint64(count) {
		byteLength = math.MaxUint64
	}

	return b.byteRange(start, byteLength)
}

// byteRange returns the byteLength bytes at position
func (b ByteBuffer) byteRange(position int, byteLength uint64) ([]byte, error) {
	available := len(b.bytes)
	if position < 0 || position > available || byteLength > uint64(available-position) {
		return nil, outOfRange(position, byteLength, available)
	}

	end := position + int(byteLength)
	return b.bytes[position:end:end], nil
}
