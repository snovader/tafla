import sys
import unittest
from collections.abc import Callable, Generator
from contextlib import contextmanager

from tafla import DecodeError, TableReader


@contextmanager
def _int_max_str_digits(limit: int) -> Generator[None]:
    """Sets the most digits that Python converts an integer to or from"""
    previous = sys.get_int_max_str_digits()
    sys.set_int_max_str_digits(limit)
    try:
        yield
    finally:
        sys.set_int_max_str_digits(previous)


class TestDecodeError(unittest.TestCase):
    def test_is_a_value_error(self) -> None:
        self.assertIsInstance(DecodeError("message"), ValueError)

    def test_raises_from_scalar_getters_outside_the_bytes(self) -> None:
        backing = bytearray(range(32))
        reader = TableReader(memoryview(backing)[:8])

        reads: list[tuple[str, Callable[[], object]]] = [
            ("get_float64", lambda: reader.get_float64(4)),
            ("get_int64", lambda: reader.get_int64(-1)),
            ("get_uint32", lambda: reader.get_uint32(5)),
            # A negative position would otherwise count from the end
            ("get_uint32", lambda: reader.get_uint32(-4)),
            ("get_int8", lambda: reader.get_int8(8)),
            ("root_offset", lambda: reader.root_offset(2)),
            ("field_offset", lambda: reader.field_offset(-8, 0)),
        ]

        for name, get in reads:
            with (
                self.subTest(read=name),
                self.assertRaisesRegex(DecodeError, "Read exceeds the supplied buffer"),
            ):
                get()

    def test_raises_at_positions_beyond_a_c_ssize_t_and_the_digit_limit(
        self,
    ) -> None:
        """struct takes positions as a C ssize_t, and once raised OverflowError
        for a larger one. Formatting a position of more digits than Python
        converts, 4,300 by default, once raised ValueError."""
        reader = TableReader(bytes(8))

        reads: list[tuple[str, Callable[[int], object]]] = [
            ("get_uint32", reader.get_uint32),
            ("get_float64", reader.get_float64),
            ("get_string", reader.get_string),
            ("get_int16_array", reader.get_int16_array),
            ("get_bool_array", reader.get_bool_array),
            ("get_vector_offsets", reader.get_vector_offsets),
            ("root_offset", reader.root_offset),
            ("field_offset", lambda position: reader.field_offset(position, 0)),
            ("field_offset", lambda index: reader.field_offset(0, index)),
        ]

        for position in [2**63, 2**100, -(2**100), 2**15000, -(2**15000)]:
            for name, get in reads:
                with (
                    # Reports the bit length, which a failure can format
                    self.subTest(read=name, bits=position.bit_length()),
                    self.assertRaisesRegex(
                        DecodeError, "Read exceeds the supplied buffer"
                    ),
                ):
                    get(position)

    def test_gives_positions_and_lengths_beyond_64_bits_by_their_bit_length(
        self,
    ) -> None:
        """Whatever the digit limit, which callers can lower to 640"""
        reader = TableReader(bytes(8))

        cases: list[tuple[Callable[[], object], str]] = [
            (
                lambda: reader.get_uint32(2**64 - 1),
                "4 bytes at 18446744073709551615 of 8",
            ),
            (lambda: reader.get_uint32(2**64), "4 bytes at <65-bit integer> of 8"),
            (
                lambda: reader.get_uint32(-(2**64) + 1),
                "4 bytes at -18446744073709551615 of 8",
            ),
            (
                lambda: reader.get_uint32(-(2**64)),
                "4 bytes at <negative 65-bit integer> of 8",
            ),
            (
                lambda: reader.get_float64(2**15000),
                "8 bytes at <15001-bit integer> of 8",
            ),
            (
                lambda: reader.get_struct(0, 2**15000),
                "<15001-bit integer> bytes at 0 of 8",
            ),
        ]

        with _int_max_str_digits(640):
            for read, range_ in cases:
                with (
                    self.subTest(range=range_),
                    self.assertRaisesRegex(DecodeError, f"Read exceeds .*: {range_}$"),
                ):
                    read()

    def test_raises_for_a_string_that_isnt_utf_8(self) -> None:
        cases = [
            ("a lone continuation byte", b"\x80"),
            ("a truncated sequence", b"\xe2\x82"),
            ("an overlong encoding", b"\xc0\xaf"),
            ("an encoded surrogate", b"\xed\xa0\x80"),
        ]

        for name, utf8 in cases:
            with self.subTest(case=name):
                reader = TableReader(len(utf8).to_bytes(4, "little") + utf8)

                with self.assertRaises(DecodeError) as context:
                    reader.get_string(0)

                self.assertIsInstance(context.exception.__cause__, UnicodeDecodeError)

    def test_raises_from_struct_and_vector_getters_outside_the_bytes(self) -> None:
        # Bytes that a read past the slice's end would find
        backing = bytearray(b"\x41" * 32)
        reader = TableReader(memoryview(backing)[:8])
        # A vector's count of 1 and its item's end at 13, which is also read as
        # the count of an array of structs
        backing[:8] = bytes([1, 0, 0, 0, 13, 0, 0, 0])

        reads: list[tuple[str, Callable[[], object], str]] = [
            ("get_struct", lambda: reader.get_struct(4, 5), "5 bytes at 4 of 8"),
            ("get_struct", lambda: reader.get_struct(-1, 1), "1 bytes at -1 of 8"),
            ("get_struct", lambda: reader.get_struct(4, -1), "-1 bytes at 4 of 8"),
            (
                "get_struct_array",
                lambda: reader.get_struct_array(4, 2),
                "26 bytes at 8 of 8",
            ),
            ("get_vector", lambda: reader.get_vector(0), "5 bytes at 8 of 8"),
            (
                "get_vector_offsets",
                lambda: reader.get_vector_offsets(0),
                "5 bytes at 8 of 8",
            ),
            (
                "get_vector_offset",
                lambda: reader.get_vector_offset(0, 0),
                "5 bytes at 8 of 8",
            ),
        ]

        for name, get, range_ in reads:
            with (
                self.subTest(read=name, range=range_),
                self.assertRaisesRegex(DecodeError, f"Read exceeds .*: {range_}"),
            ):
                get()

    def test_raises_for_an_array_of_bools_whose_count_passes_the_end(self) -> None:
        # A count of 3, of which the bytes hold 2
        reader = TableReader(bytes([3, 0, 0, 0, 1, 0]))

        with self.assertRaisesRegex(
            DecodeError, "Read exceeds the supplied buffer: 3 bytes at 4 of 6"
        ):
            reader.get_bool_array(0)

    def test_raises_for_a_vector_item_that_ends_before_it_starts(self) -> None:
        # A count of 2 puts the first item after the ends, at 12, but it ends
        # at 11
        reader = TableReader(bytes([2, 0, 0, 0, 11, 0, 0, 0, 12, 0, 0, 0]))

        reads: list[tuple[str, Callable[[], object]]] = [
            ("get_vector", lambda: reader.get_vector(0)),
            ("get_vector_offsets", lambda: reader.get_vector_offsets(0)),
            ("get_vector_offset", lambda: reader.get_vector_offset(0, 0)),
        ]

        for name, get in reads:
            with (
                self.subTest(read=name),
                self.assertRaisesRegex(
                    DecodeError, "The vector item at 12 ends before it starts, at 11"
                ),
            ):
                get()

    def test_rejects_counts_before_allocating_for_them_or_viewing_their_bytes(
        self,
    ) -> None:
        reader = TableReader((2**32 - 1).to_bytes(4, "little") + bytes(4))

        reads: list[tuple[str, Callable[[], object]]] = [
            ("get_vector_length", lambda: reader.get_vector_length(0)),
            ("get_vector_offsets", lambda: reader.get_vector_offsets(0)),
            ("get_vector", lambda: reader.get_vector(0)),
            ("get_struct_array", lambda: reader.get_struct_array(0, 8)),
            ("get_bool_array", lambda: reader.get_bool_array(0)),
        ]

        for name, get in reads:
            with self.subTest(read=name), self.assertRaises(DecodeError):
                get()


if __name__ == "__main__":
    unittest.main()
