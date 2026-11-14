package codecs

import (
	"encoding/binary"
	"errors"
	"math"

	tafla "github.com/snovader/tafla/go"
	"github.com/snovader/tafla/go/benchmarks/workloads"
)

// Tafla writes tables with TableBuilder, recording nested tables in place
// among their parent builder's fields with SetTableFunc() and SetVectorFunc().
// The builders write each array in bulk into a table of its exact size. It
// reads the tables with ByteBuffer's getters, with strings and columns in
// place.
type Tafla struct{}

// documentLayout is the document's header: its length and version
var documentLayout = tafla.TableLayout{}.Sized().Versioned()

const documentVersion = 1

func (Tafla) Name() string { return "tafla" }

func (Tafla) SerializeRecord(record workloads.Record) ([]byte, error) {
	builder, err := tafla.NewTableBuilder(4, tafla.TableLayout{})
	if err != nil {
		return nil, err
	}

	err = errors.Join(
		builder.SetFloat64(record.X),
		builder.SetFloat64(record.Y),
		builder.SetInt64(record.ID),
		builder.SetString(record.Name),
	)
	if err != nil {
		return nil, err
	}

	return builder.Bytes()
}

func (Tafla) DeserializeRecord(bytes []byte) (workloads.Record, error) {
	buffer := tafla.NewByteBuffer(bytes, tafla.TableLayout{})
	var record workloads.Record

	position, err := buffer.RootOffset(0)
	if err != nil {
		return workloads.Record{}, err
	}
	if record.X, err = buffer.GetFloat64(position); err != nil {
		return workloads.Record{}, err
	}

	if position, err = buffer.RootOffset(1); err != nil {
		return workloads.Record{}, err
	}
	if record.Y, err = buffer.GetFloat64(position); err != nil {
		return workloads.Record{}, err
	}

	if position, err = buffer.RootOffset(2); err != nil {
		return workloads.Record{}, err
	}
	if record.ID, err = buffer.GetInt64(position); err != nil {
		return workloads.Record{}, err
	}

	if position, err = buffer.RootOffset(3); err != nil {
		return workloads.Record{}, err
	}
	if record.Name, err = buffer.GetString(position); err != nil {
		return workloads.Record{}, err
	}

	return record, nil
}

func (Tafla) SerializeDocument(document workloads.Document) ([]byte, error) {
	builder, err := tafla.NewVersionedTableBuilder(4, documentLayout, documentVersion)
	if err != nil {
		return nil, err
	}
	err = errors.Join(
		builder.SetFloat64(document.X),
		builder.SetFloat64(document.Y),
		builder.SetString(document.Name),
		builder.SetTableFunc(4, func(columns tafla.Item) error {
			return errors.Join(
				columns.SetInt64(document.Base),
				columns.SetUint32Array(document.Keys),
				columns.SetFloat32Array(document.A),
				columns.SetFloat32Array(document.B),
			)
		}),
	)
	if err != nil {
		return nil, err
	}

	return builder.Bytes()
}

func (Tafla) DeserializeDocument(bytes []byte) (workloads.Document, error) {
	buffer := tafla.NewByteBuffer(bytes, documentLayout)
	if err := checkVersion(buffer); err != nil {
		return workloads.Document{}, err
	}

	var document workloads.Document

	position, err := buffer.RootOffset(0)
	if err != nil {
		return workloads.Document{}, err
	}
	if document.X, err = buffer.GetFloat64(position); err != nil {
		return workloads.Document{}, err
	}

	if position, err = buffer.RootOffset(1); err != nil {
		return workloads.Document{}, err
	}
	if document.Y, err = buffer.GetFloat64(position); err != nil {
		return workloads.Document{}, err
	}

	if position, err = buffer.RootOffset(2); err != nil {
		return workloads.Document{}, err
	}
	if document.Name, err = buffer.GetString(position); err != nil {
		return workloads.Document{}, err
	}

	columns, err := buffer.RootOffset(3)
	if err != nil {
		return workloads.Document{}, err
	}
	if err := readColumns(buffer, columns, &document); err != nil {
		return workloads.Document{}, err
	}

	return document, nil
}

// readColumns reads the document's nested table of columns, at columns,
// with each column in place where it's aligned
func readColumns(buffer tafla.ByteBuffer, columns int, document *workloads.Document) error {
	position, err := buffer.FieldOffset(columns, 0)
	if err != nil {
		return err
	}
	if document.Base, err = buffer.GetInt64(position); err != nil {
		return err
	}

	if position, err = buffer.FieldOffset(columns, 1); err != nil {
		return err
	}
	keys, err := buffer.GetUint32Array(position)
	if err != nil {
		return err
	}

	if position, err = buffer.FieldOffset(columns, 2); err != nil {
		return err
	}
	a, err := buffer.GetFloat32Array(position)
	if err != nil {
		return err
	}

	if position, err = buffer.FieldOffset(columns, 3); err != nil {
		return err
	}
	b, err := buffer.GetFloat32Array(position)
	if err != nil {
		return err
	}

	document.Keys, document.A, document.B = keys.Slice(), a.Slice(), b.Slice()

	return nil
}

func (Tafla) SumA(bytes []byte) (float64, error) {
	buffer := tafla.NewByteBuffer(bytes, documentLayout)
	if err := checkVersion(buffer); err != nil {
		return 0, err
	}

	columns, err := buffer.RootOffset(3)
	if err != nil {
		return 0, err
	}
	position, err := buffer.FieldOffset(columns, 2)
	if err != nil {
		return 0, err
	}
	a, err := buffer.GetFloat32Array(position)
	if err != nil {
		return 0, err
	}

	sum := 0.0
	for value := range a.Values() {
		sum += float64(value)
	}

	return sum, nil
}

