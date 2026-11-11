package tafla

import "iter"

// ArrayView is an array read from a table: its elements' little-endian
// bytes, which it reads in place at any alignment and in any host byte
// order. Reading it never allocates, except for [ArrayView.Append] and for
// [ArrayView.Slice] when it copies.
//
// [ArrayView.AsSlice] returns the elements as a slice of the bytes when the
// host is little-endian and the bytes are aligned to the element type in
// memory, as [TableBuilder] aligns them within an aligned table. Such a
// slice shares the table's bytes, so it changes when they do, and writing to
// it writes to them.
//
// The zero ArrayView is empty.
type ArrayView[T Element] struct {
	// A whole number of elements
	bytes []byte
}

// newArrayView views bytes, which must hold a whole number of elements
func newArrayView[T Element](bytes []byte) ArrayView[T] {
	return ArrayView[T]{bytes: bytes}
}

// Len returns the number of elements.
func (a ArrayView[T]) Len() int {
	return len(a.bytes) / sizeOf[T]()
}

// At returns the element at index. It panics if index is out of range, as
// indexing a slice does.
func (a ArrayView[T]) At(index int) T {
	size := sizeOf[T]()
	if uint(index) >= uint(len(a.bytes)/size) {
		panic(indexOutOfRange{index: index, length: len(a.bytes) / size, of: "an array"})
	}

	start := index * size
	return load[T](a.bytes[start : start+size])
}

// AsSlice returns the elements in place, and true, or nil and false when they
// aren't aligned to their type in memory or the host is big-endian. Single
// bytes, and an empty array, are always in place. The slice shares the
// table's bytes.
func (a ArrayView[T]) AsSlice() ([]T, bool) {
	return elementsInPlace[T](a.bytes)
}

// Slice returns the elements in place when [ArrayView.AsSlice] can, and
// otherwise a copy, in the host's byte order.
func (a ArrayView[T]) Slice() []T {
	if values, ok := a.AsSlice(); ok {
		return values
	}

	return a.Append(nil)
}

// Append appends a copy of the elements to values, in the host's byte order,
// and returns the extended slice.
func (a ArrayView[T]) Append(values []T) []T {
	return appendElements(values, a.bytes)
}

// All returns an iterator over the indices and elements, which reads each
// element from the bytes in turn.
func (a ArrayView[T]) All() iter.Seq2[int, T] {
	return func(yield func(int, T) bool) {
		size := sizeOf[T]()
		bytes := a.bytes
		for index := 0; len(bytes) >= size; index++ {
			if !yield(index, load[T](bytes)) {
				return
			}
			bytes = bytes[size:]
		}
	}
}

// Values returns an iterator over the elements, which reads each element
// from the bytes in turn.
func (a ArrayView[T]) Values() iter.Seq[T] {
	return func(yield func(T) bool) {
		size := sizeOf[T]()
		bytes := a.bytes
		for len(bytes) >= size {
			if !yield(load[T](bytes)) {
				return
			}
			bytes = bytes[size:]
		}
	}
}

// Bytes returns the elements' little-endian bytes, without the count before
// them. They're the table's bytes.
func (a ArrayView[T]) Bytes() []byte {
	return a.bytes
}
