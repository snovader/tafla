// Package codecs encodes the workloads' shapes with tafla and with the
// libraries that the benchmarks compare it with, each as its documentation
// shows, and as the other languages' comparisons use their counterparts.
//
// Each codec writes a workload's value into new bytes, and reads it back with
// every field, sharing strings and columns with the bytes where the library
// reads them in place, and copying them otherwise.
package codecs

import "github.com/snovader/tafla/go/benchmarks/workloads"

// Codec is a library that encodes some of the workloads.
type Codec interface {
	// Name returns the library's name, which names its benchmarks.
	Name() string
}

// RecordCodec encodes the record.
type RecordCodec interface {
	Codec
	SerializeRecord(record workloads.Record) ([]byte, error)
	DeserializeRecord(bytes []byte) (workloads.Record, error)
}

// DocumentCodec encodes the documents, and reads a column one element at a
// time.
type DocumentCodec interface {
	Codec
	SerializeDocument(document workloads.Document) ([]byte, error)
	DeserializeDocument(bytes []byte) (workloads.Document, error)
	// SumA sums column a through the library's own per-element accessor,
	// in the order of workloads.SumColumn.
	SumA(bytes []byte) (float64, error)
}

// DocumentIntoCodec writes the documents into bytes that the caller keeps
// between calls, rather than into new bytes.
type DocumentIntoCodec interface {
	Codec
	// SerializeDocumentInto writes document from the start of bytes, into
	// their capacity when it has room, and returns the bytes that hold it.
	SerializeDocumentInto(document workloads.Document, bytes []byte) ([]byte, error)
}

// ListCodec encodes the vector of nested tables.
type ListCodec interface {
	Codec
	SerializeList(list workloads.List) ([]byte, error)
	DeserializeList(bytes []byte) (workloads.List, error)
}

// StructsCodec encodes the same items as an array of structs.
type StructsCodec interface {
	Codec
	SerializeStructs(list workloads.List) ([]byte, error)
	DeserializeStructs(bytes []byte) (workloads.List, error)
}

// SampleCodec encodes the fixed-size record.
type SampleCodec interface {
	Codec
	SerializeSample(sample workloads.Sample) ([]byte, error)
	DeserializeSample(bytes []byte) (workloads.Sample, error)
}

// Suite is a library that encodes every workload, as all but the fixed-size
// record's alternatives do.
type Suite interface {
	RecordCodec
	DocumentCodec
	ListCodec
	StructsCodec
	SampleCodec
}

// All returns every codec, in the order that reports list them.
func All() []Codec {
	return []Codec{Tafla{}, TaflaGen{}, FlatBuffers{}, Protobuf{}, VTProtobuf{}, MessagePack{}, Binary{}}
}
