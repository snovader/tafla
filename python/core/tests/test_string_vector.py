"""The vector of strings: written by ``TableBuilder`` and ``TableWriter``, and
read by ``TableReader`` and ``StringVector``, a string at a time from its
ends and every string at once, which reads what reading each string reads
and raises where any of them does."""

import struct
import unittest
from collections.abc import Callable

from hypothesis import given, settings
from hypothesis.strategies import (
    SearchStrategy,
    binary,
    characters,
    integers,
    lists,
    one_of,
    sampled_from,
    text,
    tuples,
)

from tafla import (
    DecodeError,
    OffsetType,
    StringVector,
    TableBuilder,
    TableLayout,
    TableReader,
    TableWriter,
)
from tests.byte_order import big_endian_host
from tests.descriptions import STRINGS
from tests.tables import LONGEST, offset_size_of
from tests.test_fixed_fields import Misreported

VECTORS: SearchStrategy[list[str]] = lists(STRINGS, max_size=8)

# Strings of ASCII, which the reader decodes as one run
ASCII_VECTORS: SearchStrategy[list[str]] = lists(
    text(characters(max_codepoint=0x7F), max_size=40), min_size=1, max_size=12
)

# Strings of ASCII among others, whose run the reader decodes a string at a
# time
MIXED_VECTORS: SearchStrategy[list[str]] = lists(
    one_of(text(characters(max_codepoint=0x7F), max_size=40), STRINGS),
    min_size=1,
    max_size=16,
)

# Bytes to overwrite a table's with, each at an index into the bytes after its
# one offset, so that the vector is found
OVERWRITES: SearchStrategy[list[tuple[int, int]]] = lists(
    tuples(integers(0, 2**16), integers(0, 255)), min_size=1, max_size=4
)


def table_of(strings: list[str], layout: TableLayout = TableLayout.DEFAULT) -> bytes:
    """A table whose only field is a vector of ``strings``"""
    builder = TableBuilder(1, layout)
    builder.set_string_vector(strings)

    return bytes(builder.bytes())


def view_of(data: bytes, layout: TableLayout = TableLayout.DEFAULT) -> StringVector:
    reader = TableReader(data, layout)

    return StringVector(reader, reader.root_offset(0))


def overwritten(data: bytes, overwrites: list[tuple[int, int]]) -> bytes:
    """The bytes with each of ``overwrites`` written past the table's offset,
    of 4 bytes"""
    corrupted = bytearray(data)
    for at, byte in overwrites:
        corrupted[4 + at % (len(corrupted) - 4)] = byte

    return bytes(corrupted)


def outcome(read: Callable[[], object]) -> object:
    """What a read returns, or the class and message of the error that it
    raises"""
    try:
        return read()
    except Exception as error:  # noqa: BLE001
        return type(error), str(error)


def each_string(vector: StringVector) -> list[str]:
    """Every string of a vector, each read from its ends alone"""
    return [vector[index] for index in range(len(vector))]


