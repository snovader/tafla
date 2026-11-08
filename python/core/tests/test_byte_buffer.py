import mmap
import os
import sys
import tempfile
import unittest
from array import array
from collections.abc import Callable
from struct import calcsize, iter_unpack, pack, unpack
from unittest.mock import patch

from hypothesis import given
from hypothesis.strategies import (
    DataObject,
    DrawFn,
    SearchStrategy,
    binary,
    composite,
    data,
    integers,
    lists,
    one_of,
    sampled_from,
)

from tafla import SIZEOF_32, ByteBuffer, DecodeError, TableBuilder, TableLayout
from tafla.types import IntTypeCode
from tests.descriptions import (
    ARRAY_FIELDS,
    MAX_ELEMENTS,
    MAX_ITEMS,
    OFFSET_TYPES,
    STRINGS,
    scalar_values,
)
from tests.tables import (
    DEFAULT_LAYOUT,
    FORMATS,
    Description,
    OffsetType,
    build_table,
    offset_size_of,
)

# A call of one of ByteBuffer's setters: its name, its arguments, and the
# bytes that it writes
type Write = tuple[str, tuple[object, ...], bytes]


@composite
def strided_views(draw: DrawFn) -> memoryview:
    """Views with a step of bytes, or of wider elements, which are C-contiguous
    only when they hold at most one element"""
    codes: list[IntTypeCode] = ["B", "h", "i", "q"]
    type_code = draw(sampled_from(codes))
    count = draw(integers(0, 8))
    size = count * calcsize(type_code)
    data = draw(binary(min_size=size, max_size=size))
    step = draw(sampled_from([-3, -2, -1, 2, 3]))

    return memoryview(data).cast(type_code)[::step]


def writes(offset_type: OffsetType) -> SearchStrategy[Write]:
    """Calls of every setter of a ByteBuffer, with the bytes that ``struct``
    packs little-endian for their values, and counts, lengths, item ends and
    header slots of the offset type"""
    length = f"<{FORMATS[offset_type]}"
    largest = 2 ** (8 * offset_size_of(offset_type)) - 1

    def scalar(kind: str) -> SearchStrategy[Write]:
        return scalar_values(kind).map(
            lambda value: (f"set_{kind}", (value,), pack(f"<{FORMATS[kind]}", value))
        )

    def array_of(kind: str) -> SearchStrategy[Write]:
        def write(values: list[float]) -> Write:
            elements = pack(f"<{len(values)}{FORMATS[kind]}", *values)
            return f"set_{kind}_array", (values,), pack(length, len(values)) + elements

        return lists(scalar_values(kind), max_size=MAX_ELEMENTS).map(write)

    def header_slot(setter: str) -> SearchStrategy[Write]:
        return integers(0, largest).map(
            lambda value: (setter, (value,), pack(length, value))
        )

    @composite
    def struct_arrays(draw: DrawFn) -> Write:
        size = draw(integers(1, 9))
        count = draw(integers(0, 3))
        data = draw(binary(min_size=size * count, max_size=size * count))
        return "set_struct_array", (data, size), pack(length, count) + data

    def vector(items: list[bytes]) -> Write:
        # The count, the end of each item from the vector's position, and
        # then the items
        end = (1 + len(items)) * offset_size_of(offset_type)
        ends = []
        for item in items:
            end += len(item)
            ends.append(pack(length, end))

        written = pack(length, len(items)) + b"".join(ends) + b"".join(items)
        return "set_vector", (items,), written

    def string(encoded: bytes) -> Write:
        return "set_string", (encoded,), pack(length, len(encoded)) + encoded

    return one_of(
        *(scalar(kind) for kind in FORMATS),
        *(array_of(kind) for kind in FORMATS),
        *(header_slot(setter) for setter in ["set_size", "set_version", "set_offset"]),
        binary(max_size=MAX_ELEMENTS).map(lambda data: ("set_bytes", (data,), data)),
        integers(0, 7).map(lambda length: ("set_padding", (length,), bytes(length))),
        STRINGS.map(str.encode).map(string),
        struct_arrays(),
        lists(binary(max_size=MAX_ELEMENTS), max_size=MAX_ITEMS).map(vector),
    )


