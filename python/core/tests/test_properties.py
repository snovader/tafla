"""The properties that every implementation's tests share, of tables built from
descriptions in the shape of the conformance suite's: they read back the
values they were built with, follow the format's rules, read untrusted bytes
as values or fail with ``DecodeError``, and their builders reject what they
can't write."""

import struct
import unittest
from collections.abc import Callable, Iterator
from contextlib import suppress
from dataclasses import dataclass
from functools import partial
from typing import NamedTuple

from hypothesis import assume, given
from hypothesis.strategies import (
    DataObject,
    DrawFn,
    SearchStrategy,
    binary,
    booleans,
    characters,
    composite,
    data,
    integers,
    lists,
    one_of,
    sampled_from,
    tuples,
)

from tafla import DecodeError, TableReader
from tests.descriptions import (
    LAYOUTS,
    OFFSET_TYPES,
    SCALAR_VALUES,
    STRINGS,
    STRUCT_TYPES,
    longest_length,
    tables,
)
from tests.tables import (
    ELEMENT_SIZES,
    FORMATS,
    LONGEST,
    Description,
    DescriptionReader,
    MismatchError,
    OffsetType,
    Placement,
    alignment_of,
    build_table,
    comparable_field,
    comparable_table,
    create_builder,
    from_json,
    is_typed_array,
    layout_of,
    offset_size_of,
    read_table,
    set_field,
    struct_alignment,
    struct_of,
)


def placements_of(data: memoryview[int], table: Description) -> list[Placement]:
    """Where a reader finds every field of a table, nested ones included"""
    placements: list[Placement] = []
    read_table(data, table, placements.append)

    return placements


def unsigned(data: memoryview[int], position: int, size: int) -> int:
    """Reads an offset, count or length from the bytes, as the format stores
    them, without tafla"""
    return int.from_bytes(data[position : position + size], "little")


def vector_ends(placement: Placement) -> list[int]:
    """Returns the end of each item of the vector at a placement, from the
    vector's position"""
    data, position, size = placement.data, placement.position, placement.offset_size
    count = unsigned(data, position, size)

    return [unsigned(data, position + i * size, size) for i in range(1, count + 1)]


def vector_items(placement: Placement) -> list[memoryview[int]]:
    """Returns the bytes of each item of the vector at a placement, from the
    end of the item before it, or of the ends, to its own"""
    ends = vector_ends(placement)
    starts = [(len(ends) + 1) * placement.offset_size, *ends]

    position = placement.position
    return [
        placement.data[position + start : position + end]
        for start, end in zip(starts, ends, strict=False)
    ]


def tables_in_own_bytes(
    data: memoryview[int], table: Description
) -> Iterator[tuple[memoryview[int], Description]]:
    """The table and every vector item in it, at any depth, with their bytes,
    which hold them alone"""
    yield data, table

    for placement in placements_of(data, table):
        if placement.field["type"] == "vector":
            items = [item["table"] for item in placement.field["items"]]
            yield from zip(vector_items(placement), items, strict=True)


def read_corrupted(data: bytes, table: Description) -> None:
    """Reads the header and every field of the bytes as the table's
    description sets them out. Each read returns values or raises
    ``DecodeError``, or ``MismatchError`` when the values don't have the
    description's shape; anything else propagates."""
    layout = table["layout"]
    reader = TableReader(data, layout_of(layout))
    description_reader = DescriptionReader(data, layout)

    reads: list[Callable[[], object]] = [reader.get_size]
    if layout["versioned"]:
        reads.append(reader.get_version)

    for index, field in enumerate(table["fields"]):
        reads.append(partial(description_reader.read_root_field, index, field))

    for read in reads:
        with suppress(DecodeError, MismatchError):
            read()


class TestRoundTrip(unittest.TestCase):
    @given(tables(), integers(0, 7))
    def test_a_table_reads_back_the_values_it_was_built_with_at_any_alignment(
        self, table: Description, misalignment: int
    ) -> None:
        backing = bytearray(misalignment) + build_table(table).bytes()

        read = read_table(memoryview(backing)[misalignment:], table)

        self.assertEqual(comparable_table(read), comparable_table(table))


