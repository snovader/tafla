import unittest
from collections.abc import Callable, Sequence
from functools import partial
from struct import pack
from typing import Any

from hypothesis import assume, given
from hypothesis.strategies import binary, integers, sampled_from

from tafla import ByteBuffer, FixedFields, TableLayout, TableWriter
from tests.descriptions import tables
from tests.generated.byte_strategies import Overwrite, Truncation, corruptions
from tests.tables import (
    FORMATS,
    INT64_TYPES,
    Description,
    OffsetType,
    alignment_of,
    from_json,
    layout_of,
    nested_in_place,
    struct_of,
    write_field,
    write_table,
)

OFFSET_TYPES: tuple[OffsetType, ...] = ("uint8", "uint16", "uint32")

# The struct format code that FixedFields describes each scalar type by
CODES = {**FORMATS, "bool": "?"}

type Outcome = bytes | tuple[type[Exception], str]


def invalid_values(kind: str) -> list[Any]:
    """Values that a scalar field of the type can't hold, as a description
    holds them: ``from_json()`` converts those of the 64-bit integers and the
    floats, so they're only numbers out of range"""
    if kind == "float32":
        return [1e39, -1e39]
    if kind == "float64":
        return []

    bits = struct_of([kind]).size * 8
    if kind.startswith("int"):
        low, high = -(2 ** (bits - 1)), 2 ** (bits - 1) - 1
    else:
        low, high = 0, 2**bits - 1

    if kind in INT64_TYPES:
        return [low - 1, high + 1]

    return [low - 1, high + 1, 0.5, "x"]


INVALID = [(kind, value) for kind in sorted(FORMATS) for value in invalid_values(kind)]


def outcome(write: Callable[[], memoryview[int]]) -> Outcome:
    """The bytes that ``write`` returns, or the class and message of the error
    that it raises"""
    try:
        return bytes(write())
    except (ValueError, TypeError) as error:
        return type(error), str(error)


def is_fixed(field: Description) -> bool:
    """Whether a field is one that ``FixedFields`` describes: a scalar, a
    struct, or a table of them nested in place, which isn't padded"""
    kind = field["type"]
    if kind == "table":
        nested = field["table"]["fields"]
        return alignment_of(field) == 1 and all(is_fixed(each) for each in nested)

    return kind in CODES or kind == "struct"


def packed_count(table: Description) -> int:
    """The number of a table's leading fields that its ``FixedFields``
    describes: its fixed fields, and a string after them"""
    fields = table["fields"]
    count = 0
    while count < len(fields) and is_fixed(fields[count]):
        count += 1
    if count < len(fields) and fields[count]["type"] == "string":
        count += 1

    return count


def fixed_fields(
    table: Description, count: int, layout: TableLayout | None = None
) -> FixedFields:
    fields = table["fields"][:count]
    string = bool(fields) and fields[-1]["type"] == "string"
    formats = [describe(field) for field in fields[: count - string]]

    return FixedFields(len(table["fields"]), *formats, string=string, layout=layout)


def describe(field: Description) -> str | FixedFields:
    kind = field["type"]
    if kind == "table":
        nested = field["table"]
        return fixed_fields(nested, len(nested["fields"]))

    if kind == "struct":
        return f"{struct_of(scalar['type'] for scalar in field['fields']).size}s"

    return CODES[kind]


def push_values(fields: list[Description], values: list[Any]) -> None:
    """Appends the values of fixed fields as ``FixedFields`` takes them, with
    a nested table's in place of it"""
    for field in fields:
        kind = field["type"]
        if kind == "table":
            push_values(field["table"]["fields"], values)
        elif kind == "struct":
            scalars = field["fields"]
            packing = struct_of(scalar["type"] for scalar in scalars)
            values.append(packing.pack(*(scalar_value(s) for s in scalars)))
        else:
            values.append(scalar_value(field))


def scalar_value(field: Description) -> Any:
    """A scalar's value as tafla takes it, and a bool's as the description
    holds it, which FixedFields packs as 1 or 0"""
    if field["type"] == "bool":
        return field["value"]

    return from_json(field["type"], field["value"])


def set_fields(
    writer: TableWriter,
    table: Description,
    count: int,
    method: Callable[[FixedFields, tuple[Any, ...], str], None],
    layout: TableLayout | None = None,
) -> None:
    """Writes a table's first ``count`` fields with ``method``, a writer's
    ``set_fields()`` or ``write_table()``"""
    fields = table["fields"][:count]
    string = fields[-1]["value"] if fields[-1]["type"] == "string" else ""
    values: list[Any] = []
    push_values([field for field in fields if field["type"] != "string"], values)

    method(fixed_fields(table, count, layout), tuple(values), string)


