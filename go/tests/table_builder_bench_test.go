package tafla_test

// Benchmarks of TableBuilder on shapes that the comparison suite doesn't
// time: builders nested at depth, and tables of many short values, which a
// builder copies into its buffer. They compare versions of the runtime, with
// benchstat, as those in generated/ do:
//
//	go test -run '^$' -bench TableBuilder -count 10 . > new.txt

import (
	"errors"
	"fmt"
	"strings"
	"testing"

	tafla "github.com/snovader/tafla/go"
)

var (
	sinkTable []byte
	sinkError error
)

// A chain of builders, each of a string of 100 bytes and the builder before
// it
func BenchmarkTableBuilderChain(b *testing.B) {
	value := strings.Repeat("a", 100)
	for _, depth := range []int{10, 200, 2_000} {
		b.Run(fmt.Sprintf("depth=%d", depth), func(b *testing.B) {
			b.ReportAllocs()
			for range b.N {
				inner, _ := tafla.NewTableBuilder(1, tafla.TableLayout{})
				_ = inner.SetString(value)
				for range depth {
					outer, _ := tafla.NewTableBuilder(2, tafla.TableLayout{})
					sinkError = errors.Join(outer.SetString(value), outer.SetTable(inner))
					inner = outer
				}
				sinkTable, sinkError = inner.Bytes()
			}
		})
	}
}

// A tree of builders, of which each holds a vector of fanout builders, and
// the leaves a uint32, a float64 and a string of 20 bytes
func BenchmarkTableBuilderTree(b *testing.B) {
	value := strings.Repeat("b", 20)
	for _, shape := range []struct{ depth, fanout int }{{4, 10}, {12, 2}} {
		b.Run(fmt.Sprintf("depth=%d/fanout=%d", shape.depth, shape.fanout), func(b *testing.B) {
			b.ReportAllocs()
			for range b.N {
				sinkTable, sinkError = treeOf(shape.depth, shape.fanout, value).Bytes()
			}
		})
	}
}

func treeOf(depth, fanout int, value string) *tafla.TableBuilder {
	if depth == 0 {
		leaf, _ := tafla.NewTableBuilder(3, tafla.TableLayout{})
		sinkError = errors.Join(leaf.SetUint32(1), leaf.SetFloat64(2), leaf.SetString(value))
		return leaf
	}

	items := make([]tafla.NestedTable, fanout)
	for index := range items {
		items[index] = treeOf(depth-1, fanout, value)
	}
	node, _ := tafla.NewTableBuilder(2, tafla.TableLayout{})
	sinkError = errors.Join(node.SetUint32(uint32(depth)), node.SetVector(items...))

	return node
}

// A vector of 5,000 builders, each of a uint32 and a string of 300 bytes
func BenchmarkTableBuilderVectorOfBuilders(b *testing.B) {
	value := strings.Repeat("c", 300)
	b.ReportAllocs()
	for range b.N {
		items := make([]tafla.NestedTable, 5_000)
		for index := range items {
			item, _ := tafla.NewTableBuilder(2, tafla.TableLayout{})
			sinkError = errors.Join(item.SetUint32(1), item.SetString(value))
			items[index] = item
		}
		builder, _ := tafla.NewTableBuilder(1, tafla.TableLayout{})
		sinkError = builder.SetVector(items...)
		sinkTable, sinkError = builder.Bytes()
	}
}

// Tables of string fields, of lengths about those that the builder copies
// rather than keeping
func BenchmarkTableBuilderStrings(b *testing.B) {
	shapes := []struct{ fields, length int }{
		{20_000, 32}, {20_000, 63}, {20_000, 64}, {20_000, 100}, {20_000, 255}, {20_000, 256},
		{50, 100}, {50, 200},
	}
	for _, shape := range shapes {
		value := strings.Repeat("d", shape.length)
		b.Run(fmt.Sprintf("fields=%d/length=%d", shape.fields, shape.length), func(b *testing.B) {
			b.ReportAllocs()
			for range b.N {
				builder, _ := tafla.NewTableBuilder(shape.fields, tafla.TableLayout{})
				for range shape.fields {
					sinkError = builder.SetString(value)
				}
				sinkTable, sinkError = builder.Bytes()
			}
		})
	}
}

// A table of 50 arrays of 25 uint32s
func BenchmarkTableBuilderArrays(b *testing.B) {
	elements := make([]uint32, 25)
	b.ReportAllocs()
	for range b.N {
		builder, _ := tafla.NewTableBuilder(50, tafla.TableLayout{})
		for range 50 {
			sinkError = builder.SetUint32Array(elements)
		}
		sinkTable, sinkError = builder.Bytes()
	}
}

// Vectors of 100,000 items written in place: of a string each, and of a
// uint32, a float64 and a string of 20 bytes
func BenchmarkTableBuilderVectorFunc(b *testing.B) {
	for _, length := range []int{60, 255} {
		value := strings.Repeat("e", length)
		b.Run(fmt.Sprintf("strings/length=%d", length), func(b *testing.B) {
			b.ReportAllocs()
			for range b.N {
				builder, _ := tafla.NewTableBuilder(1, tafla.TableLayout{})
				sinkError = builder.SetVectorFunc(100_000, 1, func(_ int, item tafla.Item) error {
					return item.SetString(value)
				})
				sinkTable, sinkError = builder.Bytes()
			}
		})
	}

	value := strings.Repeat("f", 20)
	b.Run("scalars and strings", func(b *testing.B) {
		b.ReportAllocs()
		for range b.N {
			builder, _ := tafla.NewTableBuilder(1, tafla.TableLayout{})
			sinkError = builder.SetVectorFunc(100_000, 3, func(index int, item tafla.Item) error {
				return errors.Join(item.SetUint32(uint32(index)), item.SetFloat64(1), item.SetString(value))
			})
			sinkTable, sinkError = builder.Bytes()
		}
	})
}
