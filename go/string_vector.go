package tafla

import (
	"unicode/utf8"
	"unsafe"
)

// StringVector is a vector of strings read from a table.
//
// The vector holds its count, the end of each string, counted from the
// vector's position, and then the strings' UTF-8, one after another, as a
// [Vector] holds its items. [TableReader.GetStringVector] checks that the
// ends lie within the bytes, and each string is checked when it's read, so
// reading one can fail. [StringVector.Get] finds any string from its end and
// the one before it, without reading the others, and [StringVector.Strings]
// reads every string.
//
// Each string must be valid UTF-8 on its own, from its own ends, so an end
// that splits a character fails both strings around it. The strings share
// the table's bytes, as [TableReader.GetString] returns a string, so the
// bytes must not change while they're in use.
//
// The zero StringVector is empty.
type StringVector struct {
	vector Vector
}

// Len returns the number of strings.
func (v StringVector) Len() int {
	return v.vector.length
}

// Get reads string index, from its end and the end of the string before it,
// without reading the others. It fails with ItemEndsBeforeStart if the
// string ends before the string before it, with OutOfRange if it ends past
// the bytes, or with InvalidUTF8 if it isn't valid UTF-8, and panics if index
// is out of range, as indexing a slice does.
func (v StringVector) Get(index int) (string, error) {
	start, end := v.vector.bounds(index)
	bytes, err := v.vector.item(start, end)
	if err != nil {
		return "", err
	}

	if !utf8.Valid(bytes) {
		return "", &DecodeError{Kind: InvalidUTF8, Position: v.vector.positionOf(start)}
	}

	return unsafe.String(unsafe.SliceData(bytes), len(bytes)), nil
}

// Strings reads every string, from the first to the last, into a new slice.
// It returns the strings that [StringVector.Get] returns, and fails where
// any of them fails, with the error of the first that does.
//
// The strings lie one after another, so they're checked as one run of UTF-8,
// from the end of the ends to the last string's end, and each string is
// checked to start at a character, which its first byte does unless it's a
// continuation byte. That holds every string valid on its own, as Get checks
// it: an end that splits a character starts the next string within it, or
// leaves the run without the rest of it. On an Apple M2 Pro with Go 1.27, it
// read lists of 10 to 10,000 strings of 16 and 256 characters of ASCII in
// 0.26–0.51 times the time that Get took to read each string, and strings of
// 8 characters of 3 bytes in 0.79–0.81 times: `go test -bench
// StringVectorStrings`.
func (v StringVector) Strings() ([]string, error) {
	vector := v.vector
	strings := make([]string, vector.length)
	if len(strings) == 0 {
		return strings, nil
	}

	bytes := vector.bytes
	first := vector.itemsStart()
	start := first
	for index := range strings {
		end := vector.end(index)
		if end < start || end > uint64(len(bytes)) || (start < end && !utf8.RuneStart(bytes[start])) {
			return v.eachString(strings)
		}

		item := bytes[start:end]
		strings[index] = unsafe.String(unsafe.SliceData(item), len(item))
		start = end
	}

	if !utf8.Valid(bytes[first:start]) {
		return v.eachString(strings)
	}

	return strings, nil
}

// eachString reads every string into strings, each as [StringVector.Get]
// reads it, for a vector whose strings don't all read as one run, and
// returns the error of the first that fails
func (v StringVector) eachString(strings []string) ([]string, error) {
	for index := range strings {
		string, err := v.Get(index)
		if err != nil {
			return nil, err
		}
		strings[index] = string
	}

	return strings, nil
}
