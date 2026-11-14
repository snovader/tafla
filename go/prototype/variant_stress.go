//go:build tafla_prototype_stress

package prototype

// The choices of variant.go that the tests' build tag tafla_prototype_stress
// makes, so that every run is a segment, and the buffer starts on the heap
// at a byte and grows from there.
const (
	inlineBytes      = 0
	firstHeapBytes   = 1
	growByAppend     = false
	segmentMin       = 1
	bytesWithoutCopy = false
)
