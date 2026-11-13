package workloads

// The generator makes the TypeScript comparison's values, which these were
// printed from, so that the languages' benchmarks time the same work

import (
	"math"
	"testing"
)

func TestGeneratesTheTypeScriptDocumentOf100Rows(t *testing.T) {
	document := NewDocument(100)

	check(t, document.Keys[0], 14_727_226)
	check(t, document.Keys[99], 10_335_322)
	check(t, sum(document.Keys), 825_833_448)
	check(t, math.Float32bits(document.A[0]), 0xc221_9b29)
	check(t, math.Float32bits(document.B[99]), 0xc163_647e)
	check(t, hashColumns(document), 1_493_961_445)
}

func TestGeneratesTheTypeScriptDocumentOf100000Rows(t *testing.T) {
	document := NewDocument(100_000)

	check(t, document.Keys[0], 13_320_945)
	check(t, document.Keys[99_999], 11_621_954)
	check(t, sum(document.Keys), 839_050_642_924)
	check(t, math.Float32bits(document.A[0]), 0xc1d4_d7f6)
	check(t, math.Float32bits(document.B[99_999]), 0x3e8c_91f5)
	check(t, hashColumns(document), 1_138_446_770)
}

func TestGeneratesTheTypeScriptVectorOfNestedTables(t *testing.T) {
	items := NewList(ListItems).Items

	check(t, len(items), 100)
	check(t, bits(items[0]), [3]uint32{0x41d2_acb6, 0xbfce_db15, 0x41e3_2e23})
	check(t, bits(items[1]), [3]uint32{0x41c1_2b3a, 0x3ea9_43c1, 0x422e_ae51})
	check(t, bits(items[99]), [3]uint32{0x4193_d915, 0x3f97_9e05, 0x418f_26e1})
}

func check[T comparable](t *testing.T, got, expected T) {
	t.Helper()

	if got != expected {
		t.Errorf("%v rather than %v", got, expected)
	}
}

func sum(keys []uint32) uint64 {
	total := uint64(0)
	for _, key := range keys {
		total += uint64(key)
	}

	return total
}

// hashColumns hashes the float columns' bits, row by row, as the TypeScript
// script that printed these did with 32-bit integers
func hashColumns(document Document) uint32 {
	hash := uint32(0)
	for row := range document.A {
		hash = hash*31 + math.Float32bits(document.A[row])
		hash = hash*31 + math.Float32bits(document.B[row])
	}

	return hash
}

func bits(item Item) [3]uint32 {
	return [3]uint32{math.Float32bits(item.Value), math.Float32bits(item.First), math.Float32bits(item.Second)}
}
