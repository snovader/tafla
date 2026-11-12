package generated_test

// The code that tafla-gen generates from the example schemas in schemas/,
// whose golden files lie in the directories beside this one, reads back
// what it writes wherever its bytes lie, writes the tables that TableBuilder
// writes by hand, or fails as it fails, rejects other versions, and reads
// corrupted and arbitrary bytes as values or decode errors, through its
// views as through its getters, without panicking.

import (
	"bytes"
	"encoding"
	"errors"
	"flag"
	"os"
	"reflect"
	"testing"

	tafla "github.com/snovader/tafla/go"
	alltypes "github.com/snovader/tafla/go/tests/generated/all_types"
	"github.com/snovader/tafla/go/tests/generated/fixed_fields"
	"github.com/snovader/tafla/go/tests/generated/layouts"
	"github.com/snovader/tafla/go/tests/generated/shapes"
	"github.com/snovader/tafla/go/tests/internal/tables"
	"pgregory.net/rapid"
)

// TestMain runs 256 cases of each property, as proptest does in the Rust
// tests, unless -rapid.checks asks for another number.
func TestMain(m *testing.M) {
	flag.Parse()

	checks := flag.Lookup("rapid.checks")
	set := false
	flag.Visit(func(f *flag.Flag) { set = set || f == checks })
	if checks != nil && !set {
		_ = checks.Value.Set("256")
	}

	os.Exit(m.Run())
}

// message is a message of an example schema, with how to read and write it
type message struct {
	name    string
	layout  tables.Layout
	version uint32
	// The struct of its fields, which writes it
	fields reflect.Type
	// Reads bytes with New<Message>, which checks a versioned message's
	// version
	read func([]byte) (reflect.Value, error)
	// <Message>VersionOf, of a versioned message
	versionOf func([]byte) (uint32, error)
}

// writer is the struct of a message's fields
type writer interface {
	encoding.BinaryMarshaler
	AppendBinary(bytes []byte) ([]byte, error)
}

func unversioned[R any, F writer](name string, offsetType tafla.OffsetType, sized bool, read func([]byte) R) message {
	return message{
		name:   name,
		layout: tables.Layout{Sized: sized, OffsetType: offsetType},
		fields: reflect.TypeFor[F](),
		read: func(bytes []byte) (reflect.Value, error) {
			return reflect.ValueOf(read(bytes)), nil
		},
	}
}

func versioned[R any, F writer](
	name string,
	offsetType tafla.OffsetType,
	sized bool,
	version uint32,
	read func([]byte) (R, error),
	versionOf func([]byte) (uint32, error),
) message {
	return message{
		name:    name,
		layout:  tables.Layout{Sized: sized, Versioned: true, OffsetType: offsetType},
		version: version,
		fields:  reflect.TypeFor[F](),
		read: func(bytes []byte) (reflect.Value, error) {
			reader, err := read(bytes)
			return reflect.ValueOf(reader), err
		},
		versionOf: versionOf,
	}
}

