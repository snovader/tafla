package codecs

import (
	"encoding/binary"
	"errors"
	"math"

	tafla "github.com/snovader/tafla/go"
	"github.com/snovader/tafla/go/benchmarks/workloads"
	"github.com/snovader/tafla/go/prototype"
)

// TaflaPrototype writes the tables that Tafla writes, with the same calls,
// on the prototype of a TableBuilder that writes each field when it's set,
// in tafla/go/prototype. It reads them as Tafla does. It isn't one of All's
// codecs: cmd/prototype times it against Tafla.
type TaflaPrototype struct{}

func (TaflaPrototype) Name() string { return "tafla prototype" }

func (TaflaPrototype) SerializeRecord(record workloads.Record) ([]byte, error) {
	builder, err := prototype.NewTableBuilder(4, tafla.TableLayout{})
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

func (TaflaPrototype) DeserializeRecord(bytes []byte) (workloads.Record, error) {
	return Tafla{}.DeserializeRecord(bytes)
}

func (TaflaPrototype) SerializeDocument(document workloads.Document) ([]byte, error) {
	builder, err := prototype.NewVersionedTableBuilder(4, documentLayout, documentVersion)
	if err != nil {
		return nil, err
	}
	err = errors.Join(
		builder.SetFloat64(document.X),
		builder.SetFloat64(document.Y),
		builder.SetString(document.Name),
		builder.SetTableFunc(4, func(columns prototype.Item) error {
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

func (TaflaPrototype) DeserializeDocument(bytes []byte) (workloads.Document, error) {
	return Tafla{}.DeserializeDocument(bytes)
}

func (TaflaPrototype) SumA(bytes []byte) (float64, error) {
	return Tafla{}.SumA(bytes)
}

func (TaflaPrototype) SerializeList(list workloads.List) ([]byte, error) {
	builder, err := prototype.NewTableBuilder(1, tafla.TableLayout{})
	if err != nil {
		return nil, err
	}
	err = builder.SetVectorFunc(len(list.Items), 2, func(index int, item prototype.Item) error {
		return setPrototypeListItem(item, list.Items[index])
	})
	if err != nil {
		return nil, err
	}

	return builder.Bytes()
}

// setPrototypeListItem writes an item of the vector, and its pair, in place
func setPrototypeListItem(table prototype.Item, item workloads.Item) error {
	return errors.Join(
		table.SetFloat32(item.Value),
		table.SetTableFunc(2, func(pair prototype.Item) error {
			return errors.Join(pair.SetFloat32(item.First), pair.SetFloat32(item.Second))
		}),
	)
}

func (TaflaPrototype) DeserializeList(bytes []byte) (workloads.List, error) {
	return Tafla{}.DeserializeList(bytes)
}

func (TaflaPrototype) SerializeStructs(list workloads.List) ([]byte, error) {
	structs := make([]byte, 0, len(list.Items)*tripleSize)
	for _, item := range list.Items {
		structs = binary.LittleEndian.AppendUint32(structs, math.Float32bits(item.Value))
		structs = binary.LittleEndian.AppendUint32(structs, math.Float32bits(item.First))
		structs = binary.LittleEndian.AppendUint32(structs, math.Float32bits(item.Second))
	}

	builder, err := prototype.NewTableBuilder(1, tafla.TableLayout{})
	if err != nil {
		return nil, err
	}
	if err := builder.SetStructArray(structs, tripleSize, tafla.Align4); err != nil {
		return nil, err
	}

	return builder.Bytes()
}

func (TaflaPrototype) DeserializeStructs(bytes []byte) (workloads.List, error) {
	return Tafla{}.DeserializeStructs(bytes)
}

func (TaflaPrototype) SerializeSample(sample workloads.Sample) ([]byte, error) {
	return Tafla{}.SerializeSample(sample)
}

func (TaflaPrototype) DeserializeSample(bytes []byte) (workloads.Sample, error) {
	return Tafla{}.DeserializeSample(bytes)
}
