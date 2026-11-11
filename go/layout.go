package tafla

import (
	"math"
	"strconv"
)

// OffsetType is a table's offset type, which limits how long the table can
// be.
//
// It's the unsigned integer type of the table's header slots, field offsets,
// counts, lengths and item ends. Each constant is named after its type, as
// SPEC.md and schemas name it, and String returns that name. The zero value
// is [Uint32], the default.
type OffsetType uint8

const (
	// Uint32 is uint32, for tables of up to 4 GiB, and the zero OffsetType
	Uint32 OffsetType = iota
	// Uint16 is uint16, for tables of up to 65,535 bytes
	Uint16
	// Uint8 is uint8, for tables of up to 255 bytes
	Uint8
)

// Size returns the offset size, the type's width in bytes: 1, 2 or 4.
func (t OffsetType) Size() int {
	return 4 >> t
}

// MaxValue returns the type's largest value, which is also the longest table
// that its offsets address.
func (t OffsetType) MaxValue() uint32 {
	return uint32(uint64(1)<<(8*t.Size()) - 1)
}

// String returns the type's name, as SPEC.md and schemas spell it: "uint8",
// "uint16" or "uint32".
func (t OffsetType) String() string {
	switch t {
	case Uint8:
		return "uint8"
	case Uint16:
		return "uint16"
	case Uint32:
		return "uint32"
	default:
		return "OffsetType(" + strconv.Itoa(int(t)) + ")"
	}
}

func (t OffsetType) valid() bool {
	return t <= Uint8
}

// Alignment is a multiple of bytes from a table's start that a field starts
// at, so that its elements can be read in place: 1, 2, 4 or 8.
type Alignment uint8

const (
	Align1 Alignment = 1
	Align2 Alignment = 2
	Align4 Alignment = 4
	Align8 Alignment = 8
)

// Bytes returns the alignment in bytes.
func (a Alignment) Bytes() int {
	return int(a)
}

func (a Alignment) valid() bool {
	return a == Align1 || a == Align2 || a == Align4 || a == Align8
}

// padding returns the bytes that bring offset to a multiple of the
// alignment, a power of two. Only the offset's remainder matters, so it may
// have wrapped.
func (a Alignment) padding(offset uint64) uint64 {
	return -offset & uint64(a-1)
}

// mustBeValid panics unless the alignment is one of the constants, which
// the checked APIs take
func (a Alignment) mustBeValid() {
	if !a.valid() {
		panic("tafla: an alignment is 1, 2, 4 or 8 bytes, not " + strconv.Itoa(int(a)))
	}
}

// TableLayout is the header and offset type of a table.
//
// A format creates its layout once and passes it to both the [TableBuilder]
// that writes the table and the [ByteBuffer] that reads it, so that the two
// can't disagree. The zero TableLayout is the default: no header slots, and
// offsets of the offset type [Uint32]. Nested tables use it.
//
//	// The response's length and format version, then its field offsets
//	var layout = tafla.TableLayout{}.Sized().Versioned()
type TableLayout struct {
	offsetType OffsetType
	sized      bool
	versioned  bool
}

// Sized returns the layout with the table's total byte length at the start
// of its header.
func (l TableLayout) Sized() TableLayout {
	l.sized = true
	return l
}

// Versioned returns the layout with a slot for a version in its header,
// after the length when sized. Readers can check it before reading any
// field.
func (l TableLayout) Versioned() TableLayout {
	l.versioned = true
	return l
}

// WithOffsetType returns the layout with the offset type t, the unsigned
// integer type of the header slots, field offsets, counts, lengths and item
// ends, which limits how long the table can be. The default is [Uint32].
//
// It panics if t isn't [Uint8], [Uint16] or [Uint32].
func (l TableLayout) WithOffsetType(t OffsetType) TableLayout {
	if !t.valid() {
		panic("tafla: " + t.String() + " isn't an offset type")
	}

	l.offsetType = t
	return l
}

// IsSized reports whether the header starts with the table's byte length.
func (l TableLayout) IsSized() bool {
	return l.sized
}

// IsVersioned reports whether the header has a slot for a version.
func (l TableLayout) IsVersioned() bool {
	return l.versioned
}

// OffsetType returns the layout's offset type.
func (l TableLayout) OffsetType() OffsetType {
	return l.offsetType
}

// OffsetSize returns the offset size, the width in bytes of the offset type:
// 1, 2 or 4.
func (l TableLayout) OffsetSize() int {
	return l.offsetType.Size()
}

// HeaderSize returns the byte length of the header slots before the field
// offsets.
func (l TableLayout) HeaderSize() int {
	return l.headerSlots() * l.offsetType.Size()
}

func (l TableLayout) headerSlots() int {
	slots := 0
	if l.sized {
		slots++
	}
	if l.versioned {
		slots++
	}

	return slots
}

// headerLength returns the byte length of the header of a table of fields
// fields: its slots and an offset per field, which TableBuilder and
// TableSizer start a table with. It panics if fields is negative.
// maxFields returns the most fields whose offsets fit in a header, after
// its header slots: the offsets that the offset type addresses, its largest
// value divided by its size, a power of two
func (l TableLayout) maxFields() uint64 {
	return uint64(l.offsetType.MaxValue())>>(2-l.offsetType) - uint64(l.headerSlots())
}

// maxLength returns the longest table that the offset type addresses, and
// an int holds, on a 32-bit host
func (l TableLayout) maxLength() uint64 {
	return min(uint64(l.offsetType.MaxValue()), math.MaxInt)
}

func (l TableLayout) headerLength(fields int) (uint32, error) {
	if fields < 0 {
		panic("tafla: a table can't hold " + strconv.Itoa(fields) + " fields")
	}

	// In uint64, which holds any int fields and the header slots
	slots := uint64(fields) + uint64(l.headerSlots())
	size := uint64(l.offsetType.Size())
	if slots > uint64(l.offsetType.MaxValue())/size {
		return 0, &BuildError{Kind: HeaderTooLong, Fields: fields, OffsetType: l.offsetType}
	}

	return uint32(slots * size), nil
}

// checkVersion checks that the layout is versioned exactly when a version
// is given, which is a mistake of the caller's, and that the version fits
// in the header's slot
func (l TableLayout) checkVersion(versioned bool, version uint32) error {
	if l.versioned != versioned {
		if l.versioned {
			panic("tafla: a versioned layout needs a version")
		}
		panic("tafla: the layout has no version slot")
	}

	if versioned && version > l.offsetType.MaxValue() {
		return &BuildError{Kind: VersionTooLarge, Version: version, OffsetType: l.offsetType}
	}

	return nil
}
