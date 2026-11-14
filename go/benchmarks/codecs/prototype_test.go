package codecs_test

// The prototype of a TableBuilder that writes each field when it's set
// writes the comparison's workloads byte for byte as TableBuilder does

import (
	"bytes"
	"testing"

	"github.com/snovader/tafla/go/benchmarks/codecs"
	"github.com/snovader/tafla/go/benchmarks/workloads"
)

func TestThePrototypeWritesTheWorkloadsAsTableBuilderDoes(t *testing.T) {
	builder, direct := codecs.Tafla{}, codecs.TaflaPrototype{}
	list := workloads.NewList(workloads.ListItems)

	same(t, "record", workloads.TheRecord, builder.SerializeRecord, direct.SerializeRecord)
	for _, rows := range workloads.DocumentRows {
		same(t, "document", workloads.NewDocument(rows), builder.SerializeDocument, direct.SerializeDocument)
	}
	same(t, "vector of nested tables", list, builder.SerializeList, direct.SerializeList)
	same(t, "array of structs", list, builder.SerializeStructs, direct.SerializeStructs)
	same(t, "empty vector", workloads.List{}, builder.SerializeList, direct.SerializeList)
}

func same[T any](t *testing.T, name string, value T, expected, actual func(T) ([]byte, error)) {
	t.Helper()

	want, err := expected(value)
	if err != nil {
		t.Fatalf("%s: %v", name, err)
	}
	got, err := actual(value)
	if err != nil || !bytes.Equal(got, want) {
		t.Errorf("%s: wrote %d bytes, %v, rather than %d", name, len(got), err, len(want))
	}
}
