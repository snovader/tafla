package tafla

// Optional is a value that may be absent, as an optional field is in the
// struct of fields that the code that tafla-gen generates writes a table
// from, which writes an absent one with [TableWriter.SetAbsent]. The zero
// Optional is absent, and [Some] returns one that holds a value, so a struct
// of fields leaves out every optional field that it doesn't set.
//
// It holds its value, rather than a pointer to it, so that a string or a
// scalar needs no variable of its own to point at, and a slice can be
// present and empty. Copying an Optional copies its value.
type Optional[T any] struct {
	value   T
	present bool
}

// Some returns an Optional that holds value.
func Some[T any](value T) Optional[T] {
	return Optional[T]{value: value, present: true}
}

// Get returns the value and true, or the zero value and false when the
// Optional is absent.
func (o Optional[T]) Get() (T, bool) {
	return o.value, o.present
}
