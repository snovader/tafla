import unittest
from collections.abc import Callable
from typing import Any, cast

from hypothesis import assume, given
from hypothesis.strategies import booleans, integers, one_of, sampled_from

from tafla import FixedFields, TableBuilder, TableLayout, TableWriter
from tests.byte_order import big_endian_host
from tests.descriptions import tables
from tests.tables import (
    ELEMENT_SIZES,
    LONGEST,
    Description,
    OffsetType,
    Placement,
    build_table,
    nested_in_place,
    offset_size_of,
    read_table,
    write_table,
)

NARROW_OFFSET_TYPES: tuple[OffsetType, ...] = ("uint8", "uint16")


def outcome(write: Callable[[], memoryview[int]]) -> bytes | type[Exception]:
    """The bytes that ``write`` returns, or the class of the error it raises
    for a table too long for its offset type"""
    try:
        return bytes(write())
    except ValueError as error:
        return type(error)


def fits(table: Description) -> bool:
    """Whether a table and those nested in it fit their offset types, as their
    builders judge"""
    return not isinstance(outcome(lambda: build_table(table).bytes()), type)


def with_bytes(table: Description, length: int) -> Description:
    """The table with a uint8[] of ``length`` zeros after its fields, which
    isn't padded"""
    field = {"type": "uint8[]", "value": [0] * length}
    return {**table, "fields": [*table["fields"], field]}


def byte_swapped_arrays(data: memoryview[int], table: Description) -> bytes:
    """The bytes of a table that a writer wrote in place, with the bytes of
    each element of its arrays of scalars reversed"""
    swapped = bytearray(data)

    def swap(placement: Placement) -> None:
        kind = placement.field["type"]
        if not kind.endswith("[]") or kind == "struct[]":
            return

        size = ELEMENT_SIZES[kind.removesuffix("[]")]
        start = placement.position + placement.offset_size
        for element in range(len(placement.field["value"])):
            at = start + element * size
            swapped[at : at + size] = swapped[at : at + size][::-1]

    read_table(data, table, swap)
    return bytes(swapped)


class TestTableWriter(unittest.TestCase):
    # TableBuilder writes through a TableWriter too, so these compare the
    # length that the builder counts for each table as its fields are set,
    # padding included, which it checks against what its writer wrote, and
    # the calls that the test helpers make to a writer with those that the
    # builder makes. The bytes themselves are the conformance suite's.

    @given(tables())
    def test_writes_tables_as_long_as_table_builder_counts_them_and_the_same_bytes(
        self, table: Description
    ) -> None:
        table = nested_in_place(table)
        assume(fits(table))

        self.assertEqual(bytes(write_table(table)), bytes(build_table(table).bytes()))

    def test_pads_a_struct_array_a_nested_table_and_a_vector_as_table_builder_counts_them(
        self,
    ) -> None:
        # A uint8 leaves a uint32[] 3 bytes short of its alignment, and a
        # uint16 a nested table of float64s 1 byte short of its own
        nested = {"sized": False, "versioned": False, "offsetType": "uint16"}
        table: Description = {
            "layout": {"sized": True, "versioned": True, "offsetType": "uint16"},
            "version": 7,
            "fields": [
                {"type": "uint8", "value": 1},
                {"type": "uint32[]", "value": [2, 3]},
                {
                    "type": "struct[]",
                    "fields": ["uint16", "float64"],
                    "items": [[4, 5]],
                },
                {
                    "type": "table",
                    "from": "builder",
                    "table": {
                        "layout": nested,
                        "fields": [{"type": "float64[]", "value": [6]}],
                    },
                },
                {
                    "type": "vector",
                    "items": [
                        {
                            "from": "builder",
                            "table": {
                                "layout": nested,
                                "fields": [{"type": "string", "value": "🦀"}],
                            },
                        }
                    ],
                },
            ],
        }

        data = write_table(table)
        self.assertEqual(bytes(data), bytes(build_table(table).bytes()))
        self.assertEqual(read_table(data, table), table)

    @given(tables())
    def test_byte_swaps_the_elements_of_every_array_on_a_big_endian_host(
        self, table: Description
    ) -> None:
        table = nested_in_place(table)
        assume(fits(table))
        expected = byte_swapped_arrays(write_table(table), table)

        # The host's own order is little-endian, so each element's bytes come
        # out reversed, and everything else as it was
        with big_endian_host():
            self.assertEqual(bytes(write_table(table)), expected)
            self.assertEqual(bytes(build_table(table).bytes()), expected)


