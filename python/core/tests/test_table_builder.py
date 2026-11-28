import tracemalloc
import unittest
from array import array
from collections.abc import Buffer, Callable, Generator, Iterator
from contextlib import contextmanager
from struct import pack
from typing import Any, cast, override
from unittest.mock import patch

from hypothesis import given, settings
from hypothesis.strategies import integers, sampled_from

from tafla import (
    TableBuilder,
    TableLayout,
    TableReader,
)
from tests import tables as described
from tests.descriptions import KINDS_OF_FIELDS, UNSIZED_UINT8, any_tables, tables
from tests.tables import (
    FORMATS,
    Description,
    OffsetType,
    alignment_of,
    build_table,
    build_table_in_place,
    create_builder,
    nested_in_place,
    offset_size_of,
    set_field,
    set_field_in_place,
    write_table,
)

MAX_LENGTHS: list[tuple[OffsetType, int]] = [
    ("uint8", 2**8 - 1),
    ("uint16", 2**16 - 1),
]


def nested_as_bytes(table: Description) -> Description:
    """Returns a table's description with every table nested in it, at any
    depth, given as its bytes, and nested with ``set_table()`` at the
    alignment that its builder had"""
    return {**table, "fields": [_field_as_bytes(field) for field in table["fields"]]}


def _field_as_bytes(field: Description) -> Description:
    kind = field["type"]

    if kind == "table":
        nested = nested_as_bytes(field["table"])
        return {
            **field,
            "from": "bytes",
            "alignment": alignment_of(field),
            "table": nested,
        }

    if kind == "vector":
        items = [
            {"from": "bytes", "table": nested_as_bytes(item["table"])}
            for item in field["items"]
        ]
        return {**field, "items": items}

    return field


def root_vector(reader: TableReader, index: int) -> list[bytes]:
    """Reads the items of the vector of a root field"""
    return [bytes(item) for item in reader.get_vector(reader.root_offset(index))]


def parent_of(nested: Callable[[], TableBuilder | Buffer], length: int) -> bytes:
    """A table of a uint8[] of ``length`` zeros, which starts the next field
    at any offset from an alignment of 8, and then the table that ``nested``
    returns, nested with ``set_table()`` at an alignment of 8 and twice in a
    vector"""
    parent = TableBuilder(3)
    parent.set_uint8_array(bytes(length))
    parent.set_table(nested(), 8)
    parent.set_vector([nested(), nested()])

    return bytes(parent.bytes())


class TestTableBuilder(unittest.TestCase):
    def test_finalizes_once_and_returns_the_same_buffer(self) -> None:
        builder = TableBuilder(1)
        builder.set_string("once")

        self.assertIs(builder.finalize(), builder.finalize())
        self.assertEqual(builder.bytes(), pack("<II", 4, 4) + b"once")

    def test_finalizes_an_empty_table(self) -> None:
        self.assertEqual(TableBuilder(0).bytes(), b"")

    def test_rejects_a_scalar_that_doesnt_fit_its_type_from_its_setter_and_stays_as_it_was(
        self,
    ) -> None:
        cases = [
            ("set_uint32", 2**32, "Can't write 4294967296 at 9: "),
            ("set_int8", -129, "Can't write -129 at 9: "),
            ("set_float32", 1e300, "Can't write 1e\\+300 at 9: "),
            ("set_uint16", 1.5, "Can't write 1.5 at 9: "),
        ]
        expected = TableBuilder(2)
        expected.set_uint8(1)
        expected.set_uint8(2)

        for setter, value, message in cases:
            with self.subTest(setter=setter, value=value):
                builder = TableBuilder(2)
                builder.set_uint8(1)

                with self.assertRaisesRegex(ValueError, message):
                    getattr(builder, setter)(value)

                builder.set_uint8(2)
                self.assertEqual(builder.bytes(), expected.bytes())

    def test_rejects_a_field_beyond_the_number_the_table_holds(self) -> None:
        builder = TableBuilder(1)
        builder.set_uint8(1)

        with self.assertRaisesRegex(ValueError, "Can't add field 2 to a table of 1"):
            builder.set_uint8(2)

    @given(integers(0, 4))
    def test_rejects_finalizing_before_every_field_is_set(self, count: int) -> None:
        builder = TableBuilder(count + 1)
        for i in range(count):
            builder.set_float64(i)

        with self.assertRaisesRegex(
            ValueError, f"Only {count} of the table's {count + 1} fields were set"
        ):
            builder.finalize()

    def test_rejects_setting_a_field_after_finalizing(self) -> None:
        builder = TableBuilder(0)
        builder.finalize()

        with self.assertRaises(TypeError):
            builder.set_int8(1)
        with self.assertRaises(TypeError):
            builder.set_float64_array(array("d", [1.5]))

    def test_writes_a_buffer_as_it_was_when_set(self) -> None:
        values = array("f", [1, 2])
        builder = TableBuilder(1)
        builder.set_float32_array(values)
        values[0] = 3

        reader = builder.finalize()

        self.assertEqual(
            reader.get_float32_array(reader.root_offset(0)).tolist(), [1, 2]
        )

    def test_keeps_a_converted_list_as_it_was_when_set(self) -> None:
        values = [1.0, 2.0]
        builder = TableBuilder(1)
        builder.set_float32_array(values)
        values.append(3.0)

        reader = builder.finalize()

        self.assertEqual(
            reader.get_float32_array(reader.root_offset(0)).tolist(), [1, 2]
        )

    def test_keeps_converted_bools_as_they_were_when_set(self) -> None:
        values = [True, False]
        builder = TableBuilder(1)
        builder.set_bool_array(values)
        values.append(True)
        values[0] = False

        reader = builder.finalize()

        self.assertEqual(reader.get_bool_array(reader.root_offset(0)), [True, False])

    def test_keeps_a_vector_of_strings_as_it_was_when_set(self) -> None:
        strings = ["ab", "ç"]
        builder = TableBuilder(1)
        builder.set_string_vector(strings)
        strings.pop()
        strings[0] = "x"

        reader = builder.finalize()

        self.assertEqual(reader.get_string_vector(reader.root_offset(0)), ["ab", "ç"])

    def test_keeps_a_vector_as_it_was_when_set(self) -> None:
        vector = [b"ab", b"c"]
        builder = TableBuilder(1)
        builder.set_vector(vector)
        vector.pop()

        reader = builder.finalize()

        self.assertEqual(root_vector(reader, 0), [b"ab", b"c"])


