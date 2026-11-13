package tables

import (
	"bytes"
	"errors"
	"fmt"

	tafla "github.com/snovader/tafla/go"
)

// Reads tables with ByteBuffer as their descriptions set them out, and
// returns the descriptions with the values read in place of theirs, so that
// they equal the descriptions when the bytes hold the described values.
//
// A nested table, from SetTable or in a vector, is read in place when it has
// no header and its parent's offset type: a field from its position in its
// parent's bytes, and a vector item from the bytes that
// Vector.ItemsInPlace returns, which end where its parent's do. Any other is
// read as a table of its own: a field from its position to the end of its
// parent, since its length isn't written, and a vector item from the bytes
// that Vector.Items returns. The scalars of structs are read at their
// offsets in the bytes that GetStruct and GetStructArray return.

// ErrMismatch is the error of bytes that read without a decode error, but
// otherwise than their description: a sized table whose header holds
// another length, a vector of another number of items, or one whose items
// Get finds otherwise than iterating does.
var ErrMismatch = errors.New("the bytes don't read as described")

// ReadTable reads a table from its own bytes, as the root or a vector item is
// read: its header, whose size must be the table's byte length when it's
// sized, and then its fields.
func ReadTable(bytes []byte, table Table) (Table, error) {
	reader := reader{tafla.NewByteBuffer(bytes, table.Layout.TableLayout())}

	if table.Layout.Sized {
		size, err := reader.buffer.Size()
		if err != nil {
			return Table{}, err
		}
		if size != len(bytes) {
			return Table{}, fmt.Errorf("%w: the header holds the size %d of %d bytes", ErrMismatch, size, len(bytes))
		}
	}

	read := Table{Layout: table.Layout}
	if table.Layout.Versioned {
		version, err := reader.buffer.Version()
		if err != nil {
			return Table{}, err
		}
		read.Version = version
	}

	for index, field := range table.Fields {
		value, err := reader.readRootField(index, field)
		if err != nil {
			return Table{}, err
		}
		read.Fields = append(read.Fields, value)
	}

	return read, nil
}

// ReadRootField reads root field index of a table of layout as field,
// without reading the header.
func ReadRootField(bytes []byte, layout Layout, index int, field Field) (Field, error) {
	reader := reader{tafla.NewByteBuffer(bytes, layout.TableLayout())}
	return reader.readRootField(index, field)
}

// ReadField reads field at position of a table of layout.
func ReadField(bytes []byte, layout Layout, position int, field Field) (Field, error) {
	reader := reader{tafla.NewByteBuffer(bytes, layout.TableLayout())}
	return reader.readField(position, field)
}

type reader struct {
	buffer tafla.ByteBuffer
}

func (r reader) readRootField(index int, field Field) (Field, error) {
	position, err := r.buffer.RootOffset(index)
	if err != nil {
		return nil, err
	}

	return r.readField(position, field)
}

