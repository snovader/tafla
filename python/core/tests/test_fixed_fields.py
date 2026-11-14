import re
import unittest
from collections.abc import Callable, Iterator, Sequence
from functools import partial
from struct import error as StructError, pack
from typing import Any, Literal, overload, override

from hypothesis import assume, given
from hypothesis.strategies import binary, integers, sampled_from

from tafla import ByteBuffer, DecodeError, FixedFields, TableLayout, TableWriter
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


def outcome(write: Callable[[], memoryview[int] | bytes]) -> Outcome:
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


class TestMessages(unittest.TestCase):
    # write_fields() writes a message of only fixed fields as the constructor,
    # set_fields() and bytes() write it, in one call, and read_fields() reads
    # it as get_fields() reads its root, without a ByteBuffer

    @given(tables(), sampled_from([None, *INVALID]), corruptions)
    def test_writes_and_reads_a_message_as_the_writer_and_the_buffer_do(
        self,
        table: Description,
        invalid: tuple[str, Any] | None,
        corruption: Truncation | Overwrite,
    ) -> None:
        fixed = whole_fixed(nested_in_place(table))
        assume(fixed is not None)
        assert fixed is not None
        if invalid is not None:
            # A value that the field can't hold, before the message's fields
            invalid_first: Description = {
                **fixed,
                "fields": [scalar(*invalid), *fixed["fields"]],
            }
            fixed = invalid_first
        layout = layout_of(fixed["layout"])
        fields = fixed_fields(fixed, len(fixed["fields"]), layout)
        version: int | None = fixed["version"] if layout.versioned() else None

        written = outcome(partial(write_message, fixed, fields, version))
        self.assertEqual(written, outcome(partial(write_packed, fixed)))
        if not isinstance(written, bytes):
            return

        for data in [written, corruption.apply(written)]:
            buffer = ByteBuffer(data, layout)
            expected = read_outcome(
                lambda buffer=buffer: [buffer.get_fields(0, fields)]
            )
            self.assertEqual(
                read_outcome(lambda data=data: [read_message(data, fields, None)]),
                expected,
            )
            # And None for a version other than the one that it was written in
            if version is not None and outcome_of(buffer.get_version) != version:
                expected = [None]
            self.assertEqual(
                read_outcome(lambda data=data: [read_message(data, fields, version)]),
                expected,
            )

    @given(binary(max_size=64), sampled_from(OFFSET_TYPES), integers(0, 3))
    def test_reads_arbitrary_bytes_as_get_fields_or_none_for_another_version(
        self, data: bytes, offset_type: OffsetType, version: int
    ) -> None:
        pair = nested([scalar("float32", 0.0), scalar("float32", 0.0)])
        table = {"fields": [scalar("uint16", 0), pair, scalar("string", "")]}
        layout = TableLayout(sized=True, versioned=True, offset_type=offset_type)
        fields = fixed_fields(table, 3, layout)
        buffer = ByteBuffer(data, layout)

        expected = outcome_of(partial(buffer.get_fields, 0, fields))
        if expected is not None and outcome_of(buffer.get_version) != version:
            expected = None
        self.assertEqual(
            outcome_of(partial(ByteBuffer.read_fields, data, fields, version)),
            expected,
        )
        self.assertEqual(
            outcome_of(partial(ByteBuffer.read_fields, data, fields)),
            outcome_of(partial(buffer.get_fields, 0, fields)),
        )

    def test_raises_type_error_for_fields_without_a_layout_or_bytes_or_version(
        self,
    ) -> None:
        nested_fields = FixedFields(2, "h", "h")
        with self.assertRaises(TypeError):
            TableWriter.write_fields(nested_fields, (1, 2))
        with self.assertRaises(TypeError):
            ByteBuffer.read_fields(bytes(16), nested_fields)

        fields = FixedFields(2, "h", "h", layout=TableLayout.DEFAULT)
        data = TableWriter.write_fields(fields, (1, 2))
        self.assertEqual(ByteBuffer.read_fields(data, fields), (1, 2))
        with self.assertRaises(TypeError):
            ByteBuffer.read_fields(data, fields, 1)
        with self.assertRaises(TypeError):
            ByteBuffer.read_fields(memoryview(data * 2)[::2], fields)
        # The writer's constructor raises for a version that the layout has no
        # slot for, or needs
        with self.assertRaises(TypeError):
            TableWriter.write_fields(fields, (1, 2), version=1)
        versioned = FixedFields(1, "h", layout=TableLayout(versioned=True))
        with self.assertRaises(TypeError):
            TableWriter.write_fields(versioned, (1,))
        with self.assertRaises(ValueError):
            TableWriter.write_fields(versioned, (1,), version=2**32)


