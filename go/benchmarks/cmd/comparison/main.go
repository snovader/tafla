// Command comparison times tafla's codecs and the libraries that it's
// compared with on every workload, interleaved, and prints each benchmark's
// fastest call, its ratio to tafla's, the median call and what a call
// allocates. README.md describes the method.
//
//	go run ./cmd/comparison [-run regexp] [-rounds n] [-warmup n] [-sample duration] [-quick]
package main

import (
	"bytes"
	"flag"
	"fmt"
	"os"
	"os/exec"
	"regexp"
	"runtime"
	"strings"
	"time"

	"github.com/snovader/tafla/go/benchmarks/codecs"
	"github.com/snovader/tafla/go/benchmarks/harness"
	"github.com/snovader/tafla/go/benchmarks/itemaccess"
	"github.com/snovader/tafla/go/benchmarks/workloads"
)

// The results of the timed calls, which the loops store so that the
// compiler can't leave the calls out
var (
	sinkBytes    []byte
	sinkErr      error
	sinkRecord   workloads.Record
	sinkDocument workloads.Document
	sinkList     workloads.List
	sinkSample   workloads.Sample
	sinkFloat    float64
)

// benchmark is a row of the report: a workload's operation, and each
// library's variant of it
type benchmark struct {
	name     string
	variants []harness.Variant
}

func main() {
	run := flag.String("run", "", "a regular expression of the benchmarks to run")
	rounds := flag.Int("rounds", 30, "the timed rounds of each benchmark")
	warmup := flag.Int("warmup", 3, "the warm-up rounds of each benchmark")
	sample := flag.Duration("sample", 20*time.Millisecond, "the time of a round of a variant")
	quick := flag.Bool("quick", false, "runs every benchmark briefly, to check that it works")
	flag.Parse()

	options := harness.Options{Rounds: *rounds, WarmupRounds: *warmup, SampleTime: *sample}
	if *quick {
		options = harness.Options{Rounds: 2, WarmupRounds: 0, SampleTime: time.Millisecond}
	}
	selected := regexp.MustCompile(*run)

	benchmarks, err := benchmarks()
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}

	fmt.Printf("%s on %s/%s, %d CPUs\n", runtime.Version(), runtime.GOOS, runtime.GOARCH, runtime.NumCPU())
	fmt.Printf("Start: %s", uptime())
	started := time.Now()

	var results []result
	for _, benchmark := range benchmarks {
		if !selected.MatchString(benchmark.name) {
			continue
		}

		measurements := harness.Measure(benchmark.variants, options)
		results = append(results, result{benchmark.name, measurements})
		fmt.Fprintf(os.Stderr, "%s: %s\n", benchmark.name, summary(measurements))
	}

	fmt.Printf("End: %s", uptime())
	fmt.Printf("Took %s, %d rounds of %s after %d warm-up rounds\n\n",
		time.Since(started).Round(time.Second), options.Rounds, options.SampleTime, options.WarmupRounds)
	report(results)

	keepResults()
}

// keepResults keeps the last results of the timed calls alive to the end
func keepResults() {
	runtime.KeepAlive(sinkBytes)
	runtime.KeepAlive(sinkErr)
	runtime.KeepAlive(sinkRecord)
	runtime.KeepAlive(sinkDocument)
	runtime.KeepAlive(sinkList)
	runtime.KeepAlive(sinkSample)
	runtime.KeepAlive(sinkFloat)
}

type result struct {
	name         string
	measurements []harness.Measurement
}