func (r reader) readField(position int, field Field) (Field, error) {
	buffer := r.buffer

	switch field := field.(type) {
	case Int8:
		value, err := buffer.GetInt8(position)
		return Int8(value), err
	case Uint8:
		value, err := buffer.GetUint8(position)
		return Uint8(value), err
	case Int16:
		value, err := buffer.GetInt16(position)
		return Int16(value), err
	case Uint16:
		value, err := buffer.GetUint16(position)
		return Uint16(value), err
	case Int32:
		value, err := buffer.GetInt32(position)
		return Int32(value), err
	case Uint32:
		value, err := buffer.GetUint32(position)
		return Uint32(value), err
	case Int64:
		value, err := buffer.GetInt64(position)
		return Int64(value), err
	case Uint64:
		value, err := buffer.GetUint64(position)
		return Uint64(value), err
	case Float32:
		value, err := buffer.GetFloat32(position)
		return Float32(value), err
	case Float64:
		value, err := buffer.GetFloat64(position)
		return Float64(value), err
	case Bool:
		value, err := buffer.GetUint8(position)
		return Bool(value != 0), err
	case Int8Array:
		values, err := buffer.GetInt8Array(position)
		return Int8Array(values.Append(nil)), err
	case Uint8Array:
		values, err := buffer.GetUint8Array(position)
		return Uint8Array(bytes.Clone(values)), err
	case Int16Array:
		values, err := buffer.GetInt16Array(position)
		return Int16Array(values.Append(nil)), err
	case Uint16Array:
		values, err := buffer.GetUint16Array(position)
		return Uint16Array(values.Append(nil)), err
	case Int32Array:
		values, err := buffer.GetInt32Array(position)
		return Int32Array(values.Append(nil)), err
	case Uint32Array:
		values, err := buffer.GetUint32Array(position)
		return Uint32Array(values.Append(nil)), err
	case Int64Array:
		values, err := buffer.GetInt64Array(position)
		return Int64Array(values.Append(nil)), err
	case Uint64Array:
		values, err := buffer.GetUint64Array(position)
		return Uint64Array(values.Append(nil)), err
	case Float32Array:
		values, err := buffer.GetFloat32Array(position)
		return Float32Array(values.Append(nil)), err
	case Float64Array:
		values, err := buffer.GetFloat64Array(position)
		return Float64Array(values.Append(nil)), err
	case String:
		value, err := buffer.GetString(position)
		return String(value), err
	case Struct:
		types := StructTypes(field)
		bytes, err := buffer.GetStruct(position, StructSize(types))
		if err != nil {
			return nil, err
		}
		scalars, err := readStruct(bytes, types)
		return Struct(scalars), err
	case StructArray:
		size := StructSize(field.Types)
		bytes, err := buffer.GetStructArray(position, size)
		if err != nil {
			return nil, err
		}
		read := StructArray{Types: field.Types, Items: [][]Field{}}
		for start := 0; start < len(bytes); start += size {
			scalars, err := readStruct(bytes[start:start+size], field.Types)
			if err != nil {
				return nil, err
			}
			read.Items = append(read.Items, scalars)
		}
		return read, nil
	case TableField:
		table, err := r.readNested(position, field.Nested.Table)
		if err != nil {
			return nil, err
		}
		return TableField{Nested: Nested{From: field.Nested.From, Table: table}, Alignment: field.Alignment}, nil
	case Vector:
		return r.readVector(position, field)
	default:
		panic("unknown field")
	}
}

// readNested reads a table nested with SetTable
func (r reader) readNested(position int, table Table) (Table, error) {
	if r.readsInPlace(table) {
		return r.readInPlace(position, table)
	}

	// A position past the end leaves no bytes, which the first read fails for
	bytes := r.buffer.Bytes()
	return ReadTable(bytes[min(max(position, 0), len(bytes)):], table)
}

// item is the bytes of a vector's item, or the error of reading it
type item struct {
	bytes []byte
	err   error
}

// readVector reads a vector's items, after checking that finding each from
// its ends alone finds what iterating finds
func (r reader) readVector(position int, items Vector) (Field, error) {
	vector, err := r.buffer.GetVector(position)
	if err != nil {
		return nil, err
	}

	var iterated, inPlace []item
	for bytes, err := range vector.Items() {
		iterated = append(iterated, item{bytes, err})
	}
	for bytes, err := range vector.ItemsInPlace() {
		inPlace = append(inPlace, item{bytes, err})
	}

	if len(iterated) != vector.Len() || len(inPlace) != vector.Len() {
		return nil, fmt.Errorf("%w: a vector of %d items iterates over %d and %d in place",
			ErrMismatch, vector.Len(), len(iterated), len(inPlace))
	}

	for index := range vector.Len() {
		bytes, err := vector.Get(index)
		bytesInPlace, errInPlace := vector.GetInPlace(index)
		if !sameItem(item{bytes, err}, iterated[index]) || !sameItem(item{bytesInPlace, errInPlace}, inPlace[index]) {
			return nil, fmt.Errorf("%w: Get finds item %d otherwise than iterating", ErrMismatch, index)
		}
	}

	for index := range iterated {
		if iterated[index].err != nil {
			return nil, iterated[index].err
		}
		if inPlace[index].err != nil {
			return nil, inPlace[index].err
		}
	}

	if vector.Len() != len(items) {
		return nil, fmt.Errorf("%w: a vector of %d items, rather than %d", ErrMismatch, vector.Len(), len(items))
	}

	read := make(Vector, 0, len(items))
	for index, nested := range items {
		var table Table
		var err error
		if r.readsInPlace(nested.Table) {
			// At its start, in a buffer of its offset type, which is its
			// parent's
			buffer := tafla.NewByteBuffer(inPlace[index].bytes, nested.Table.Layout.TableLayout())
			table, err = reader{buffer}.readInPlace(0, nested.Table)
		} else {
			table, err = ReadTable(iterated[index].bytes, nested.Table)
		}
		if err != nil {
			return nil, err
		}

		read = append(read, Nested{From: nested.From, Table: table})
	}

	return read, nil
}