class TestStructs(unittest.TestCase):
    def test_writes_a_struct_as_it_is_without_padding(self) -> None:
        struct = pack("<Hd", 0xFEDC, -0.0)
        builder = TableBuilder(2)
        builder.set_uint8(1)
        builder.set_struct(struct)

        reader = builder.finalize()

        self.assertEqual(reader.root_offset(1), 9)
        self.assertEqual(bytes(reader.get_struct(9, len(struct))), struct)

    def test_rejects_struct_sizes_and_bytes_it_cant_write_and_stays_as_it_was(
        self,
    ) -> None:
        cases = [
            (bytes(5), 2, "5 bytes aren't a whole number of 2-byte structs"),
            (bytes(4), 0, "A struct's size must be a positive integer, not 0"),
            (bytes(4), -1, "A struct's size must be a positive integer, not -1"),
            # A size of more digits than Python converts to a string is given
            # by its bit length
            (
                bytes(5),
                2**15000,
                "5 bytes aren't a whole number of <15001-bit integer>-byte structs",
            ),
        ]

        for data, size, message in cases:
            with self.subTest(message):
                builder = TableBuilder(1)

                with self.assertRaisesRegex(ValueError, message):
                    builder.set_struct_array(data, size, 1)

                builder.set_struct_array(bytes([1, 2, 3, 4]), 2, 2)
                self.assertEqual(
                    builder.bytes(), pack("<II", 4, 2) + b"\x01\x02\x03\x04"
                )

    def test_rejects_bytes_that_arent_c_contiguous_from_the_setter_and_stays_as_it_was(
        self,
    ) -> None:
        setters: list[tuple[str, Callable[[TableBuilder, memoryview], None]]] = [
            ("set_struct", TableBuilder.set_struct),
            (
                "set_struct_array",
                lambda builder, data: builder.set_struct_array(data, 2, 2),
            ),
            ("set_table", TableBuilder.set_table),
            ("set_vector", lambda builder, data: builder.set_vector([b"", data])),
        ]
        # A step makes a view of bytes, or of wider elements, that isn't
        # C-contiguous
        strided_views = [
            memoryview(bytes(range(8)))[::2],
            memoryview(array("h", range(8)))[::-2],
        ]

        for strided in strided_views:
            for name, setter in setters:
                with self.subTest(setter=name, format=strided.format):
                    builder = TableBuilder(1)

                    with self.assertRaisesRegex(TypeError, "isn't C-contiguous"):
                        setter(builder, strided)

                    builder.set_struct(b"\x01\x02")
                    self.assertEqual(builder.bytes(), pack("<I", 4) + b"\x01\x02")

    def test_pads_an_array_of_structs_so_that_they_start_at_their_alignment(
        self,
    ) -> None:
        builder = TableBuilder(2)
        builder.set_uint8(7)
        builder.set_struct_array(bytes(16), 8, 8)

        # The count goes at 12, after the uint8 at 8 and 3 bytes of padding, so
        # that the structs start at 16
        reader = builder.finalize()

        self.assertEqual(reader.root_offset(1), 12)
        self.assertEqual(reader.get_uint32(12), 2)
        self.assertEqual(len(reader.get_struct_array(12, 8)), 16)

    def test_aligns_a_nested_builder_to_its_arrays_of_structs(self) -> None:
        nested = TableBuilder(1)
        nested.set_struct_array(bytes(9), 9, 8)

        parent = TableBuilder(2)
        parent.set_uint8(7)
        parent.set_table(nested)

        # After the uint8 at 8, the nested table is padded to 16, so that its
        # structs, 8 bytes from its start, start at 24
        self.assertEqual(parent.finalize().root_offset(1), 16)

    def test_aligns_a_nested_builder_to_its_arrays_from_buffers_and_lists(
        self,
    ) -> None:
        for kind, code in FORMATS.items():
            size = array(code).itemsize
            for values in (array(code, [1]), [1]):
                with self.subTest(kind=kind, values=type(values).__name__):
                    nested = TableBuilder(1)
                    getattr(nested, f"set_{kind}_array")(values)

                    parent = TableBuilder(2)
                    parent.set_uint8(7)
                    parent.set_table(nested)

                    # After the uint8 at 8, the nested table is padded to the
                    # next multiple of its elements' size
                    offset = parent.finalize().root_offset(1)
                    self.assertEqual(offset, -(-9 // size) * size)

    def test_checks_a_nested_tables_alignment_before_the_number_of_fields(
        self,
    ) -> None:
        builder = TableBuilder(1)
        builder.set_uint8(1)

        for name, table in [
            ("bytes", b""),
            ("builder", TableBuilder(0)),
            ("itself", builder),
        ]:
            with (
                self.subTest(table=name),
                self.assertRaisesRegex(ValueError, "An alignment of 3 isn't"),
            ):
                builder.set_table(table, cast(Any, 3))


def nested_uint8_array(values: bytearray) -> TableBuilder:
    """A builder of one uint8[] of ``values``, which a parent nests"""
    nested = TableBuilder(1)
    nested.set_uint8_array(values)
    return nested


class TestBuffersChangedAfterTheyreSet(unittest.TestCase):
    """A setter copies its value, so a buffer that changes after it returns
    leaves the table as it was"""

    def test_writes_each_kind_of_buffer_as_it_was_when_set(self) -> None:
        cases: list[tuple[str, Callable[[TableBuilder, Any], None], Any]] = [
            ("uint8 array", TableBuilder.set_uint8_array, bytearray(b"ab")),
            ("float64 array", TableBuilder.set_float64_array, array("d", [1.5, 2.5])),
            ("struct", TableBuilder.set_struct, bytearray(b"ab")),
            (
                "struct array",
                lambda b, v: b.set_struct_array(v, 1, 1),
                bytearray(b"ab"),
            ),
            ("table", TableBuilder.set_table, bytearray(b"abcd")),
            ("vector item", lambda b, v: b.set_vector([v]), bytearray(b"ab")),
            (
                "nested builder's array",
                lambda b, v: b.set_table(nested_uint8_array(v)),
                bytearray(b"ab"),
            ),
        ]

        for name, set_buffer, values in cases:
            original = values[:]
            expected = TableBuilder(2)
            set_buffer(expected, original[:])
            expected.set_uint8(7)

            for change in ["pop", "append", "replace"]:
                with self.subTest(field=name, change=change):
                    changed = values[:]
                    builder = TableBuilder(2)
                    set_buffer(builder, changed)
                    builder.set_uint8(7)

                    if change == "pop":
                        changed.pop()
                    elif change == "append":
                        changed.append(0)
                    else:
                        changed[0] = changed[1]

                    self.assertEqual(builder.bytes(), expected.bytes())


class TestVersions(unittest.TestCase):
    def test_rejects_a_version_that_its_slot_cannot_hold(self) -> None:
        cases: list[tuple[OffsetType, int]] = [
            ("uint8", 2**8),
            ("uint16", 2**16),
            ("uint32", 2**32),
            ("uint32", -1),
        ]

        for offset_type, version in cases:
            with self.subTest(offset_type=offset_type, version=version):
                layout = TableLayout(version=0, offset_type=offset_type)
                message = f"Version {version} doesn't fit in a {offset_type} slot"

                with self.assertRaises(ValueError) as raised:
                    TableLayout(version=version, offset_type=offset_type)
                self.assertEqual(str(raised.exception), message)
                with self.assertRaises(ValueError) as raised:
                    layout.with_version(version)
                self.assertEqual(str(raised.exception), message)

    def test_reads_a_tables_version_whatever_the_version_of_the_readers_layout(
        self,
    ) -> None:
        first = TableLayout(sized=True, version=1)
        second = first.with_version(2)

        table = TableBuilder(0, second).bytes()

        self.assertEqual(second.version(), 2)
        self.assertTrue(second.sized())
        self.assertEqual(TableReader(table, first).get_version(), 2)
        self.assertEqual(TableReader(table, second).get_version(), 2)

    def test_rejects_reading_the_version_of_an_unversioned_table(self) -> None:
        layout = TableLayout(sized=True)
        reader = TableReader(TableBuilder(0, layout).bytes(), layout)

        with self.assertRaises(TypeError):
            reader.get_version()


class TestTableLength(unittest.TestCase):
    def test_rejects_a_header_longer_than_the_offsets_address(self) -> None:
        for offset_type, max_length in MAX_LENGTHS:
            for sized in [False, True]:
                with self.subTest(offset_type=offset_type, sized=sized):
                    layout = TableLayout(sized=sized, offset_type=offset_type)
                    size = offset_size_of(offset_type)
                    count = max_length // size - (1 if sized else 0)
                    message = (
                        f"A header of {count + 1} fields doesn't fit in "
                        f"{offset_type} offsets"
                    )

                    TableBuilder(count, layout)
                    with self.assertRaises(ValueError) as raised:
                        TableBuilder(count + 1, layout)
                    self.assertEqual(str(raised.exception), message)

    def test_rejects_a_field_that_makes_the_table_too_long_for_its_offset_type(
        self,
    ) -> None:
        for offset_type, max_length in MAX_LENGTHS:
            with self.subTest(offset_type=offset_type):
                builder = TableBuilder(2, TableLayout(offset_type=offset_type))
                builder.set_uint8(1)
                # A struct as long as the longest table but its two offsets
                # makes it one byte too long, after the uint8
                length = max_length - 2 * offset_size_of(offset_type)
                message = (
                    f"Field 2 makes the table {max_length + 1} bytes, too long "
                    f"for {offset_type} offsets"
                )

                with self.assertRaises(ValueError) as raised:
                    builder.set_struct(bytes(length))
                self.assertEqual(str(raised.exception), message)


class TestNestedBuilders(unittest.TestCase):
    @given(tables())
    def test_writes_nested_builders_as_their_bytes_at_the_builders_alignment(
        self, table: Description
    ) -> None:
        built = bytes(build_table(table).bytes())

        self.assertEqual(built, build_table(nested_as_bytes(table)).bytes())

    @given(tables(), integers(0, 7))
    def test_writes_a_builder_that_was_finalized_or_of_any_layout_as_the_bytes_that_it_writes(
        self, table: Description, length: int
    ) -> None:
        """A builder is copied as the bytes that it writes, whether it was
        finalized or not, and whatever its layout, as a table nested as bytes
        is"""
        own = bytes(build_table(table).bytes())
        finalized = build_table(table)
        finalized.finalize()

        expected = parent_of(lambda: own, length)
        self.assertEqual(parent_of(lambda: build_table(table), length), expected)
        self.assertEqual(parent_of(lambda: finalized, length), expected)

        reader = TableReader(expected)
        self.assertEqual(root_vector(reader, 2), [own, own])

    def test_rejects_a_builder_with_fields_left_to_set(self) -> None:
        nested = TableBuilder(2)
        nested.set_uint8(1)

        parent = TableBuilder(2)
        with self.assertRaisesRegex(ValueError, "Only 1 of the table's 2 fields"):
            parent.set_table(nested)
        with self.assertRaises(ValueError):
            parent.set_vector([bytes(1), nested])

        # The parent is as it was
        nested.set_uint8(2)
        parent.set_table(nested)
        parent.set_vector([nested])

        reader = parent.finalize()
        table = reader.root_offset(0)

        self.assertEqual(reader.get_uint8(reader.field_offset(table, 1)), 2)
        self.assertEqual(root_vector(reader, 1), [nested.bytes()])

    def test_writes_a_builder_nested_several_times_the_same_each_time(self) -> None:
        nested = TableBuilder(2)
        nested.set_float64_array([1, 2])
        nested.set_string("nested")

        # Copied before it's finalized and after
        first = TableBuilder(2)
        first.set_table(nested)
        first.set_vector([nested, nested])
        written = bytes(first.bytes())

        own = bytes(nested.bytes())
        second = TableBuilder(2)
        second.set_table(nested)
        second.set_vector([nested, nested])

        self.assertEqual(second.bytes(), written)
        self.assertEqual(root_vector(second.finalize(), 1), [own, own])

    def test_writes_a_nested_builders_values_as_they_were_when_set(self) -> None:
        values = array("f", [1, 2])
        nested = TableBuilder(1)
        nested.set_float32_array(values)
        values[0] = 3

        parent = TableBuilder(1)
        parent.set_table(nested)

        reader = parent.finalize()
        position = reader.field_offset(reader.root_offset(0), 0)

        self.assertEqual(reader.get_float32_array(position).tolist(), [1, 2])


class FillError(Exception):
    """What a fill raises, which no builder raises itself"""


@contextmanager
def fills_raising_at(call: int | None) -> Generator[None]:
    """Makes the ``call``th fill of a table written in place set half its
    fields and raise ``FillError``, and every other fill set all its fields"""
    fill = described._set_fields_in_place
    calls = 0

    def raising(builder: Any, fields: list[Description], offset_type: Any) -> None:
        nonlocal calls
        calls += 1
        if calls == call:
            fill(builder, fields[: len(fields) // 2], offset_type)
            raise FillError

        fill(builder, fields, offset_type)

    with patch.object(described, "_set_fields_in_place", raising):
        yield


def attempt(call: Callable[[], object]) -> object:
    """What a call returns, or the type and message of what it raises"""
    try:
        return call()
    except (TypeError, ValueError, FillError) as error:
        return type(error).__name__, str(error)


def outcomes(
    table: Description,
    *,
    extra_fields: int = 0,
    in_place: bool,
    raise_at: int | None = None,
    retry: bool = False,
) -> list[object]:
    """Sets each field of ``table`` on a builder whose header has
    ``extra_fields`` more fields than the table, carrying on past a field
    that raises, and then writes it. With ``retry``, a field whose fill
    raised is set again. Returns what each call returned or raised."""
    fields = max(len(table["fields"]) + extra_fields, 0)
    builder = create_builder({**table, "fields": [None] * fields})
    offset_type = table["layout"]["offsetType"]

    def set_one(field: Description) -> None:
        if in_place:
            set_field_in_place(builder, field, offset_type)
        else:
            set_field(builder, field)

    results: list[object] = []
    with fills_raising_at(raise_at):
        for field in table["fields"]:
            result = attempt(lambda field=field: set_one(field))
            if retry and result == (FillError.__name__, ""):
                result = attempt(lambda field=field: set_one(field))
            results.append(result)
    results.append(attempt(lambda: bytes(builder.bytes())))

    return results


def built_by(
    build: Callable[[Description], TableBuilder], table: Description
) -> object:
    """The bytes that a builder of the table writes, or the type of what it
    raises"""
    try:
        return bytes(build(table).bytes())
    except (TypeError, ValueError) as error:
        return type(error)


def items_of(values: list[float], builder: TableBuilder) -> bytes:
    """Records the vector of nested tables that the benchmarks write in
    place, an item of a float32 and a pair of float32s for each value, and
    returns the builder's bytes"""
    builder.set_vector_in_place(values, set_item, fields=2)
    return bytes(builder.bytes())


def set_item(item: TableBuilder, value: float) -> None:
    item.set_float32(value)
    item.set_table_in_place(set_pair, value + 1, value + 2, fields=2)


def set_pair(pair: TableBuilder, first: float, second: float) -> None:
    pair.set_float32(first)
    pair.set_float32(second)


def items_from_builders(values: list[float]) -> bytes:
    builder = TableBuilder(1)
    builder.set_vector([_item_builder(value) for value in values])
    return bytes(builder.bytes())


def _item_builder(value: float) -> TableBuilder:
    pair = TableBuilder(2)
    pair.set_float32(value + 1)
    pair.set_float32(value + 2)

    item = TableBuilder(2)
    item.set_float32(value)
    item.set_table(pair)
    return item


VALUES = [1.0, 2.0, 3.0]


def written_by_a_writer(table: Description) -> object:
    """The bytes that a ``TableWriter`` writes for a table whose nested tables
    it writes in place, or the type of what it raises, which is
    ``ValueError`` for a table too long for its offsets"""
    try:
        return bytes(write_table(table))
    except ValueError as error:
        return type(error)


class TestFieldsAtTheLimit(unittest.TestCase):
    def test_each_kind_of_field_fits_or_raises_as_a_writer_writes_it(self) -> None:
        """After an array that starts it at every offset near the limit of
        uint8 offsets, each kind of field, nested from builders or written in
        place, fits or makes the table too long as it does for a
        ``TableWriter``, which checks the table's length on its own"""
        for field in KINDS_OF_FIELDS:
            for length in range(200, 256):
                array_field = {"type": "uint8[]", "value": [0] * length}
                table = {"layout": UNSIZED_UINT8, "fields": [array_field, field]}
                expected = written_by_a_writer(nested_in_place(table))
                for build in (build_table, build_table_in_place):
                    with self.subTest(field=field, length=length, build=build.__name__):
                        self.assertEqual(built_by(build, table), expected)


class TestTablesInPlace(unittest.TestCase):
    @settings(max_examples=300)
    @given(any_tables())
    def test_tables_written_in_place_write_what_their_builders_write(
        self, table: Description
    ) -> None:
        """Tables nested from builders and written in place write the bytes
        that their builders write, and raise as they raise, for tables of any
        offset type and of short offsets, whose limits the fields reach, and
        long vectors and tables, which often make their parent too long"""
        self.assertEqual(
            built_by(build_table_in_place, table), built_by(build_table, table)
        )

    @settings(max_examples=300)
    @given(any_tables(), sampled_from([-1, 0, 1]), integers(1, 6))
    def test_a_fill_that_raises_leaves_the_builder_as_it_was(
        self, table: Description, extra_fields: int, raise_at: int
    ) -> None:
        """A fill that sets half its fields and raises, at any depth of
        tables and vectors written in place, leaves the builder as it was: set
        again, the field writes what it would have, and so does every call
        after it, whether it fits or raises, in a table with a field more or
        fewer than its header holds"""
        expected = outcomes(table, extra_fields=extra_fields, in_place=True)

        retried = outcomes(
            table,
            extra_fields=extra_fields,
            in_place=True,
            raise_at=raise_at,
            retry=True,
        )

        self.assertEqual(retried, expected)

    def test_rejects_an_in_place_table_whose_slots_are_too_long_before_fill_runs(
        self,
    ) -> None:
        filled: list[object] = []

        def fill(item: TableBuilder, *values: object) -> None:
            filled.append(values)

        builder = TableBuilder(2, TableLayout(offset_type="uint8"))
        builder.set_uint8(1)
        # The parent's 2 slots and its uint8 make 3 bytes
        with self.assertRaisesRegex(ValueError, "Field 2 makes the table 259 bytes"):
            builder.set_table_in_place(fill, fields=256)
        with self.assertRaisesRegex(
            ValueError, "Field 2 makes the table <1001-bit integer> bytes"
        ):
            builder.set_table_in_place(fill, fields=2**1000)
        # The vector's count and end make 5 bytes
        with self.assertRaisesRegex(ValueError, "Field 2 makes the table 261 bytes"):
            builder.set_vector_in_place([1], fill, fields=256)
        with self.assertRaisesRegex(ValueError, "negative count"):
            builder.set_table_in_place(fill, fields=-1)
        with self.assertRaisesRegex(ValueError, "negative count"):
            builder.set_vector_in_place([1], fill, fields=-1)

        self.assertEqual(filled, [])
        builder.set_vector_in_place([], fill, fields=256)
        self.assertEqual(builder.bytes(), bytes([2, 3, 1, 0]))

    def test_writes_the_vector_of_nested_tables_as_its_builders_do(self) -> None:
        self.assertEqual(items_of(VALUES, TableBuilder(1)), items_from_builders(VALUES))

    def test_an_item_that_raises_leaves_the_builder_as_it_was(self) -> None:
        def fill(item: TableBuilder, value: float) -> None:
            item.set_float32(value)
            if value == 3:
                raise LookupError("The caller's")
            item.set_table_in_place(set_pair, 1, 2, fields=2)

        builder = TableBuilder(1)
        with self.assertRaisesRegex(LookupError, "The caller's"):
            builder.set_vector_in_place(VALUES, fill, fields=2)

        self.assertEqual(items_of(VALUES, builder), items_from_builders(VALUES))

    def test_an_incomplete_item_raises_and_leaves_the_builder_as_it_was(self) -> None:
        def fill(item: TableBuilder, value: float) -> None:
            item.set_float32(value)
            if value != 2:
                item.set_table_in_place(set_pair, 1, 2, fields=2)

        builder = TableBuilder(1)
        with self.assertRaisesRegex(ValueError, "Only 1 of item 1's 2 fields were set"):
            builder.set_vector_in_place(VALUES, fill, fields=2)

        self.assertEqual(items_of(VALUES, builder), items_from_builders(VALUES))

    def test_an_incomplete_nested_table_raises_and_leaves_the_builder_as_it_was(
        self,
    ) -> None:
        def fill(item: TableBuilder, value: float) -> None:
            item.set_float32(value)
            item.set_table_in_place(TableBuilder.set_float32, value, fields=2)

        builder = TableBuilder(1)
        with self.assertRaisesRegex(
            ValueError, "Only 1 of the nested table's 2 fields were set"
        ):
            builder.set_vector_in_place(VALUES, fill, fields=2)

        self.assertEqual(items_of(VALUES, builder), items_from_builders(VALUES))

    def test_a_field_past_an_items_count_raises_and_leaves_the_builder_as_it_was(
        self,
    ) -> None:
        def fill(item: TableBuilder, value: float) -> None:
            set_item(item, value)
            item.set_float32(value)

        builder = TableBuilder(1)
        with self.assertRaisesRegex(ValueError, "Can't add field 3 to a table of 2"):
            builder.set_vector_in_place(VALUES, fill, fields=2)

        self.assertEqual(items_of(VALUES, builder), items_from_builders(VALUES))

    def test_rejects_a_count_too_long_for_the_table_before_recording_any_item(
        self,
    ) -> None:
        """A vector of 253 items of no fields holds its count and an end for
        each, of a byte each, which make a table of uint8 offsets, with its
        own offset, 255 bytes long. Another item makes it too long."""
        filled: list[int] = []

        def fill(item: TableBuilder, value: int) -> None:
            filled.append(value)

        builder = TableBuilder(1, TableLayout(offset_type="uint8"))
        with self.assertRaisesRegex(ValueError, "makes the table 256 bytes"):
            builder.set_vector_in_place(range(254), fill, fields=0)

        self.assertEqual(filled, [])
        builder.set_vector_in_place(range(253), fill, fields=0)
        self.assertEqual(len(builder.bytes()), 255)

    def test_raises_for_values_that_give_other_than_their_length(self) -> None:
        class Shrinking(list[float]):
            @override
            def __len__(self) -> int:
                return super().__len__() + 1

        builder = TableBuilder(1)
        with self.assertRaisesRegex(ValueError, r"other than the 4 of len\(\)"):
            builder.set_vector_in_place(Shrinking(VALUES), set_item, fields=2)

        self.assertEqual(items_of(VALUES, builder), items_from_builders(VALUES))

    def test_pads_tables_in_place_as_their_fields_raise_their_alignment(
        self,
    ) -> None:
        """A table in place is moved forward whenever a field raises its
        alignment, and so is its parent, in place too, with the parent's own
        slots: each lands where builders put it"""

        def inner(table: TableBuilder) -> None:
            table.set_uint8(1)
            table.set_float64_array([0.5, 1.5])

        def outer(table: TableBuilder) -> None:
            table.set_uint8(2)
            table.set_int16_array([3])
            table.set_table_in_place(inner, fields=2)

        for leading in range(8):
            with self.subTest(leading=leading):
                builder = TableBuilder(2)
                builder.set_uint8_array(bytes(leading))
                builder.set_table_in_place(outer, fields=3)

                inner_builder = TableBuilder(2)
                inner(inner_builder)
                outer_builder = TableBuilder(3)
                outer_builder.set_uint8(2)
                outer_builder.set_int16_array([3])
                outer_builder.set_table(inner_builder)
                expected = TableBuilder(2)
                expected.set_uint8_array(bytes(leading))
                expected.set_table(outer_builder)

                self.assertEqual(builder.bytes(), expected.bytes())

    def test_pads_tables_nested_in_place_at_any_depth_where_builders_put_them(
        self,
    ) -> None:
        """An array in the innermost of tables nested in place, each after a
        uint8, raises the alignment of every table it's nested in, which
        pads each where builders pad it"""

        def level(table: TableBuilder, left: int) -> None:
            table.set_uint8(left)
            if left == 0:
                table.set_float64_array([0.5, 1.5])
            else:
                table.set_table_in_place(level, left - 1, fields=2)

        def level_builder(left: int) -> TableBuilder:
            table = TableBuilder(2)
            table.set_uint8(left)
            if left == 0:
                table.set_float64_array([0.5, 1.5])
            else:
                table.set_table(level_builder(left - 1))
            return table

        for depth in range(1, 6):
            for leading in range(8):
                with self.subTest(depth=depth, leading=leading):
                    builder = TableBuilder(2)
                    builder.set_uint8_array(bytes(leading))
                    builder.set_table_in_place(level, depth - 1, fields=2)

                    expected = TableBuilder(2)
                    expected.set_uint8_array(bytes(leading))
                    expected.set_table(level_builder(depth - 1))

                    self.assertEqual(builder.bytes(), expected.bytes())

    def test_a_table_keeps_the_alignment_that_a_table_in_place_gave_it(
        self,
    ) -> None:
        """A table takes the alignment of a table written in place in it once
        that table ends: a later array of its own pads it no further, and
        nesting its builder pads it to that alignment, as builders do"""

        def inner(table: TableBuilder) -> None:
            table.set_float64_array([0.5])

        def middle(table: TableBuilder) -> None:
            table.set_uint8(1)
            table.set_table_in_place(inner, fields=1)
            table.set_uint16_array([2])

        for leading in range(8):
            with self.subTest(leading=leading):
                in_place = TableBuilder(2)
                in_place.set_uint8_array(bytes(leading))
                in_place.set_table_in_place(middle, fields=3)

                middle_builder = TableBuilder(3)
                middle(middle_builder)
                from_builder = TableBuilder(2)
                from_builder.set_uint8_array(bytes(leading))
                from_builder.set_table(middle_builder)

                inner_builder = TableBuilder(1)
                inner(inner_builder)
                builders = TableBuilder(3)
                builders.set_uint8(1)
                builders.set_table(inner_builder)
                builders.set_uint16_array([2])
                expected = TableBuilder(2)
                expected.set_uint8_array(bytes(leading))
                expected.set_table(builders)

                self.assertEqual(in_place.bytes(), expected.bytes())
                self.assertEqual(from_builder.bytes(), expected.bytes())

    def test_a_fill_that_raises_takes_back_the_padding_its_fields_gave(
        self,
    ) -> None:
        """The innermost of three tables nested in place pads the two it's
        nested in as its float64 array raises their alignment, and then
        raises: the middle table, which sets a uint16 instead, and the outer
        one are padded as if it had never been set"""

        def inner(table: TableBuilder) -> None:
            table.set_uint8(1)
            table.set_float64_array([0.5])
            raise FillError

        def middle(table: TableBuilder) -> None:
            table.set_uint8(2)
            with self.assertRaises(FillError):
                table.set_table_in_place(inner, fields=2)
            table.set_uint16(3)

        def outer(table: TableBuilder) -> None:
            table.set_uint8(4)
            table.set_int16_array([5])
            table.set_table_in_place(middle, fields=2)

        for leading in range(8):
            with self.subTest(leading=leading):
                builder = TableBuilder(2)
                builder.set_uint8_array(bytes(leading))
                builder.set_table_in_place(outer, fields=3)

                middle_builder = TableBuilder(2)
                middle_builder.set_uint8(2)
                middle_builder.set_uint16(3)
                outer_builder = TableBuilder(3)
                outer_builder.set_uint8(4)
                outer_builder.set_int16_array([5])
                outer_builder.set_table(middle_builder)
                expected = TableBuilder(2)
                expected.set_uint8_array(bytes(leading))
                expected.set_table(outer_builder)

                self.assertEqual(builder.bytes(), expected.bytes())

    def test_an_array_from_a_list_that_raises_leaves_the_tables_unpadded(
        self,
    ) -> None:
        """A list whose elements don't convert, in a table in place that its
        array would pad, raises and leaves the table as it was, and a list
        that converts is then written where a builder writes it, whatever the
        table's offset"""
        for leading in range(8):
            with self.subTest(leading=leading):

                def fill(table: TableBuilder) -> None:
                    with self.assertRaises(TypeError):
                        table.set_float64_array(cast(Any, [0.5, "1.5"]))
                    table.set_float64_array([0.5, 1.5])

                builder = TableBuilder(2)
                builder.set_uint8_array(bytes(leading))
                builder.set_table_in_place(fill, fields=1)

                nested = TableBuilder(1)
                nested.set_float64_array([0.5, 1.5])
                expected = TableBuilder(2)
                expected.set_uint8_array(bytes(leading))
                expected.set_table(nested)

                self.assertEqual(builder.bytes(), expected.bytes())

    def test_values_whose_iterator_uses_the_builder_leave_it_as_it_was(
        self,
    ) -> None:
        """The values' iterator runs before the first item and between items,
        when the builder holds the vector rather than its parent: a field or
        a write there raises, as within a fill, rather than writing over the
        vector"""
        attempts: list[str] = []

        class Using(list[float]):
            def __init__(self, builder: TableBuilder, values: list[float]) -> None:
                super().__init__(values)
                self.builder = builder

            @override
            def __iter__(self) -> Iterator[float]:
                for value in super().__iter__():
                    for name, use in [
                        ("set_uint8", lambda: self.builder.set_uint8(7)),
                        ("bytes", self.builder.bytes),
                        ("finalize", self.builder.finalize),
                    ]:
                        try:
                            use()
                        except ValueError as error:
                            attempts.append(f"{name}: {error}")
                    yield value

        builder = TableBuilder(2)
        builder.set_float32(0)
        builder.set_vector_in_place(Using(builder, VALUES), set_item, fields=2)

        expected = TableBuilder(2)
        expected.set_float32(0)
        expected.set_vector([_item_builder(value) for value in VALUES])
        self.assertEqual(builder.bytes(), expected.bytes())
        self.assertEqual(
            attempts,
            3
            * [
                "set_uint8: Can't add field 3 to a table of 2",
                "bytes: The builder was written while it recorded a nested table",
                "finalize: The builder was written while it recorded a nested table",
            ],
        )

    def test_holds_memory_in_proportion_to_a_vector_of_items_of_any_size(
        self,
    ) -> None:
        """Regression: the native builder made room for the rest of a vector's
        items as long as its first, so a first item of 10 KB before 2,000
        items of a few bytes held 20 MB for a table of 34 KB"""

        def fill(item: TableBuilder, value: int) -> None:
            item.set_uint8_array(bytes(10_000 if value == 0 else 4))

        tracemalloc.start()
        try:
            builder = TableBuilder(1)
            builder.set_vector_in_place(range(2_000), fill, fields=1)
            length = len(builder.bytes())
            _, peak = tracemalloc.get_traced_memory()
        finally:
            tracemalloc.stop()

        self.assertLess(peak, 2 * length + 64 * 1024)

    def test_holds_memory_in_proportion_to_a_vector_of_bytes(self) -> None:
        """Regression: the pure-Python builder held a view of each item of
        ``set_vector()``, a few hundred bytes for each, so 300,000 items of 8
        bytes held 102 MB for a table of 3.6 MB"""
        items = [bytes(8)] * 100_000

        tracemalloc.start()
        try:
            builder = TableBuilder(1)
            builder.set_vector(items)
            length = len(builder.bytes())
            _, peak = tracemalloc.get_traced_memory()
        finally:
            tracemalloc.stop()

        self.assertLess(peak, 2 * length + 64 * 1024)

    def test_a_builder_written_within_fill_raises_and_is_left_as_it_was(
        self,
    ) -> None:
        """The builder holds the nested table's fields while fill runs, so
        writing it there, or nesting it in another table, raises, and the
        in-place setter undoes the nested table"""
        writes: list[tuple[str, Callable[[TableBuilder], object]]] = [
            ("bytes", TableBuilder.bytes),
            ("finalize", TableBuilder.finalize),
            ("set_table", lambda item: TableBuilder(1).set_table(item)),
            ("set_vector", lambda item: TableBuilder(1).set_vector([item])),
        ]
        for name, write in writes:
            with self.subTest(write=name):

                def fill(
                    item: TableBuilder,
                    value: float,
                    write: Callable[[TableBuilder], object] = write,
                ) -> None:
                    set_item(item, value)
                    write(item)

                table = TableBuilder(2)
                table.set_float32(0)
                with self.assertRaisesRegex(ValueError, "was written while"):
                    table.set_table_in_place(fill, 1, fields=2)

                vector = TableBuilder(1)
                with self.assertRaisesRegex(ValueError, "was written while"):
                    vector.set_vector_in_place(VALUES, fill, fields=2)

                table.set_table_in_place(set_item, 1, fields=2)
                expected = TableBuilder(2)
                expected.set_float32(0)
                expected.set_table(_item_builder(1))
                self.assertEqual(table.bytes(), expected.bytes())
                self.assertEqual(items_of(VALUES, vector), items_from_builders(VALUES))

    def test_rejects_an_in_place_table_beyond_the_number_the_table_holds(
        self,
    ) -> None:
        filled: list[object] = []

        def fill(item: TableBuilder, *values: object) -> None:
            filled.append(values)

        builder = TableBuilder(0)
        with self.assertRaisesRegex(ValueError, "Can't add field 1 to a table of 0"):
            builder.set_table_in_place(fill, fields=0)
        with self.assertRaisesRegex(ValueError, "Can't add field 1 to a table of 0"):
            builder.set_vector_in_place([1], fill, fields=0)

        self.assertEqual(filled, [])


if __name__ == "__main__":
    unittest.main()