// benchmarks returns every benchmark, after checking that each codec reads
// back exactly what it wrote, and sums column a to the same bits
func benchmarks() ([]benchmark, error) {
	record := workloads.TheRecord
	list := workloads.NewList(workloads.ListItems)
	sample := workloads.TheSample

	var all []benchmark
	all = append(all, recordBenchmarks(record)...)
	for _, rows := range workloads.DocumentRows {
		all = append(all, documentBenchmarks(workloads.NewDocument(rows))...)
	}
	all = append(all, listBenchmarks(list)...)
	all = append(all, structsBenchmarks(list)...)
	all = append(all, sampleBenchmarks(sample)...)

	items, err := itemBenchmarks(list)
	if err != nil {
		return nil, err
	}
	all = append(all, items...)

	return all, check(record, list, sample)
}

func recordBenchmarks(record workloads.Record) []benchmark {
	serialize := benchmark{name: "record: serialize"}
	deserialize := benchmark{name: "record: deserialize"}

	for _, codec := range codecs.All() {
		codec, ok := codec.(codecs.RecordCodec)
		if !ok {
			continue
		}
		bytes, _ := codec.SerializeRecord(record)

		serialize.variants = append(serialize.variants, harness.Variant{Name: codec.Name(), Loop: func(n int) {
			for range n {
				sinkBytes, sinkErr = codec.SerializeRecord(record)
			}
		}})
		deserialize.variants = append(deserialize.variants, harness.Variant{Name: codec.Name(), Loop: func(n int) {
			for range n {
				sinkRecord, sinkErr = codec.DeserializeRecord(bytes)
			}
		}})
	}

	return []benchmark{serialize, deserialize}
}

func documentBenchmarks(document workloads.Document) []benchmark {
	rows := fmt.Sprintf("document of %s rows", thousands(len(document.A)))
	serialize := benchmark{name: rows + ": serialize"}
	deserialize := benchmark{name: rows + ": deserialize"}
	sum := benchmark{name: rows + ": sum a column per element"}

	for _, codec := range codecs.All() {
		codec, ok := codec.(codecs.DocumentCodec)
		if !ok {
			continue
		}
		bytes, _ := codec.SerializeDocument(document)

		serialize.variants = append(serialize.variants, harness.Variant{Name: codec.Name(), Loop: func(n int) {
			for range n {
				sinkBytes, sinkErr = codec.SerializeDocument(document)
			}
		}})
		deserialize.variants = append(deserialize.variants, harness.Variant{Name: codec.Name(), Loop: func(n int) {
			for range n {
				sinkDocument, sinkErr = codec.DeserializeDocument(bytes)
			}
		}})
		sum.variants = append(sum.variants, harness.Variant{Name: codec.Name(), Loop: func(n int) {
			for range n {
				sinkFloat, sinkErr = codec.SumA(bytes)
			}
		}})
	}

	benchmarks := []benchmark{serialize, deserialize, sum}
	if len(document.A) == reusedRows {
		benchmarks = append(benchmarks, serializeIntoBenchmark(rows, document))
	}

	return benchmarks
}

// reusedRows are the rows of the document that's also written into reused
// bytes, the large one, whose write new bytes slow most
const reusedRows = 100_000

// serializeIntoBenchmark writes the document into bytes that each variant
// keeps between calls, as long as the document that it writes
func serializeIntoBenchmark(rows string, document workloads.Document) benchmark {
	serialize := benchmark{name: rows + ": serialize into reused bytes"}

	for _, codec := range codecs.All() {
		codec, ok := codec.(codecs.DocumentIntoCodec)
		if !ok {
			continue
		}
		written, _ := codec.SerializeDocumentInto(document, nil)
		reused := make([]byte, 0, len(written))

		serialize.variants = append(serialize.variants, harness.Variant{Name: codec.Name(), Loop: func(n int) {
			for range n {
				sinkBytes, sinkErr = codec.SerializeDocumentInto(document, reused)
			}
		}})
	}

	return serialize
}

