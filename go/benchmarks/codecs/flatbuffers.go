package codecs

import (
	"errors"

	flatbuffers "github.com/google/flatbuffers/go"
	"github.com/snovader/tafla/go/benchmarks/generated/fbs"
	"github.com/snovader/tafla/go/benchmarks/workloads"
)

// FlatBuffers builds each message with a new Builder, which starts empty and
// doubles as it grows, and each table with the functions that flatc --go
// generates, in generated/fbs. Columns are written element by element, in
// reverse, as the builder writes vectors back to front, and so is the vector
// of structs. A message is the builder's bytes from where it finished, from
// FinishedBytes, without a copy.
//
// Readers read the root with GetRootAs, which Go's FlatBuffers doesn't
// verify, and copy columns through their per-element accessors, and strings
// into Go strings.
type FlatBuffers struct{}

var errMissing = errors.New("a field is missing")

func (FlatBuffers) Name() string { return "flatbuffers" }

func (FlatBuffers) SerializeRecord(record workloads.Record) ([]byte, error) {
	builder := flatbuffers.NewBuilder(0)
	name := builder.CreateString(record.Name)

	fbs.RecordStart(builder)
	fbs.RecordAddX(builder, record.X)
	fbs.RecordAddY(builder, record.Y)
	fbs.RecordAddId(builder, record.ID)
	fbs.RecordAddName(builder, name)
	builder.Finish(fbs.RecordEnd(builder))

	return builder.FinishedBytes(), nil
}

func (FlatBuffers) DeserializeRecord(bytes []byte) (workloads.Record, error) {
	record := fbs.GetRootAsRecord(bytes, 0)

	return workloads.Record{
		X:    record.X(),
		Y:    record.Y(),
		ID:   record.Id(),
		Name: string(record.Name()),
	}, nil
}

func (FlatBuffers) SerializeDocument(document workloads.Document) ([]byte, error) {
	builder := flatbuffers.NewBuilder(0)
	name := builder.CreateString(document.Name)

	fbs.ColumnsStartKeysVector(builder, len(document.Keys))
	for index := len(document.Keys) - 1; index >= 0; index-- {
		builder.PrependUint32(document.Keys[index])
	}
	keys := builder.EndVector(len(document.Keys))
	a := float32Vector(builder, fbs.ColumnsStartAVector, document.A)
	b := float32Vector(builder, fbs.ColumnsStartBVector, document.B)

	fbs.ColumnsStart(builder)
	fbs.ColumnsAddBase(builder, document.Base)
	fbs.ColumnsAddKeys(builder, keys)
	fbs.ColumnsAddA(builder, a)
	fbs.ColumnsAddB(builder, b)
	columns := fbs.ColumnsEnd(builder)

	fbs.DocumentStart(builder)
	fbs.DocumentAddX(builder, document.X)
	fbs.DocumentAddY(builder, document.Y)
	fbs.DocumentAddName(builder, name)
	fbs.DocumentAddColumns(builder, columns)
	builder.Finish(fbs.DocumentEnd(builder))

	return builder.FinishedBytes(), nil
}

// float32Vector writes a vector of float32s, back to front
func float32Vector(
	builder *flatbuffers.Builder,
	start func(*flatbuffers.Builder, int) flatbuffers.UOffsetT,
	values []float32,
) flatbuffers.UOffsetT {
	start(builder, len(values))
	for index := len(values) - 1; index >= 0; index-- {
		builder.PrependFloat32(values[index])
	}

	return builder.EndVector(len(values))
}

func (FlatBuffers) DeserializeDocument(bytes []byte) (workloads.Document, error) {
	document := fbs.GetRootAsDocument(bytes, 0)
	columns := document.Columns(nil)
	if columns == nil {
		return workloads.Document{}, errMissing
	}

	keys := make([]uint32, columns.KeysLength())
	for index := range keys {
		keys[index] = columns.Keys(index)
	}
	a := make([]float32, columns.ALength())
	for index := range a {
		a[index] = columns.A(index)
	}
	b := make([]float32, columns.BLength())
	for index := range b {
		b[index] = columns.B(index)
	}

	return workloads.Document{
		X:    document.X(),
		Y:    document.Y(),
		Name: string(document.Name()),
		Base: columns.Base(),
		Keys: keys,
		A:    a,
		B:    b,
	}, nil
}

