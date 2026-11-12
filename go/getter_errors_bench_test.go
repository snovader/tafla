package tafla_test

// The cost of a getter's error, which decides how the getters report
// failure. The runtime's getters return their value and an error. These
// read the same four float64s in three ways: with the runtime's GetFloat64,
// which returns (float64, error), and with copies of it that return
// (float64, bool), or record the first failure in a reader and return the
// value alone, as bufio.Scanner does. The getters inline into the loop, as
// they do into the code that tafla-gen generates.
//
//	go test -run '^$' -bench GetterErrors -count 10 .

import (
	"encoding/binary"
	"math"
	"testing"

	tafla "github.com/snovader/tafla/go"
)

var (
	sinkSum   float64
	sinkError error
	sinkOK    bool
)

// getterErrorTable returns 32 bytes of four float64s
func getterErrorTable() []byte {
	bytes := make([]byte, 0, 32)
	for index := range 4 {
		bytes = binary.LittleEndian.AppendUint64(bytes, math.Float64bits(float64(index)+0.5))
	}

	return bytes
}

func BenchmarkGetterErrors(b *testing.B) {
	bytes := getterErrorTable()

	b.Run("error", func(b *testing.B) {
		buffer := tafla.NewByteBuffer(bytes, tafla.TableLayout{})
		for range b.N {
			sum := 0.0
			var err error
			for position := 0; position < 32; position += 8 {
				value, readErr := buffer.GetFloat64(position)
				if readErr != nil {
					err = readErr
					break
				}
				sum += value
			}
			sinkSum, sinkError = sum, err
		}
	})

	b.Run("bool", func(b *testing.B) {
		for range b.N {
			sum := 0.0
			ok := true
			for position := 0; position < 32; position += 8 {
				value, read := getFloat64OK(bytes, position)
				if !read {
					ok = false
					break
				}
				sum += value
			}
			sinkSum, sinkOK = sum, ok
		}
	})

	b.Run("sticky", func(b *testing.B) {
		for range b.N {
			reader := stickyReader{bytes: bytes}
			sum := 0.0
			for position := 0; position < 32; position += 8 {
				sum += reader.getFloat64(position)
			}
			sinkSum, sinkError = sum, reader.err
		}
	})
}

// getFloat64OK reads as GetFloat64 does, reporting failure as false
func getFloat64OK(bytes []byte, position int) (float64, bool) {
	if position < 0 || position > len(bytes)-8 {
		return 0, false
	}

	return math.Float64frombits(binary.LittleEndian.Uint64(bytes[position:])), true
}

// stickyReader reads as GetFloat64 does, recording the first failure, after
// which every read returns 0
type stickyReader struct {
	bytes []byte
	err   error
}

func (r *stickyReader) getFloat64(position int) float64 {
	if r.err != nil {
		return 0
	}
	if position < 0 || position > len(r.bytes)-8 {
		r.err = &tafla.DecodeError{Kind: tafla.OutOfRange, Position: position, ByteLength: 8, Available: len(r.bytes)}
		return 0
	}

	return math.Float64frombits(binary.LittleEndian.Uint64(r.bytes[position:]))
}