func listBenchmarks(list workloads.List) []benchmark {
	name := fmt.Sprintf("vector of %d nested tables", len(list.Items))
	serialize := benchmark{name: name + ": serialize"}
	deserialize := benchmark{name: name + ": deserialize"}

	for _, codec := range codecs.All() {
		codec, ok := codec.(codecs.ListCodec)
		if !ok {
			continue
		}
		bytes, _ := codec.SerializeList(list)

		serialize.variants = append(serialize.variants, harness.Variant{Name: codec.Name(), Loop: func(n int) {
			for range n {
				sinkBytes, sinkErr = codec.SerializeList(list)
			}
		}})
		deserialize.variants = append(deserialize.variants, harness.Variant{Name: codec.Name(), Loop: func(n int) {
			for range n {
				sinkList, sinkErr = codec.DeserializeList(bytes)
			}
		}})
	}

	return []benchmark{serialize, deserialize}
}

func structsBenchmarks(list workloads.List) []benchmark {
	name := fmt.Sprintf("array of %d structs", len(list.Items))
	serialize := benchmark{name: name + ": serialize"}
	deserialize := benchmark{name: name + ": deserialize"}

	for _, codec := range codecs.All() {
		codec, ok := codec.(codecs.StructsCodec)
		if !ok {
			continue
		}
		bytes, _ := codec.SerializeStructs(list)

		serialize.variants = append(serialize.variants, harness.Variant{Name: codec.Name(), Loop: func(n int) {
			for range n {
				sinkBytes, sinkErr = codec.SerializeStructs(list)
			}
		}})
		deserialize.variants = append(deserialize.variants, harness.Variant{Name: codec.Name(), Loop: func(n int) {
			for range n {
				sinkList, sinkErr = codec.DeserializeStructs(bytes)
			}
		}})
	}

	return []benchmark{serialize, deserialize}
}

func sampleBenchmarks(sample workloads.Sample) []benchmark {
	serialize := benchmark{name: "fixed-size record: serialize"}
	deserialize := benchmark{name: "fixed-size record: deserialize"}

	for _, codec := range codecs.All() {
		codec, ok := codec.(codecs.SampleCodec)
		if !ok {
			continue
		}
		bytes, _ := codec.SerializeSample(sample)

		serialize.variants = append(serialize.variants, harness.Variant{Name: codec.Name(), Loop: func(n int) {
			for range n {
				sinkBytes, sinkErr = codec.SerializeSample(sample)
			}
		}})
		deserialize.variants = append(deserialize.variants, harness.Variant{Name: codec.Name(), Loop: func(n int) {
			for range n {
				sinkSample, sinkErr = codec.DeserializeSample(bytes)
			}
		}})
	}

	return []benchmark{serialize, deserialize}
}

// itemBenchmarks are the groups that read tafla's items in each way
func itemBenchmarks(list workloads.List) ([]benchmark, error) {
	tables, err := codecs.Tafla{}.SerializeList(list)
	if err != nil {
		return nil, err
	}
	structs, err := codecs.Tafla{}.SerializeStructs(list)
	if err != nil {
		return nil, err
	}

	tablesName := fmt.Sprintf("vector of %d nested tables", len(list.Items))
	structsName := fmt.Sprintf("array of %d structs", len(list.Items))
	groups := []struct {
		name  string
		bytes []byte
		read  func(itemaccess.Way) func([]byte) (float64, error)
	}{
		{tablesName + ": read the last item", tables, func(way itemaccess.Way) func([]byte) (float64, error) { return way.LastTable }},
		{tablesName + ": sum every item", tables, func(way itemaccess.Way) func([]byte) (float64, error) { return way.EveryTable }},
		{structsName + ": read the last item", structs, func(way itemaccess.Way) func([]byte) (float64, error) { return way.LastStruct }},
		{structsName + ": sum every item", structs, func(way itemaccess.Way) func([]byte) (float64, error) { return way.EveryStruct }},
	}

	var benchmarks []benchmark
	for _, group := range groups {
		benchmark := benchmark{name: group.name}
		for _, way := range itemaccess.Ways() {
			read, bytes := group.read(way), group.bytes
			benchmark.variants = append(benchmark.variants, harness.Variant{Name: way.Name(), Loop: func(n int) {
				for range n {
					sinkFloat, sinkErr = read(bytes)
				}
			}})
		}
		benchmarks = append(benchmarks, benchmark)
	}

	return benchmarks, nil
}