// messages are every message of the example schemas
var messages = []message{
	unversioned[shapes.Point, shapes.PointFields]("shapes.Point", tafla.Uint32, false, shapes.NewPoint),
	versioned[shapes.Document, shapes.DocumentFields](
		"shapes.Document", tafla.Uint32, true, 1, shapes.NewDocument, shapes.DocumentVersionOf),
	unversioned[shapes.ItemList, shapes.ItemListFields]("shapes.ItemList", tafla.Uint32, false, shapes.NewItemList),

	unversioned[layouts.Uint8Offsets, layouts.Uint8OffsetsFields](
		"layouts.Uint8Offsets", tafla.Uint8, false, layouts.NewUint8Offsets),
	unversioned[layouts.Uint8OffsetsSized, layouts.Uint8OffsetsSizedFields](
		"layouts.Uint8OffsetsSized", tafla.Uint8, true, layouts.NewUint8OffsetsSized),
	versioned[layouts.Uint8OffsetsVersioned, layouts.Uint8OffsetsVersionedFields](
		"layouts.Uint8OffsetsVersioned", tafla.Uint8, false, 255,
		layouts.NewUint8OffsetsVersioned, layouts.Uint8OffsetsVersionedVersionOf),
	versioned[layouts.Uint8OffsetsSizedVersioned, layouts.Uint8OffsetsSizedVersionedFields](
		"layouts.Uint8OffsetsSizedVersioned", tafla.Uint8, true, 0,
		layouts.NewUint8OffsetsSizedVersioned, layouts.Uint8OffsetsSizedVersionedVersionOf),
	unversioned[layouts.Uint16Offsets, layouts.Uint16OffsetsFields](
		"layouts.Uint16Offsets", tafla.Uint16, false, layouts.NewUint16Offsets),
	unversioned[layouts.Uint16OffsetsSized, layouts.Uint16OffsetsSizedFields](
		"layouts.Uint16OffsetsSized", tafla.Uint16, true, layouts.NewUint16OffsetsSized),
	versioned[layouts.Uint16OffsetsVersioned, layouts.Uint16OffsetsVersionedFields](
		"layouts.Uint16OffsetsVersioned", tafla.Uint16, false, 65535,
		layouts.NewUint16OffsetsVersioned, layouts.Uint16OffsetsVersionedVersionOf),
	versioned[layouts.Uint16OffsetsSizedVersioned, layouts.Uint16OffsetsSizedVersionedFields](
		"layouts.Uint16OffsetsSizedVersioned", tafla.Uint16, true, 1,
		layouts.NewUint16OffsetsSizedVersioned, layouts.Uint16OffsetsSizedVersionedVersionOf),
	unversioned[layouts.Uint32Offsets, layouts.Uint32OffsetsFields](
		"layouts.Uint32Offsets", tafla.Uint32, false, layouts.NewUint32Offsets),
	unversioned[layouts.Uint32OffsetsSized, layouts.Uint32OffsetsSizedFields](
		"layouts.Uint32OffsetsSized", tafla.Uint32, true, layouts.NewUint32OffsetsSized),
	versioned[layouts.Uint32OffsetsVersioned, layouts.Uint32OffsetsVersionedFields](
		"layouts.Uint32OffsetsVersioned", tafla.Uint32, false, 4294967295,
		layouts.NewUint32OffsetsVersioned, layouts.Uint32OffsetsVersionedVersionOf),
	versioned[layouts.Uint32OffsetsSizedVersioned, layouts.Uint32OffsetsSizedVersionedFields](
		"layouts.Uint32OffsetsSizedVersioned", tafla.Uint32, true, 2,
		layouts.NewUint32OffsetsSizedVersioned, layouts.Uint32OffsetsSizedVersionedVersionOf),

	unversioned[alltypes.Scalars, alltypes.ScalarsFields]("all_types.Scalars", tafla.Uint32, false, alltypes.NewScalars),
	unversioned[alltypes.Arrays, alltypes.ArraysFields]("all_types.Arrays", tafla.Uint32, false, alltypes.NewArrays),
	unversioned[alltypes.Nested, alltypes.NestedFields]("all_types.Nested", tafla.Uint32, false, alltypes.NewNested),
	unversioned[alltypes.Reserved, alltypes.ReservedFields]("all_types.Reserved", tafla.Uint32, false, alltypes.NewReserved),
	unversioned[alltypes.Structs, alltypes.StructsFields]("all_types.Structs", tafla.Uint32, false, alltypes.NewStructs),

	// Fixed fields, which the generated code lays out itself. Overflow, which
	// is always too long, has a test of its own.
	unversioned[fixedfields.Tiny, fixedfields.TinyFields]("fixed_fields.Tiny", tafla.Uint8, true, fixedfields.NewTiny),
	versioned[fixedfields.Tagged, fixedfields.TaggedFields](
		"fixed_fields.Tagged", tafla.Uint8, true, 3, fixedfields.NewTagged, fixedfields.TaggedVersionOf),
	unversioned[fixedfields.Header, fixedfields.HeaderFields]("fixed_fields.Header", tafla.Uint8, false, fixedfields.NewHeader),
	versioned[fixedfields.Pixel, fixedfields.PixelFields](
		"fixed_fields.Pixel", tafla.Uint16, true, 2, fixedfields.NewPixel, fixedfields.PixelVersionOf),
	unversioned[fixedfields.Strip, fixedfields.StripFields]("fixed_fields.Strip", tafla.Uint16, false, fixedfields.NewStrip),
	versioned[fixedfields.Box, fixedfields.BoxFields](
		"fixed_fields.Box", tafla.Uint32, false, 1, fixedfields.NewBox, fixedfields.BoxVersionOf),
	unversioned[fixedfields.Track, fixedfields.TrackFields]("fixed_fields.Track", tafla.Uint32, false, fixedfields.NewTrack),
	unversioned[fixedfields.Label, fixedfields.LabelFields]("fixed_fields.Label", tafla.Uint32, false, fixedfields.NewLabel),
	unversioned[fixedfields.Span, fixedfields.SpanFields]("fixed_fields.Span", tafla.Uint32, false, fixedfields.NewSpan),
}

