package codecs

import (
	"github.com/snovader/tafla/go/benchmarks/workloads"
	"github.com/vmihailenco/msgpack/v5"
)

// MessagePack packs the workloads' own types with vmihailenco/msgpack,
// through reflection over their tags, which pack a struct as a map with its
// field names, as msgpackr packs an object, and a column element by element.
// The array of structs packs each struct, and the table that holds them, as
// an array of its fields in order, without their names, as every language's
// MessagePack codec packs that fixed shape.
type MessagePack struct{}

// itemArray and listArray pack the array of structs as arrays
type itemArray struct {
	//lint:ignore U1000 msgpack reads the struct's tag from it
	_msgpack struct{} `msgpack:",as_array"`
	Value    float32
	First    float32
	Second   float32
}

type listArray struct {
	//lint:ignore U1000 msgpack reads the struct's tag from it
	_msgpack struct{} `msgpack:",as_array"`
	Items    []itemArray
}

func (MessagePack) Name() string { return "msgpack" }

func (MessagePack) SerializeRecord(record workloads.Record) ([]byte, error) {
	return msgpack.Marshal(&record)
}

func (MessagePack) DeserializeRecord(bytes []byte) (workloads.Record, error) {
	var record workloads.Record
	err := msgpack.Unmarshal(bytes, &record)
	return record, err
}

func (MessagePack) SerializeDocument(document workloads.Document) ([]byte, error) {
	return msgpack.Marshal(&document)
}

func (MessagePack) DeserializeDocument(bytes []byte) (workloads.Document, error) {
	var document workloads.Document
	err := msgpack.Unmarshal(bytes, &document)
	return document, err
}

func (MessagePack) SumA(bytes []byte) (float64, error) {
	var document workloads.Document
	if err := msgpack.Unmarshal(bytes, &document); err != nil {
		return 0, err
	}

	return workloads.SumColumn(document.A), nil
}

func (MessagePack) SerializeList(list workloads.List) ([]byte, error) {
	return msgpack.Marshal(&list)
}

func (MessagePack) DeserializeList(bytes []byte) (workloads.List, error) {
	var list workloads.List
	err := msgpack.Unmarshal(bytes, &list)
	return list, err
}

func (MessagePack) SerializeStructs(list workloads.List) ([]byte, error) {
	array := listArray{Items: make([]itemArray, len(list.Items))}
	for index, item := range list.Items {
		array.Items[index] = itemArray{Value: item.Value, First: item.First, Second: item.Second}
	}

	return msgpack.Marshal(&array)
}

func (MessagePack) DeserializeStructs(bytes []byte) (workloads.List, error) {
	var array listArray
	if err := msgpack.Unmarshal(bytes, &array); err != nil {
		return workloads.List{}, err
	}

	list := workloads.List{Items: make([]workloads.Item, len(array.Items))}
	for index, item := range array.Items {
		list.Items[index] = workloads.Item{Value: item.Value, First: item.First, Second: item.Second}
	}

	return list, nil
}

func (MessagePack) SerializeSample(sample workloads.Sample) ([]byte, error) {
	return msgpack.Marshal(&sample)
}

func (MessagePack) DeserializeSample(bytes []byte) (workloads.Sample, error) {
	var sample workloads.Sample
	err := msgpack.Unmarshal(bytes, &sample)
	return sample, err
}
