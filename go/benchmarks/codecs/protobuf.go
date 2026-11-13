package codecs

import (
	"github.com/snovader/tafla/go/benchmarks/generated/pb"
	"github.com/snovader/tafla/go/benchmarks/workloads"
	"google.golang.org/protobuf/proto"
)

// Protobuf builds the messages that protoc-gen-go generates from
// schemas/messages.proto, in generated/pb, from the input, and writes them
// with proto.Marshal, which sizes the bytes first, and reads them with
// proto.Unmarshal. Its messages hold their strings and repeated fields, so
// building one shares the input's columns, and the decoded columns move into
// the output. protobuf has no structs, so the array of them is a repeated
// message of the three floats, which protobuf writes as fixed32s; packed
// repeated floats would make a column of each field, which the documents
// time already.
type Protobuf struct{}

func (Protobuf) Name() string { return "protobuf" }

func (Protobuf) SerializeRecord(record workloads.Record) ([]byte, error) {
	return proto.Marshal(recordMessage(record))
}

func (Protobuf) DeserializeRecord(bytes []byte) (workloads.Record, error) {
	var message pb.Record
	if err := proto.Unmarshal(bytes, &message); err != nil {
		return workloads.Record{}, err
	}

	return recordOf(&message), nil
}

func (Protobuf) SerializeDocument(document workloads.Document) ([]byte, error) {
	return proto.Marshal(documentMessage(document))
}

func (Protobuf) DeserializeDocument(bytes []byte) (workloads.Document, error) {
	var message pb.Document
	if err := proto.Unmarshal(bytes, &message); err != nil {
		return workloads.Document{}, err
	}

	return documentOf(&message)
}

func (Protobuf) SumA(bytes []byte) (float64, error) {
	var message pb.Document
	if err := proto.Unmarshal(bytes, &message); err != nil {
		return 0, err
	}

	return sumOf(&message)
}

func (Protobuf) SerializeList(list workloads.List) ([]byte, error) {
	return proto.Marshal(listMessage(list))
}

func (Protobuf) DeserializeList(bytes []byte) (workloads.List, error) {
	var message pb.List
	if err := proto.Unmarshal(bytes, &message); err != nil {
		return workloads.List{}, err
	}

	return listOf(&message)
}

func (Protobuf) SerializeStructs(list workloads.List) ([]byte, error) {
	return proto.Marshal(triplesMessage(list))
}

func (Protobuf) DeserializeStructs(bytes []byte) (workloads.List, error) {
	var message pb.Triples
	if err := proto.Unmarshal(bytes, &message); err != nil {
		return workloads.List{}, err
	}

	return triplesOf(&message), nil
}

func (Protobuf) SerializeSample(sample workloads.Sample) ([]byte, error) {
	return proto.Marshal(&pb.Sample{Key: sample.Key, A: sample.A, B: sample.B})
}

func (Protobuf) DeserializeSample(bytes []byte) (workloads.Sample, error) {
	var message pb.Sample
	if err := proto.Unmarshal(bytes, &message); err != nil {
		return workloads.Sample{}, err
	}

	return workloads.Sample{Key: message.Key, A: message.A, B: message.B}, nil
}

// VTProtobuf writes and reads the same messages as Protobuf with the
// methods that protoc-gen-go-vtproto generates, in generated/pb: MarshalVT
// and UnmarshalVT, which write and read each field without reflection. It's
// the protobuf that Go's projects that need speed use, as Vitess does.
type VTProtobuf struct{}

func (VTProtobuf) Name() string { return "vtprotobuf" }

func (VTProtobuf) SerializeRecord(record workloads.Record) ([]byte, error) {
	return recordMessage(record).MarshalVT()
}

func (VTProtobuf) DeserializeRecord(bytes []byte) (workloads.Record, error) {
	var message pb.Record
	if err := message.UnmarshalVT(bytes); err != nil {
		return workloads.Record{}, err
	}

	return recordOf(&message), nil
}

func (VTProtobuf) SerializeDocument(document workloads.Document) ([]byte, error) {
	return documentMessage(document).MarshalVT()
}

func (VTProtobuf) DeserializeDocument(bytes []byte) (workloads.Document, error) {
	var message pb.Document
	if err := message.UnmarshalVT(bytes); err != nil {
		return workloads.Document{}, err
	}

	return documentOf(&message)
}