// structs are every struct of the example schemas, with their sizes
var structs = []struct {
	name string
	t    reflect.Type
	size int
}{
	{"shapes.Sample", reflect.TypeFor[shapes.Sample](), 12},
	{"all_types.Segment", reflect.TypeFor[alltypes.Segment](), 16},
	{"all_types.Point", reflect.TypeFor[alltypes.Point](), 8},
	{"all_types.Packed", reflect.TypeFor[alltypes.Packed](), 43},
	{"all_types.Documented", reflect.TypeFor[alltypes.Documented](), 7},
	{"fixed_fields.Vec2", reflect.TypeFor[fixedfields.Vec2](), 8},
	{"fixed_fields.Quad", reflect.TypeFor[fixedfields.Quad](), 64},
	{"fixed_fields.Block", reflect.TypeFor[fixedfields.Block](), 256},
}

// write writes a message's fields with the generated code
func write(fields reflect.Value) ([]byte, error) {
	return fields.Interface().(writer).MarshalBinary()
}

// readAll reads bytes as the message, every field through its getter, and
// every vector and array of structs through its view too
func (m message) readAll(bytes []byte) (reflect.Value, error) {
	reader, err := m.read(bytes)
	if err != nil {
		return reflect.Value{}, err
	}

	return readFields(reader, m.fields)
}

// written draws a message's fields, and writes them, skipping a draw that's
// too long for the message's offset type
func (m message) written(t *rapid.T) (reflect.Value, []byte) {
	fields := drawFields(t, m.fields, 4)
	written, err := write(fields)

	var buildError *tafla.BuildError
	if errors.As(err, &buildError) && buildError.Kind == tafla.TooLong {
		t.Skip("too long for the offset type")
	}
	if err != nil {
		t.Fatal(err)
	}

	return fields, written
}

func TestEveryMessageReadsBackItsFieldsWhereverItsBytesLie(t *testing.T) {
	for _, message := range messages {
		t.Run(message.name, func(t *testing.T) {
			rapid.Check(t, func(t *rapid.T) {
				fields, written := message.written(t)
				byteOffset := rapid.IntRange(0, 7).Draw(t, "byteOffset")

				read, err := message.readAll(tables.AtByteOffset(written, byteOffset))
				if err != nil || !sameValues(read, fields) {
					t.Fatalf("read %+v, %v\nrather than %+v", read, err, fields)
				}
			})
		})
	}
}

func TestEveryMessageIsWrittenAsTableBuilderWritesItOrFailsAsItFails(t *testing.T) {
	for _, message := range messages {
		t.Run(message.name, func(t *testing.T) {
			rapid.Check(t, func(t *rapid.T) {
				fields := drawFields(t, message.fields, 4)
				written, writeErr := write(fields)
				built, buildErr := describe(fields, message.layout, message.version).Bytes()

				var writeError, buildError *tafla.BuildError
				switch {
				case writeErr == nil && buildErr == nil:
					if !bytes.Equal(written, built) {
						t.Fatalf("wrote % x\nrather than % x", written, built)
					}
				case errors.As(writeErr, &writeError) && errors.As(buildErr, &buildError):
					if *writeError != *buildError {
						t.Fatalf("%#v rather than %#v", writeError, buildError)
					}
				default:
					t.Fatalf("%v rather than %v", writeErr, buildErr)
				}
			})
		})
	}
}

