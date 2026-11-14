"""Ways to read the items of tafla's vector of nested tables and of its array of
structs, which tafla and tafla-gen write alike: the last item alone, which a
reader can find without reading the others, and every item. Each sums the three
float32s of the items it reads, so that it returns a float rather than creating
objects of its own."""

from collections.abc import Buffer, Callable
from dataclasses import dataclass, replace
from struct import Struct

from tafla import ByteBuffer
from tafla_benchmarks.generated.tafla.workloads import ItemList, Triple, Triples
from tafla_benchmarks.workloads import ItemInput, ListInput


@dataclass(frozen=True, slots=True)
class ItemAccess:
    name: str
    last_table: Callable[[Buffer], float]
    every_table: Callable[[Buffer], float]
    last_struct: Callable[[Buffer], float]
    every_struct: Callable[[Buffer], float]


@dataclass(frozen=True, slots=True)
class ItemBytes:
    """The list as a vector of nested tables and as an array of structs"""

    tables: Buffer
    structs: Buffer


def write_items(input: ListInput) -> ItemBytes:
    tables = ItemList.serialize(
        items=[
            {"value": item.value, "pair": {"first": item.first, "second": item.second}}
            for item in input.items
        ]
    )
    structs = Triples.serialize(
        items=[Triple(item.value, item.first, item.second) for item in input.items]
    )

    return ItemBytes(tables, structs)


def sum_item(item: ItemInput) -> float:
    """The sum of an item's three float32s, in the order that every access adds
    them"""
    return item.value + item.first + item.second


ITEM_STRUCT = Struct("<fff")
"""An item as a struct: its three float32s"""

# ByteBuffer's getters by hand, as the tafla codec reads the list


def _last_table_by_hand(data: Buffer) -> float:
    buffer = ByteBuffer(data)
    vector = buffer.root_offset(0)
    item = buffer.get_vector_offset(vector, buffer.get_vector_length(vector) - 1)

    return _sum_table(buffer, item)


def _every_table_by_hand(data: Buffer) -> float:
    buffer = ByteBuffer(data)

    total = 0.0
    for item in buffer.get_vector_offsets(buffer.root_offset(0)):
        total += _sum_table(buffer, item)

    return total


def _sum_table(buffer: ByteBuffer, item: int) -> float:
    pair = buffer.field_offset(item, 1)

    return (
        buffer.get_float32(buffer.field_offset(item, 0))
        + buffer.get_float32(buffer.field_offset(pair, 0))
        + buffer.get_float32(buffer.field_offset(pair, 1))
    )


def _last_struct_by_hand(data: Buffer) -> float:
    buffer = ByteBuffer(data)
    structs = buffer.get_struct_array(buffer.root_offset(0), ITEM_STRUCT.size)
    value, first, second = ITEM_STRUCT.unpack_from(
        structs, len(structs) - ITEM_STRUCT.size
    )

    return value + first + second


def _every_struct_by_hand(data: Buffer) -> float:
    buffer = ByteBuffer(data)
    structs = buffer.get_struct_array(buffer.root_offset(0), ITEM_STRUCT.size)

    total = 0.0
    for value, first, second in ITEM_STRUCT.iter_unpack(structs):
        total += value + first + second

    return total


BYTE_BUFFER = ItemAccess(
    "tafla",
    _last_table_by_hand,
    _every_table_by_hand,
    _last_struct_by_hand,
    _every_struct_by_hand,
)

# tafla-gen's getters, which return views that read each item when it's asked
# for: the last as a tuple of its values, which the vector reads in one call
# and the array's struct unpacks, as the hand-written reads do, and every
# vector item as its table's reader, and every struct as a tuple


def _last_table_of_getter(data: Buffer) -> float:
    value, first, second = ItemList(data).items().item_field_values(-1)

    return value + first + second


def _every_table_of_getter(data: Buffer) -> float:
    total = 0.0
    for item in ItemList(data).items():
        pair = item.pair()
        total += item.value() + pair.first() + pair.second()

    return total


def _last_struct_of_getter(data: Buffer) -> float:
    value, first, second = Triples(data).items().item_field_values(-1)

    return value + first + second


def _every_struct_of_getter(data: Buffer) -> float:
    total = 0.0
    for value, first, second in Triples(data).items().iter_field_values():
        total += value + first + second

    return total


GETTERS = ItemAccess(
    "tafla-gen",
    _last_table_of_getter,
    _every_table_of_getter,
    _last_struct_of_getter,
    _every_struct_of_getter,
)

ACCESSES: tuple[ItemAccess, ...] = (
    BYTE_BUFFER,
    GETTERS,
    # The getters twice, an A/A pair that shows the harness's noise
    replace(GETTERS, name="tafla-gen A/A"),
)
