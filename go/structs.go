package tafla

import "unsafe"

// StructsInPlace returns bytes, which [ByteBuffer.GetStructArray] read, as
// structs in place, and true, or nil and false when they aren't aligned to S
// in memory or the host is big-endian. An empty array, and structs of single
// bytes on any host, are always in place. The slice shares the bytes, so it
// changes when they do, and writing to it writes to them.
//
// It's for the code that tafla-gen generates, which calls it only for a
// struct S of numbers whose fields lie in memory, on every platform, as its
// scalars lie in its bytes, so that its size is theirs, and only with bytes
// of a whole number of structs. Any other S reads memory as a type that it
// doesn't hold.
func StructsInPlace[S any](bytes []byte) ([]S, bool) {
	var zero S
	if len(bytes) == 0 {
		return []S{}, true
	}
	if !hostLittleEndian && unsafe.Alignof(zero) > 1 {
		return nil, false
	}

	pointer := unsafe.Pointer(unsafe.SliceData(bytes))
	if uintptr(pointer)%unsafe.Alignof(zero) != 0 {
		return nil, false
	}

	return unsafe.Slice((*S)(pointer), uintptr(len(bytes))/unsafe.Sizeof(zero)), true
}