// check checks that every codec reads back exactly what it wrote, with
// floats compared by their bits, and sums column a to the same bits, before
// any is timed
func check(record workloads.Record, list workloads.List, sample workloads.Sample) error {
	for _, codec := range codecs.All() {
		if codec, ok := codec.(codecs.RecordCodec); ok {
			if err := roundTrip(codec, "the record", record, codec.SerializeRecord, codec.DeserializeRecord, workloads.Record.Same); err != nil {
				return err
			}
		}
		if codec, ok := codec.(codecs.DocumentCodec); ok {
			for _, rows := range workloads.DocumentRows {
				document := workloads.NewDocument(rows)
				if err := roundTrip(codec, "a document", document, codec.SerializeDocument, codec.DeserializeDocument, workloads.Document.Same); err != nil {
					return err
				}
				bytes, _ := codec.SerializeDocument(document)
				if sum, _ := codec.SumA(bytes); sum != workloads.SumColumn(document.A) {
					return fmt.Errorf("%s sums column a to %v", codec.Name(), sum)
				}
			}
		}
		if codec, ok := codec.(codecs.DocumentIntoCodec); ok {
			if err := checkInto(codec, workloads.NewDocument(reusedRows)); err != nil {
				return err
			}
		}
		if codec, ok := codec.(codecs.ListCodec); ok {
			if err := roundTrip(codec, "the vector", list, codec.SerializeList, codec.DeserializeList, workloads.List.Same); err != nil {
				return err
			}
		}
		if codec, ok := codec.(codecs.StructsCodec); ok {
			if err := roundTrip(codec, "the structs", list, codec.SerializeStructs, codec.DeserializeStructs, workloads.List.Same); err != nil {
				return err
			}
		}
		if codec, ok := codec.(codecs.SampleCodec); ok {
			if err := roundTrip(codec, "the fixed-size record", sample, codec.SerializeSample, codec.DeserializeSample, workloads.Sample.Same); err != nil {
				return err
			}
		}
	}

	return nil
}

// checkInto checks that a codec writes the document into the start of the
// bytes that it's given when they have room, as it writes it into new
// bytes, and reads it back
func checkInto(codec codecs.DocumentIntoCodec, document workloads.Document) error {
	written, err := codec.SerializeDocumentInto(document, nil)
	if err != nil {
		return fmt.Errorf("%s can't write a document into bytes: %w", codec.Name(), err)
	}

	reused := make([]byte, 3, len(written))
	into, err := codec.SerializeDocumentInto(document, reused)
	if err != nil || !bytes.Equal(into, written) || &into[0] != &reused[0] {
		return fmt.Errorf("%s doesn't write a document into the bytes it's given: %v", codec.Name(), err)
	}

	if reader, ok := codec.(codecs.DocumentCodec); ok {
		read, err := reader.DeserializeDocument(into)
		if err != nil || !read.Same(document) {
			return fmt.Errorf("%s doesn't read back a document written into bytes: %v", codec.Name(), err)
		}
	}

	return nil
}

func roundTrip[T any](
	codec codecs.Codec,
	name string,
	value T,
	serialize func(T) ([]byte, error),
	deserialize func([]byte) (T, error),
	same func(T, T) bool,
) error {
	bytes, err := serialize(value)
	if err != nil {
		return fmt.Errorf("%s can't write %s: %w", codec.Name(), name, err)
	}
	read, err := deserialize(bytes)
	if err != nil || !same(read, value) {
		return fmt.Errorf("%s doesn't read back %s: %v", codec.Name(), name, err)
	}

	return nil
}