def write_message(
    table: Description, fields: FixedFields, version: int | None
) -> bytes:
    """Writes a message of only fixed fields, as ``set_fields()`` takes them,
    with ``write_fields()``"""
    values: list[Any] = []
    leading = table["fields"]
    string = leading[-1]["value"] if leading[-1]["type"] == "string" else ""
    push_values([field for field in leading if field["type"] != "string"], values)

    return TableWriter.write_fields(fields, tuple(values), string, version)


def read_message(
    data: bytes, fields: FixedFields, version: int | None
) -> tuple[Any, ...] | None:
    return ByteBuffer.read_fields(data, fields, version)


def outcome_of(call: Callable[[], object]) -> object:
    """What ``call`` returns, with each float as its bits, or the class and
    message of the error that it raises"""
    try:
        value = call()
    except Exception as error:  # noqa: BLE001
        return type(error), str(error)

    if isinstance(value, tuple):
        return comparable(value)
    return value


class TestVectorTables(unittest.TestCase):
    # set_vector_tables() writes what set_vector(), write_table() for each item
    # and set_item_end() write, and get_vector_fields() reads what get_fields()
    # reads for each item, or None, or raises what get_vector_offsets() raises

    @given(
        tables(),
        integers(0, 4),
        sampled_from(OFFSET_TYPES),
        sampled_from([None, *INVALID]),
        corruptions,
    )
    def test_writes_and_reads_a_vector_of_tables_as_each_item_alone(
        self,
        table: Description,
        count: int,
        offset_type: OffsetType,
        invalid: tuple[str, Any] | None,
        corruption: Truncation | Overwrite,
    ) -> None:
        item = whole_fixed(nested_in_place(table))
        assume(item is not None)
        assert item is not None
        if invalid is not None:
            # A value that the item's field can't hold, which the setters raise
            # for, and the item's other fields
            item = {**item, "fields": [scalar(*invalid), *item["fields"]]}
        fields = fixed_fields(item, len(item["fields"]))
        items = [item_values(item)] * count
        layout = TableLayout(offset_type=offset_type)

        has_string = item["fields"][-1]["type"] == "string"

        written = outcome(partial(write_vector_tables, layout, fields, items))
        by_items = partial(write_each_item, layout, fields, items, has_string)
        self.assertEqual(written, outcome(by_items))
        if not isinstance(written, bytes):
            return

        for data in [written, corruption.apply(written)]:
            buffer = ByteBuffer(data, layout)
            with self.subTest(data=data):
                self.assertEqual(
                    read_outcome(partial(read_vector_fields, buffer, fields)),
                    read_outcome(partial(read_each_item, buffer, fields)),
                )
                self.assertEqual(
                    iterated(partial(iterate_vector_fields, buffer, fields)),
                    iterated(partial(iterate_each_item, buffer, fields)),
                )

    @given(binary(max_size=64), integers(-4, 70), sampled_from(OFFSET_TYPES))
    def test_reads_a_vector_of_arbitrary_bytes_as_each_item_alone(
        self, data: bytes, position: int, offset_type: OffsetType
    ) -> None:
        pair = nested([scalar("float32", 0.0), scalar("float32", 0.0)])
        table = {"fields": [scalar("uint16", 0), pair, scalar("string", "")]}
        fields = fixed_fields(table, 3)
        buffer = ByteBuffer(data, TableLayout(offset_type=offset_type))

        self.assertEqual(
            read_outcome(partial(buffer.get_vector_fields, position, fields)),
            read_outcome(partial(read_each_item, buffer, fields, position)),
        )
        self.assertEqual(
            iterated(partial(iterate_vector_fields, buffer, fields, position)),
            iterated(partial(iterate_each_item, buffer, fields, position)),
        )

    def test_iterates_over_the_items_before_one_whose_end_is_corrupted(
        self,
    ) -> None:
        fields = FixedFields(2, "h", string=True)
        writer = TableWriter(1)
        writer.set_vector_tables(0, fields, [(1, "a"), (2, "b"), (3, "c")])
        data = bytearray(writer.bytes())
        # The vector at 4 holds its count and the ends of its items, the
        # second's at 12, which is made to end past the bytes
        data[12:16] = (len(data)).to_bytes(4, "little")
        buffer = ByteBuffer(data)

        items = buffer.iter_vector_fields(buffer.root_offset(0), fields, _read_by_hand)
        self.assertEqual(next(items), (1, "a"))
        with self.assertRaises(DecodeError):
            next(items)
        # An iterator that raised is done
        self.assertEqual(list(items), [])
        # As get_vector_fields() raises before it reads any item
        with self.assertRaises(DecodeError):
            buffer.get_vector_fields(buffer.root_offset(0), fields)

    def test_reads_the_items_that_get_fields_cant_read_with_read(self) -> None:
        """Items whose offsets aren't those that the writer writes, such as
        tables built field by field in another order"""
        fields = FixedFields(2, "h", string=True)
        writer = TableWriter(1)
        vector = writer.set_vector(0, 2)
        for index, (number, text) in enumerate([(1, "a"), (2, "b")]):
            # The string first, so that the offsets aren't the ones that
            # FixedFields expects
            item = writer.start_table(2)
            writer.set_string(1, text)
            writer.set_int16(0, number)
            writer.end_table(item)
            writer.set_item_end(vector, index)
        buffer = ByteBuffer(writer.bytes())
        vector = buffer.root_offset(0)
        offsets = list(buffer.get_vector_offsets(vector))

        items = buffer.iter_vector_fields(vector, fields, _read_by_hand)
        self.assertEqual(list(items), [("read", offsets[0]), ("read", offsets[1])])

    def test_raises_value_error_for_items_whose_len_isnt_their_number(
        self,
    ) -> None:
        """A sequence whose iteration gives more or fewer items than its
        ``len()``, whose vector would have ends over its items' bytes or ends
        left at 0"""
        fields = FixedFields(2, "h", string=True)
        for length, items in [(1, [(1, "a"), (2, "b")]), (3, [(1, "a")]), (1, [])]:
            message = (
                f"The items' len() is {length}, but iterating them gave {len(items)}"
            )
            with (
                self.subTest(length=length, items=items),
                self.assertRaisesRegex(ValueError, f"^{re.escape(message)}$"),
            ):
                TableWriter(1).set_vector_tables(0, fields, Misreported(items, length))


