package codecs_test

// Every codec reads back exactly what it wrote, with floats compared by
// their bits, and sums column a to the same bits, which the benchmarks check
// again before timing them, and writes no more than the shape's capacity

import (
	"bytes"
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
				check(t, "record", workloads.TheRecord, workloads.TheRecord.Capacity(), codec.SerializeRecord, codec.DeserializeRecord, workloads.Record.Same)
			}

			if codec, ok := codec.(codecs.DocumentCodec); ok {
				for _, document := range documents {
					check(t, "document", document, document.Capacity(), codec.SerializeDocument, codec.DeserializeDocument, workloads.Document.Same)

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

			if codec, ok := codec.(codecs.DocumentIntoCodec); ok {
				for _, document := range documents {
					checkInto(t, codec, document)
				}
			}

			if codec, ok := codec.(codecs.ListCodec); ok {
				check(t, "vector of nested tables", list, list.Capacity(), codec.SerializeList, codec.DeserializeList, workloads.List.Same)
			}

			if codec, ok := codec.(codecs.StructsCodec); ok {
				check(t, "array of structs", list, list.StructsCapacity(), codec.SerializeStructs, codec.DeserializeStructs, workloads.List.Same)
			}

			if codec, ok := codec.(codecs.SampleCodec); ok {
				check(t, "fixed-size record", workloads.TheSample, workloads.TheSample.Capacity(), codec.SerializeSample, codec.DeserializeSample, workloads.Sample.Same)
			}
		})
	}
}

// checkInto checks that a codec writes the document into the start of the
// bytes it's given, into their capacity when it has room and into new bytes
// otherwise, as it writes it into new bytes, and reads it back
func checkInto(t *testing.T, codec codecs.DocumentIntoCodec, document workloads.Document) {
	t.Helper()

	written, err := codec.SerializeDocumentInto(document, nil)
	if err != nil {
		t.Fatal(err)
	}

	for _, room := range []int{len(written) - 1, len(written), len(written) + 8} {
		reused := make([]byte, 5, room)
		into, err := codec.SerializeDocumentInto(document, reused)
		if err != nil || !bytes.Equal(into, written) {
			t.Fatalf("wrote % x, %v into bytes of %d", into[:min(len(into), 16)], err, room)
		}
		if inPlace := &into[0] == &reused[0]; inPlace != (room >= len(written)) {
			t.Errorf("wrote into the bytes of %d: %v, for %d", room, inPlace, len(written))
		}
	}

	if reader, ok := codec.(codecs.DocumentCodec); ok {
		check(t, "document written into bytes", document, document.Capacity(), func(document workloads.Document) ([]byte, error) {
			return codec.SerializeDocumentInto(document, make([]byte, 0, len(written)))
		}, reader.DeserializeDocument, workloads.Document.Same)
	}
}

// check checks that a codec reads back what it wrote, which is no longer
// than the value's capacity, which the codecs whose API takes one are given
func check[T any](
	t *testing.T,
	name string,
	value T,
	capacity int,
	serialize func(T) ([]byte, error),
	deserialize func([]byte) (T, error),
	same func(T, T) bool,
) {
	t.Helper()

	bytes, err := serialize(value)
	if err != nil {
		t.Fatalf("%s: %v", name, err)
	}
	if len(bytes) > capacity {
		t.Errorf("%s: wrote %d bytes, past its capacity of %d", name, len(bytes), capacity)
	}

	read, err := deserialize(bytes)
	if err != nil || !same(read, value) {
		t.Errorf("%s: read back %+v, %v", name, read, err)
	}
}
