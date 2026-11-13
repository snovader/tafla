package codecs

import (
	"encoding/binary"
	"errors"
	"math"

	"github.com/snovader/tafla/go/benchmarks/workloads"
)

// Binary writes the fixed-size record with encoding/binary's little-endian
// functions at fixed offsets, and reads it with them, as tafla's README
// recommends for records without a header or offsets.
type Binary struct{}

const sampleSize = 12

func (Binary) Name() string { return "encoding/binary" }

func (Binary) SerializeSample(sample workloads.Sample) ([]byte, error) {
	bytes := make([]byte, sampleSize)
	binary.LittleEndian.PutUint32(bytes[0:], sample.Key)
	binary.LittleEndian.PutUint32(bytes[4:], math.Float32bits(sample.A))
	binary.LittleEndian.PutUint32(bytes[8:], math.Float32bits(sample.B))

	return bytes, nil
}

var errShortSample = errors.New("the bytes are shorter than a sample")

func (Binary) DeserializeSample(bytes []byte) (workloads.Sample, error) {
	if len(bytes) < sampleSize {
		return workloads.Sample{}, errShortSample
	}

	return workloads.Sample{
		Key: binary.LittleEndian.Uint32(bytes[0:]),
		A:   math.Float32frombits(binary.LittleEndian.Uint32(bytes[4:])),
		B:   math.Float32frombits(binary.LittleEndian.Uint32(bytes[8:])),
	}, nil
}
