import unittest
from array import array
from collections.abc import Buffer, Callable
from struct import pack
from typing import Any, cast

from hypothesis import given
from hypothesis.strategies import integers

from tafla import (
    ByteBuffer,
    TableBuilder,
    TableLayout,
)
from tests.descriptions import tables
from tests.tables import (
    Description,
    OffsetType,
    alignment_of,
    build_table,
    offset_size_of,
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


def root_vector(buffer: ByteBuffer, index: int) -> list[bytes]:
    """Reads the items of the vector of a root field"""
    return [bytes(item) for item in buffer.get_vector(buffer.root_offset(index))]


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

    def test_rejects_a_scalar_that_doesnt_fit_its_type_from_finalize(self) -> None:
        for setter, value in [("set_uint32", 2**32), ("set_float32", 1e300)]:
            builder = TableBuilder(1)
            getattr(builder, setter)(value)

            with self.subTest(setter=setter), self.assertRaises(ValueError):
                builder.finalize()

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

    def test_writes_values_as_they_are_when_finalizing(self) -> None:
        values = array("f", [1, 2])
        builder = TableBuilder(1)
        builder.set_float32_array(values)
        values[0] = 3

        buffer = builder.finalize()

        self.assertEqual(
            buffer.get_float32_array(buffer.root_offset(0)).tolist(), [3, 2]
        )

    def test_keeps_a_converted_list_as_it_was_when_set(self) -> None:
        values = [1.0, 2.0]
        builder = TableBuilder(1)
        builder.set_float32_array(values)
        values.append(3.0)

        buffer = builder.finalize()

        self.assertEqual(
            buffer.get_float32_array(buffer.root_offset(0)).tolist(), [1, 2]
        )

    def test_keeps_a_vector_as_it_was_when_set(self) -> None:
        vector = [b"ab", b"c"]
        builder = TableBuilder(1)
        builder.set_vector(vector)
        vector.pop()

        buffer = builder.finalize()

        self.assertEqual(root_vector(buffer, 0), [b"ab", b"c"])


class TestStructs(unittest.TestCase):
    def test_writes_a_struct_as_it_is_without_padding(self) -> None:
        struct = pack("<Hd", 0xFEDC, -0.0)
        builder = TableBuilder(2)
        builder.set_uint8(1)
        builder.set_struct(struct)

        buffer = builder.finalize()

        self.assertEqual(buffer.root_offset(1), 9)
        self.assertEqual(bytes(buffer.get_struct(9, len(struct))), struct)

    def test_rejects_struct_sizes_and_bytes_it_cant_write_and_stays_as_it_was(
        self,
    ) -> None:
        cases = [
            (bytes(5), 2, "5 bytes aren't a whole number of 2-byte structs"),
            (bytes(4), 0, "A struct's size must be a positive integer, not 0"),
            (bytes(4), -1, "A struct's size must be a positive integer, not -1"),
        ]

        for data, size, message in cases:
            with self.subTest(size=size):
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
            for name, set_field in setters:
                with self.subTest(setter=name, format=strided.format):
                    builder = TableBuilder(1)

                    with self.assertRaisesRegex(TypeError, "isn't C-contiguous"):
                        set_field(builder, strided)

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
        buffer = builder.finalize()

        self.assertEqual(buffer.root_offset(1), 12)
        self.assertEqual(buffer.get_uint32(12), 2)
        self.assertEqual(len(buffer.get_struct_array(12, 8)), 16)

    def test_aligns_a_nested_builder_to_its_arrays_of_structs(self) -> None:
        nested = TableBuilder(1)
        nested.set_struct_array(bytes(9), 9, 8)

        parent = TableBuilder(2)
        parent.set_uint8(7)
        parent.set_table(nested)

        # After the uint8 at 8, the nested table is padded to 16, so that its
        # structs, 8 bytes from its start, start at 24
        self.assertEqual(parent.finalize().root_offset(1), 16)


def nested_uint8_array(values: bytearray) -> TableBuilder:
    """A builder of one uint8[] of ``values``, which a parent writes in place"""
    nested = TableBuilder(1)
    nested.set_uint8_array(values)
    return nested


def float64s_and_uint8(values: array[float]) -> TableBuilder:
    """A float64[] after the header at 8: its count at 12, after 4 bytes of
    padding, and its elements from 16, followed by a uint8"""
    builder = TableBuilder(2)
    builder.set_float64_array(values)
    builder.set_uint8(7)
    return builder


class TestChangedLengths(unittest.TestCase):
    """Buffers whose length changed after they were set, which the builder's
    contract rules out. The writer writes each offset as it reaches its
    field, so the table's length is what shows the change."""

    def test_raises_value_error_for_an_array_that_grew_or_shrank_and_writes_it_once_restored(
        self,
    ) -> None:
        expected = bytes(float64s_and_uint8(array("d", [1, 2, 3])).bytes())
        cases = [
            ("grew", [1, 2, 3, 4], "The fields wrote 49 bytes rather than the 41"),
            ("shrank", [1, 2], "The fields wrote 33 bytes rather than the 41"),
        ]

        for change, changed, message in cases:
            with self.subTest(change=change):
                values = array("d", [1, 2, 3])
                builder = float64s_and_uint8(values)

                values[:] = array("d", changed)
                with self.assertRaisesRegex(ValueError, message):
                    builder.bytes()
                with self.assertRaisesRegex(ValueError, message):
                    builder.finalize()

                values[:] = array("d", [1, 2, 3])
                self.assertEqual(builder.bytes(), expected)
                self.assertEqual(builder.finalize().bytes(), expected)

    def test_raises_value_error_for_any_buffer_whose_length_changed_the_tables(
        self,
    ) -> None:
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
            for change in ["pop", "append"]:
                with self.subTest(field=name, change=change):
                    builder = TableBuilder(2)
                    set_buffer(builder, values)
                    builder.set_uint8(7)

                    original = values[:]
                    if change == "pop":
                        values.pop()
                    else:
                        values.append(0)

                    with self.assertRaisesRegex(ValueError, "The fields wrote"):
                        builder.finalize()

                    values[:] = original
                    unchanged = TableBuilder(2)
                    set_buffer(unchanged, original)
                    unchanged.set_uint8(7)
                    self.assertEqual(builder.bytes(), unchanged.bytes())

    def test_writes_the_values_of_buffers_whose_lengths_changed_but_not_the_tables(
        self,
    ) -> None:
        # The uint8[]'s count is at 8 and its bytes at 12, and the float64[]'s
        # count at 20, after padding, and its elements at 24. A uint8[] of up
        # to 8 bytes takes some of that padding, and leaves the table's length
        # as it was.
        values = bytearray(b"ab")
        builder = TableBuilder(2)
        builder.set_uint8_array(values)
        builder.set_float64_array([1.5])

        values.append(ord("c"))
        buffer = builder.finalize()

        self.assertEqual(bytes(buffer.get_uint8_array(buffer.root_offset(0))), b"abc")
        self.assertEqual(
            buffer.get_float64_array(buffer.root_offset(1)).tolist(), [1.5]
        )
        self.assertEqual(buffer.root_offset(1), 20)


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
                layout = TableLayout(versioned=True, offset_type=offset_type)
                message = f"Version {version} doesn't fit in a {offset_type} slot"

                with self.assertRaises(ValueError) as raised:
                    TableBuilder(1, layout, version)
                self.assertEqual(str(raised.exception), message)

    def test_rejects_a_version_missing_from_or_given_for_a_layout(self) -> None:
        with self.assertRaisesRegex(TypeError, "needs a version"):
            TableBuilder(1, cast(Any, TableLayout(versioned=True)))

        with self.assertRaisesRegex(TypeError, "has no version slot"):
            TableBuilder(1, cast(Any, TableLayout()), 1)

    def test_rejects_reading_the_version_of_an_unversioned_table(self) -> None:
        layout = TableLayout(sized=True)
        buffer = ByteBuffer(TableBuilder(0, layout).bytes(), layout)

        with self.assertRaises(TypeError):
            buffer.get_version()


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
        """A builder that was finalized, or whose layout has a header or
        another offset type than its parent's, is copied as a table nested as
        bytes is; any other is written in place, as the same bytes"""
        own = bytes(build_table(table).bytes())
        finalized = build_table(table)
        finalized.finalize()

        expected = parent_of(lambda: own, length)
        self.assertEqual(parent_of(lambda: build_table(table), length), expected)
        self.assertEqual(parent_of(lambda: finalized, length), expected)

        buffer = ByteBuffer(expected)
        self.assertEqual(root_vector(buffer, 2), [own, own])

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

        buffer = parent.finalize()
        table = buffer.root_offset(0)

        self.assertEqual(buffer.get_uint8(buffer.field_offset(table, 1)), 2)
        self.assertEqual(root_vector(buffer, 1), [nested.bytes()])

    def test_writes_a_builder_nested_several_times_the_same_each_time(self) -> None:
        nested = TableBuilder(2)
        nested.set_float64_array([1, 2])
        nested.set_string("nested")

        # Written straight into the parent, and then copied once finalized
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

    def test_writes_a_nested_builders_values_as_they_are_when_finalized(self) -> None:
        values = array("f", [1, 2])
        nested = TableBuilder(1)
        nested.set_float32_array(values)

        parent = TableBuilder(1)
        parent.set_table(nested)
        values[0] = 3

        buffer = parent.finalize()
        position = buffer.field_offset(buffer.root_offset(0), 0)

        self.assertEqual(buffer.get_float32_array(position).tolist(), [3, 2])


if __name__ == "__main__":
    unittest.main()