func (Tafla) SerializeList(list workloads.List) ([]byte, error) {
	builder, err := tafla.NewTableBuilder(1, tafla.TableLayout{})
	if err != nil {
		return nil, err
	}
	err = builder.SetVectorFunc(len(list.Items), 2, func(index int, item tafla.Item) error {
		return setListItem(item, list.Items[index])
	})
	if err != nil {
		return nil, err
	}

	return builder.Bytes()
}

// setListItem records an item of the vector, and its pair, in place
func setListItem(table tafla.Item, item workloads.Item) error {
	return errors.Join(
		table.SetFloat32(item.Value),
		table.SetTableFunc(2, func(pair tafla.Item) error {
			return errors.Join(pair.SetFloat32(item.First), pair.SetFloat32(item.Second))
		}),
	)
}

func (Tafla) DeserializeList(bytes []byte) (workloads.List, error) {
	buffer := tafla.NewByteBuffer(bytes, tafla.TableLayout{})
	position, err := buffer.RootOffset(0)
	if err != nil {
		return workloads.List{}, err
	}
	vector, err := buffer.GetVector(position)
	if err != nil {
		return workloads.List{}, err
	}

	list := workloads.List{Items: make([]workloads.Item, 0, vector.Len())}
	for item, err := range vector.ItemsInPlace() {
		if err != nil {
			return workloads.List{}, err
		}

		// In place, at its start, in a buffer of its parent's offset type
		read, err := readListItem(tafla.NewByteBuffer(item, tafla.TableLayout{}))
		if err != nil {
			return workloads.List{}, err
		}
		list.Items = append(list.Items, read)
	}

	return list, nil
}

// readListItem reads an item of the vector, whose table starts at 0
func readListItem(buffer tafla.ByteBuffer) (workloads.Item, error) {
	var item workloads.Item

	position, err := buffer.FieldOffset(0, 0)
	if err != nil {
		return workloads.Item{}, err
	}
	if item.Value, err = buffer.GetFloat32(position); err != nil {
		return workloads.Item{}, err
	}

	pair, err := buffer.FieldOffset(0, 1)
	if err != nil {
		return workloads.Item{}, err
	}

	if position, err = buffer.FieldOffset(pair, 0); err != nil {
		return workloads.Item{}, err
	}
	if item.First, err = buffer.GetFloat32(position); err != nil {
		return workloads.Item{}, err
	}

	if position, err = buffer.FieldOffset(pair, 1); err != nil {
		return workloads.Item{}, err
	}
	if item.Second, err = buffer.GetFloat32(position); err != nil {
		return workloads.Item{}, err
	}

	return item, nil
}

// tripleSize is the size of a struct of an item's three float32s
const tripleSize = 12

func (Tafla) SerializeStructs(list workloads.List) ([]byte, error) {
	structs := make([]byte, 0, len(list.Items)*tripleSize)
	for _, item := range list.Items {
		structs = binary.LittleEndian.AppendUint32(structs, math.Float32bits(item.Value))
		structs = binary.LittleEndian.AppendUint32(structs, math.Float32bits(item.First))
		structs = binary.LittleEndian.AppendUint32(structs, math.Float32bits(item.Second))
	}

	builder, err := tafla.NewTableBuilder(1, tafla.TableLayout{})
	if err != nil {
		return nil, err
	}
	if err := builder.SetStructArray(structs, tripleSize, tafla.Align4); err != nil {
		return nil, err
	}

	return builder.Bytes()
}

func (Tafla) DeserializeStructs(bytes []byte) (workloads.List, error) {
	buffer := tafla.NewByteBuffer(bytes, tafla.TableLayout{})
	position, err := buffer.RootOffset(0)
	if err != nil {
		return workloads.List{}, err
	}
	structs, err := buffer.GetStructArray(position, tripleSize)
	if err != nil {
		return workloads.List{}, err
	}

	list := workloads.List{Items: make([]workloads.Item, len(structs)/tripleSize)}
	for index := range list.Items {
		triple := structs[index*tripleSize : (index+1)*tripleSize]
		list.Items[index] = workloads.Item{
			Value:  math.Float32frombits(binary.LittleEndian.Uint32(triple[0:])),
			First:  math.Float32frombits(binary.LittleEndian.Uint32(triple[4:])),
			Second: math.Float32frombits(binary.LittleEndian.Uint32(triple[8:])),
		}
	}

	return list, nil
}

// SerializeSample writes the fixed-size record with encoding/binary, as
// tafla's README recommends, since it has no header or offsets.
func (Tafla) SerializeSample(sample workloads.Sample) ([]byte, error) {
	return Binary{}.SerializeSample(sample)
}

// DeserializeSample reads the fixed-size record with ByteBuffer's getters,
// which check each read's range.
func (Tafla) DeserializeSample(bytes []byte) (workloads.Sample, error) {
	buffer := tafla.NewByteBuffer(bytes, tafla.TableLayout{})
	var sample workloads.Sample

	var err error
	if sample.Key, err = buffer.GetUint32(0); err != nil {
		return workloads.Sample{}, err
	}
	if sample.A, err = buffer.GetFloat32(4); err != nil {
		return workloads.Sample{}, err
	}
	if sample.B, err = buffer.GetFloat32(8); err != nil {
		return workloads.Sample{}, err
	}

	return sample, nil
}

func checkVersion(buffer tafla.ByteBuffer) error {
	version, err := buffer.Version()
	if err != nil {
		return err
	}
	if version != documentVersion {
		return tafla.NewUnsupportedVersionError(version, documentVersion)
	}

	return nil
}
