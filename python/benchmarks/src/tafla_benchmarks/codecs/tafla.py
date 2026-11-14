from collections.abc import Buffer
from struct import Struct

from tafla import SIZEOF_32, ByteBuffer, TableBuilder, TableLayout
from tafla_benchmarks.codecs.codec import Codec, DocumentWorkload, Workload
from tafla_benchmarks.workloads import (
    DocumentInput,
    DocumentOutput,
    ItemInput,
    ListInput,
    RecordInput,
    SampleInput,
)

DOCUMENT_LAYOUT = TableLayout(sized=True, versioned=True)
DOCUMENT_VERSION = 1

SAMPLE_SIZE = 3 * SIZEOF_32
# The fixed-size record's fields, a uint32 and two float32s, which
# serialize_sample() packs each at its offset
UINT32 = Struct("<I")
FLOAT32 = Struct("<f")

ITEM_STRUCT = Struct("<fff")
"""An item as a struct: its three float32s, aligned to 4 in an array"""


def serialize_record(input: RecordInput) -> memoryview:
    builder = TableBuilder(4)
    builder.set_float64(input.x)
    builder.set_float64(input.y)
    builder.set_int64(input.id)
    builder.set_string(input.name)

    return builder.bytes()


def deserialize_record(data: Buffer) -> RecordInput:
    buffer = ByteBuffer(data)

    return RecordInput(
        buffer.get_float64(buffer.root_offset(0)),
        buffer.get_float64(buffer.root_offset(1)),
        buffer.get_int64(buffer.root_offset(2)),
        buffer.get_string(buffer.root_offset(3)),
    )


def serialize_document(input: DocumentInput) -> memoryview:
    """Arrays of the columns' element types are copied as bytes, and lists are
    converted as ``array()`` converts them"""
    document = TableBuilder(4, DOCUMENT_LAYOUT, DOCUMENT_VERSION)
    document.set_float64(input.x)
    document.set_float64(input.y)
    document.set_string(input.name)
    document.set_table_in_place(_set_columns, input, fields=4)

    return document.bytes()


def _set_columns(columns: TableBuilder, input: DocumentInput) -> None:
    columns.set_int64(input.base)
    columns.set_uint32_array(input.keys)
    columns.set_float32_array(input.a)
    columns.set_float32_array(input.b)


def deserialize_document(data: Buffer) -> DocumentOutput:
    buffer = _open_document(data)
    columns = buffer.root_offset(3)

    return DocumentOutput(
        buffer.get_float64(buffer.root_offset(0)),
        buffer.get_float64(buffer.root_offset(1)),
        buffer.get_string(buffer.root_offset(2)),
        buffer.get_int64(buffer.field_offset(columns, 0)),
        buffer.get_uint32_array(buffer.field_offset(columns, 1)),
        buffer.get_float32_array(buffer.field_offset(columns, 2)),
        buffer.get_float32_array(buffer.field_offset(columns, 3)),
    )


def sum_a(data: Buffer) -> float:
    """Sums the elements of the column's view, as tafla's documentation advises
    over a getter call per element"""
    buffer = _open_document(data)
    a = buffer.get_float32_array(buffer.field_offset(buffer.root_offset(3), 2))

    total = 0.0
    for i in range(len(a)):
        total += a[i]

    return total


def _open_document(data: Buffer) -> ByteBuffer:
    """Raises ``ValueError`` if the document is in another version"""
    buffer = ByteBuffer(data, DOCUMENT_LAYOUT)
    if buffer.get_version() != DOCUMENT_VERSION:
        raise ValueError(f"Unsupported document version {buffer.get_version()}")

    return buffer


def serialize_list(input: ListInput) -> memoryview:
    builder = TableBuilder(1)
    builder.set_vector_in_place(input.items, _set_item, fields=2)

    return builder.bytes()


def _set_item(item: TableBuilder, input: ItemInput) -> None:
    item.set_float32(input.value)
    item.set_table_in_place(_set_pair, input, fields=2)


def _set_pair(pair: TableBuilder, input: ItemInput) -> None:
    pair.set_float32(input.first)
    pair.set_float32(input.second)


def deserialize_list(data: Buffer) -> ListInput:
    buffer = ByteBuffer(data)
    offsets = buffer.get_vector_offsets(buffer.root_offset(0))

    items: list[ItemInput] = []
    for item in offsets:
        pair = buffer.field_offset(item, 1)
        items.append(
            ItemInput(
                buffer.get_float32(buffer.field_offset(item, 0)),
                buffer.get_float32(buffer.field_offset(pair, 0)),
                buffer.get_float32(buffer.field_offset(pair, 1)),
            )
        )

    return ListInput(items)


def serialize_structs(input: ListInput) -> memoryview:
    """Packs the structs one after another with ``struct``, as tafla-gen's code
    writes an array of them"""
    items = input.items
    size = ITEM_STRUCT.size

    data = bytearray(len(items) * size)
    for index, item in enumerate(items):
        ITEM_STRUCT.pack_into(data, index * size, item.value, item.first, item.second)

    builder = TableBuilder(1)
    builder.set_struct_array(data, size, SIZEOF_32)

    return builder.bytes()


def deserialize_structs(data: Buffer) -> ListInput:
    """Unpacks the array's view with ``struct``, as tafla-gen's code reads it"""
    buffer = ByteBuffer(data)
    structs = buffer.get_struct_array(buffer.root_offset(0), ITEM_STRUCT.size)

    return ListInput(
        [ItemInput(*values) for values in ITEM_STRUCT.iter_unpack(structs)]
    )


def serialize_sample(input: SampleInput) -> bytearray:
    """Packs each field at its offset with ``struct``, which tafla's
    documentation recommends for records at fixed offsets, as the
    TypeScript codec writes each with ``StructWriter``. The ``struct`` codec
    packs the whole record in one call."""
    data = bytearray(SAMPLE_SIZE)
    UINT32.pack_into(data, 0, input.key)
    FLOAT32.pack_into(data, SIZEOF_32, input.a)
    FLOAT32.pack_into(data, 2 * SIZEOF_32, input.b)

    return data


def deserialize_sample(data: Buffer) -> SampleInput:
    buffer = ByteBuffer(data)

    return SampleInput(
        buffer.get_uint32(0),
        buffer.get_float32(SIZEOF_32),
        buffer.get_float32(2 * SIZEOF_32),
    )


TAFLA = Codec(
    name="tafla",
    record=Workload(serialize_record, deserialize_record),
    document=DocumentWorkload(serialize_document, deserialize_document, sum_a),
    list=Workload(serialize_list, deserialize_list),
    structs=Workload(serialize_structs, deserialize_structs),
    sample=Workload(serialize_sample, deserialize_sample),
)
"""tafla's tables, nested from their builders, with columns read back as
views. The fixed-size record is packed field by field with ``struct`` and read
with ``ByteBuffer``'s getters at fixed positions, and an array of structs is
packed and unpacked with ``struct``, as tafla-gen's code does."""
