package codecs

import (
	gen "github.com/snovader/tafla/go/benchmarks/generated/tafla"
	"github.com/snovader/tafla/go/benchmarks/workloads"
)

// TaflaGen writes and reads tafla's tables with the code that tafla-gen
// generates from workloads.tafla, in generated/tafla. A message's
// MarshalBinary writes it into bytes of its exact length, in one allocation,
// and lays out the fields whose sizes the schema fixes itself, as it lays out
// structs: the record, whose only other field is a string, into the bytes
// that tafla.AppendTable returns, without a sizer and a writer, and the
// others with a TableSizer and a TableWriter, which take the document's
// first scalars together and each of the vector's items whole. Its readers
// share strings and columns with the bytes, as tafla's do.
//
// It takes the workloads' types, as the other codecs do, so it collects the
// generated fields of the vector's items and the generated structs of the
// array into a slice in each call, as the protobuf codecs collect their
// messages. The getters of the vector and of the array return views, which
// read each item when it's asked for, and the codec reads every item through
// their All().
type TaflaGen struct{}

func (TaflaGen) Name() string { return "tafla-gen" }

func (TaflaGen) SerializeRecord(record workloads.Record) ([]byte, error) {
	return gen.RecordFields{X: record.X, Y: record.Y, Id: record.ID, Name: record.Name}.MarshalBinary()
}

func (TaflaGen) DeserializeRecord(bytes []byte) (workloads.Record, error) {
	reader := gen.NewRecord(bytes)

	x, err := reader.X()
	if err != nil {
		return workloads.Record{}, err
	}
	y, err := reader.Y()
	if err != nil {
		return workloads.Record{}, err
	}
	id, err := reader.Id()
	if err != nil {
		return workloads.Record{}, err
	}
	name, err := reader.Name()
	if err != nil {
		return workloads.Record{}, err
	}

	return workloads.Record{X: x, Y: y, ID: id, Name: name}, nil
}

func (TaflaGen) SerializeDocument(document workloads.Document) ([]byte, error) {
	return gen.DocumentFields{
		X:    document.X,
		Y:    document.Y,
		Name: document.Name,
		Columns: gen.ColumnsFields{
			Base: document.Base,
			Keys: document.Keys,
			A:    document.A,
			B:    document.B,
		},
	}.MarshalBinary()
}

func (TaflaGen) DeserializeDocument(bytes []byte) (workloads.Document, error) {
	reader, err := gen.NewDocument(bytes)
	if err != nil {
		return workloads.Document{}, err
	}

	x, err := reader.X()
	if err != nil {
		return workloads.Document{}, err
	}
	y, err := reader.Y()
	if err != nil {
		return workloads.Document{}, err
	}
	name, err := reader.Name()
	if err != nil {
		return workloads.Document{}, err
	}
	columns, err := reader.Columns()
	if err != nil {
		return workloads.Document{}, err
	}
	base, err := columns.Base()
	if err != nil {
		return workloads.Document{}, err
	}
	keys, err := columns.Keys()
	if err != nil {
		return workloads.Document{}, err
	}
	a, err := columns.A()
	if err != nil {
		return workloads.Document{}, err
	}
	b, err := columns.B()
	if err != nil {
		return workloads.Document{}, err
	}

	return workloads.Document{
		X:    x,
		Y:    y,
		Name: name,
		Base: base,
		Keys: keys.Slice(),
		A:    a.Slice(),
		B:    b.Slice(),
	}, nil
}

func (TaflaGen) SumA(bytes []byte) (float64, error) {
	reader, err := gen.NewDocument(bytes)
	if err != nil {
		return 0, err
	}
	columns, err := reader.Columns()
	if err != nil {
		return 0, err
	}
	a, err := columns.A()
	if err != nil {
		return 0, err
	}

	sum := 0.0
	for value := range a.Values() {
		sum += float64(value)
	}

	return sum, nil
}

func (TaflaGen) SerializeList(list workloads.List) ([]byte, error) {
	items := make([]gen.ItemFields, len(list.Items))
	for index, item := range list.Items {
		items[index] = gen.ItemFields{
			Value: item.Value,
			Pair:  gen.PairFields{First: item.First, Second: item.Second},
		}
	}

	return gen.ItemListFields{Items: items}.MarshalBinary()
}

func (TaflaGen) DeserializeList(bytes []byte) (workloads.List, error) {
	items, err := gen.NewItemList(bytes).Items()
	if err != nil {
		return workloads.List{}, err
	}

	list := workloads.List{Items: make([]workloads.Item, 0, items.Len())}
	for item, err := range items.All() {
		if err != nil {
			return workloads.List{}, err
		}
		read, err := readItem(item)
		if err != nil {
			return workloads.List{}, err
		}
		list.Items = append(list.Items, read)
	}

	return list, nil
}

// readItem reads an item of the vector, and its pair
func readItem(item gen.Item) (workloads.Item, error) {
	value, err := item.Value()
	if err != nil {
		return workloads.Item{}, err
	}
	pair, err := item.Pair()
	if err != nil {
		return workloads.Item{}, err
	}
	first, err := pair.First()
	if err != nil {
		return workloads.Item{}, err
	}
	second, err := pair.Second()
	if err != nil {
		return workloads.Item{}, err
	}

	return workloads.Item{Value: value, First: first, Second: second}, nil
}

func (TaflaGen) SerializeStructs(list workloads.List) ([]byte, error) {
	items := make([]gen.Triple, len(list.Items))
	for index, item := range list.Items {
		items[index] = gen.Triple{Value: item.Value, First: item.First, Second: item.Second}
	}

	return gen.TriplesFields{Items: items}.MarshalBinary()
}

func (TaflaGen) DeserializeStructs(bytes []byte) (workloads.List, error) {
	items, err := gen.NewTriples(bytes).Items()
	if err != nil {
		return workloads.List{}, err
	}

	list := workloads.List{Items: make([]workloads.Item, items.Len())}
	for index, triple := range items.All() {
		list.Items[index] = workloads.Item{Value: triple.Value, First: triple.First, Second: triple.Second}
	}

	return list, nil
}

func (TaflaGen) SerializeSample(sample workloads.Sample) ([]byte, error) {
	return gen.Sample{Key: sample.Key, A: sample.A, B: sample.B}.MarshalBinary()
}

func (TaflaGen) DeserializeSample(bytes []byte) (workloads.Sample, error) {
	var sample gen.Sample
	if err := sample.UnmarshalBinary(bytes); err != nil {
		return workloads.Sample{}, err
	}

	return workloads.Sample{Key: sample.Key, A: sample.A, B: sample.B}, nil
}
