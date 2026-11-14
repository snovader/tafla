//go:build prototype_variants

// Command prototype times tafla's TableBuilder against the variants of the
// prototype of a builder that writes each field when it's set, on the
// comparison's builder rows, interleaved with the comparison's harness. Each
// builder runs twice, as an A/A pair. variants.sh generates the variants,
// which the build tag prototype_variants builds this command with:
//
//	sh variants.sh && go run -tags prototype_variants . [-rounds n] [-warmup n] [-sample duration]
package main

import (
	"bytes"
	"errors"
	"flag"
	"fmt"
	"os"
	"os/exec"
	"runtime"
	"time"

	tafla "github.com/snovader/tafla/go"

	"github.com/snovader/tafla/go/benchmarks/cmd/prototype/variants/appendgrowth"
	"github.com/snovader/tafla/go/benchmarks/cmd/prototype/variants/copying"
	"github.com/snovader/tafla/go/benchmarks/cmd/prototype/variants/copyingappend"
	"github.com/snovader/tafla/go/benchmarks/cmd/prototype/variants/direct"
	"github.com/snovader/tafla/go/benchmarks/cmd/prototype/variants/nocopy"
	"github.com/snovader/tafla/go/benchmarks/cmd/prototype/variants/noinline128"
	"github.com/snovader/tafla/go/benchmarks/cmd/prototype/variants/noinline128nocopy"
	"github.com/snovader/tafla/go/benchmarks/cmd/prototype/variants/noinline512"
	"github.com/snovader/tafla/go/benchmarks/codecs"
	"github.com/snovader/tafla/go/benchmarks/harness"
	"github.com/snovader/tafla/go/benchmarks/workloads"
)

// The results of the timed calls, which the loops store so that the
// compiler can't leave the calls out
var (
	sinkBytes []byte
	sinkErr   error
)

// builder is a builder's serializers of the rows
type builder struct {
	name      string
	record    func(workloads.Record) ([]byte, error)
	document  func(workloads.Document) ([]byte, error)
	list      func(workloads.List) ([]byte, error)
	structs   func(workloads.List) ([]byte, error)
	builders  func(workloads.List) ([]byte, error)
	reference bool
}

func builders() []builder {
	tafla := codecs.Tafla{}
	builders := []builder{
		{"TableBuilder", tafla.SerializeRecord, tafla.SerializeDocument, tafla.SerializeList, tafla.SerializeStructs, taflaListOfBuilders, true},
		{"TableBuilder (A/A)", tafla.SerializeRecord, tafla.SerializeDocument, tafla.SerializeList, tafla.SerializeStructs, taflaListOfBuilders, false},
		{"direct", direct.SerializeRecord, direct.SerializeDocument, direct.SerializeList, direct.SerializeStructs, direct.SerializeListOfBuilders, false},
		{"direct (A/A)", direct.SerializeRecord, direct.SerializeDocument, direct.SerializeList, direct.SerializeStructs, direct.SerializeListOfBuilders, false},
		{"append's growth", appendgrowth.SerializeRecord, appendgrowth.SerializeDocument, appendgrowth.SerializeList, appendgrowth.SerializeStructs, appendgrowth.SerializeListOfBuilders, false},
		{"no inline, heap from 128", noinline128.SerializeRecord, noinline128.SerializeDocument, noinline128.SerializeList, noinline128.SerializeStructs, noinline128.SerializeListOfBuilders, false},
		{"no inline, heap from 512", noinline512.SerializeRecord, noinline512.SerializeDocument, noinline512.SerializeList, noinline512.SerializeStructs, noinline512.SerializeListOfBuilders, false},
		{"Bytes without copy", nocopy.SerializeRecord, nocopy.SerializeDocument, nocopy.SerializeList, nocopy.SerializeStructs, nocopy.SerializeListOfBuilders, false},
		{"no inline, 128, without copy", noinline128nocopy.SerializeRecord, noinline128nocopy.SerializeDocument, noinline128nocopy.SerializeList, noinline128nocopy.SerializeStructs, noinline128nocopy.SerializeListOfBuilders, false},
		{"copying", copying.SerializeRecord, copying.SerializeDocument, copying.SerializeList, copying.SerializeStructs, copying.SerializeListOfBuilders, false},
		{"copying, append's growth", copyingappend.SerializeRecord, copyingappend.SerializeDocument, copyingappend.SerializeList, copyingappend.SerializeStructs, copyingappend.SerializeListOfBuilders, false},
	}

	return builders
}

// row is a row of the report: a workload's serialization by every builder
type row struct {
	name     string
	variants []harness.Variant
}

