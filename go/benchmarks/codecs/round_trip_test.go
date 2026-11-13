package codecs_test

// Every codec reads back exactly what it wrote, with floats compared by
// their bits, and sums column a to the same bits, which the benchmarks check
// again before timing them

import (
	"math"
	"testing"

	"github.com/snovader/tafla/go/benchmarks/codecs"
	"github.com/snovader/tafla/go/benchmarks/workloads"
)

func TestEveryCodecReadsBackWhatItWrote(t *testing.T) {
	documents := make([]workloads.Document, len(workloads.DocumentRows))
	for index, rows := range workloads.DocumentRows {
		documents[index] = workloads.NewDocument(rows)
	}
	list := workloads.NewList(workloads.ListItems)

	for _, codec := range codecs.All() {
		t.Run(codec.Name(), func(t *testing.T) {
			if codec, ok := codec.(codecs.RecordCodec); ok {
				check(t, "record", workloads.TheRecord, codec.SerializeRecord, codec.DeserializeRecord, workloads.Record.Same)
			}

			if codec, ok := codec.(codecs.DocumentCodec); ok {
				for _, document := range documents {
					check(t, "document", document, codec.SerializeDocument, codec.DeserializeDocument, workloads.Document.Same)

					bytes, err := codec.SerializeDocument(document)
					if err != nil {
						t.Fatal(err)
					}
					sum, err := codec.SumA(bytes)
					if expected := workloads.SumColumn(document.A); err != nil || math.Float64bits(sum) != math.Float64bits(expected) {
						t.Errorf("summed column a to %v, %v rather than %v", sum, err, expected)
					}
				}
			}

			if codec, ok := codec.(codecs.ListCodec); ok {
				check(t, "vector of nested tables", list, codec.SerializeList, codec.DeserializeList, workloads.List.Same)
			}

			if codec, ok := codec.(codecs.StructsCodec); ok {
				check(t, "array of structs", list, codec.SerializeStructs, codec.DeserializeStructs, workloads.List.Same)
			}

			if codec, ok := codec.(codecs.SampleCodec); ok {
				check(t, "fixed-size record", workloads.TheSample, codec.SerializeSample, codec.DeserializeSample, workloads.Sample.Same)
			}
		})
	}
}

func check[T any](
	t *testing.T,
	name string,
	value T,
	serialize func(T) ([]byte, error),
	deserialize func([]byte) (T, error),
	same func(T, T) bool,
) {
	t.Helper()

	bytes, err := serialize(value)
	if err != nil {
		t.Fatalf("%s: %v", name, err)
	}

	read, err := deserialize(bytes)
	if err != nil || !same(read, value) {
		t.Errorf("%s: read back %+v, %v", name, read, err)
	}
}