def write_packed(table: Description) -> memoryview[int]:
    """Writes a table as ``write_table()`` does, but with each table's leading
    fixed fields and string packed: a nested table's whole with
    ``write_table()`` when they're all its fields, and otherwise with
    ``set_fields()`` after the constructor or ``start_table()``"""
    fields = table["fields"]
    sized, offset_type = table["layout"]["sized"], table["layout"]["offsetType"]
    layout: TableLayout
    if table["layout"]["versioned"]:
        versioned = TableLayout(sized=sized, versioned=True, offset_type=offset_type)
        writer = TableWriter(len(fields), versioned, table["version"])
        layout = versioned
    else:
        layout = TableLayout(sized=sized, offset_type=offset_type)
        writer = TableWriter(len(fields), layout)

    count = packed_count(table)
    if count > 0:
        set_fields(writer, table, count, writer.set_fields, layout)
    for index in range(count, len(fields)):
        write_packed_field(writer, index, fields[index])

    return writer.bytes()


def write_packed_field(writer: TableWriter, index: int, field: Description) -> None:
    kind = field["type"]
    if kind == "table":
        writer.set_table(index, alignment_of(field))
        write_packed_nested(writer, field["table"])
    elif kind == "vector":
        vector = writer.set_vector(index, len(field["items"]))
        for item, nested in enumerate(field["items"]):
            write_packed_nested(writer, nested["table"])
            writer.set_item_end(vector, item)
    else:
        write_field(writer, index, field)


def write_packed_nested(writer: TableWriter, table: Description) -> None:
    fields = table["fields"]
    count = packed_count(table)
    if 0 < count == len(fields):
        set_fields(writer, table, count, writer.write_table)
        return

    parent = writer.start_table(len(fields))
    if count > 0:
        set_fields(writer, table, count, writer.set_fields)
    for index in range(count, len(fields)):
        write_packed_field(writer, index, fields[index])
    writer.end_table(parent)


def whole_fixed(table: Description) -> Description | None:
    """The table, cut to its leading fixed fields and string, which
    ``FixedFields`` then describes whole, or ``None`` if it has none"""
    count = packed_count(table)
    if count == 0:
        return None

    return {**table, "fields": table["fields"][:count]}


def read_by_getters(
    buffer: ByteBuffer, table: Description, position: int | None = None
) -> list[Any]:
    """The values of a table's fields, read by their getters in the order that
    ``get_fields()`` returns them, a nested table's in place of it and a
    struct's as its bytes. ``position`` is a nested table's, and ``None`` the
    root's."""
    values: list[Any] = []
    for index, field in enumerate(table["fields"]):
        if position is None:
            at = buffer.root_offset(index)
        else:
            at = buffer.field_offset(position, index)

        kind = field["type"]
        if kind == "table":
            values.extend(read_by_getters(buffer, field["table"], at))
        elif kind == "struct":
            size = struct_of(scalar["type"] for scalar in field["fields"]).size
            values.append(bytes(buffer.get_struct(at, size)))
        elif kind == "string":
            values.append(buffer.get_string(at))
        elif kind == "bool":
            values.append(buffer.get_uint8(at) != 0)
        else:
            values.append(getattr(buffer, f"get_{kind}")(at))

    return values


def comparable(values: Sequence[Any] | None) -> list[Any] | None:
    """Values with each float as its bits, which tells NaNs and -0 apart"""
    if values is None:
        return None

    return [
        pack("<d", value) if isinstance(value, float) else value for value in values
    ]


def scalar(kind: str, value: Any) -> Description:
    return {"type": kind, "value": value}


def message(
    fields: list[Description], offset_type: OffsetType = "uint32"
) -> Description:
    layout = {"sized": False, "versioned": False, "offsetType": offset_type}
    return {"layout": layout, "fields": fields}


def nested(fields: list[Description]) -> Description:
    """A field of a table nested in place, from its builder"""
    table = {"layout": message([])["layout"], "fields": fields}
    return {"type": "table", "from": "builder", "table": table}


class TestFixedFields(unittest.TestCase):
    @given(tables())
    def test_writes_the_bytes_that_the_setters_write(self, table: Description) -> None:
        table = nested_in_place(table)

        self.assertEqual(
            outcome(lambda: write_packed(table)), outcome(lambda: write_table(table))
        )

    def test_writes_a_bool_as_1_when_its_value_is_truthy_as_the_generated_code_did(
        self,
    ) -> None:
        for value in [False, True, 0, 1, 2, -1, 0.0, float("nan"), "", "x", [], None]:
            with self.subTest(value=value):
                table = message([scalar("bool", value), scalar("int8", 1)])
                expected = TableWriter(2)
                expected.set_uint8(0, 1 if value else 0)
                expected.set_int8(1, 1)

                self.assertEqual(bytes(write_packed(table)), bytes(expected.bytes()))