class TestStringVector(unittest.TestCase):
    @given(VECTORS)
    def test_reads_back_every_string_at_once_and_each_from_its_ends_alone(
        self, strings: list[str]
    ) -> None:
        data = table_of(strings)
        reader = TableReader(data)
        vector = StringVector(reader, reader.root_offset(0))

        self.assertEqual(len(vector), len(strings))
        self.assertEqual(vector.tolist(), strings)
        self.assertEqual(list(vector), strings)
        self.assertEqual(each_string(vector), strings)
        self.assertEqual(reader.get_string_vector(reader.root_offset(0)), strings)

    @given(VECTORS)
    def test_counts_negative_indices_from_the_end(self, strings: list[str]) -> None:
        vector = view_of(table_of(strings))

        self.assertEqual(
            [vector[-i] for i in range(1, len(strings) + 1)], strings[::-1]
        )

    @given(VECTORS)
    def test_takes_the_bytes_of_a_vector_whose_items_are_the_strings_utf8(
        self, strings: list[str]
    ) -> None:
        builder = TableBuilder(1)
        builder.set_vector([string.encode() for string in strings])

        self.assertEqual(table_of(strings), bytes(builder.bytes()))

    @given(VECTORS)
    def test_takes_the_end_of_each_string_and_a_count_beside_their_utf8(
        self, strings: list[str]
    ) -> None:
        utf8 = sum(len(string.encode()) for string in strings)

        # The table's one offset, the count and the ends, of 4 bytes each
        self.assertEqual(len(table_of(strings)), 4 + 4 + 4 * len(strings) + utf8)

    @given(VECTORS, sampled_from(["uint8", "uint16", "uint32"]))
    def test_is_written_by_a_writer_as_by_a_builder(
        self, strings: list[str], offset_type: OffsetType
    ) -> None:
        layout = TableLayout(offset_type=offset_type)
        builder = TableBuilder(2, layout)
        builder.set_uint8(1)
        try:
            builder.set_string_vector(strings)
        except ValueError:
            return  # Too long for the offset type

        writer = TableWriter(2, layout)
        writer.set_uint8(0, 1)
        writer.set_string_vector(1, strings)

        self.assertEqual(bytes(writer.bytes()), bytes(builder.bytes()))

    @given(VECTORS)
    def test_is_written_from_a_tuple_as_from_a_list(self, strings: list[str]) -> None:
        builder = TableBuilder(1)
        builder.set_string_vector(tuple(strings))
        writer = TableWriter(1)
        writer.set_string_vector(0, tuple(strings))

        self.assertEqual(bytes(builder.bytes()), table_of(strings))
        self.assertEqual(bytes(writer.bytes()), table_of(strings))

    def test_reads_strings_longer_than_short_ones(self) -> None:
        strings = ["a" * 33, "é" * 40, "東" * 300, "b" * 4096, "🌍" * 70]
        vector = view_of(table_of(strings))

        self.assertEqual(vector.tolist(), strings)
        self.assertEqual(vector[2], strings[2])

    def test_reads_empty_strings_that_end_the_bytes(self) -> None:
        """Strings of no bytes, whose run is empty and ends the table"""
        for strings in [[""], ["", "", ""], ["é", ""], ["", "a"]]:
            with self.subTest(strings=strings):
                data = table_of(strings)

                self.assertEqual(view_of(data).tolist(), strings)
                self.assertEqual(each_string(view_of(data)), strings)

    def test_reads_lists_whose_first_string_isnt_ascii_and_others_are(self) -> None:
        strings = ["Zürich", "Oslo", "Stockholm", "東京", "a" * 40]

        self.assertEqual(view_of(table_of(strings)).tolist(), strings)

    def test_writes_its_count_and_ends_little_endian_on_a_big_endian_host(
        self,
    ) -> None:
        """Counts and ends are little-endian on any host, as the format's
        offsets are, so a big-endian host writes the same bytes"""
        strings = ["a", "東京", ""]
        expected = table_of(strings)

        with big_endian_host():
            writer = TableWriter(1)
            writer.set_string_vector(0, strings)

            self.assertEqual(table_of(strings), expected)
            self.assertEqual(bytes(writer.bytes()), expected)


