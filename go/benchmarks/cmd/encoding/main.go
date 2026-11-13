// Command encoding prints each codec's encoded size of every workload, and
// what it does with a truncated document of 100,000 rows: an error, or a
// panic, which it recovers from.
//
//	go run ./cmd/encoding
package main

import (
	"fmt"
	"strings"

	"github.com/snovader/tafla/go/benchmarks/codecs"
	"github.com/snovader/tafla/go/benchmarks/workloads"
)

func main() {
	record := workloads.TheRecord
	small := workloads.NewDocument(workloads.DocumentRows[0])
	large := workloads.NewDocument(workloads.DocumentRows[1])
	list := workloads.NewList(workloads.ListItems)
	sample := workloads.TheSample

	fmt.Println("Encoded size (bytes)")
	fmt.Printf("  %-16s%12s%12s%12s%12s%12s%12s\n", "", "record", "doc 100", "doc 100000", "vector", "structs", "fixed")
	for _, codec := range codecs.All() {
		sizes := make([]string, 6)
		if codec, ok := codec.(codecs.RecordCodec); ok {
			sizes[0] = size(codec.SerializeRecord(record))
		}
		if codec, ok := codec.(codecs.DocumentCodec); ok {
			sizes[1] = size(codec.SerializeDocument(small))
			sizes[2] = size(codec.SerializeDocument(large))
		}
		if codec, ok := codec.(codecs.ListCodec); ok {
			sizes[3] = size(codec.SerializeList(list))
		}
		if codec, ok := codec.(codecs.StructsCodec); ok {
			sizes[4] = size(codec.SerializeStructs(list))
		}
		if codec, ok := codec.(codecs.SampleCodec); ok {
			sizes[5] = size(codec.SerializeSample(sample))
		}

		fmt.Printf("  %-16s%12s%12s%12s%12s%12s%12s\n", codec.Name(), sizes[0], sizes[1], sizes[2], sizes[3], sizes[4], sizes[5])
	}

	fmt.Println()
	fmt.Println("Truncated document of 100000 rows: deserialize()")
	for _, codec := range codecs.All() {
		codec, ok := codec.(codecs.DocumentCodec)
		if !ok {
			continue
		}

		bytes, err := codec.SerializeDocument(large)
		if err != nil {
			panic(err)
		}
		for _, length := range []int{len(bytes) / 2, 24} {
			fmt.Printf("  %-16s%8d B  %s\n", codec.Name(), length, outcome(codec, bytes[:length:length]))
		}
	}
}

func size(bytes []byte, err error) string {
	if err != nil {
		return "error"
	}

	return fmt.Sprint(len(bytes))
}

// outcome reads truncated bytes, and describes what happened, recovering
// from a panic
func outcome(codec codecs.DocumentCodec, bytes []byte) (description string) {
	defer func() {
		if recovered := recover(); recovered != nil {
			description = shortened(fmt.Sprintf("panicked: %v", recovered))
		}
	}()

	_, err := codec.DeserializeDocument(bytes)
	if err == nil {
		return "returned a document"
	}

	return shortened("returned an error: " + err.Error())
}

func shortened(text string) string {
	text = strings.ReplaceAll(text, "\n", " ")
	if len(text) > 96 {
		return text[:96]
	}

	return text
}
