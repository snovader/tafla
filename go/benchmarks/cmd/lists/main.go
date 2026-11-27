// Command lists times how tafla-gen's code writes and reads lists of 10, 100
// and 10,000 strings, as a vector of strings and as a vector of tables of
// one string each, interleaved, and prints each benchmark's fastest call,
// its ratio to the vector of strings', the median call, what a call
// allocates and each list's encoded size. README.md describes the suite.
//
//	go run ./cmd/lists [-run regexp] [-rounds n] [-warmup n] [-sample duration] [-quick]
package main

import (
	"flag"
	"fmt"
	"os"
	"regexp"
	"runtime"
	"time"

	"github.com/snovader/tafla/go/benchmarks/harness"
	"github.com/snovader/tafla/go/benchmarks/stringlists"
)

// The results of the timed calls, which the loops store so that the
// compiler can't leave the calls out
var (
	sinkBytes   []byte
	sinkStrings []string
	sinkString  string
	sinkErr     error
)

// benchmark is a row of the report: an operation on a list, and each way's
// variant of it
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

	codecs := stringlists.Codecs()
	if err := check(codecs); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}

	fmt.Printf("%s on %s/%s, %d CPUs\n", runtime.Version(), runtime.GOOS, runtime.GOARCH, runtime.NumCPU())
	fmt.Printf("Start: %s", harness.Uptime())
	started := time.Now()

	var results []harness.Result
	for _, shape := range stringlists.Shapes {
		for _, benchmark := range benchmarks(codecs, shape) {
			if !selected.MatchString(benchmark.name) {
				continue
			}

			measurements := harness.Measure(benchmark.variants, options)
			results = append(results, harness.Result{Name: benchmark.name, Measurements: measurements})
			fmt.Fprintf(os.Stderr, "%s: %s\n", benchmark.name, harness.Summary(measurements))
		}
	}

	fmt.Printf("End: %s", harness.Uptime())
	fmt.Printf("Took %s, %d rounds of %s after %d warm-up rounds\n\n",
		time.Since(started).Round(time.Second), options.Rounds, options.SampleTime, options.WarmupRounds)
	harness.Report(results)
	printSizes(codecs)

	runtime.KeepAlive(sinkBytes)
	runtime.KeepAlive(sinkStrings)
	runtime.KeepAlive(sinkString)
	runtime.KeepAlive(sinkErr)
}

// check checks that every way reads back exactly what it wrote of every
// list, before any is timed
func check(codecs []stringlists.Codec) error {
	for _, codec := range codecs {
		for _, shape := range stringlists.Shapes {
			if err := stringlists.Check(codec, shape); err != nil {
				return err
			}
		}
	}

	return nil
}

// benchmarks returns a list's benchmarks: writing it, reading every string
// and reading the last string, each in every way
func benchmarks(codecs []stringlists.Codec, shape stringlists.Shape) []benchmark {
	serialize := benchmark{name: shape.Name + ": serialize"}
	deserialize := benchmark{name: shape.Name + ": deserialize"}
	readLast := benchmark{name: shape.Name + ": read the last string"}

	strings := shape.Strings
	for _, codec := range codecs {
		bytes, _ := codec.Serialize(strings)

		serialize.variants = append(serialize.variants, harness.Variant{Name: codec.Name(), Loop: func(n int) {
			for range n {
				sinkBytes, sinkErr = codec.Serialize(strings)
			}
		}})
		deserialize.variants = append(deserialize.variants, harness.Variant{Name: codec.Name(), Loop: func(n int) {
			for range n {
				sinkStrings, sinkErr = codec.Deserialize(bytes)
			}
		}})
		readLast.variants = append(readLast.variants, harness.Variant{Name: codec.Name(), Loop: func(n int) {
			for range n {
				sinkString, sinkErr = codec.ReadLast(bytes)
			}
		}})
	}

	return []benchmark{serialize, deserialize, readLast}
}

// printSizes prints the bytes of each list in each way, as a table of
// Markdown
func printSizes(codecs []stringlists.Codec) {
	fmt.Println()
	fmt.Println("Encoded size in bytes:")
	fmt.Println()
	fmt.Print("| List |")
	for _, codec := range codecs {
		fmt.Printf(" %s |", codec.Name())
	}
	fmt.Println()
	fmt.Print("| --- |")
	for range codecs {
		fmt.Print(" ---: |")
	}
	fmt.Println()

	for _, shape := range stringlists.Shapes {
		fmt.Printf("| %s |", shape.Name)
		for _, codec := range codecs {
			bytes, _ := codec.Serialize(shape.Strings)
			fmt.Printf(" %s |", harness.Thousands(len(bytes)))
		}
		fmt.Println()
	}
}
