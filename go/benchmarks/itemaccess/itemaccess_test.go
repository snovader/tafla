package itemaccess_test

// Every way reads the same sums from the items that tafla writes

import (
	"math"
	"testing"

	"github.com/snovader/tafla/go/benchmarks/codecs"
	"github.com/snovader/tafla/go/benchmarks/itemaccess"
	"github.com/snovader/tafla/go/benchmarks/workloads"
)

func TestEveryWayReadsTheSameSums(t *testing.T) {
	list := workloads.NewList(workloads.ListItems)
	tables, err := codecs.Tafla{}.SerializeList(list)
	if err != nil {
		t.Fatal(err)
	}
	structs, err := codecs.Tafla{}.SerializeStructs(list)
	if err != nil {
		t.Fatal(err)
	}

	last := list.Items[len(list.Items)-1]
	lastSum := float64(last.Value) + float64(last.First) + float64(last.Second)
	everySum := 0.0
	for _, item := range list.Items {
		everySum += float64(item.Value) + float64(item.First) + float64(item.Second)
	}

	for _, way := range itemaccess.Ways() {
		for _, read := range []struct {
			name     string
			read     func([]byte) (float64, error)
			bytes    []byte
			expected float64
		}{
			{"last table", way.LastTable, tables, lastSum},
			{"every table", way.EveryTable, tables, everySum},
			{"last struct", way.LastStruct, structs, lastSum},
			{"every struct", way.EveryStruct, structs, everySum},
		} {
			sum, err := read.read(read.bytes)
			if err != nil || math.Float64bits(sum) != math.Float64bits(read.expected) {
				t.Errorf("%s, %s: %v, %v rather than %v", way.Name(), read.name, sum, err, read.expected)
			}
		}
	}
}