class TestFormatRules(unittest.TestCase):
    @given(tables())
    def test_the_size_slot_holds_the_tables_length_and_the_version_slot_its_version(
        self, table: Description
    ) -> None:
        for table_bytes, description in tables_in_own_bytes(
            build_table(table).bytes(), table
        ):
            layout = description["layout"]
            size = offset_size_of(layout["offsetType"])

            if layout["sized"]:
                self.assertEqual(unsigned(table_bytes, 0, size), len(table_bytes))

            if layout["versioned"]:
                slot = size if layout["sized"] else 0
                self.assertEqual(
                    unsigned(table_bytes, slot, size), description["version"]
                )

    @given(tables())
    def test_root_offsets_lie_within_the_table_in_field_order(
        self, table: Description
    ) -> None:
        for table_bytes, description in tables_in_own_bytes(
            build_table(table).bytes(), table
        ):
            layout = description["layout"]
            size = offset_size_of(layout["offsetType"])
            fields = len(description["fields"])

            header = (int(layout["sized"]) + int(layout["versioned"])) * size
            offsets = [
                unsigned(table_bytes, header + i * size, size) for i in range(fields)
            ]

            for offset in offsets:
                self.assertGreaterEqual(offset, header + fields * size)
                self.assertLessEqual(offset, len(table_bytes))

            self.assertEqual(offsets, sorted(offsets))

    @given(tables())
    def test_arrays_start_at_a_multiple_of_their_alignment_in_their_table(
        self, table: Description
    ) -> None:
        """The alignment of an array of scalars is their size, and of an
        array of structs the size of their largest scalar. Arrays of bools
        are bytes, which need none."""
        for placement in placements_of(build_table(table).bytes(), table):
            field = placement.field
            kind = field["type"]

            if kind == "struct[]":
                alignment = struct_alignment(field["fields"])
            elif is_typed_array(kind):
                alignment = ELEMENT_SIZES[kind.removesuffix("[]")]
            else:
                continue

            first_element = placement.position + placement.offset_size
            self.assertEqual((first_element - placement.table) % alignment, 0)

    @given(tables())
    def test_nested_tables_start_at_a_multiple_of_their_alignment_in_their_parent(
        self, table: Description
    ) -> None:
        for placement in placements_of(build_table(table).bytes(), table):
            if placement.field["type"] != "table":
                continue

            alignment = alignment_of(placement.field)
            self.assertEqual((placement.position - placement.table) % alignment, 0)

    @given(tables())
    def test_values_counts_and_lengths_are_little_endian(
        self, table: Description
    ) -> None:
        for placement in placements_of(build_table(table).bytes(), table):
            self.assert_little_endian(placement)

    def assert_little_endian(self, placement: Placement) -> None:
        """Compares the bytes of a scalar, struct, array or string with the
        ones that ``struct`` packs little-endian, an array of bools with a
        byte of 1 or 0 for each, and a vector's count and ends with its
        items"""
        data, position, size = placement.data, placement.position, placement.offset_size
        field = placement.field
        kind = field["type"]

        if kind in FORMATS:
            value = packed_scalar(field)
            self.assertEqual(bytes(data[position : position + len(value)]), value)
        elif kind == "struct":
            value = b"".join(packed_scalar(scalar) for scalar in field["fields"])
            self.assertEqual(bytes(data[position : position + len(value)]), value)
        elif kind == "struct[]":
            types = field["fields"]
            structs = b"".join(
                packed_struct(types, values) for values in field["items"]
            )

            start = position + size
            self.assertEqual(unsigned(data, position, size), len(field["items"]))
            self.assertEqual(bytes(data[start : start + len(structs)]), structs)
        elif kind == "bool[]":
            values = field["value"]
            elements = bytes(1 if value else 0 for value in values)

            # Unpadded, right after the count
            start = position + size
            self.assertEqual(unsigned(data, position, size), len(values))
            self.assertEqual(bytes(data[start : start + len(elements)]), elements)
        elif kind == "string[]":
            # The count, then the end of each string from the vector's
            # position, then the strings' UTF-8, one after another
            strings = field["value"]
            self.assertEqual(unsigned(data, position, size), len(strings))

            start = (len(strings) + 1) * size
            for index, string in enumerate(strings):
                encoded = string.encode()
                end = unsigned(data, position + (index + 1) * size, size)

                self.assertEqual(end, start + len(encoded))
                self.assertEqual(
                    bytes(data[position + start : position + end]), encoded
                )
                start = end
        elif kind.endswith("[]"):
            element = kind.removesuffix("[]")
            values = [from_json(element, value) for value in field["value"]]
            elements = struct.pack(f"<{len(values)}{FORMATS[element]}", *values)

            start = position + size
            self.assertEqual(unsigned(data, position, size), len(values))
            self.assertEqual(bytes(data[start : start + len(elements)]), elements)
        elif kind == "string":
            encoded = field["value"].encode()

            start = position + size
            self.assertEqual(unsigned(data, position, size), len(encoded))
            self.assertEqual(bytes(data[start : start + len(encoded)]), encoded)
        elif kind == "vector":
            # The count, then the ends, which only grow from the first item's
            # start, after them
            count = len(field["items"])
            ends = vector_ends(placement)

            self.assertEqual(unsigned(data, position, size), count)
            self.assertEqual(ends, sorted(ends))
            self.assertTrue(all(end >= (count + 1) * size for end in ends))


