from collections.abc import Buffer

from tafla_benchmarks.codecs.codec import Codec, DocumentWorkload, Workload
from tafla_benchmarks.generated.tafla.workloads import (
    Document,
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


def serialize_record(input: RecordInput) -> bytes:
    """Writes the fields' values in their order, as ``serialize_values()``
    takes them, into bytes of the record's own in one call"""
    return Record.serialize_values(input.x, input.y, input.id, input.name)


def deserialize_record(data: Buffer) -> RecordInput:
    """Reads every field's value at once with ``read_values()``, without a
    reader, as an application that wants them all would"""
    return RecordInput(*Record.read_values(data))


def serialize_document(input: DocumentInput) -> memoryview:
    return Document.serialize(
        x=input.x,
        y=input.y,
        name=input.name,
        columns={"base": input.base, "keys": input.keys, "a": input.a, "b": input.b},
    )


def deserialize_document(data: Buffer) -> DocumentOutput:
    """Reads every field's value at once with ``read_values()``, without a
    reader, the nested columns' as a tuple of their own"""
    x, y, name, (base, keys, a, b) = Document.read_values(data)

    return DocumentOutput(x, y, name, base, keys, a, b)


def sum_a(data: Buffer) -> float:
    """Sums the elements of the column's view, as the builder codec does"""
    a = Document(data).columns().a()

    total = 0.0
    for i in range(len(a)):
        total += a[i]

    return total


def serialize_list(input: ListInput) -> memoryview:
    """Writes the items' values, each pair's in place of it, as the view's
    ``field_values()`` reads them, with ``serialize_values()``"""
    items = [(item.value, item.first, item.second) for item in input.items]

    return ItemList.serialize_values(items)


def deserialize_list(data: Buffer) -> ListInput:
    """Reads every item's values at once with the view's ``field_values()``,
    each nested pair's in place of it"""
    values = ItemList(data).items().field_values()

    return ListInput([ItemInput(*item) for item in values])


def serialize_structs(input: ListInput) -> memoryview:
    """Writes the items themselves, whose attributes are a ``Triple``'s
    fields, as ``TripleFields`` types them"""
    return Triples.serialize(items=input.items)


def deserialize_structs(data: Buffer) -> ListInput:
    """Reads each struct's values with the view's ``iter_field_values()``,
    rather than into a ``Triple`` each"""
    values = Triples(data).items().iter_field_values()

    return ListInput([ItemInput(*triple) for triple in values])


def serialize_sample(input: SampleInput) -> bytes:
    return Sample.serialize_values(input.key, input.a, input.b)


def deserialize_sample(data: Buffer) -> SampleInput:
    return SampleInput(*Sample.read_values(data))


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
nesting ``TableBuilder`` instances, and a vector of fixed-layout tables in one
call too.

The record and the fixed-size record are written from their values, as
positional arguments, and read into tuples of them, with ``serialize_values()``
and ``read_values()``, the record into bytes of its own and from them in one
call each, without a writer or a reader. The document is written from a
``TypedDict`` of each table's fields, and read into a tuple of its values with
``read_values()``, its columns' in a tuple of their own, as views. The vector
is written from a tuple of each item's values, each pair's in place of it,
with ``serialize_values()``, and read into them with the ``field_values()``
of the view that its getter returns, which reads every item's values at
once. The array of structs is written from the
workload's own items, which the struct's protocol of fields types, and read
with its view's ``iter_field_values()``, into a tuple of each struct's values
rather than a struct.
Reading creates the workloads' dataclasses from the values that the generated
readers return."""
