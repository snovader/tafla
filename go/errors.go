package tafla

import (
	"strconv"
)

// DecodeErrorKind says why bytes couldn't be read.
type DecodeErrorKind uint8

const (
	// OutOfRange is a read of ByteLength bytes at Position that would leave
	// the Available bytes.
	OutOfRange DecodeErrorKind = iota + 1
	// InvalidUTF8 is a string at Position that isn't valid UTF-8.
	InvalidUTF8
	// ItemEndsBeforeStart is a vector item that starts at Position and ends
	// before it, at End. An item starts where the item before it ends, and
	// the first after the ends.
	ItemEndsBeforeStart
	// UnsupportedVersion is a header that carries format Version, which the
	// reader can't read, since it reads version Supported. [ByteBuffer]
	// doesn't compare versions: a reader returns it when
	// [ByteBuffer.Version] isn't its own.
	UnsupportedVersion
)

// String returns the kind's name, such as "OutOfRange".
func (k DecodeErrorKind) String() string {
	switch k {
	case OutOfRange:
		return "OutOfRange"
	case InvalidUTF8:
		return "InvalidUTF8"
	case ItemEndsBeforeStart:
		return "ItemEndsBeforeStart"
	case UnsupportedVersion:
		return "UnsupportedVersion"
	default:
		return "DecodeErrorKind(" + strconv.Itoa(int(k)) + ")"
	}
}

// DecodeError is returned when bytes can't be read as the table, array,
// string, struct, vector or value that a read expects. Every read of a
// [ByteBuffer], an [ArrayView]'s bytes and a [Vector] returns it, as a
// *DecodeError, which errors.As finds.
//
// Every read checks its range against the supplied bytes before it returns,
// so a truncated or corrupted buffer returns this rather than values from
// outside them. Strings must also be valid UTF-8.
//
// Checks can't tell a value that was corrupted within range from a real one,
// such as an offset that points at another field. Formats that need that
// guarantee can add a checksum.
type DecodeError struct {
	Kind DecodeErrorKind
	// Position is where the read starts, for OutOfRange, where the string's
	// length lies, for InvalidUTF8, and where the item starts, for
	// ItemEndsBeforeStart.
	Position int
	// ByteLength is the length of the read, for OutOfRange. It can exceed
	// any int, as an array's count times its elements' size can.
	ByteLength uint64
	// Available is the length of the bytes, for OutOfRange.
	Available int
	// End is where the item ends, for ItemEndsBeforeStart.
	End int
	// Version is the version that the header carries, and Supported the one
	// that the reader reads, for UnsupportedVersion.
	Version, Supported uint32
}

func (e *DecodeError) Error() string {
	switch e.Kind {
	case OutOfRange:
		return "tafla: read exceeds the supplied bytes: " + strconv.FormatUint(e.ByteLength, 10) +
			" bytes at " + strconv.Itoa(e.Position) + " of " + strconv.Itoa(e.Available)
	case InvalidUTF8:
		return "tafla: the string at " + strconv.Itoa(e.Position) + " isn't valid UTF-8"
	case ItemEndsBeforeStart:
		return "tafla: the vector item at " + strconv.Itoa(e.Position) +
			" ends before it starts, at " + strconv.Itoa(e.End)
	case UnsupportedVersion:
		return "tafla: unsupported format version " + strconv.FormatUint(uint64(e.Version), 10) +
			"; the reader reads version " + strconv.FormatUint(uint64(e.Supported), 10)
	default:
		return "tafla: decode error of kind " + e.Kind.String()
	}
}

// NewUnsupportedVersionError returns the error of a header that carries
// version, which a reader of version supported can't read, as a reader
// returns it when [ByteBuffer.Version] isn't its own.
func NewUnsupportedVersionError(version, supported uint32) error {
	return &DecodeError{Kind: UnsupportedVersion, Version: version, Supported: supported}
}

// outOfRange returns the error of a read of byteLength bytes at position in
// available bytes. It's inlined, since a call to a function that isn't costs
// a getter most of the compiler's budget for inlining it.
func outOfRange(position int, byteLength uint64, available int) error {
	return &DecodeError{Kind: OutOfRange, Position: position, ByteLength: byteLength, Available: available}
}

// BuildErrorKind says why a table couldn't be written.
type BuildErrorKind uint8