def packed_scalar(scalar: Description) -> bytes:
    """The little-endian bytes of a scalar, as ``struct`` packs it"""
    kind = scalar["type"]
    return struct.pack(f"<{FORMATS[kind]}", from_json(kind, scalar["value"]))


def packed_struct(types: list[str], values: list[object]) -> bytes:
    """The bytes of a struct of an array of structs, each scalar packed one
    by one"""
    scalars = zip(types, values, strict=True)
    return b"".join(
        packed_scalar({"type": kind, "value": value}) for kind, value in scalars
    )


def _last_vector_item(reader: TableReader, position: int) -> int | None:
    """Finds the last item of the vector at ``position`` from its ends, which
    reads the ends before and after it"""
    length = reader.get_vector_length(position)
    return None if length == 0 else reader.get_vector_offset(position, length - 1)


def _last_string(reader: TableReader, position: int) -> str | None:
    """Reads the last string of the vector of strings at ``position`` from
    its ends, which reads the ends before and after it"""
    length = reader.get_vector_length(position)
    return (
        None
        if length == 0
        else reader.get_string_vector_item(position, length, length - 1)
    )


type _Getter = Callable[[TableReader, int], object]

# Every getter of a value at a position, with sizes of structs that need one
_GETTERS: dict[str, _Getter] = {
    **{
        name: getattr(TableReader, name)
        for name in [
            *(f"get_{kind}" for kind in FORMATS),
            *(f"get_{kind}_array" for kind in FORMATS),
            "get_bool_array",
            "get_string",
            "get_vector",
            "get_vector_offsets",
        ]
    },
    "get_struct": lambda reader, position: reader.get_struct(position, 9),
    "get_struct_array": lambda reader, position: reader.get_struct_array(position, 3),
    "get_vector_offset": _last_vector_item,
    "get_string_vector": TableReader.get_string_vector,
    "get_string_vector_item": _last_string,
}

# Positions and indices that a caller gives: around the bytes, around the
# limits of the C ssize_t that struct takes positions as, of more digits than
# Python converts to a string by default, and any
_POSITIONS = one_of(
    integers(-8, 72),
    integers(2**63 - 16, 2**63 + 16),
    integers(-(2**63) - 16, -(2**63) + 16),
    integers(2**15000 - 16, 2**15000 + 16),
    integers(-(2**15000) - 16, -(2**15000) + 16),
    integers(),
)


