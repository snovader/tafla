package workloads

// The capacities that a codec whose API takes one is given to write a value
// into, as Rust's comparison gives them: an estimate from what a caller
// knows before writing, the value's counts of rows or items and its
// strings' lengths, at as many bytes for each as the most that any of the
// comparison's formats writes for it, after CapacityOverhead bytes for
// headers, offsets and names. The codecs' round-trip test checks that none
// writes more.

// CapacityOverhead is the bytes of a capacity beyond those of a value's
// counts and strings.
const CapacityOverhead = 128

const (
	// The bytes of a capacity for each row of a document: three values of up
	// to 5 bytes each in msgpack
	rowCapacity = 16
	// The bytes of a capacity for each item of the vector of nested tables,
	// whose field names msgpack writes for each
	itemCapacity = 48
	// The bytes of a capacity for each struct of the array of structs, three
	// float32s, which msgpack writes as an array of 16 bytes
	structCapacity = 16
)

// Capacity returns the record's capacity.
func (r Record) Capacity() int {
	return CapacityOverhead + len(r.Name)
}

// Capacity returns the document's capacity.
func (d Document) Capacity() int {
	return CapacityOverhead + len(d.Name) + len(d.Keys)*rowCapacity
}

// Capacity returns the capacity of the list as a vector of nested tables.
func (l List) Capacity() int {
	return CapacityOverhead + len(l.Items)*itemCapacity
}

// StructsCapacity returns the capacity of the list as an array of structs.
func (l List) StructsCapacity() int {
	return CapacityOverhead + len(l.Items)*structCapacity
}

// Capacity returns the fixed-size record's capacity.
func (Sample) Capacity() int {
	return CapacityOverhead
}