class TestGetFields(unittest.TestCase):
    # get_fields() returns what the getters read, or None for the getters to
    # read the fields and raise, and never raises itself

    @given(tables(), corruptions)
    def test_reads_what_the_getters_read_or_none_whatever_the_bytes(
        self, table: Description, corruption: Truncation | Overwrite
    ) -> None:
        fixed = whole_fixed(nested_in_place(table))
        assume(fixed is not None)
        assert fixed is not None
        layout = layout_of(fixed["layout"])
        fields = fixed_fields(fixed, len(fixed["fields"]), layout)
        data = bytes(write_table(fixed))

        written = ByteBuffer(data, layout)
        self.assertEqual(
            comparable(written.get_fields(0, fields)),
            comparable(read_by_getters(written, fixed)),
        )

        corrupted = ByteBuffer(corruption.apply(data), layout)
        values = corrupted.get_fields(0, fields)
        if values is not None:
            self.assertEqual(
                comparable(values), comparable(read_by_getters(corrupted, fixed))
            )

    @given(binary(max_size=64), integers(-4, 70), sampled_from(OFFSET_TYPES))
    def test_reads_a_nested_table_of_arbitrary_bytes_as_the_getters_or_none(
        self, data: bytes, position: int, offset_type: OffsetType
    ) -> None:
        pair = nested([scalar("float32", 0.0), scalar("float32", 0.0)])
        table = {"fields": [scalar("uint16", 0), pair, scalar("string", "")]}
        fields = fixed_fields(table, 3)
        buffer = ByteBuffer(data, TableLayout(offset_type=offset_type))

        values = buffer.get_fields(position, fields)
        if values is not None:
            self.assertEqual(
                comparable(values), comparable(read_by_getters(buffer, table, position))
            )


class TestErrors(unittest.TestCase):
    # The setters raise ValueError for a value that they can't write, naming
    # it and its position, and for a table too long for its offset type, and
    # set_fields() and write_table() fall back to them

    @given(sampled_from(INVALID), sampled_from(OFFSET_TYPES))
    def test_raises_the_setters_error_for_a_value_that_doesnt_fit_its_field(
        self, invalid: tuple[str, Any], offset_type: OffsetType
    ) -> None:
        kind, value = invalid
        fields = [scalar("float64", 0.5), scalar(kind, value), scalar("string", "a")]
        for table in [
            message(fields, offset_type),
            message([nested(fields[:2])], offset_type),
        ]:
            with self.subTest(table=table):
                written = outcome(partial(write_packed, table))

                self.assertIsInstance(written, tuple)
                self.assertEqual(written, outcome(partial(write_table, table)))

    def test_raises_for_a_scalar_before_a_lone_surrogate_in_the_string_first(
        self,
    ) -> None:
        for first in [scalar("uint8", 256), scalar("uint8", 255)]:
            table = message([first, scalar("string", "\ud800")])
            with self.subTest(table=table):
                with self.assertRaises(ValueError) as packed:
                    write_packed(table)
                with self.assertRaises(ValueError) as by_setters:
                    write_table(table)

                self.assertIs(type(packed.exception), type(by_setters.exception))
                self.assertEqual(str(packed.exception), str(by_setters.exception))

    @given(sampled_from(OFFSET_TYPES[:2]), integers(0, 300))
    def test_raises_the_setters_error_for_a_string_too_long_for_its_offset_type(
        self, offset_type: OffsetType, extra: int
    ) -> None:
        longest = 0xFF if offset_type == "uint8" else 0xFFFF
        size = 1 if offset_type == "uint8" else 2
        # A uint32 and the string's length after two offsets, and the string
        length = longest - 2 * size - 4 - size + extra
        table = message(
            [scalar("uint32", 1), scalar("string", "a" * length)], offset_type
        )

        written = outcome(lambda: write_packed(table))
        self.assertEqual(written, outcome(lambda: write_table(table)))
        self.assertEqual(isinstance(written, bytes), extra == 0)

    def test_raises_the_setters_error_for_fields_whose_offsets_dont_fit(self) -> None:
        # 40 float64s take 320 bytes, past what uint8 offsets address
        table = message([scalar("float64", 0.5)] * 40, "uint8")

        written = outcome(lambda: write_packed(table))
        self.assertIsInstance(written, tuple)
        self.assertEqual(written, outcome(lambda: write_table(table)))


if __name__ == "__main__":
    unittest.main()