class TestUntrustedBytes(unittest.TestCase):
    @given(tables(), data())
    def test_a_truncated_table_reads_the_values_it_was_built_with_or_raises_decode_error(
        self, table: Description, data: DataObject
    ) -> None:
        """Truncating leaves the bytes before the cut as they were, so a read
        that stays within them reads what it read before"""
        serialized = bytes(build_table(table).bytes())
        assume(serialized)

        length = data.draw(integers(0, len(serialized) - 1), label="length")
        truncated = serialized[:length]

        layout = table["layout"]
        reader = TableReader(truncated, layout_of(layout))
        description_reader = DescriptionReader(truncated, layout)

        if layout["sized"]:
            with suppress(DecodeError):
                self.assertEqual(reader.get_size(), len(serialized))

        if layout["versioned"]:
            with suppress(DecodeError):
                self.assertEqual(reader.get_version(), table["version"])

        for index, field in enumerate(table["fields"]):
            with suppress(DecodeError):
                read = description_reader.read_root_field(index, field)
                self.assertEqual(comparable_field(read), comparable_field(field))

    @given(tables(), data())
    def test_a_table_with_overwritten_bytes_reads_as_values_or_raises_decode_error(
        self, table: Description, data: DataObject
    ) -> None:
        corrupted = bytearray(build_table(table).bytes())
        assume(corrupted)

        positions = integers(0, len(corrupted) - 1)
        overwrites = data.draw(
            lists(
                tuples(positions, binary(min_size=1, max_size=4)),
                min_size=1,
                max_size=3,
            ),
            label="overwrites",
        )
        for at, replacement in overwrites:
            corrupted[at : at + len(replacement)] = replacement[: len(corrupted) - at]

        read_corrupted(bytes(corrupted), table)

    @given(
        binary(max_size=64),
        LAYOUTS,
        sampled_from(list(_GETTERS)),
        lists(integers(0, 16), min_size=1, max_size=3),
    )
    def test_any_getter_reads_arbitrary_bytes_as_a_value_or_raises_decode_error(
        self, data: bytes, layout: Description, getter: str, indices: list[int]
    ) -> None:
        """Reads the header, and a value at the offset of a root field or of a
        field of a table nested in it, to the depth of ``indices``"""
        reader = TableReader(data, layout_of(layout))

        def read_value() -> object:
            position = reader.root_offset(indices[0])
            for index in indices[1:]:
                position = reader.field_offset(position, index)

            return _GETTERS[getter](reader, position)

        reads: list[Callable[[], object]] = [reader.get_size, read_value]
        if layout["versioned"]:
            reads.append(reader.get_version)

        for read in reads:
            with suppress(DecodeError):
                read()

    @given(
        binary(max_size=64),
        LAYOUTS,
        sampled_from(list(_GETTERS)),
        _POSITIONS,
        _POSITIONS,
    )
    def test_any_getter_at_any_position_reads_a_value_or_raises_decode_error(
        self,
        data: bytes,
        layout: Description,
        getter: str,
        position: int,
        index: int,
    ) -> None:
        """Positions and field indices come from the caller as well as from
        the bytes, and may lie past any position that the bytes hold"""
        reader = TableReader(data, layout_of(layout))

        reads: list[Callable[[], object]] = [
            partial(_GETTERS[getter], reader, position),
            partial(reader.root_offset, index),
            partial(reader.field_offset, position, index),
        ]

        for read in reads:
            with suppress(DecodeError):
                read()


@dataclass(frozen=True, slots=True)
class GrowingField:
    """A field of any number of units, such as the elements of an array, that
    makes a table ``unit`` bytes longer per unit when it's the last field"""

    of: Callable[[int], Description]
    unit: int


_FROM = sampled_from(["builder", "bytes"])


def _growing(table: Description) -> Callable[[int], Description]:
    """Returns a function that gives the table with a last ``uint8[]`` of any
    number of elements, which grows it a byte at a time"""

    def of(count: int) -> Description:
        last = {"type": "uint8[]", "value": [0] * count}
        return {**table, "fields": [*table["fields"], last]}

    return of


# Growing tables with no nested tables and uint32 offsets, with and without a
# header
_GROWING_TABLES: dict[bool, SearchStrategy[Callable[[int], Description]]] = {
    header: tables(0, header=header, offset_types=("uint32",)).map(_growing)
    for header in (False, True)
}


