from collections.abc import Buffer

from google.protobuf.internal import api_implementation

from tafla_benchmarks.codecs.codec import Codec, DocumentWorkload, Workload
from tafla_benchmarks.generated import messages_pb2
from tafla_benchmarks.workloads import (
    DocumentInput,
    DocumentOutput,
    ItemInput,
    ListInput,
    RecordInput,
    SampleInput,
)

IMPLEMENTATION_VARIABLE = "PROTOCOL_BUFFERS_PYTHON_IMPLEMENTATION"
"""The environment variable that selects protobuf's implementation, which
protobuf reads when a process first imports it: ``python`` selects its
pure-Python implementation, rather than upb, which its wheels use"""

PROTOBUF_NAMES = {"upb": "protobuf", "python": "protobuf (pure)"}
"""The codec's names by protobuf's implementation"""


def serialize_record(input: RecordInput) -> bytes:
    return messages_pb2.Record(
        x=input.x, y=input.y, id=input.id, name=input.name
    ).SerializeToString()


def deserialize_record(data: Buffer) -> RecordInput:
    record = messages_pb2.Record.FromString(data)

    return RecordInput(record.x, record.y, record.id, record.name)


def serialize_document(input: DocumentInput) -> bytes:
    document = messages_pb2.Document(x=input.x, y=input.y, name=input.name)

    columns = document.columns
    columns.base = input.base
    columns.keys.extend(input.keys)
    columns.a.extend(input.a)
    columns.b.extend(input.b)

    return document.SerializeToString()


def deserialize_document(data: Buffer) -> DocumentOutput:
    """Reads the columns as the message's repeated fields"""
    document = messages_pb2.Document.FromString(data)
    columns = document.columns

    return DocumentOutput(
        document.x,
        document.y,
        document.name,
        columns.base,
        columns.keys,
        columns.a,
        columns.b,
    )


def sum_a(data: Buffer) -> float:
    a = messages_pb2.Document.FromString(data).columns.a

    total = 0.0
    for i in range(len(a)):
        total += a[i]

    return total


def serialize_list(input: ListInput) -> bytes:
    vector = messages_pb2.List()

    items = vector.items
    for item in input.items:
        pair = items.add(value=item.value).pair
        pair.first = item.first
        pair.second = item.second

    return vector.SerializeToString()


def deserialize_list(data: Buffer) -> ListInput:
    items: list[ItemInput] = []
    for item in messages_pb2.List.FromString(data).items:
        pair = item.pair
        items.append(ItemInput(item.value, pair.first, pair.second))

    return ListInput(items)


def serialize_structs(input: ListInput) -> bytes:
    triples = messages_pb2.Triples()

    items = triples.items
    for item in input.items:
        items.add(value=item.value, first=item.first, second=item.second)

    return triples.SerializeToString()


def deserialize_structs(data: Buffer) -> ListInput:
    items: list[ItemInput] = []
    for triple in messages_pb2.Triples.FromString(data).items:
        items.append(ItemInput(triple.value, triple.first, triple.second))

    return ListInput(items)


def serialize_sample(input: SampleInput) -> bytes:
    return messages_pb2.Sample(key=input.key, a=input.a, b=input.b).SerializeToString()


def deserialize_sample(data: Buffer) -> SampleInput:
    sample = messages_pb2.Sample.FromString(data)

    return SampleInput(sample.key, sample.a, sample.b)


def _name() -> str:
    """Names the codec after the implementation that this process imported"""
    implementation = api_implementation.Type()
    return PROTOBUF_NAMES.get(implementation, f"protobuf ({implementation})")


PROTOBUF = Codec(
    name=_name(),
    record=Workload(serialize_record, deserialize_record),
    document=DocumentWorkload(serialize_document, deserialize_document, sum_a),
    list=Workload(serialize_list, deserialize_list),
    structs=Workload(serialize_structs, deserialize_structs),
    sample=Workload(serialize_sample, deserialize_sample),
)
"""protoc's generated messages, parsed with ``FromString()``, in the
implementation of protobuf that this process imported, which the codec's name
says: the upb runtime that protobuf's wheels use, or its pure-Python
implementation, which ``PROTOCOL_BUFFERS_PYTHON_IMPLEMENTATION=python`` selects,
and which writes the same bytes. Messages are built from their fields, and
nested messages in place, as protobuf's tutorial builds them: passing a nested
message to its parent's constructor copies it, which made writing the vector of
nested tables 2.6 times slower, and the large document 13% slower, in upb.
Repeated fields are converted element by element when a message is built, and
read back as the message's containers. protobuf has no structs, so an array of
them is a repeated message of the three floats, whose fields protobuf writes as
fixed32s. Packed repeated floats would be a column for each field, which the
documents time already."""