// sameItem reports whether two items are the same bytes, from the same
// position, or the same error
func sameItem(a, b item) bool {
	if a.err != nil || b.err != nil {
		var x, y *tafla.DecodeError
		return errors.As(a.err, &x) && errors.As(b.err, &y) && *x == *y
	}

	return len(a.bytes) == len(b.bytes) && (len(a.bytes) == 0 || &a.bytes[0] == &b.bytes[0])
}

// readsInPlace reports whether a nested table is read in place: it has no
// header, and its parent's offset type
func (r reader) readsInPlace(table Table) bool {
	layout := table.Layout
	return !layout.Sized && !layout.Versioned && layout.OffsetType == r.buffer.Layout().OffsetType()
}

// readInPlace reads the fields of a table nested at position, from offsets
// counted from its start, within its parent's bytes
func (r reader) readInPlace(position int, table Table) (Table, error) {
	read := Table{Layout: table.Layout}
	for index, field := range table.Fields {
		offset, err := r.buffer.FieldOffset(position, index)
		if err != nil {
			return Table{}, err
		}
		value, err := r.readField(offset, field)
		if err != nil {
			return Table{}, err
		}
		read.Fields = append(read.Fields, value)
	}

	return read, nil
}

// readStruct reads the scalars of a struct of types at their offsets in its
// bytes, which a ByteBuffer reads in any layout
func readStruct(bytes []byte, types []ScalarType) ([]Field, error) {
	buffer := tafla.NewByteBuffer(bytes, tafla.TableLayout{})
	scalars := make([]Field, 0, len(types))

	offset := 0
	for _, scalar := range types {
		value, err := readScalar(buffer, scalar, offset)
		if err != nil {
			return nil, err
		}
		scalars = append(scalars, value)
		offset += scalar.Size()
	}

	return scalars, nil
}

func readScalar(buffer tafla.ByteBuffer, scalar ScalarType, position int) (Field, error) {
	switch scalar {
	case Int8Type:
		value, err := buffer.GetInt8(position)
		return Int8(value), err
	case Uint8Type:
		value, err := buffer.GetUint8(position)
		return Uint8(value), err
	case Int16Type:
		value, err := buffer.GetInt16(position)
		return Int16(value), err
	case Uint16Type:
		value, err := buffer.GetUint16(position)
		return Uint16(value), err
	case Int32Type:
		value, err := buffer.GetInt32(position)
		return Int32(value), err
	case Uint32Type:
		value, err := buffer.GetUint32(position)
		return Uint32(value), err
	case Int64Type:
		value, err := buffer.GetInt64(position)
		return Int64(value), err
	case Uint64Type:
		value, err := buffer.GetUint64(position)
		return Uint64(value), err
	case Float32Type:
		value, err := buffer.GetFloat32(position)
		return Float32(value), err
	default:
		value, err := buffer.GetFloat64(position)
		return Float64(value), err
	}
}
