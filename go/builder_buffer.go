//go:build !tafla_builder_stress

package tafla

// The sizes of a TableBuilder's buffer and of the runs that it keeps rather
// than copying them. With the build tag tafla_builder_stress, it keeps every
// run, and grows its buffer from a byte, so that its tests run those paths
// on small tables too.
const (
	// The bytes within the builder, which hold a table of a few scalars and a
	// short string without an allocation but the table's own
	inlineBytes = 128
	// The first buffer on the heap, which is at least twice as long
	firstHeapBytes = 512
	// The shortest run kept rather than copied: an array, a string, an array
	// of structs or a table nested as bytes
	runMin = 256
)
