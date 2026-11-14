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
// first scalars together and each of the vector's items whole. It reads the
// record, the document and each of the vector's items whole, with their
// Fields(), which shares strings and columns with the bytes, as tafla's
// readers do.
//
// It takes the workloads' types, as the other codecs do, so it collects the
// generated fields of the vector's items and the generated structs of the
// array into a slice in each call, as the protobuf codecs collect their
// messages. The getters of the vector and of the array return views, which
// read each item when it's asked for, and the codec reads every item through
// their All(), each of the vector's items with its Fields().
type TaflaGen struct{}

func (TaflaGen) Name() string { return "tafla-gen" }

func (TaflaGen) SerializeRecord(record workloads.Record) ([]byte, error) {
	return gen.RecordFields{X: record.X, Y: record.Y, Id: record.ID, Name: record.Name}.MarshalBinary()
}

func (TaflaGen) DeserializeRecord(bytes []byte) (workloads.Record, error) {
	fields, err := gen.NewRecord(bytes).Fields()
	if err != nil {
		return workloads.Record{}, err
	}

	return workloads.Record{X: fields.X, Y: fields.Y, ID: fields.Id, Name: fields.Name}, nil
}

func (TaflaGen) SerializeDocument(document workloads.Document) ([]byte, error) {
	return documentFields(document).MarshalBinary()
}

// SerializeDocumentInto appends the document to bytes from their start,
// with the generated AppendBinary, which writes it into their capacity when
// it has room, without allocating or zeroing new bytes.
func (TaflaGen) SerializeDocumentInto(document workloads.Document, bytes []byte) ([]byte, error) {
	return documentFields(document).AppendBinary(bytes[:0])
}

// documentFields are the generated fields of a document, which share its
// string and columns
func documentFields(document workloads.Document) gen.DocumentFields {
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
	}
}

func (TaflaGen) DeserializeDocument(bytes []byte) (workloads.Document, error) {
	reader, err := gen.NewDocument(bytes)
	if err != nil {
		return workloads.Document{}, err
	}
	fields, err := reader.Fields()
	if err != nil {
		return workloads.Document{}, err
	}

	columns := fields.Columns
	return workloads.Document{
		X:    fields.X,
		Y:    fields.Y,
		Name: fields.Name,
		Base: columns.Base,
		Keys: columns.Keys,
		A:    columns.A,
		B:    columns.B,
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
		fields, err := item.Fields()
		if err != nil {
			return workloads.List{}, err
		}
		list.Items = append(list.Items, workloads.Item{
			Value:  fields.Value,
			First:  fields.Pair.First,
			Second: fields.Pair.Second,
		})
	}

	return list, nil
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

// DeserializeSample converts the generated Sample, whose fields are the
// workload's, without checking the error, since ReadSample returns the zero
// Sample with it. That keeps the method within Go's inlining budget, so that
// it inlines into the wrapper that the codec's interface calls, as Binary's
// does, rather than costing a second call.
func (TaflaGen) DeserializeSample(bytes []byte) (workloads.Sample, error) {
	sample, err := gen.ReadSample(bytes)
	return workloads.Sample(sample), err
}