func (VTProtobuf) SumA(bytes []byte) (float64, error) {
	var message pb.Document
	if err := message.UnmarshalVT(bytes); err != nil {
		return 0, err
	}

	return sumOf(&message)
}

func (VTProtobuf) SerializeList(list workloads.List) ([]byte, error) {
	return listMessage(list).MarshalVT()
}

func (VTProtobuf) DeserializeList(bytes []byte) (workloads.List, error) {
	var message pb.List
	if err := message.UnmarshalVT(bytes); err != nil {
		return workloads.List{}, err
	}

	return listOf(&message)
}

func (VTProtobuf) SerializeStructs(list workloads.List) ([]byte, error) {
	return triplesMessage(list).MarshalVT()
}

func (VTProtobuf) DeserializeStructs(bytes []byte) (workloads.List, error) {
	var message pb.Triples
	if err := message.UnmarshalVT(bytes); err != nil {
		return workloads.List{}, err
	}

	return triplesOf(&message), nil
}

func (VTProtobuf) SerializeSample(sample workloads.Sample) ([]byte, error) {
	return (&pb.Sample{Key: sample.Key, A: sample.A, B: sample.B}).MarshalVT()
}

func (VTProtobuf) DeserializeSample(bytes []byte) (workloads.Sample, error) {
	var message pb.Sample
	if err := message.UnmarshalVT(bytes); err != nil {
		return workloads.Sample{}, err
	}

	return workloads.Sample{Key: message.Key, A: message.A, B: message.B}, nil
}

func recordMessage(record workloads.Record) *pb.Record {
	return &pb.Record{X: record.X, Y: record.Y, Id: record.ID, Name: record.Name}
}

func recordOf(message *pb.Record) workloads.Record {
	return workloads.Record{X: message.X, Y: message.Y, ID: message.Id, Name: message.Name}
}

func documentMessage(document workloads.Document) *pb.Document {
	return &pb.Document{
		X:    document.X,
		Y:    document.Y,
		Name: document.Name,
		Columns: &pb.Columns{
			Base: document.Base,
			Keys: document.Keys,
			A:    document.A,
			B:    document.B,
		},
	}
}

// documentOf returns a document's fields, with its columns moved out of the
// message, which a field that the bytes leave out fails
func documentOf(message *pb.Document) (workloads.Document, error) {
	columns := message.Columns
	if columns == nil {
		return workloads.Document{}, errMissing
	}

	return workloads.Document{
		X:    message.X,
		Y:    message.Y,
		Name: message.Name,
		Base: columns.Base,
		Keys: columns.Keys,
		A:    columns.A,
		B:    columns.B,
	}, nil
}

func sumOf(message *pb.Document) (float64, error) {
	if message.Columns == nil {
		return 0, errMissing
	}

	sum := 0.0
	for _, value := range message.Columns.A {
		sum += float64(value)
	}

	return sum, nil
}

func listMessage(list workloads.List) *pb.List {
	items := make([]*pb.Item, len(list.Items))
	for index, item := range list.Items {
		items[index] = &pb.Item{Value: item.Value, Pair: &pb.Pair{First: item.First, Second: item.Second}}
	}

	return &pb.List{Items: items}
}

func listOf(message *pb.List) (workloads.List, error) {
	list := workloads.List{Items: make([]workloads.Item, len(message.Items))}
	for index, item := range message.Items {
		if item.Pair == nil {
			return workloads.List{}, errMissing
		}
		list.Items[index] = workloads.Item{Value: item.Value, First: item.Pair.First, Second: item.Pair.Second}
	}

	return list, nil
}

func triplesMessage(list workloads.List) *pb.Triples {
	items := make([]*pb.Triple, len(list.Items))
	for index, item := range list.Items {
		items[index] = &pb.Triple{Value: item.Value, First: item.First, Second: item.Second}
	}

	return &pb.Triples{Items: items}
}

func triplesOf(message *pb.Triples) workloads.List {
	list := workloads.List{Items: make([]workloads.Item, len(message.Items))}
	for index, item := range message.Items {
		list.Items[index] = workloads.Item{Value: item.Value, First: item.First, Second: item.Second}
	}

	return list
}
