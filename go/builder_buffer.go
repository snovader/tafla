//go:build !tafla_builder_stress

package tafla

// The sizes of a TableBuilder's buffer and of the runs that it keeps rather
// than copying them. With the build tag tafla_builder_stress, it keeps every
// run, and grows its buffer from a byte, so that its tests run those paths
// on small tables too.
//
// A smaller builder, of 64 bytes within it and room for 2 runs rather than 4,
// wrote the benchmarks' vector of 100 nested builders in 13% less time, but
// their document of 100 rows in 35% more, in 4 allocations rather than 2.
const (
	// The bytes within the builder, which hold a table of a few scalars and a
	// short string without an allocation but the table's own
	inlineBytes = 128
	// The first buffer on the heap, which is at least twice as long
	firstHeapBytes = 512
	// The shortest run kept rather than copied: an array, a string, an array
	// of structs or a table nested as bytes
	runMin = 256
	// The most room that a vector's first item makes for the others
	itemRoom = 64 << 10
)