class TestByteBuffer(unittest.TestCase):
    @given(strided_views())
    def test_takes_bytes_only_when_theyre_c_contiguous(self, view: memoryview) -> None:
        """Reads return views of the bytes, which a copy of them wouldn't be"""
        if view.c_contiguous:
            self.assertEqual(bytes(ByteBuffer(view).bytes()), view.tobytes())
        else:
            with self.assertRaisesRegex(TypeError, "isn't C-contiguous"):
                ByteBuffer(view)

    def test_rejects_bytes_to_write_that_arent_c_contiguous_and_writes_nothing(
        self,
    ) -> None:
        backing = bytearray(b"\xaa" * 32)
        buffer = ByteBuffer(backing)
        strided = memoryview(bytes(range(8)))[::2]

        writes = [
            lambda: buffer.set_bytes(strided),
            lambda: buffer.set_string(strided),
            lambda: buffer.set_struct_array(strided, 2),
            lambda: buffer.set_vector([b"\x01", strided]),
        ]

        for write in writes:
            with (
                self.subTest(),
                self.assertRaisesRegex(TypeError, "isn't C-contiguous"),
            ):
                write()

        self.assertEqual(backing, b"\xaa" * 32)

    @unittest.skipUnless(sys.byteorder == "little", "Big-endian hosts read copies")
    @given(ARRAY_FIELDS, integers(1, 7))
    def test_reads_arrays_at_any_alignment_as_views_of_the_bytes(
        self, field: Description, misalignment: int
    ) -> None:
        table = {"layout": DEFAULT_LAYOUT, "fields": [field]}
        backing = bytearray(misalignment) + build_table(table).bytes()
        buffer = ByteBuffer(memoryview(backing)[misalignment:])

        element = field["type"].removesuffix("[]")
        values = getattr(buffer, f"get_{element}_array")(buffer.root_offset(0))

        self.assertIs(values.obj, backing)

    def test_swaps_each_elements_bytes_on_big_endian_hosts(self) -> None:
        values = array("i", [1, -2])

        # Take the big-endian branches on any host. The host's own order is
        # little-endian, so each element's bytes come out reversed.
        with patch("tafla.elements.IS_LITTLE_ENDIAN", False):
            builder = TableBuilder(1)
            builder.set_int32_array(values)
            serialized = bytes(builder.bytes())

            read = ByteBuffer(serialized).get_int32_array(SIZEOF_32)

        self.assertEqual(values.tolist(), [1, -2])
        self.assertEqual(serialized[2 * SIZEOF_32 :], b"\0\0\0\x01\xff\xff\xff\xfe")
        self.assertEqual(read.tolist(), [1, -2])
        self.assertIsNot(read.obj, serialized)

    def test_writes_nothing_past_the_supplied_bytes(self) -> None:
        backing = bytearray(b"\xaa" * 32)
        buffer = ByteBuffer(memoryview(backing)[:12])

        writes = [
            lambda: buffer.set_float32_array(array("f", [1, 2, 3])),
            lambda: buffer.set_float32_array([1, 2, 3]),
            lambda: buffer.set_string(bytes(9)),
            lambda: buffer.set_struct_array(bytes(9), 3),
            lambda: buffer.set_vector([bytes(4), bytes(1)]),
            lambda: buffer.set_bytes(bytes(13)),
            lambda: buffer.set_padding(13),
        ]

        for write in writes:
            with self.subTest(), self.assertRaises(ValueError):
                write()

        buffer.set_bytes(bytes(8))
        with self.assertRaises(ValueError):
            buffer.set_float64(1.5)

        self.assertEqual(backing, bytes(8) + b"\xaa" * 24)

    def test_rejects_a_value_that_doesnt_fit_its_slot_and_keeps_its_position(
        self,
    ) -> None:
        # CPython's struct zeroes a slot before it checks the value, so only
        # the bytes after the slot are left as they were
        for write, value, size in [
            ("set_uint8", 256, 1),
            ("set_int16", -(2**15) - 1, 2),
            ("set_uint32", 2**32, 4),
            ("set_uint64", -1, 8),
            ("set_float32", 1e300, 4),
            ("set_float32", -1e300, 4),
            ("set_offset", 2**32, 4),
        ]:
            with self.subTest(write=write, value=value):
                backing = bytearray(b"\xaa" * 12)
                buffer = ByteBuffer(backing)

                with self.assertRaises(ValueError):
                    getattr(buffer, write)(value)
                buffer.set_uint8(1)

                self.assertEqual(backing[0], 1)
                self.assertEqual(backing[size:], b"\xaa" * (12 - size))

    def test_rejects_a_count_length_or_item_end_too_large_for_its_slot_and_writes_nothing(
        self,
    ) -> None:
        backing = bytearray(b"\xaa" * 600)
        buffer = ByteBuffer(backing, TableLayout(offset_type="uint8"))

        writes = [
            lambda: buffer.set_uint8_array(bytes(256)),
            lambda: buffer.set_int16_array(array("h", range(256))),
            lambda: buffer.set_string(bytes(256)),
            lambda: buffer.set_struct_array(bytes(256), 1),
            # The last item ends 256 bytes from the vector's count
            lambda: buffer.set_vector([bytes(1), bytes(252)]),
            lambda: buffer.set_vector([b""] * 256),
        ]

        for write in writes:
            with self.subTest():
                with self.assertRaisesRegex(
                    ValueError, "a uint8 count, length or item end holds at most 255"
                ):
                    write()
                self.assertEqual(backing, b"\xaa" * 600)

        # The count, then the ends of the items, which start at 3
        buffer.set_vector([bytes(1), bytes(251)])
        self.assertEqual(backing[:3], b"\x02\x04\xff")

    def test_rejects_struct_sizes_less_than_1_and_bytes_that_arent_whole_structs(
        self,
    ) -> None:
        backing = bytearray(b"\xaa" * 16)
        buffer = ByteBuffer(backing)

        cases: list[tuple[str, Callable[[], object]]] = [
            (
                "5 bytes aren't a whole number of 2-byte structs",
                lambda: buffer.set_struct_array(bytes(5), 2),
            ),
            (
                "must be a positive integer, not 0",
                lambda: buffer.set_struct_array(b"", 0),
            ),
            (
                "must be a positive integer, not -1",
                lambda: buffer.get_struct_array(0, -1),
            ),
            (
                "must be a positive integer, not 0",
                lambda: buffer.get_struct_array(0, 0),
            ),
            (
                "must be a positive integer, not <negative 15001-bit integer>",
                lambda: buffer.get_struct_array(0, -(2**15000)),
            ),
            (
                "5 bytes aren't a whole number of <15001-bit integer>-byte structs",
                lambda: buffer.set_struct_array(bytes(5), 2**15000),
            ),
        ]

        for message, call in cases:
            with self.subTest(message), self.assertRaisesRegex(ValueError, message):
                call()

        self.assertEqual(backing, b"\xaa" * 16)

    @unittest.skipUnless(sys.byteorder == "little", "Big-endian hosts read copies")
    def test_reads_structs_as_views_of_the_bytes_at_any_alignment(self) -> None:
        builder = TableBuilder(2)
        builder.set_struct(pack("<hf", -2, 1.5))
        builder.set_struct_array(pack("<hfhf", 3, 0.5, 4, -0.5), 6, 4)

        backing = bytearray(1) + builder.bytes()
        buffer = ByteBuffer(memoryview(backing)[1:])
        struct_bytes = buffer.get_struct(buffer.root_offset(0), 6)
        array_bytes = buffer.get_struct_array(buffer.root_offset(1), 6)

        self.assertEqual(unpack("<hf", struct_bytes), (-2, 1.5))
        self.assertEqual(list(iter_unpack("<hf", array_bytes)), [(3, 0.5), (4, -0.5)])
        self.assertIs(struct_bytes.obj, backing)
        self.assertIs(array_bytes.obj, backing)

    def test_finds_each_vector_item_from_its_ends_as_reading_every_item_finds_it(
        self,
    ) -> None:
        builder = TableBuilder(1)
        builder.set_vector([b"\x01", b"", b"\x02\x03"])
        buffer = builder.finalize()
        vector = buffer.root_offset(0)

        # The count and the three ends, at 4, then the items from 20
        self.assertEqual(buffer.get_vector_offsets(vector).tolist(), [20, 21, 21])
        self.assertEqual(
            [buffer.get_vector_offset(vector, index) for index in range(3)],
            [20, 21, 21],
        )
        self.assertEqual(buffer.get_vector_length(vector), 3)
        self.assertEqual(
            [bytes(item) for item in buffer.get_vector(vector)],
            [b"\x01", b"", b"\x02\x03"],
        )

    @unittest.skipUnless(
        os.environ.get("TAFLA_LARGE_BUFFER_TESTS"),
        "Maps 4 GiB of address space; set TAFLA_LARGE_BUFFER_TESTS=1 to run",
    )
    def test_finds_vector_items_past_4_gib_in_a_larger_buffer(self) -> None:
        """A memory map of many tables can hold a table past 4 GiB, whose
        positions don't fit in 32 bits, though its own offsets do"""
        builder = TableBuilder(1)
        builder.set_vector([b"\x01", b"\x02\x03"])
        table = builder.bytes()
        position = 2**32

        with tempfile.TemporaryFile() as file:
            # A sparse file, which takes no disk space for the bytes before
            # the table
            file.truncate(position + len(table))
            file.seek(position)
            file.write(table)
            file.flush()

            with mmap.mmap(file.fileno(), 0, access=mmap.ACCESS_READ) as mapped:
                buffer = ByteBuffer(mapped)
                try:
                    vector = position + buffer.get_uint32(position)
                    # The table's offset, then the vector's count and two ends
                    expected = [position + 16, position + 17]

                    self.assertEqual(
                        buffer.get_vector_offsets(vector).tolist(), expected
                    )
                    self.assertEqual(
                        [buffer.get_vector_offset(vector, index) for index in range(2)],
                        expected,
                    )
                    self.assertEqual(
                        [bytes(item) for item in buffer.get_vector(vector)],
                        [b"\x01", b"\x02\x03"],
                    )
                finally:
                    # Release the views of the map, so that it can close even
                    # when an assertion fails, whose error then surfaces
                    # rather than the map's BufferError
                    del buffer

    def test_rejects_an_index_outside_a_vector_with_index_error(self) -> None:
        builder = TableBuilder(1)
        builder.set_vector([b"\x01", b"\x02\x03"])
        buffer = builder.finalize()

        # An index of more digits than Python converts to a string is given by
        # its bit length
        for index, described in [
            (-1, "-1"),
            (2, "2"),
            (2**15000, "<15001-bit integer>"),
            (-(2**15000), "<negative 15001-bit integer>"),
        ]:
            with (
                self.subTest(index=described),
                self.assertRaisesRegex(
                    IndexError, f"Item {described} is outside the vector of 2 at 4"
                ),
            ):
                buffer.get_vector_offset(buffer.root_offset(0), index)

    def test_rejects_a_vector_count_that_the_bytes_cant_hold_before_the_index(
        self,
    ) -> None:
        # A count of 3, whose ends need 12 bytes after it, where 4 are left
        buffer = ByteBuffer(bytes([3, 0, 0, 0, 8, 0, 0, 0]))

        for index in [0, 3]:
            with self.subTest(index=index), self.assertRaises(DecodeError):
                buffer.get_vector_offset(0, index)

    @given(sampled_from(OFFSET_TYPES), data())
    def test_writes_each_value_after_the_previous_one_little_endian_as_struct_packs_it(
        self, offset_type: OffsetType, data: DataObject
    ) -> None:
        """TableBuilder writes through a TableWriter, so the conformance suite
        no longer checks the values that these setters write"""
        calls = data.draw(lists(writes(offset_type), max_size=8))
        expected = b"".join(written for _, _, written in calls)
        # Bytes that aren't zero, so that padding shows
        backing = bytearray(b"\xa5" * len(expected))
        buffer = ByteBuffer(backing, TableLayout(offset_type=offset_type))

        for setter, arguments, _ in calls:
            getattr(buffer, setter)(*arguments)

        self.assertEqual(backing, expected)

    def test_writes_padding_as_zeros(self) -> None:
        backing = bytearray(b"\xff" * 4)
        buffer = ByteBuffer(backing)
        buffer.set_padding(3)
        buffer.set_uint8(1)

        self.assertEqual(backing, b"\0\0\0\x01")


if __name__ == "__main__":
    unittest.main()