func (FlatBuffers) SumA(bytes []byte) (float64, error) {
	columns := fbs.GetRootAsDocument(bytes, 0).Columns(nil)
	if columns == nil {
		return 0, errMissing
	}

	sum := 0.0
	for index := range columns.ALength() {
		sum += float64(columns.A(index))
	}

	return sum, nil
}

func (FlatBuffers) SerializeList(list workloads.List) ([]byte, error) {
	builder := flatbuffers.NewBuilder(0)

	items := make([]flatbuffers.UOffsetT, len(list.Items))
	for index, item := range list.Items {
		fbs.PairStart(builder)
		fbs.PairAddFirst(builder, item.First)
		fbs.PairAddSecond(builder, item.Second)
		pair := fbs.PairEnd(builder)

		fbs.ItemStart(builder)
		fbs.ItemAddValue(builder, item.Value)
		fbs.ItemAddPair(builder, pair)
		items[index] = fbs.ItemEnd(builder)
	}

	fbs.ListStartItemsVector(builder, len(items))
	for index := len(items) - 1; index >= 0; index-- {
		builder.PrependUOffsetT(items[index])
	}
	vector := builder.EndVector(len(items))

	fbs.ListStart(builder)
	fbs.ListAddItems(builder, vector)
	builder.Finish(fbs.ListEnd(builder))

	return builder.FinishedBytes(), nil
}

func (FlatBuffers) DeserializeList(bytes []byte) (workloads.List, error) {
	root := fbs.GetRootAsList(bytes, 0)

	list := workloads.List{Items: make([]workloads.Item, root.ItemsLength())}
	var item fbs.Item
	var pair fbs.Pair
	for index := range list.Items {
		if !root.Items(&item, index) || item.Pair(&pair) == nil {
			return workloads.List{}, errMissing
		}
		list.Items[index] = workloads.Item{Value: item.Value(), First: pair.First(), Second: pair.Second()}
	}

	return list, nil
}

func (FlatBuffers) SerializeStructs(list workloads.List) ([]byte, error) {
	builder := flatbuffers.NewBuilder(0)

	fbs.TriplesStartItemsVector(builder, len(list.Items))
	for index := len(list.Items) - 1; index >= 0; index-- {
		item := list.Items[index]
		fbs.CreateTriple(builder, item.Value, item.First, item.Second)
	}
	vector := builder.EndVector(len(list.Items))

	fbs.TriplesStart(builder)
	fbs.TriplesAddItems(builder, vector)
	builder.Finish(fbs.TriplesEnd(builder))

	return builder.FinishedBytes(), nil
}

func (FlatBuffers) DeserializeStructs(bytes []byte) (workloads.List, error) {
	root := fbs.GetRootAsTriples(bytes, 0)

	list := workloads.List{Items: make([]workloads.Item, root.ItemsLength())}
	var triple fbs.Triple
	for index := range list.Items {
		if !root.Items(&triple, index) {
			return workloads.List{}, errMissing
		}
		list.Items[index] = workloads.Item{Value: triple.Value(), First: triple.First(), Second: triple.Second()}
	}

	return list, nil
}

func (FlatBuffers) SerializeSample(sample workloads.Sample) ([]byte, error) {
	builder := flatbuffers.NewBuilder(0)

	fbs.SampleStart(builder)
	fbs.SampleAddKey(builder, sample.Key)
	fbs.SampleAddA(builder, sample.A)
	fbs.SampleAddB(builder, sample.B)
	builder.Finish(fbs.SampleEnd(builder))

	return builder.FinishedBytes(), nil
}

func (FlatBuffers) DeserializeSample(bytes []byte) (workloads.Sample, error) {
	sample := fbs.GetRootAsSample(bytes, 0)

	return workloads.Sample{Key: sample.Key(), A: sample.A(), B: sample.B()}, nil
}
