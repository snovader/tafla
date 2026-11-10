from collections.abc import Buffer

from tafla_benchmarks.codecs.codec import Codec, DocumentWorkload, Workload
from tafla_benchmarks.generated.tafla.workloads import (
    Document,
    ItemFields,
    ItemList,
    Record,
    Sample,
    Triples,
)
from tafla_benchmarks.workloads import (
    DocumentInput,
    DocumentOutput,
    ItemInput,
    ListInput,
    RecordInput,
    SampleInput,
)


def serialize_record(input: RecordInput) -> memoryview:
    return Record.serialize(x=input.x, y=input.y, id=input.id, name=input.name)


def deserialize_record(data: Buffer) -> RecordInput:
    """Reads every field at once with ``fields()``, as an application that
    wants them all would"""
    fields = Record(data).fields()

    return RecordInput(fields["x"], fields["y"], fields["id"], fields["name"])


def serialize_document(input: DocumentInput) -> memoryview:
    return Document.serialize(
        x=input.x,
        y=input.y,
        name=input.name,
        columns={"base": input.base, "keys": input.keys, "a": input.a, "b": input.b},
    )


def deserialize_document(data: Buffer) -> DocumentOutput:
    document = Document(data)
    columns = document.columns()

    return DocumentOutput(
        document.x(),
        document.y(),
        document.name(),
        columns.base(),
        columns.keys(),
        columns.a(),
        columns.b(),
    )


def sum_a(data: Buffer) -> float:
    """Sums the elements of the column's view, as the builder codec does"""
    a = Document(data).columns().a()

    total = 0.0
    for i in range(len(a)):
        total += a[i]

    return total


def serialize_list(input: ListInput) -> memoryview:
    return ItemList.serialize(items=[_item_fields(item) for item in input.items])


def _item_fields(item: ItemInput) -> ItemFields:
    return {"value": item.value, "pair": {"first": item.first, "second": item.second}}


def deserialize_list(data: Buffer) -> ListInput:
    """Reads each item's fields at once with ``fields()``"""
    items: list[ItemInput] = []
    for item in ItemList(data).items():
        fields = item.fields()
        pair = fields["pair"]
        items.append(ItemInput(fields["value"], pair["first"], pair["second"]))

    return ListInput(items)


def serialize_structs(input: ListInput) -> memoryview:
    """Writes the items themselves, whose attributes are a ``Triple``'s
    fields, as ``TripleFields`` types them"""
    return Triples.serialize(items=input.items)


def deserialize_structs(data: Buffer) -> ListInput:
    triples = Triples(data).items()

    return ListInput([ItemInput(t.value, t.first, t.second) for t in triples])


def serialize_sample(input: SampleInput) -> bytes:
    return Sample(input.key, input.a, input.b).serialize()


def deserialize_sample(data: Buffer) -> SampleInput:
    sample = Sample.read(data)

    return SampleInput(sample.key, sample.a, sample.b)


TAFLA_GEN = Codec(
    name="tafla-gen",
    record=Workload(serialize_record, deserialize_record),
    document=DocumentWorkload(serialize_document, deserialize_document, sum_a),
    list=Workload(serialize_list, deserialize_list),
    structs=Workload(serialize_structs, deserialize_structs),
    sample=Workload(serialize_sample, deserialize_sample),
)
"""The code that tafla-gen generates from ``generated/workloads.tafla``, which
appends each message's fields to a ``TableWriter`` in one call, rather than
nesting ``TableBuilder`` instances. The record and the vector's items are read
with ``fields()``, which reads a fixed-layout table's fields at once into the
``TypedDict`` that it's written from, and the document with the getters,
columns as views.

The generated code writes from a ``TypedDict`` of each table's fields, and a
frozen dataclass of the fixed-size record, which ``serialize()`` creates from
the workloads' inputs in each call, as the protobuf codec creates its
messages. The array of structs is written from the workload's own items, which
the struct's protocol of fields types. Reading creates the workloads'
dataclasses from the generated readers' values, after the generated readers
create the structs' own."""
