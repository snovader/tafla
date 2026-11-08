import mmap
import os
import sys
import tempfile
import unittest
from array import array
from struct import calcsize, iter_unpack, pack, unpack
from unittest.mock import patch

from hypothesis import given
from hypothesis.strategies import DrawFn, binary, composite, integers, sampled_from

from tafla import SIZEOF_32, ByteBuffer, DecodeError, TableBuilder
from tafla.types import IntTypeCode
from tests.descriptions import ARRAY_FIELDS
from tests.tables import DEFAULT_LAYOUT, Description, build_table


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


class TestByteBuffer(unittest.TestCase):
    @given(strided_views())
    def test_takes_bytes_only_when_theyre_c_contiguous(self, view: memoryview) -> None:
        """Reads return views of the bytes, which a copy of them wouldn't be"""
        if view.c_contiguous:
            self.assertEqual(bytes(ByteBuffer(view).bytes()), view.tobytes())
        else:
            with self.assertRaisesRegex(TypeError, "isn't C-contiguous"):
                ByteBuffer(view)

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

    def test_rejects_struct_sizes_less_than_1(self) -> None:
        buffer = ByteBuffer(bytes(16))

        # A size of more digits than Python converts to a string is given by
        # its bit length
        for size, described in [
            (-1, "-1"),
            (0, "0"),
            (-(2**15000), "<negative 15001-bit integer>"),
        ]:
            with (
                self.subTest(size=described),
                self.assertRaisesRegex(
                    ValueError, f"must be a positive integer, not {described}"
                ),
            ):
                buffer.get_struct_array(0, size)

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


if __name__ == "__main__":
    unittest.main()
