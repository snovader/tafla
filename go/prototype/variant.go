//go:build !tafla_prototype_stress

package prototype

// The choices that the evaluation compares. Each variant that it times is a
// copy of this package with other values here, which variants.sh generates,
// so that the compiler folds each choice into the code that it times.
const (
	// inlineBytes is the room for the table's bytes within the builder, which
	// a table that fits needs nothing else for until Bytes.
	inlineBytes = 128
	// firstHeapBytes is the least capacity of the bytes once they outgrow
	// inlineBytes.
	firstHeapBytes = 512
	// growByAppend grows bytes already on the heap with append's own policy,
	// through slices.Grow, rather than doubling them with make and copy.
	growByAppend = false
	// segmentMin is the shortest run that the builder keeps by reference, as
	// a segment, rather than copying it when it's set: an array's elements, a
	// string, an array of structs, or a table nested as bytes.
	segmentMin = 256
	// bytesWithoutCopy makes Bytes return the builder's own bytes on the heap,
	// rather than a copy of the table's exact length, when it kept no segment.
	bytesWithoutCopy = false
)
