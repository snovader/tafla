package codecs

import (
	"bytes"

	"github.com/snovader/tafla/go/benchmarks/workloads"
	"github.com/vmihailenco/msgpack/v5"
)

// MessagePack packs the workloads' own types with vmihailenco/msgpack,
// through reflection over their tags, which pack a struct as a map with its
// field names, as msgpackr packs an object, and a column element by element.
// The array of structs packs each struct, and the table that holds them, as
// an array of its fields in order, without their names, as every language's
// MessagePack codec packs that fixed shape. Each message is packed into new
// bytes of the shape's capacity, as msgpack.Marshal packs into bytes that
// grow from none.
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

// marshal packs value as msgpack.Marshal does, with a pooled encoder, into
// new bytes of capacity bytes rather than into bytes that grow from none
func marshal(value any, capacity int) ([]byte, error) {
	encoder := msgpack.GetEncoder()
	buffer := bytes.NewBuffer(make([]byte, 0, capacity))
	encoder.Reset(buffer)

	err := encoder.Encode(value)
	msgpack.PutEncoder(encoder)
	if err != nil {
		return nil, err
	}

	return buffer.Bytes(), nil
}

func (MessagePack) SerializeRecord(record workloads.Record) ([]byte, error) {
	return marshal(&record, record.Capacity())
}

func (MessagePack) DeserializeRecord(bytes []byte) (workloads.Record, error) {
	var record workloads.Record
	err := msgpack.Unmarshal(bytes, &record)
	return record, err
}

func (MessagePack) SerializeDocument(document workloads.Document) ([]byte, error) {
	return marshal(&document, document.Capacity())
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
	return marshal(&list, list.Capacity())
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

	return marshal(&array, list.StructsCapacity())
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
	return marshal(&sample, sample.Capacity())
}

func (MessagePack) DeserializeSample(bytes []byte) (workloads.Sample, error) {
	var sample workloads.Sample
	err := msgpack.Unmarshal(bytes, &sample)
	return sample, err
}