// A message whose fixed fields alone are too long for its offset type is
// written field by field, rather than laid out, and fails as TableBuilder
// fails, naming the field that makes it too long
func TestAMessageWhoseFixedFieldsAreTooLongFailsAsTableBuilderFails(t *testing.T) {
	layout := tables.Layout{OffsetType: tafla.Uint8}

	rapid.Check(t, func(t *rapid.T) {
		fields := drawFields(t, reflect.TypeFor[fixedfields.OverflowFields](), 4)
		_, writeErr := write(fields)
		_, buildErr := describe(fields, layout, 0).Bytes()

		// The offsets, the id and then the block's 256 bytes
		expected := &tafla.BuildError{Kind: tafla.TooLong, Index: 1, Length: 259, OffsetType: tafla.Uint8}
		if !sameError(writeErr, expected) || !sameError(buildErr, expected) {
			t.Fatalf("wrote %v and built %v rather than %v", writeErr, buildErr, expected)
		}
	})
}

// AppendBinary appends a message to bytes, into their capacity when they
// have room, which it overwrites, and into new bytes otherwise, as
// MarshalBinary writes it
func TestEveryMessageIsAppendedAsItsWritten(t *testing.T) {
	for _, message := range messages {
		t.Run(message.name, func(t *testing.T) {
			rapid.Check(t, func(t *rapid.T) {
				fields, written := message.written(t)
				before := rapid.SliceOfN(rapid.Byte(), 0, 9).Draw(t, "before")
				room := rapid.IntRange(0, len(written)+8).Draw(t, "room")

				backing := bytes.Repeat([]byte{0xa5}, len(before)+room)
				dst := backing[:len(before)]
				copy(dst, before)

				appended, err := fields.Interface().(writer).AppendBinary(dst)
				if err != nil || !bytes.Equal(appended, append(append([]byte{}, before...), written...)) {
					t.Fatalf("appended % x, %v", appended, err)
				}
				inPlace := len(backing) > 0 && &appended[0] == &backing[0]
				if fits := room >= len(written); fits != inPlace {
					t.Fatalf("appended into the given bytes: %v, with room for %d of %d", inPlace, room, len(written))
				}
			})
		})
	}
}

func TestEveryVersionedMessageRejectsOtherVersionsWithUnsupportedVersion(t *testing.T) {
	for _, message := range messages {
		if message.versionOf == nil {
			continue
		}

		t.Run(message.name, func(t *testing.T) {
			rapid.Check(t, func(t *rapid.T) {
				_, written := message.written(t)
				width := message.layout.OffsetType.Size()
				version := rapid.Uint32Range(0, message.layout.OffsetType.MaxValue()).Draw(t, "version")

				slot := 0
				if message.layout.Sized {
					slot = width
				}
				for index := range width {
					written[slot+index] = byte(version >> (8 * index))
				}

				if read, err := message.versionOf(written); read != version || err != nil {
					t.Fatalf("read version %d, %v", read, err)
				}

				_, err := message.read(written)
				expected := tafla.NewUnsupportedVersionError(version, message.version)
				if version == message.version && err != nil || version != message.version && !sameError(err, expected) {
					t.Fatalf("%v rather than %v", err, expected)
				}

				// Bytes that don't hold the version's slot
				short := written[:slot+width-1]
				var decodeError *tafla.DecodeError
				if _, err := message.versionOf(short); !errors.As(err, &decodeError) || decodeError.Kind != tafla.OutOfRange {
					t.Fatalf("%v rather than OutOfRange", err)
				}
			})
		})
	}
}

// Overwritten bytes read as values, which may be others than those written,
// or fail with the decode error, through the getters and the views
func TestEveryCorruptedMessageReadsAsValuesOrDecodeErrors(t *testing.T) {
	for _, message := range messages {
		t.Run(message.name, func(t *testing.T) {
			rapid.Check(t, func(t *rapid.T) {
				_, written := message.written(t)
				overwrites := rapid.IntRange(1, 4).Draw(t, "overwrites")
				for range overwrites {
					at := rapid.IntRange(0, len(written)-1).Draw(t, "at")
					written[at] = rapid.SampledFrom([]byte{0, 1, 2, 7, 0x80, 0xff}).Draw(t, "byte")
				}
				cut := rapid.IntRange(0, len(written)).Draw(t, "cut")

				for _, bytes := range [][]byte{written, written[:cut:cut]} {
					checkReadError(t, message, bytes)
				}
			})
		})
	}
}

