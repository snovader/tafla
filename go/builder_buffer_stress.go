//go:build tafla_builder_stress

package tafla

// The sizes of a TableBuilder's buffer and of the runs that it keeps, with
// the build tag tafla_builder_stress: it keeps every run, and grows its
// buffer from a byte, so that its tests run those paths on small tables too.
const (
	inlineBytes    = 0
	firstHeapBytes = 1
	runMin         = 1
	itemRoom       = 64 << 10
)
