package tafla

import (
	"encoding/binary"
	"iter"
	"math"
)

// Vector is a vector read from a table: its items, which are tables.
//
// The vector holds its count, the end of each item, counted from the
// vector's position, and then the items. [ByteBuffer.GetVector] checks that
// the ends lie within the bytes, and each item is checked when it's read, so
// reading one can fail. [Vector.Get] and [Vector.GetInPlace] find any item
// from its end and the one before it, without reading the others.
//
// An item that has no header and its parent's offset type is read in place:
// from the bytes that [Vector.GetInPlace] returns, which start where it does
// and end where its parent's do, with a [ByteBuffer] of its parent's offset
// type, at position 0. Its reads may reach past its end, as those of any
// table read in place may. Any other item is read as a table of its own,
// from the bytes that [Vector.Get] returns, with a ByteBuffer of its own
// layout.
//
// The zero Vector is empty.
type Vector struct {
	// The bytes from the vector's position, which is its count's, to the end
	// of the buffer
	bytes []byte
	// The vector's position in the buffer, which errors report positions
	// from
	position   int
	length     int
	offsetType OffsetType
}

// Len returns the number of items.
func (v Vector) Len() int {
	return v.length
}

// Get returns the bytes of item index, from its start to its end, to read it
// as a table of its own. Only the ends of the item and the one before it are
// read. It fails with ItemEndsBeforeStart if the item ends before the item
// before it, or with OutOfRange if it ends past the bytes, and panics if
// index is out of range, as indexing a slice does.
func (v Vector) Get(index int) ([]byte, error) {
	start, end := v.bounds(index)
	return v.item(start, end)
}

// GetInPlace returns the bytes of item index to read it in place, from its
// start to the end of the buffer. Only the ends of the item and the one
// before it are read. It fails as [Vector.Get] does.
func (v Vector) GetInPlace(index int) ([]byte, error) {
	start, end := v.bounds(index)
	return v.itemInPlace(start, end)
}

// Items returns an iterator over the items' bytes, as [Vector.Get] returns
// them, which reads each item's end once. An item that fails doesn't keep
// the others from being read.
func (v Vector) Items() iter.Seq2[[]byte, error] {
	return func(yield func([]byte, error) bool) {
		start := v.itemsStart()
		for index := range v.length {
			end := v.end(index)
			if !yield(v.item(start, end)) {
				return
			}
			start = end
		}
	}
}

// ItemsInPlace returns an iterator over the items' bytes, as
// [Vector.GetInPlace] returns them, which reads each item's end once. An item
// that fails doesn't keep the others from being read.
func (v Vector) ItemsInPlace() iter.Seq2[[]byte, error] {
	return func(yield func([]byte, error) bool) {
		start := v.itemsStart()
		for index := range v.length {
			end := v.end(index)
			if !yield(v.itemInPlace(start, end)) {
				return
			}
			start = end
		}
	}
}

// bounds returns where item index starts and ends, from the vector's
// position
func (v Vector) bounds(index int) (start, end uint64) {
	if uint(index) >= uint(v.length) {
		panic(indexOutOfRange{index: index, length: v.length, of: "a vector"})
	}

	if index == 0 {
		start = v.itemsStart()
	} else {
		start = v.end(index - 1)
	}

	return start, v.end(index)
}

// itemsStart returns where item 0 starts, from the vector's position: after
// the count and the ends, which lie within the bytes
func (v Vector) itemsStart() uint64 {
	return uint64(v.length+1) * uint64(v.offsetType.Size())
}

// end reads the end of item index, from the vector's position. The
// vector's reader checked that the ends lie within the bytes. Ends are
// uint64s, as starts are, so that an end past any int, which only a 32-bit
// int can't hold, fails as any end past the bytes does.
func (v Vector) end(index int) uint64 {
	slot := (index + 1) * v.offsetType.Size()

	switch v.offsetType {
	case Uint8:
		return uint64(v.bytes[slot])
	case Uint16:
		return uint64(binary.LittleEndian.Uint16(v.bytes[slot:]))
	default:
		return uint64(binary.LittleEndian.Uint32(v.bytes[slot:]))
	}
}

// item returns the bytes of the item from start to end, from the vector's
// position
func (v Vector) item(start, end uint64) ([]byte, error) {
	if start > end {
		return nil, v.endsBeforeStart(start, end)
	}

	if end > uint64(len(v.bytes)) {
		return nil, v.outside(start, end)
	}

	return v.bytes[start:end:end], nil
}

// itemInPlace returns the bytes from the start of the item from start to
// end, from the vector's position, to the end of the buffer
func (v Vector) itemInPlace(start, end uint64) ([]byte, error) {
	if start > end {
		return nil, v.endsBeforeStart(start, end)
	}

	if end > uint64(len(v.bytes)) {
		return nil, v.outside(start, end)
	}

	return v.bytes[start:], nil
}

func (v Vector) endsBeforeStart(start, end uint64) error {
	return &DecodeError{Kind: ItemEndsBeforeStart, Position: v.positionOf(start), End: v.positionOf(end)}
}

// outside returns the error of an item from start to end that ends past the
// bytes
func (v Vector) outside(start, end uint64) error {
	return outOfRange(v.positionOf(start), end-start, v.available())
}

// positionOf returns the position in the buffer of offset from the vector's
// position, which an error reports. It saturates, since only a 32-bit int
// can overflow, at a position past any bytes.
func (v Vector) positionOf(offset uint64) int {
	return int(min(uint64(v.position)+offset, math.MaxInt))
}

// available returns the length of the buffer that the vector lies in
func (v Vector) available() int {
	return v.position + len(v.bytes)
}
