package tafla

import (
	"encoding/binary"
	"unsafe"
)

// Element is a number type that tables store in arrays: int8, uint8, int16,
// uint16, int32, uint32, int64, uint64, float32 or float64. Elements are
// stored little-endian and aligned to their size from the table's start.
type Element interface {
	int8 | uint8 | int16 | uint16 | int32 | uint32 | int64 | uint64 | float32 | float64
}

// sizeOf returns an element's size, which is also its alignment in arrays. It's
// a constant in each instantiation, since every element type has a shape of
// its own.
func sizeOf[T Element]() int {
	var zero T
	return int(unsafe.Sizeof(zero))
}

// load reads the element at the start of bytes, little-endian, which hold at
// least its size. The integer of its bits is reinterpreted as the element,
// which only the case of its size does.
func load[T Element](bytes []byte) T {
	var zero T
	switch unsafe.Sizeof(zero) {
	case 1:
		bits := bytes[0]
		return *(*T)(unsafe.Pointer(&bits))
	case 2:
		bits := binary.LittleEndian.Uint16(bytes)
		return *(*T)(unsafe.Pointer(&bits))
	case 4:
		bits := binary.LittleEndian.Uint32(bytes)
		return *(*T)(unsafe.Pointer(&bits))
	default:
		bits := binary.LittleEndian.Uint64(bytes)
		return *(*T)(unsafe.Pointer(&bits))
	}
}

// store writes value at the start of bytes, little-endian, which hold at
// least its size
func store[T Element](bytes []byte, value T) {
	switch unsafe.Sizeof(value) {
	case 1:
		bytes[0] = *(*uint8)(unsafe.Pointer(&value))
	case 2:
		binary.LittleEndian.PutUint16(bytes, *(*uint16)(unsafe.Pointer(&value)))
	case 4:
		binary.LittleEndian.PutUint32(bytes, *(*uint32)(unsafe.Pointer(&value)))
	default:
		binary.LittleEndian.PutUint64(bytes, *(*uint64)(unsafe.Pointer(&value)))
	}
}

// elementsInPlace returns little-endian bytes as elements in place, which is
// possible on a little-endian host when the bytes are aligned to the
// element's alignment in memory, as TableBuilder aligns them to their size
// within an aligned table. That's their size but on 32-bit hosts, which align
// 64-bit elements to 4. Single bytes have no order and are always aligned.
// The bytes hold a whole number of elements.
func elementsInPlace[T Element](bytes []byte) ([]T, bool) {
	var zero T
	size := sizeOf[T]()
	if !hostLittleEndian && size > 1 {
		return nil, false
	}

	pointer := unsafe.Pointer(unsafe.SliceData(bytes))
	if uintptr(pointer)%unsafe.Alignof(zero) != 0 {
		return nil, false
	}

	return unsafe.Slice((*T)(pointer), len(bytes)/size), true
}

// elementBytes returns the bytes of values in place, in the host's byte
// order
func elementBytes[T Element](values []T) []byte {
	pointer := (*byte)(unsafe.Pointer(unsafe.SliceData(values)))
	return unsafe.Slice(pointer, len(values)*sizeOf[T]())
}

// putElements writes values into bytes, little-endian, which are exactly
// their length. On a little-endian host, that copies their bytes in bulk.
func putElements[T Element](bytes []byte, values []T) {
	size := sizeOf[T]()
	if hostLittleEndian || size == 1 {
		copy(bytes, elementBytes(values))
		return
	}

	for index, value := range values {
		store(bytes[index*size:], value)
	}
}

// appendElements appends the elements of little-endian bytes, which are a
// whole number of them, to values. On a little-endian host, that copies
// their bytes in bulk, at any alignment.
func appendElements[T Element](values []T, bytes []byte) []T {
	size := sizeOf[T]()
	count := len(bytes) / size
	start := len(values)
	values = append(values, make([]T, count)...)

	if hostLittleEndian || size == 1 {
		copy(elementBytes(values[start:]), bytes)
		return values
	}

	elements := values[start:]
	for index := range elements {
		elements[index] = load[T](bytes[index*size:])
	}

	return values
}