@composite
def growing_fields(draw: DrawFn, offset_type: OffsetType) -> GrowingField:
    """Strings, alone and last in a vector of strings, arrays of scalars, of
    bools and of structs, and tables nested with ``set_table()`` and in a
    vector that end in a ``uint8[]``, which grow by a character, element, bool
    or struct at a time, and vectors of tables and of strings that grow by an
    empty item, for tables of the offset type ``offset_type``. Nested tables
    have uint32 offsets, so that they fit any length that their parent does."""
    kinds = [
        "array",
        "bool array",
        "struct array",
        "string",
        "string vector item",
        "table",
        "vector item",
    ]
    if offset_type == "uint8":
        # A vector of thousands of items would take most of the suite's time
        kinds += ["vector count", "string vector count"]

    match draw(sampled_from(kinds)):
        case "array":
            element = draw(sampled_from(list(SCALAR_VALUES)))
            value = draw(SCALAR_VALUES[element])

            return GrowingField(
                lambda count: {"type": f"{element}[]", "value": [value] * count},
                ELEMENT_SIZES[element],
            )
        case "bool array":
            value = draw(booleans())

            return GrowingField(
                lambda count: {"type": "bool[]", "value": [value] * count}, 1
            )
        case "struct array":
            types = draw(STRUCT_TYPES)
            values = [draw(SCALAR_VALUES[kind]) for kind in types]

            return GrowingField(
                lambda count: {
                    "type": "struct[]",
                    "fields": types,
                    "items": [values] * count,
                },
                struct_of(types).size,
            )
        case "string":
            character = draw(characters(codec="utf-8"))

            return GrowingField(
                lambda count: {"type": "string", "value": character * count},
                len(character.encode()),
            )
        case "string vector item":
            character = draw(characters(codec="utf-8"))
            others = draw(lists(STRINGS, max_size=3))

            return GrowingField(
                lambda count: {
                    "type": "string[]",
                    "value": [*others, character * count],
                },
                len(character.encode()),
            )
        case "string vector count":
            return GrowingField(
                lambda count: {"type": "string[]", "value": [""] * count},
                offset_size_of(offset_type),
            )
        case "table":
            nested = draw(_GROWING_TABLES[False])
            field = {"type": "table", "from": draw(_FROM)}
            alignment = draw(sampled_from([None, 1, 2, 4, 8]))
            if alignment is not None:
                field["alignment"] = alignment

            return GrowingField(lambda count: {**field, "table": nested(count)}, 1)
        case "vector item":
            nested = draw(_GROWING_TABLES[True])
            source = draw(_FROM)

            return GrowingField(
                lambda count: {
                    "type": "vector",
                    "items": [{"from": source, "table": nested(count)}],
                },
                1,
            )
        case _:
            # A vector's count, of items of no fields and no header, which have
            # no bytes
            layout = {
                "sized": False,
                "versioned": False,
                "offsetType": draw(sampled_from(OFFSET_TYPES)),
            }
            empty = {"from": draw(_FROM), "table": {"layout": layout, "fields": []}}

            return GrowingField(
                lambda count: {"type": "vector", "items": [empty] * count},
                offset_size_of(offset_type),
            )


class Boundary(NamedTuple):
    """A field that grows after a table's fields, the bytes of the table with
    it at no length, and the most units of it that fit in the table's offset
    type. Each unit makes the table ``field.unit`` bytes longer, since the
    field is the last."""

    table: Description
    field: GrowingField
    empty: bytes
    longest: int

    def with_field(self, count: int) -> Description:
        return {**self.table, "fields": [*self.table["fields"], self.field.of(count)]}


def draw_boundary(table: Description, data: DataObject) -> Boundary:
    offset_type = table["layout"]["offsetType"]
    field = data.draw(growing_fields(offset_type), label="growing field")

    empty_field = {**table, "fields": [*table["fields"], field.of(0)]}
    assume(longest_length(empty_field) <= LONGEST[offset_type])

    empty = bytes(build_table(empty_field).bytes())
    longest = (LONGEST[offset_type] - len(empty)) // field.unit

    return Boundary(table, field, empty, longest)


class TestBuilderLimits(unittest.TestCase):
    @given(tables(offset_types=("uint8", "uint16")), data())
    def test_writes_the_longest_last_field_that_the_offset_type_addresses(
        self, table: Description, data: DataObject
    ) -> None:
        boundary = draw_boundary(table, data)

        longest = boundary.with_field(boundary.longest)
        built = build_table(longest).bytes()

        unit = boundary.field.unit
        self.assertEqual(len(built), len(boundary.empty) + boundary.longest * unit)
        self.assertEqual(
            comparable_table(read_table(built, longest)), comparable_table(longest)
        )

    @given(tables(offset_types=("uint8", "uint16")), data())
    def test_a_too_long_field_raises_from_its_setter_and_leaves_the_builder_as_it_was(
        self, table: Description, data: DataObject
    ) -> None:
        """The field is one unit longer than the longest that fits. The
        builder then takes the field at no length instead, and writes the same
        bytes as a builder that was given that alone."""
        boundary = draw_boundary(table, data)

        builder = create_builder(boundary.with_field(0))
        for field in table["fields"]:
            set_field(builder, field)

        with self.assertRaises(ValueError):
            set_field(builder, boundary.field.of(boundary.longest + 1))

        set_field(builder, boundary.field.of(0))
        self.assertEqual(bytes(builder.bytes()), boundary.empty)


if __name__ == "__main__":
    unittest.main()