const (
	// VersionTooLarge is a Version that doesn't fit in the header slots of
	// OffsetType.
	VersionTooLarge BuildErrorKind = iota + 1
	// HeaderTooLong is a header of Fields offsets that alone is longer than
	// offsets of OffsetType address.
	HeaderTooLong
	// TooManyFields is a field set on a table that already holds its Fields
	// fields.
	TooManyFields
	// TooLong is the field at Index, which would make the table Length
	// bytes, longer than offsets of OffsetType address.
	TooLong
	// Incomplete is a table written, or nested, with Set of its Fields
	// fields set.
	Incomplete
	// NotWholeStructs is an array of structs whose Length bytes aren't a
	// whole number of Size-byte structs, or whose Size is 0.
	NotWholeStructs
	// NotAsSized is a [TableWriter] whose fields differ from those that its
	// [TableSizer] added: they wrote Written of the table's Length bytes.
	NotAsSized
)

// String returns the kind's name, such as "TooLong".
func (k BuildErrorKind) String() string {
	switch k {
	case VersionTooLarge:
		return "VersionTooLarge"
	case HeaderTooLong:
		return "HeaderTooLong"
	case TooManyFields:
		return "TooManyFields"
	case TooLong:
		return "TooLong"
	case Incomplete:
		return "Incomplete"
	case NotWholeStructs:
		return "NotWholeStructs"
	case NotAsSized:
		return "NotAsSized"
	default:
		return "BuildErrorKind(" + strconv.Itoa(int(k)) + ")"
	}
}

// BuildError is returned when a [TableBuilder], or a [TableSizer] and its
// [TableWriter], can't write a table, whatever the values' bytes, as a
// *BuildError, which errors.As finds. A setter that fails leaves the builder
// as it was. Each kind sets the fields that its documentation names.
type BuildError struct {
	Kind BuildErrorKind
	// OffsetType is the offset type of the table, for VersionTooLarge,
	// HeaderTooLong and TooLong.
	OffsetType OffsetType
	// Version is the version that doesn't fit, for VersionTooLarge.
	Version uint32
	// Fields is the number of the table's fields, for HeaderTooLong,
	// TooManyFields and Incomplete.
	Fields int
	// Set is the number of fields set, for Incomplete.
	Set int
	// Index is the index of the field that makes the table too long, for
	// TooLong.
	Index int
	// Length is the table's length with the field, for TooLong, the length
	// of the structs' bytes, for NotWholeStructs, and the table's length,
	// for NotAsSized.
	Length uint64
	// Size is the size of a struct, for NotWholeStructs.
	Size int
	// Written is the number of bytes that the fields wrote, for NotAsSized.
	Written int
}

func (e *BuildError) Error() string {
	switch e.Kind {
	case VersionTooLarge:
		return "tafla: version " + strconv.FormatUint(uint64(e.Version), 10) +
			" doesn't fit in a " + e.OffsetType.String() + " slot"
	case HeaderTooLong:
		return "tafla: a header of " + strconv.Itoa(e.Fields) + " fields doesn't fit in " +
			e.OffsetType.String() + " offsets"
	case TooManyFields:
		return "tafla: can't add field " + strconv.Itoa(e.Fields+1) + " to a table of " +
			strconv.Itoa(e.Fields)
	case TooLong:
		return "tafla: field " + strconv.Itoa(e.Index+1) + " makes the table " +
			strconv.FormatUint(e.Length, 10) + " bytes, too long for " + e.OffsetType.String() +
			" offsets"
	case Incomplete:
		return "tafla: only " + strconv.Itoa(e.Set) + " of the table's " + strconv.Itoa(e.Fields) +
			" fields were set"
	case NotWholeStructs:
		if e.Size == 0 {
			return "tafla: a struct can't be 0 bytes"
		}
		return "tafla: " + strconv.FormatUint(e.Length, 10) + " bytes aren't a whole number of " +
			strconv.Itoa(e.Size) + "-byte structs"
	case NotAsSized:
		return "tafla: the fields wrote " + strconv.Itoa(e.Written) + " of the table's " +
			strconv.FormatUint(e.Length, 10) + " bytes"
	default:
		return "tafla: build error of kind " + e.Kind.String()
	}
}

// indexOutOfRange is the panic of an index past an array's or a vector's
// length. Its message is built when it's printed, rather than where it
// panics, which keeps the getters that check the index cheap enough to
// inline.
type indexOutOfRange struct {
	index  int
	length int
	// "an array" or "a vector"
	of string
}

func (e indexOutOfRange) Error() string {
	return "tafla: index " + strconv.Itoa(e.index) + " out of range of " + e.of + " of " + strconv.Itoa(e.length)
}