class TestReadingEveryString(unittest.TestCase):
    @given(ASCII_VECTORS)
    def test_reads_lists_of_ascii_as_each_string_on_its_own(
        self, strings: list[str]
    ) -> None:
        self.assertEqual(view_of(table_of(strings)).tolist(), strings)

    @given(MIXED_VECTORS)
    def test_reads_lists_of_ascii_among_others_as_each_string_on_its_own(
        self, strings: list[str]
    ) -> None:
        self.assertEqual(view_of(table_of(strings)).tolist(), strings)

    @given(one_of(ASCII_VECTORS, VECTORS, MIXED_VECTORS), OVERWRITES)
    def test_reads_what_reading_each_string_reads_and_raises_where_any_does(
        self, strings: list[str], overwrites: list[tuple[int, int]]
    ) -> None:
        """Reading every string raises the error that reading each, one after
        another, raises first, and reads what they read otherwise"""
        data = overwritten(table_of(strings), overwrites)
        reader = TableReader(data)
        position = reader.root_offset(0)

        try:
            vector = StringVector(reader, position)
        except DecodeError:
            with self.assertRaises(DecodeError):
                reader.get_string_vector(position)
            return

        each = outcome(lambda: each_string(vector))
        self.assertEqual(outcome(vector.tolist), each)
        self.assertEqual(outcome(lambda: reader.get_string_vector(position)), each)
        if isinstance(each, tuple):
            self.assertIs(each[0], DecodeError)

    @given(binary(max_size=64), sampled_from(["uint8", "uint16", "uint32"]))
    def test_reads_arbitrary_bytes_as_reading_each_string_does(
        self, data: bytes, offset_type: OffsetType
    ) -> None:
        """At every position of the bytes, as a vector of strings"""
        reader = TableReader(data, TableLayout(offset_type=offset_type))

        for position in range(len(data)):
            try:
                vector = StringVector(reader, position)
            except DecodeError:
                with self.assertRaises(DecodeError):
                    reader.get_string_vector(position)
                continue

            each = outcome(lambda vector=vector: each_string(vector))
            self.assertEqual(
                outcome(lambda position=position: reader.get_string_vector(position)),
                each,
            )
            if isinstance(each, tuple):
                self.assertIs(each[0], DecodeError)

    @settings(max_examples=30)
    @given(integers(100, 1500), integers(1, 60), OVERWRITES)
    def test_reads_a_corrupted_long_list_as_reading_each_string_does(
        self, count: int, length: int, overwrites: list[tuple[int, int]]
    ) -> None:
        strings = [str(index).rjust(length, "x") for index in range(count)]
        data = overwritten(table_of(strings), overwrites)

        try:
            vector = view_of(data)
        except DecodeError:
            return

        self.assertEqual(outcome(vector.tolist), outcome(lambda: each_string(vector)))

    def test_raises_for_an_end_that_splits_a_character_within_a_run_of_strings(
        self,
    ) -> None:
        data = bytearray(table_of(["first", "a" * 20, "é東", "b" * 20]))
        # The end of "é東", after the table's offset, the count and two ends,
        # is a byte less, within 東
        end = 4 + 4 + 2 * 4
        struct.pack_into("<I", data, end, struct.unpack_from("<I", data, end)[0] - 1)

        with self.assertRaises(DecodeError):
            view_of(bytes(data)).tolist()

    def test_raises_for_a_string_of_a_run_of_ascii_that_ends_past_the_run(
        self,
    ) -> None:
        data = bytearray(table_of(["first", "a" * 20, "b" * 20, "c" * 20]))
        # The end of the second string, after the table's offset, the count
        # and the first end, is past the last string's end, within the bytes
        struct.pack_into("<I", data, 4 + 4 + 4, len(data) - 4 - 1)

        with self.assertRaisesRegex(DecodeError, "ends before it starts"):
            view_of(bytes(data)).tolist()

    def test_names_the_first_string_that_isnt_valid_utf8(self) -> None:
        data = bytearray(table_of(["first", "a" * 20, "b" * 20]))
        data[-1] = 0xFF
        data[-21] = 0xFF

        with self.assertRaises(DecodeError) as raised:
            view_of(bytes(data)).tolist()

        self.assertEqual(
            str(raised.exception), f"The string at {len(data) - 40} isn't valid UTF-8"
        )
        self.assertIsInstance(raised.exception.__cause__, UnicodeDecodeError)


class TestRejectedStringVectors(unittest.TestCase):
    def test_raises_index_error_for_a_string_outside_the_vector(self) -> None:
        vector = view_of(table_of(["a", "b"]))

        for index in [-3, 2, 2**70]:
            with (
                self.subTest(index=index),
                self.assertRaisesRegex(IndexError, "outside the vector of 2 at 4"),
            ):
                vector[index]

    def test_rejects_an_impossible_count_before_anything_allocates_for_it(
        self,
    ) -> None:
        reader = TableReader(bytes([255, 255, 255, 255]))

        with self.assertRaises(DecodeError):
            reader.get_string_vector(0)
        with self.assertRaises(DecodeError):
            StringVector(reader, 0)

    def test_raises_decode_error_for_a_string_that_a_negative_position_puts_before_the_bytes(
        self,
    ) -> None:
        """String 1 of a vector at -4 is found from the slots at 0 and 4,
        which both end it at 0, and so before the bytes"""
        reader = TableReader(bytes(8))

        with self.assertRaisesRegex(
            DecodeError, "^Read exceeds the supplied buffer: 0 bytes at -4 of 8$"
        ):
            reader.get_string_vector_item(-4, 2, 1)

    def test_raises_for_the_strings_on_either_side_of_an_end_that_splits_a_character(
        self,
    ) -> None:
        data = bytearray(table_of(["a", "東", "京", "b"]))
        # The end of the second string, after the table's offset, the count
        # and the first end
        data[4 + 4 + 4] -= 1
        vector = view_of(bytes(data))

        self.assertEqual(vector[0], "a")
        with self.assertRaises(DecodeError):
            vector[1]
        with self.assertRaises(DecodeError):
            vector[2]
        self.assertEqual(vector[3], "b")
        with self.assertRaises(DecodeError):
            vector.tolist()

    def test_names_the_position_of_a_string_that_isnt_valid_utf8(self) -> None:
        for length in [3, 40]:
            with self.subTest(length=length):
                data = bytearray(table_of(["a", "b" * length]))
                # The second string starts after the offset, the count, two
                # ends and the first string
                data[17] = 0xFF

                with self.assertRaisesRegex(
                    DecodeError, "^The string at 17 isn't valid UTF-8$"
                ):
                    view_of(bytes(data))[1]