// report prints the fastest call of each benchmark, and its ratio to the
// first variant's, tafla's, and then the bytes and allocations of a call, as
// tables of Markdown
func report(results []result) {
	columns := columnsOf(results)

	fmt.Println("Fastest call in µs, and its ratio to tafla's:")
	fmt.Println()
	table(results, columns, func(measurement, first harness.Measurement) string {
		ratio := ""
		if measurement.Name != first.Name {
			ratio = fmt.Sprintf(" (%s×)", significant(measurement.Fastest/first.Fastest))
		}
		return micros(measurement.Fastest) + ratio
	})

	fmt.Println()
	fmt.Println("Median call's distance from the fastest:")
	fmt.Println()
	table(results, columns, func(measurement, _ harness.Measurement) string {
		return fmt.Sprintf("%+.1f%%", 100*(measurement.Median/measurement.Fastest-1))
	})

	fmt.Println()
	fmt.Println("Bytes allocated per call, and the allocations that ask for them:")
	fmt.Println()
	table(results, columns, func(measurement, _ harness.Measurement) string {
		return fmt.Sprintf("%s (%s)", thousands(int(measurement.Bytes+0.5)), trimmed(measurement.Allocations))
	})
}

// columnsOf returns the variants' names, in their first order, of the
// benchmarks of codecs and of the ways of reading items apart
func columnsOf(results []result) []string {
	var columns []string
	for _, result := range results {
		for _, measurement := range result.measurements {
			if !contains(columns, measurement.Name) {
				columns = append(columns, measurement.Name)
			}
		}
	}

	return columns
}

func table(results []result, columns []string, cell func(measurement, first harness.Measurement) string) {
	fmt.Printf("| Benchmark | %s |\n", strings.Join(columns, " | "))
	fmt.Printf("| --- |%s\n", strings.Repeat(" ---: |", len(columns)))
	for _, result := range results {
		cells := make([]string, len(columns))
		for index, column := range columns {
			for _, measurement := range result.measurements {
				if measurement.Name == column {
					cells[index] = cell(measurement, result.measurements[0])
				}
			}
		}
		fmt.Printf("| %s | %s |\n", result.name, strings.Join(cells, " | "))
	}
}

func summary(measurements []harness.Measurement) string {
	parts := make([]string, len(measurements))
	for index, measurement := range measurements {
		parts[index] = fmt.Sprintf("%s %s µs", measurement.Name, micros(measurement.Fastest))
	}

	return strings.Join(parts, ", ")
}

// micros returns a time in nanoseconds in µs, to three significant digits,
// or more integer digits
func micros(nanoseconds float64) string {
	return significant(nanoseconds / 1000)
}

func significant(value float64) string {
	switch {
	case value >= 100:
		return fmt.Sprintf("%.1f", value)
	case value >= 10:
		return fmt.Sprintf("%.2f", value)
	case value >= 1:
		return fmt.Sprintf("%.3f", value)
	case value >= 0.1:
		return fmt.Sprintf("%.3f", value)
	case value >= 0.01:
		return fmt.Sprintf("%.4f", value)
	default:
		return fmt.Sprintf("%.5f", value)
	}
}

func trimmed(value float64) string {
	return strings.TrimSuffix(strings.TrimRight(fmt.Sprintf("%.2f", value), "0"), ".")
}

func thousands(value int) string {
	digits := fmt.Sprint(value)
	var grouped strings.Builder
	for index, digit := range digits {
		if index > 0 && (len(digits)-index)%3 == 0 {
			grouped.WriteByte(',')
		}
		grouped.WriteRune(digit)
	}

	return grouped.String()
}

func contains(values []string, value string) bool {
	for _, existing := range values {
		if existing == value {
			return true
		}
	}

	return false
}

// uptime returns uptime's line, with the load averages
func uptime() string {
	output, err := exec.Command("uptime").Output()
	if err != nil {
		return err.Error() + "\n"
	}

	return string(output)
}