class Misreported(Sequence[tuple[Any, ...]]):
    """Items whose ``len()`` is ``length``, whatever their number"""

    def __init__(self, items: list[tuple[Any, ...]], length: int) -> None:
        self.items = items
        self.length = length

    @override
    def __len__(self) -> int:
        return self.length

    @overload
    def __getitem__(self, index: int) -> tuple[Any, ...]: ...

    @overload
    def __getitem__(self, index: slice) -> Sequence[tuple[Any, ...]]: ...

    @override
    def __getitem__(
        self, index: int | slice
    ) -> tuple[Any, ...] | Sequence[tuple[Any, ...]]:
        return self.items[index]

    @override
    def __iter__(self) -> Iterator[tuple[Any, ...]]:
        return iter(self.items)


def item_values(table: Description) -> tuple[Any, ...]:
    """A table's values as ``set_vector_tables()`` takes them, and
    ``get_fields()`` returns them: a nested table's in place of it, and the
    string's last"""
    fields = table["fields"]
    values: list[Any] = []
    push_values([field for field in fields if field["type"] != "string"], values)
    if fields[-1]["type"] == "string":
        values.append(fields[-1]["value"])

    return tuple(values)


def write_vector_tables(
    layout: TableLayout[Literal[False]],
    fields: FixedFields,
    items: list[tuple[Any, ...]],
) -> memoryview[int]:
    writer = TableWriter(1, layout)
    writer.set_vector_tables(0, fields, items)
    return writer.bytes()