func main() {
	rounds := flag.Int("rounds", 30, "the timed rounds of each row")
	warmup := flag.Int("warmup", 3, "the warm-up rounds of each row")
	sample := flag.Duration("sample", 20*time.Millisecond, "the time of a round of a variant")
	flag.Parse()
	options := harness.Options{Rounds: *rounds, WarmupRounds: *warmup, SampleTime: *sample}

	rows, err := rowsOf(builders())
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}

	fmt.Printf("%s on %s/%s, %d CPUs\n", runtime.Version(), runtime.GOOS, runtime.GOARCH, runtime.NumCPU())
	fmt.Printf("Start: %s", uptime())
	started := time.Now()

	results := make([][]harness.Measurement, len(rows))
	for index, row := range rows {
		results[index] = harness.Measure(row.variants, options)
		fmt.Fprintf(os.Stderr, "%s done\n", row.name)
	}

	fmt.Printf("End: %s", uptime())
	fmt.Printf("Took %s, %d rounds of %s after %d warm-up rounds\n\n",
		time.Since(started).Round(time.Second), options.Rounds, options.SampleTime, options.WarmupRounds)
	for index, row := range rows {
		report(row.name, results[index])
	}

	runtime.KeepAlive(sinkBytes)
	runtime.KeepAlive(sinkErr)
}

// rowsOf returns the rows, after checking that every builder writes every
// row's bytes as TableBuilder does
func rowsOf(builders []builder) ([]row, error) {
	record := workloads.TheRecord
	small := workloads.NewDocument(workloads.DocumentRows[0])
	large := workloads.NewDocument(workloads.DocumentRows[1])
	list := workloads.NewList(workloads.ListItems)

	rows := []row{
		{name: "record: serialize"},
		{name: "document of 100 rows: serialize"},
		{name: "document of 100,000 rows: serialize"},
		{name: "vector of 100 nested tables: serialize"},
		{name: "array of 100 structs: serialize"},
		{name: "vector of 100 nested builders: serialize"},
	}
	for _, builder := range builders {
		calls := []func() ([]byte, error){
			func() ([]byte, error) { return builder.record(record) },
			func() ([]byte, error) { return builder.document(small) },
			func() ([]byte, error) { return builder.document(large) },
			func() ([]byte, error) { return builder.list(list) },
			func() ([]byte, error) { return builder.structs(list) },
			func() ([]byte, error) { return builder.builders(list) },
		}
		loops := []func(n int){
			func(n int) {
				for range n {
					sinkBytes, sinkErr = builder.record(record)
				}
			},
			func(n int) {
				for range n {
					sinkBytes, sinkErr = builder.document(small)
				}
			},
			func(n int) {
				for range n {
					sinkBytes, sinkErr = builder.document(large)
				}
			},
			func(n int) {
				for range n {
					sinkBytes, sinkErr = builder.list(list)
				}
			},
			func(n int) {
				for range n {
					sinkBytes, sinkErr = builder.structs(list)
				}
			},
			func(n int) {
				for range n {
					sinkBytes, sinkErr = builder.builders(list)
				}
			},
		}

		for index := range rows {
			if err := checkSame(builders[0], builder, rows[index].name, calls[index]); err != nil {
				return nil, err
			}
			rows[index].variants = append(rows[index].variants, harness.Variant{Name: builder.name, Loop: loops[index]})
		}
	}

	return rows, nil
}

// expected holds TableBuilder's bytes of each row, which the first builder
// writes
var expected = map[string][]byte{}

func checkSame(reference, builder builder, name string, call func() ([]byte, error)) error {
	written, err := call()
	if err != nil {
		return fmt.Errorf("%s, %s: %w", builder.name, name, err)
	}
	if builder.reference {
		expected[name] = written
		return nil
	}
	if !bytes.Equal(written, expected[name]) {
		return fmt.Errorf("%s, %s: wrote other bytes than %s", builder.name, name, reference.name)
	}

	return nil
}

// report prints a row's variants: the fastest call, its ratio to the
// first's, the median call, and what a call allocates
func report(name string, measurements []harness.Measurement) {
	fmt.Printf("%s\n", name)
	fmt.Printf("  %-30s %12s %8s %12s %12s %8s\n", "builder", "fastest ns", "ratio", "median ns", "bytes", "allocs")
	reference := measurements[0].Fastest
	for _, m := range measurements {
		fmt.Printf("  %-30s %12.2f %7.3fx %12.2f %12.0f %8.2f\n",
			m.Name, m.Fastest, m.Fastest/reference, m.Median, m.Bytes, m.Allocations)
	}
	fmt.Println()
}

func uptime() string {
	output, err := exec.Command("uptime").Output()
	if err != nil {
		return err.Error() + "\n"
	}

	return string(output)
}

// taflaListOfBuilders writes the vector of nested tables with TableBuilder
// from a builder of each item and of its pair, as the variants'
// SerializeListOfBuilders does
func taflaListOfBuilders(list workloads.List) ([]byte, error) {
	items := make([]tafla.NestedTable, len(list.Items))
	for index, item := range list.Items {
		pair, err := tafla.NewTableBuilder(2, tafla.TableLayout{})
		if err != nil {
			return nil, err
		}
		table, err := tafla.NewTableBuilder(2, tafla.TableLayout{})
		if err != nil {
			return nil, err
		}
		err = errors.Join(
			pair.SetFloat32(item.First),
			pair.SetFloat32(item.Second),
			table.SetFloat32(item.Value),
			table.SetTable(pair),
		)
		if err != nil {
			return nil, err
		}
		items[index] = table
	}

	builder, err := tafla.NewTableBuilder(1, tafla.TableLayout{})
	if err != nil {
		return nil, err
	}
	if err := builder.SetVector(items...); err != nil {
		return nil, err
	}

	return builder.Bytes()
}