func TestArbitraryBytesReadAsEveryMessageOrDecodeErrors(t *testing.T) {
	rapid.Check(t, func(t *rapid.T) {
		bytes := rapid.SliceOfN(rapid.SampledFrom([]byte{0, 1, 2, 4, 8, 12, 0x80, 0xff}), 0, 64).Draw(t, "bytes")

		for _, message := range messages {
			checkReadError(t, message, bytes)
		}
	})
}

// checkReadError reads bytes as the message, which may fail only with the
// decode error
func checkReadError(t *rapid.T, message message, bytes []byte) {
	_, err := message.readAll(bytes)
	var decodeError *tafla.DecodeError
	if err != nil && !errors.As(err, &decodeError) {
		t.Fatalf("%s: %v rather than a decode error", message.name, err)
	}
}

func TestEveryStructIsWrittenAtFixedOffsetsAndReadBackFromTheStartOfItsBytes(t *testing.T) {
	for _, structure := range structs {
		t.Run(structure.name, func(t *testing.T) {
			rapid.Check(t, func(t *rapid.T) {
				value := drawFields(t, structure.t, 4)
				written, err := value.Interface().(encoding.BinaryMarshaler).MarshalBinary()

				expected := tables.StructBytes(structScalars(value))
				if err != nil || len(written) != structure.size || !bytes.Equal(written, expected) {
					t.Fatalf("wrote % x, %v\nrather than % x", written, err, expected)
				}

				// From the start of longer bytes, wherever they lie
				after := rapid.SliceOfN(rapid.Byte(), 0, 8).Draw(t, "after")
				byteOffset := rapid.IntRange(0, 7).Draw(t, "byteOffset")
				longer := tables.AtByteOffset(append(written, after...), byteOffset)

				read := reflect.New(structure.t)
				err = read.Interface().(encoding.BinaryUnmarshaler).UnmarshalBinary(longer)
				if err != nil || !sameValues(read.Elem(), value) {
					t.Fatalf("read %+v, %v", read.Elem(), err)
				}

				expectedError := &tafla.DecodeError{Kind: tafla.OutOfRange, ByteLength: uint64(structure.size), Available: len(written) - 1}
				err = read.Interface().(encoding.BinaryUnmarshaler).UnmarshalBinary(written[:len(written)-1])
				if !sameError(err, expectedError) {
					t.Fatalf("%v rather than %v", err, expectedError)
				}
			})
		})
	}
}

// An item of a vector is read in place, within the bytes of the message
// that holds it, so that an offset of its own can lead past its end, at
// bytes that it reads as every other target does
func TestAnItemIsReadInPlacePastItsEndWithinTheBytesOfItsItemList(t *testing.T) {
	fields := shapes.ItemListFields{Items: []shapes.ItemFields{
		{Value: 1.5, Pair: shapes.PairFields{First: 2.5, Second: 3.5}},
	}}
	written := must(fields.MarshalBinary())

	// The vector at 4 holds its count and the item's end, 36, and then the
	// item at 12: its two offsets, its value at 20 and its Pair at 24. An end
	// of 16 leaves the item only its offsets, which point past it.
	if !bytes.Equal(written[8:12], []byte{36, 0, 0, 0}) {
		t.Fatalf("the item ends at % x", written[8:12])
	}
	copy(written[8:12], []byte{16, 0, 0, 0})

	read, err := readFields(reflect.ValueOf(shapes.NewItemList(written)), reflect.TypeFor[shapes.ItemListFields]())
	if err != nil || !sameValues(read, reflect.ValueOf(fields)) {
		t.Fatalf("read %+v, %v", read, err)
	}
}

func sameError(a, b error) bool {
	if a == nil || b == nil {
		return a == b
	}

	return reflect.DeepEqual(a, b)
}

func must[T any](value T, err error) T {
	if err != nil {
		panic(err)
	}

	return value
}