def write_each_item(
    layout: TableLayout[Literal[False]],
    fields: FixedFields,
    items: list[tuple[Any, ...]],
    has_string: bool,
) -> memoryview[int]:
    """Writes the vector item by item, as ``set_vector_tables()`` writes it"""
    writer = TableWriter(1, layout)
    vector = writer.set_vector(0, len(items))
    for index, values in enumerate(items):
        if has_string:
            writer.write_table(fields, values[:-1], values[-1])
        else:
            writer.write_table(fields, values)
        writer.set_item_end(vector, index)

    return writer.bytes()


def read_vector_fields(buffer: ByteBuffer, fields: FixedFields) -> object:
    return buffer.get_vector_fields(buffer.root_offset(0), fields)


def read_each_item(
    buffer: ByteBuffer, fields: FixedFields, position: int | None = None
) -> object:
    """Reads the vector's items one by one, as ``get_vector_fields()`` reads
    them, at ``position``, or the root's first field"""
    if position is None:
        position = buffer.root_offset(0)

    items = []
    for offset in buffer.get_vector_offsets(position):
        values = buffer.get_fields(offset, fields)
        if values is None:
            return None
        items.append(values)

    return items


def iterate_vector_fields(
    buffer: ByteBuffer, fields: FixedFields, position: int | None = None
) -> Iterator[tuple[Any, ...]]:
    if position is None:
        position = buffer.root_offset(0)
    return buffer.iter_vector_fields(position, fields, _read_by_hand)


def iterate_each_item(
    buffer: ByteBuffer, fields: FixedFields, position: int | None = None
) -> Iterator[tuple[Any, ...]]:
    """Reads the vector's items one by one, as ``iter_vector_fields()`` reads
    them, at ``position``, or the root's first field: after checking its count,
    each item when it's reached, from the ends of that item and the one before
    it"""
    if position is None:
        position = buffer.root_offset(0)
    count = buffer.get_vector_length(position)

    def items() -> Iterator[tuple[Any, ...]]:
        for index in range(count):
            offset = buffer.get_vector_item_offset(position, count, index)
            values = buffer.get_fields(offset, fields)
            yield _read_by_hand(offset) if values is None else values

    return items()


def _read_by_hand(offset: int) -> tuple[str, int]:
    """What reads an item that ``get_fields()`` can't, which the tests tell
    from the items that it reads"""
    return ("read", offset)


def iterated(iterate: Callable[[], Iterator[tuple[Any, ...]]]) -> list[object]:
    """Each item that the iterator that ``iterate`` returns gives, comparable,
    until the error that it or the iterator raises, as its class and message"""
    items: list[object] = []
    try:
        for values in iterate():
            items.append(comparable(values))
    except Exception as error:  # noqa: BLE001
        items.append((type(error), str(error)))

    return items


def read_outcome(read: Callable[[], object]) -> object:
    """The items that ``read`` returns, each comparable, or ``None``, or the
    class and message of the error that it raises"""
    try:
        items = read()
    except Exception as error:  # noqa: BLE001
        return type(error), str(error)

    if items is None:
        return None
    assert isinstance(items, list)
    return [comparable(values) for values in items]


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

    def test_raises_struct_error_for_fields_longer_than_a_struct_describes(
        self,
    ) -> None:
        """Fields whose bytes from the first slot pass a C ssize_t, however
        their sizes add up to it, which struct.calcsize() rejects too"""
        for fields, *formats in [
            (2, f"{2**30}s", f"{2**64 - 2**30}s"),
            (2, f"{2**62}s", f"{2**62}s"),
            (1, f"{2**63 - 1}s"),
            (1, f"{2**63}s"),
            (1, f"{10**30}s"),
            (2**62, "i"),
            (2**63, "i"),
            (2**64, "i"),
            (2**15000, "i"),
        ]:
            for layout in [None, TableLayout(offset_type="uint32")]:
                with (
                    self.subTest(fields=fields.bit_length(), formats=formats),
                    self.assertRaisesRegex(StructError, "total struct size too long"),
                ):
                    FixedFields(fields, *formats, layout=layout)

    def test_raises_value_error_for_fewer_fields_than_it_describes(self) -> None:
        for fields in [-(2**63), -1, 0, 1]:
            with (
                self.subTest(fields=fields),
                self.assertRaisesRegex(ValueError, f"A table of {fields} fields"),
            ):
                FixedFields(fields, "i", string=True)


if __name__ == "__main__":
    unittest.main()
