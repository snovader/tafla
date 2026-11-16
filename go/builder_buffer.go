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
	// of structs or a table nested as bytes. Keeping only runs of 256 bytes or
	// more wrote a table of 50 strings of 100 bytes in 57% more time, and one
	// of 20,000 strings of 255 bytes in 81% more. Keeping runs of 32 bytes or
	// more wrote one of 20,000 strings of 32 bytes in 51% more, and keeping
	// those of 64 to 100 bytes takes 6-8% longer than copying them would.
	runMin = 64
	// The most room that a vector's first item makes for the others
	itemRoom = 64 << 10
	// The fewest bytes, counting each run as the bytes of its record, that
	// copying a nested builder's table into its parent would copy, from which
	// the parent keeps a copy of the builder instead. Each level of a chain of
	// builders copies those below it until one is kept, so that nesting copies
	// fewer than these bytes per builder. Keeping them from 256 bytes, a chain
	// of 200 builders, each of a string of 100 bytes, took 17% longer, and from
	// 1 KiB 2.2 times as long. Keeping them from 64 bytes, a document of 100
	// rows whose columns came from a builder took 17% longer, in 2 allocations
	// rather than 1.
	tableRunMin = 128
	// The runs that the heap first holds once the builder's own are full.
	// Growing them from one wrote a table of 50 strings of 100 bytes in 13%
	// more time.
	moreRuns = 16
)