class TestTableLength(unittest.TestCase):
    @given(tables(offset_types=NARROW_OFFSET_TYPES))
    def test_writes_the_longest_table_that_its_offsets_address_and_rejects_a_longer_one(
        self, table: Description
    ) -> None:
        table = nested_in_place(table)
        longest_length = LONGEST[table["layout"]["offsetType"]]

        shortest = with_bytes(table, 0)
        assume(fits(shortest))
        room = longest_length - len(build_table(shortest).bytes())

        longest = write_table(with_bytes(table, room))
        self.assertEqual(len(longest), longest_length)
        self.assertEqual(
            bytes(longest), bytes(build_table(with_bytes(table, room)).bytes())
        )

        with self.assertRaises(ValueError):
            write_table(with_bytes(table, room + 1))

    @given(tables(offset_types=NARROW_OFFSET_TYPES), integers(0, 300))
    def test_raises_for_any_table_that_table_builder_rejects_as_too_long(
        self, table: Description, length: int
    ) -> None:
        longer = nested_in_place(with_bytes(table, length))

        self.assertEqual(
            outcome(lambda: write_table(longer)),
            outcome(lambda: build_table(longer).bytes()),
        )

    @given(sampled_from(NARROW_OFFSET_TYPES), booleans())
    def test_rejects_a_header_longer_than_the_offsets_address_as_table_builder_does(
        self, offset_type: OffsetType, sized: bool
    ) -> None:
        layout = TableLayout(sized=sized, offset_type=offset_type)
        fields = LONGEST[offset_type] // offset_size_of(offset_type) - int(sized)

        TableWriter(fields, layout)
        with self.assertRaises(ValueError):
            TableBuilder(fields + 1, layout)
        with self.assertRaises(ValueError):
            TableWriter(fields + 1, layout)

    @given(sampled_from(NARROW_OFFSET_TYPES))
    def test_starts_a_nested_table_whose_slots_fit_and_leaves_its_length_to_bytes(
        self, offset_type: OffsetType
    ) -> None:
        size = offset_size_of(offset_type)
        writer = TableWriter(1, TableLayout(offset_type=offset_type))
        writer.set_table(0, 1)

        writer.end_table(writer.start_table(LONGEST[offset_type] // size))
        # With the root's slot before them, the table is too long
        with self.assertRaisesRegex(ValueError, f"too long for {offset_type} offsets"):
            writer.bytes()

    @given(
        sampled_from(list(LONGEST)),
        one_of(integers(1, 2**70), sampled_from([2**63 - 1, 2**64])),
    )
    def test_rejects_a_nested_table_whose_slots_alone_are_longer_than_the_offsets_address(
        self, offset_type: OffsetType, beyond: int
    ) -> None:
        """Rather than allocating them, which raised MemoryError or
        OverflowError, by the count and the implementation"""
        size = offset_size_of(offset_type)
        writer = TableWriter(1, TableLayout(offset_type=offset_type))
        writer.set_table(0, 1)

        fields = LONGEST[offset_type] // size + beyond
        length = size + fields * size
        with self.assertRaises(ValueError) as raised:
            writer.start_table(fields)
        self.assertEqual(
            str(raised.exception),
            f"The table's fields make it {length} bytes, "
            f"too long for {offset_type} offsets",
        )

    def test_names_the_offset_type_that_the_table_is_too_long_for(self) -> None:
        for offset_type in NARROW_OFFSET_TYPES:
            with self.subTest(offset_type=offset_type):
                layout = TableLayout(offset_type=offset_type)
                longest = LONGEST[offset_type]

                # The struct's offset fits, but the table ends past the longest
                ends_too_late = TableWriter(1, layout)
                ends_too_late.set_struct(0, bytes(longest))
                length = longest + offset_size_of(offset_type)
                with self.assertRaises(ValueError) as raised:
                    ends_too_late.bytes()
                self.assertEqual(
                    str(raised.exception),
                    f"The table's fields make it {length} bytes, "
                    f"too long for {offset_type} offsets",
                )

                # The uint8's offset is past the longest table
                starts_too_late = TableWriter(2, layout)
                starts_too_late.set_struct(0, bytes(longest))
                length = longest + 2 * offset_size_of(offset_type)
                with self.assertRaises(ValueError) as raised:
                    starts_too_late.set_uint8(1, 1)
                self.assertEqual(
                    str(raised.exception),
                    f"The table's fields make it {length} bytes, "
                    f"too long for {offset_type} offsets",
                )

                # The string's length doesn't fit, so the table would end
                # after its bytes, past the longest table
                counts_too_many = TableWriter(1, layout)
                length = 2 * offset_size_of(offset_type) + longest + 1
                with self.assertRaises(ValueError) as raised:
                    counts_too_many.set_string(0, "a" * (longest + 1))
                self.assertEqual(
                    str(raised.exception),
                    f"The table's fields make it {length} bytes, "
                    f"too long for {offset_type} offsets",
                )


class TestVersions(unittest.TestCase):
    def test_rejects_the_versions_that_table_builder_rejects(self) -> None:
        versioned = TableLayout(versioned=True, offset_type="uint8")
        cases: list[tuple[TableLayout, int | None, type[Exception]]] = [
            (TableLayout.DEFAULT, 1, TypeError),
            (versioned, None, TypeError),
            (versioned, 256, ValueError),
            (versioned, -1, ValueError),
        ]

        for layout, version, error in cases:
            with self.subTest(layout=layout, version=version):
                with self.assertRaises(error):
                    TableBuilder(0, cast(Any, layout), cast(Any, version))
                with self.assertRaises(error):
                    TableWriter(0, cast(Any, layout), cast(Any, version))


class TestValues(unittest.TestCase):
    def test_raises_the_errors_that_table_builder_raises_for_values_it_cant_write(
        self,
    ) -> None:
        cases: list[tuple[str, object, type[Exception]]] = [
            ("set_uint8", 256, ValueError),
            ("set_int64", 2**63, ValueError),
            ("set_float32", 1e39, ValueError),
            ("set_uint16_array", [70000], OverflowError),
            ("set_float64_array", ["x"], TypeError),
            ("set_string", "\ud800", UnicodeEncodeError),
        ]

        for setter, value, error in cases:
            with self.subTest(setter=setter, value=value):
                builder = TableBuilder(1)
                writer = TableWriter(1)
                with self.assertRaises(error):
                    getattr(builder, setter)(value)
                    builder.finalize()
                with self.assertRaises(error):
                    getattr(writer, setter)(0, value)

    def test_converts_values_as_the_package_docstring_states(self) -> None:
        nan = float("nan")
        raised: list[tuple[str, object, type[Exception]]] = [
            ("set_uint8", 300, ValueError),
            ("set_uint8", -1, ValueError),
            ("set_uint8_array", [300], OverflowError),
            ("set_uint8", 1.5, ValueError),
            ("set_uint8", nan, ValueError),
            ("set_uint8_array", [1.5], TypeError),
            ("set_uint8_array", [nan], TypeError),
            ("set_float32", 1e300, ValueError),
            ("set_string", "\ud800", UnicodeEncodeError),
        ]
        for setter, value, error in raised:
            with self.subTest(setter=setter, value=value):
                builder = TableBuilder(1)
                with self.assertRaises(error):
                    getattr(builder, setter)(value)
                    builder.finalize()
                with self.assertRaises(error):
                    getattr(TableWriter(1), setter)(0, value)

        # A float32 array writes infinity of the value's sign, as array("f")
        # converts it, and rounds a float within range to the nearest float32
        written: list[tuple[str, object, bytes]] = [
            ("set_float32_array", [1e300, -1e300], b"\x00\x00\x80\x7f\x00\x00\x80\xff"),
            ("set_float32", 0.1, b"\xcd\xcc\xcc\x3d"),
        ]
        for setter, value, expected in written:
            with self.subTest(setter=setter, value=value):
                builder = TableBuilder(1)
                getattr(builder, setter)(value)
                writer = TableWriter(1)
                getattr(writer, setter)(0, value)
                self.assertEqual(bytes(writer.bytes()), bytes(builder.bytes()))
                self.assertTrue(bytes(writer.bytes()).endswith(expected))

    def test_raises_type_error_for_a_struct_that_isnt_c_contiguous_as_table_builder_does(
        self,
    ) -> None:
        strided = memoryview(bytes(range(8)))[::2]
        cases: list[tuple[str, tuple[object, ...]]] = [
            ("set_struct", (strided,)),
            ("set_struct_array", (strided, 2, 2)),
        ]

        for setter, arguments in cases:
            with self.subTest(setter=setter):
                with self.assertRaises(TypeError):
                    getattr(TableBuilder(1), setter)(*arguments)
                with self.assertRaises(TypeError):
                    getattr(TableWriter(1), setter)(0, *arguments)


class TestIndices(unittest.TestCase):
    def test_raises_value_error_for_an_index_or_item_that_has_no_slot_in_the_bytes(
        self,
    ) -> None:
        """Indices past the bytes, and past the C ssize_t that struct takes
        positions as, which once raised OverflowError"""
        writes: list[tuple[str, Callable[[TableWriter, int], object]]] = [
            ("set_uint8", lambda writer, index: writer.set_uint8(index, 1)),
            ("set_float64", lambda writer, index: writer.set_float64(index, 0.5)),
            ("set_string", lambda writer, index: writer.set_string(index, "a")),
            ("set_struct", lambda writer, index: writer.set_struct(index, b"\x01")),
            (
                "set_int32_array",
                lambda writer, index: writer.set_int32_array(index, [1]),
            ),
            ("set_table", lambda writer, index: writer.set_table(index, 4)),
            ("set_vector", lambda writer, index: writer.set_vector(index, 0)),
            ("set_item_end", lambda writer, item: writer.set_item_end(4, item)),
            ("set_item_end", lambda writer, vector: writer.set_item_end(vector, 0)),
        ]

        for index in [16, 2**63, 2**100, 2**15000]:
            for name, write in writes:
                with self.subTest(write=name, bits=index.bit_length()):
                    writer = TableWriter(1)
                    if name == "set_item_end":
                        writer.set_vector(0, 1)

                    with self.assertRaises(ValueError):
                        write(writer, index)

    @given(tables(), integers(min_value=0), one_of(integers(-4, 64), integers()))
    def test_writes_nothing_outside_its_bytes_whatever_the_index_of_a_field_or_the_item_of_an_end(
        self, table: Description, call: int, value: int
    ) -> None:
        """The table's bytes are undefined then, but the writer's bytes bound
        them"""
        table = nested_in_place(table)
        assume(fits(table))
        length = len(write_table(table))
        counter = IndexReplacer(-1, 0)
        write_table(table, wrap=counter.wrap)
        assume(counter.calls > 0)

        replacer = IndexReplacer(call % counter.calls, value)
        try:
            data = write_table(table, wrap=replacer.wrap)
        except ValueError:
            return

        self.assertEqual(len(data), length)


class IndexReplacer:
    """Passes a writer its calls, with the index of one field or the item of
    one end replaced by another, as a caller other than the generated code
    might pass it"""

    def __init__(self, call: int, value: int) -> None:
        """Replaces the index or item of call ``call`` of those that take one"""
        self.calls = 0
        self.__call = call
        self.__value = value
        self.__writer: TableWriter | None = None

    def wrap(self, writer: TableWriter) -> Any:
        self.__writer = writer
        return self

    def __getattr__(self, name: str) -> Any:
        method = getattr(self.__writer, name)
        # set_item_end(vector, item), and every other setter set_*(index, ...)
        if name == "set_item_end":
            argument = 1
        elif name.startswith("set_"):
            argument = 0
        else:
            return method

        def forward(*args: Any) -> Any:
            replaced = list(args)
            if self.calls == self.__call:
                replaced[argument] = self.__value
            self.calls += 1
            return method(*replaced)

        return forward


def second_uint8(writer: TableWriter) -> bytes:
    """Sets field 1 of ``writer`` to the uint8 2, and returns its bytes"""
    writer.set_uint8(1, 2)
    return bytes(writer.bytes())


class TestFailedSetters(unittest.TestCase):
    @given(
        sampled_from(
            [
                ("set_uint8", 256),
                ("set_int8", -129),
                ("set_uint16", -1),
                ("set_int32", 2**31),
                ("set_uint64", 2**64),
                ("set_float32", 1e39),
            ]
        ),
        sampled_from(["uint8", "uint16", "uint32"]),
    )
    def test_a_scalar_that_doesnt_fit_leaves_the_table_as_it_was(
        self, case: tuple[str, float], offset_type: OffsetType
    ) -> None:
        setter, value = case
        layout = TableLayout(offset_type=offset_type)
        writer = TableWriter(2, layout)

        with self.assertRaises(ValueError):
            getattr(writer, setter)(0, value)

        # Field 0 is still absent, rather than pointing at field 1's value
        self.assertEqual(second_uint8(writer), second_uint8(TableWriter(2, layout)))


class TestAlignments(unittest.TestCase):
    def test_rejects_an_alignment_other_than_1_2_4_or_8_and_writes_nothing(
        self,
    ) -> None:
        for alignment in [0, 3, -4, 16, 5, 2.0, True, [2]]:
            message = f"An alignment of {alignment!r} isn't 1, 2, 4 or 8 bytes"
            with self.subTest(alignment=alignment):
                writer = TableWriter(2)
                with self.assertRaises(ValueError) as raised:
                    writer.set_table(0, cast(Any, alignment))
                self.assertEqual(str(raised.exception), message)
                with self.assertRaises(ValueError) as raised:
                    writer.set_struct_array(0, b"\x01", 1, cast(Any, alignment))
                self.assertEqual(str(raised.exception), message)
                self.assertEqual(second_uint8(writer), second_uint8(TableWriter(2)))

                builder = TableBuilder(1)
                with self.assertRaises(ValueError) as raised:
                    builder.set_table(b"", cast(Any, alignment))
                self.assertEqual(str(raised.exception), message)
                with self.assertRaises(ValueError) as raised:
                    builder.set_struct_array(b"\x01", 1, cast(Any, alignment))
                self.assertEqual(str(raised.exception), message)


class TestBytes(unittest.TestCase):
    def test_a_memoryview_of_the_writer_is_its_bytes_with_its_length(
        self,
    ) -> None:
        for layout in [TableLayout.DEFAULT, TableLayout(sized=True)]:
            with self.subTest(layout=layout):
                writer = TableWriter(1, layout)
                writer.set_uint8(0, 7)
                with memoryview(writer) as view:
                    exported = bytes(view)
                self.assertEqual(exported, bytes(writer.bytes()))
                if layout.sized():
                    self.assertEqual(exported[0], len(exported))

    def test_a_memoryview_of_a_writer_too_long_for_its_offsets_raises(
        self,
    ) -> None:
        writer = TableWriter(1, TableLayout(offset_type="uint8"))
        writer.set_struct(0, bytes(300))
        for export in [writer.bytes, lambda: memoryview(writer)]:
            with self.assertRaisesRegex(ValueError, "too long for uint8 offsets"):
                export()

    def test_writes_in_place_while_a_view_is_held_what_appends_no_bytes(
        self,
    ) -> None:
        def write(writer: TableWriter) -> None:
            """No leading fixed fields, nested tables of no fields and an
            empty struct, whose offsets are written in the root's slots"""
            writer.set_fields(FixedFields(3), ())
            writer.set_table(0, 1)
            parent = writer.start_table(0)
            writer.set_fields(FixedFields(0), ())
            writer.end_table(parent)
            writer.set_struct(1, b"")
            writer.set_table(2, 1)
            writer.write_table(FixedFields(0), ())

        write(expected := TableWriter(3))

        writer = TableWriter(3)
        with memoryview(writer) as view:
            write(writer)
            self.assertEqual(bytes(view), bytes(expected.bytes()))

    def test_raises_buffer_error_for_fixed_fields_while_a_view_is_held_once_they_pack(
        self,
    ) -> None:
        fields = FixedFields(2, "b", string=True)
        writer = TableWriter(2)
        with memoryview(writer):
            with self.assertRaises(BufferError):
                writer.set_fields(fields, (1,), "")
            # The value doesn't pack, and raises what set_int8() raises
            with self.assertRaisesRegex(ValueError, "Can't write 1000 at 8"):
                writer.set_fields(fields, (1000,), "")

        # Each raises what start_table() raises for the slots that it appends
        nested = TableWriter(1)
        nested.set_table(0, 1)
        with memoryview(nested):
            for value in [1, 1000]:
                with self.assertRaises(BufferError):
                    nested.write_table(fields, (value,), "")

    def test_takes_fields_again_once_its_views_are_released(self) -> None:
        writer = TableWriter(2, TableLayout(sized=True))
        writer.set_uint8(0, 1)
        views = [writer.bytes(), memoryview(writer)]

        for view in views:
            with self.assertRaises(BufferError):
                writer.set_uint8(1, 2)
            view.release()

        writer.set_uint8(1, 2)
        data = bytes(writer.bytes())
        self.assertEqual(data[0], len(data))
        self.assertEqual(data[-2:], b"\x01\x02")


if __name__ == "__main__":
    unittest.main()
