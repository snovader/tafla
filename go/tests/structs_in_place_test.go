package tafla_test

// StructsInPlace reads an array of structs as Go structs in place, for the
// code that tafla-gen generates, where the bytes are aligned to the struct in
// memory on a little-endian host.

import (
	"encoding/binary"
	"math"
	"testing"
	"unsafe"

	tafla "github.com/snovader/tafla/go"
	"github.com/snovader/tafla/go/tests/internal/tables"
	"pgregory.net/rapid"
)

// mixed is a struct whose fields lie in memory as its scalars lie in its 16
// bytes, at 0, 8, 12, 14 and 15, on every platform: aligned to 8 on 64-bit
// ones and to 4 on 32-bit ones
type mixed struct {
	A float64
	B int32
	C uint16
	D int8
	E uint8
}

const mixedSize = 16

// readMixed reads a mixed from its little-endian bytes, as generated code
// reads structs
func readMixed(bytes []byte) mixed {
	return mixed{
		A: math.Float64frombits(binary.LittleEndian.Uint64(bytes)),
		B: int32(binary.LittleEndian.Uint32(bytes[8:])),
		C: binary.LittleEndian.Uint16(bytes[12:]),
		D: int8(bytes[14]),
		E: bytes[15],
	}
}

// sameMixed compares mixeds by the bits of their float, so that NaN and -0
// compare as their bytes do
func sameMixed(a, b mixed) bool {
	return math.Float64bits(a.A) == math.Float64bits(b.A) && a.B == b.B && a.C == b.C && a.D == b.D && a.E == b.E
}

// bytePair is a struct of single bytes, which have no byte order and are
// aligned to 1
type bytePair struct {
	First  int8
	Second uint8
}

func TestStructsInPlaceReadsTheStructsOfAlignedBytes(t *testing.T) {
	rapid.Check(t, func(t *rapid.T) {
		count := rapid.IntRange(0, 6).Draw(t, "count")
		structBytes := make([]byte, count*mixedSize)
		for index := range structBytes {
			structBytes[index] = drawByte(t)
		}
		byteOffset := rapid.IntRange(0, 15).Draw(t, "byteOffset")
		bytes := tables.AtByteOffset(structBytes, byteOffset)

		structs, ok := tafla.StructsInPlace[mixed](bytes)
		inPlace := count == 0 || readsInPlace && byteOffset%int(unsafe.Alignof(mixed{})) == 0
		if ok != inPlace {
			t.Fatalf("in place: %v", ok)
		}
		if !ok {
			if structs != nil {
				t.Fatalf("read %v out of place", structs)
			}
			return
		}

		if len(structs) != count {
			t.Fatalf("read %d structs", len(structs))
		}
		for index, structure := range structs {
			if expected := readMixed(bytes[index*mixedSize:]); !sameMixed(structure, expected) {
				t.Fatalf("read %+v rather than %+v at %d", structure, expected, index)
			}
		}
		if count > 0 && unsafe.Pointer(&structs[0]) != unsafe.Pointer(&bytes[0]) {
			t.Fatal("the structs are a copy")
		}
	})
}

func TestStructsOfSingleBytesAreInPlaceOnEveryHost(t *testing.T) {
	rapid.Check(t, func(t *rapid.T) {
		bytes := tables.AtByteOffset(drawBytes(t, 12, "bytes"), rapid.IntRange(0, 7).Draw(t, "byteOffset"))
		bytes = bytes[:len(bytes)/2*2]

		pairs, ok := tafla.StructsInPlace[bytePair](bytes)
		if !ok || len(pairs) != len(bytes)/2 {
			t.Fatalf("read %v, %v", pairs, ok)
		}
		for index, pair := range pairs {
			if pair.First != int8(bytes[2*index]) || pair.Second != bytes[2*index+1] {
				t.Fatalf("read %+v at %d", pair, index)
			}
		}
	})
}

func TestStructsInPlaceShareTheirBytes(t *testing.T) {
	bytes := tables.AtByteOffset(make([]byte, 2*mixedSize), 0)
	structs, ok := tafla.StructsInPlace[mixed](bytes)
	if !ok {
		if readsInPlace {
			t.Fatal("aligned structs aren't in place")
		}
		return
	}

	binary.LittleEndian.PutUint32(bytes[mixedSize+8:], math.MaxUint32)
	if structs[1].B != -1 {
		t.Errorf("read %d after the bytes changed", structs[1].B)
	}
}