class TestWritingStringVectors(unittest.TestCase):
    def test_raises_for_a_lone_surrogate_leaving_the_builder_and_writer_as_they_were(
        self,
    ) -> None:
        builder = TableBuilder(1)
        writer = TableWriter(1)

        with self.assertRaises(UnicodeEncodeError):
            builder.set_string_vector(["a", "\ud800"])
        with self.assertRaises(UnicodeEncodeError):
            writer.set_string_vector(0, ["a", "\ud800"])

        builder.set_string_vector(["a"])
        writer.set_string_vector(0, ["a"])
        self.assertEqual(bytes(builder.bytes()), table_of(["a"]))
        self.assertEqual(bytes(writer.bytes()), table_of(["a"]))

    def test_rejects_a_str_which_is_a_sequence_of_its_characters(self) -> None:
        builder = TableBuilder(1)
        writer = TableWriter(1)

        with self.assertRaises(TypeError):
            builder.set_string_vector("abc")
        with self.assertRaises(TypeError):
            writer.set_string_vector(0, "abc")

    def test_raises_the_error_of_the_first_item_that_cant_be_encoded(self) -> None:
        cases: list[tuple[list[object], type[Exception]]] = [
            (["a", 5, "\ud800"], TypeError),
            (["a", "\ud800", 5], UnicodeEncodeError),
            (["\ud800", 5], UnicodeEncodeError),
            ([5, "\ud800"], TypeError),
            (["é", b"b"], TypeError),
        ]
        for strings, error in cases:
            with self.subTest(strings=strings):
                with self.assertRaises(error):
                    TableBuilder(1).set_string_vector(strings)  # ty: ignore[invalid-argument-type]
                with self.assertRaises(error):
                    TableWriter(1).set_string_vector(0, strings)  # ty: ignore[invalid-argument-type]

    def test_takes_strings_that_the_caller_changes_after_setting_them_as_they_were(
        self,
    ) -> None:
        for strings in [["a", "b"], ["é", "b"]]:
            with self.subTest(strings=strings):
                builder = TableBuilder(1)
                builder.set_string_vector(strings)
                strings.append("c")

                self.assertEqual(bytes(builder.bytes()), table_of(strings[:2]))

    def test_fills_a_table_of_narrow_offsets_and_raises_for_a_byte_more(self) -> None:
        for offset_type in ("uint8", "uint16"):
            with self.subTest(offset_type=offset_type):
                layout = TableLayout(offset_type=offset_type)
                size = offset_size_of(offset_type)
                longest = LONGEST[offset_type]
                # Two offsets, a uint8, and the vector's count and its one end
                room = longest - 4 * size - 1
                builder = TableBuilder(2, layout)
                builder.set_uint8(7)

                with self.assertRaisesRegex(
                    ValueError,
                    f"^Field 2 makes the table {longest + 1} bytes, too long for "
                    f"{offset_type} offsets$",
                ):
                    builder.set_string_vector(["é" + "a" * (room - 1)])
                builder.set_string_vector(["a" * room])

                data = bytes(builder.bytes())
                reader = TableReader(data, layout)
                self.assertEqual(len(data), longest)
                self.assertEqual(reader.get_uint8(reader.root_offset(0)), 7)
                self.assertEqual(
                    reader.get_string_vector(reader.root_offset(1)), ["a" * room]
                )

    def test_raises_from_the_writer_for_an_end_too_long_for_the_offset_type(
        self,
    ) -> None:
        for offset_type in ("uint8", "uint16"):
            with self.subTest(offset_type=offset_type):
                longest = LONGEST[offset_type]
                writer = TableWriter(1, TableLayout(offset_type=offset_type))

                with self.assertRaisesRegex(
                    ValueError, f"too long for {offset_type} offsets$"
                ):
                    writer.set_string_vector(0, ["a" * longest])
                # Nothing was written
                self.assertEqual(len(writer.bytes()), offset_size_of(offset_type))

    def test_raises_for_strings_whose_len_isnt_their_number(self) -> None:
        cases = [(["a", "b"], 1), (["a", "b"], 3), (["é", "b"], 1), ([], 1)]
        for strings, length in cases:
            with self.subTest(strings=strings, length=length):
                message = (
                    f"^The items' len\\(\\) is {length}, but iterating them gave "
                    f"{len(strings)}$"
                )
                with self.assertRaisesRegex(ValueError, message):
                    writer = TableWriter(1)
                    try:
                        writer.set_string_vector(0, Misreported(strings, length))
                    finally:
                        self.assertEqual(len(writer.bytes()), 4)


if __name__ == "__main__":
    unittest.main()
