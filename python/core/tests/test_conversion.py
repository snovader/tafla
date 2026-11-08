import ctypes
import unittest
from array import array
from collections.abc import Buffer, Callable, Iterable
from typing import Any

from hypothesis import given
from hypothesis.strategies import floats, integers, lists, one_of

from tafla import TableBuilder
from tests.tables import FLOAT_TYPES, FORMATS

INT_TYPES = [kind for kind in FORMATS if kind not in FLOAT_TYPES]

small_ints = lists(integers(0, 127), max_size=16)
numbers = lists(one_of(integers(-(2**31), 2**31), floats(allow_nan=False)), max_size=16)


def inputs(values: list[int]) -> list[tuple[str, Any]]:
    """The kinds of input that ``values`` can arrive as, holding the same
    integers, which fit in every element type"""
    return [
        ("list", values),
        ("tuple", tuple(values)),
        ("generator", (value for value in values)),
        ("bytes", bytes(values)),
        ("bytearray", bytearray(values)),
        ("array of int32", array("i", values)),
        ("memoryview of int16", memoryview(array("h", values))),
        ("ctypes array of int16", (ctypes.c_int16 * len(values))(*values)),
    ]


def round_trip(element: str, values: Buffer | Iterable[Any]) -> list[Any]:
    """Writes values with the array setter of ``element`` and reads them back"""
    builder = TableBuilder(1)
    set_array(builder, element)(values)

    buffer = builder.finalize()
    return getattr(buffer, f"get_{element}_array")(buffer.root_offset(0)).tolist()


def set_array(builder: TableBuilder, element: str) -> Callable[[Any], None]:
    return getattr(builder, f"set_{element}_array")


class TestConversion(unittest.TestCase):
    # subTest() is left out of the tests with @given, which turns it off

    @given(small_ints)
    def test_converts_every_kind_of_input_like_array(self, values: list[int]) -> None:
        for element, type_code in FORMATS.items():
            expected = array(type_code, values).tolist()

            for name, input in inputs(values):
                self.assertEqual(
                    round_trip(element, input), expected, f"{element}, {name}"
                )

    @given(numbers)
    def test_converts_floats_like_array(self, values: list[float]) -> None:
        for element in FLOAT_TYPES:
            expected = array(FORMATS[element], values).tolist()

            for input in [values, array("d", values)]:
                self.assertEqual(round_trip(element, input), expected, element)

    def test_rejects_floats_for_integer_elements_like_array(self) -> None:
        for element in INT_TYPES:
            with self.subTest(element=element), self.assertRaises(TypeError):
                set_array(TableBuilder(1), element)(array("d", [1.0]))

    def test_rejects_a_view_in_a_format_that_memoryview_cant_read(self) -> None:
        view = memoryview((ctypes.c_int16 * 2)(1, 2))

        with self.assertRaisesRegex(TypeError, "<h"):
            TableBuilder(1).set_int32_array(view)

    def test_converts_bytes_element_by_element(self) -> None:
        # array("f", b"...") would take the bytes as raw float32s instead
        self.assertEqual(round_trip("float32", b"\x00\x00\x80?"), [0, 0, 128, 63])

    def test_rejects_a_value_out_of_range_from_the_setter(self) -> None:
        for element in INT_TYPES:
            with self.subTest(element=element), self.assertRaises(OverflowError):
                set_array(TableBuilder(1), element)([2**64])

    def test_rejects_a_value_of_another_type_from_the_setter(self) -> None:
        for element in INT_TYPES:
            with self.subTest(element=element), self.assertRaises(TypeError):
                set_array(TableBuilder(1), element)([1.5])

        for element in FLOAT_TYPES:
            with self.subTest(element=element), self.assertRaises(TypeError):
                set_array(TableBuilder(1), element)(["1.5"])

    def test_writes_buffers_of_other_formats_of_the_same_kind_and_size(self) -> None:
        # numpy's int64 and uint64 arrays have the formats of a C long, which
        # has 8 bytes on most 64-bit platforms, and ctypes arrays have formats
        # with a byte order
        views: list[tuple[str, memoryview]] = [
            ("float32", memoryview((ctypes.c_float * 2)(1, 2))),
            ("int64", memoryview((ctypes.c_int64 * 2)(1, 2))),
            ("uint32", memoryview((ctypes.c_uint32 * 2)(1, 2))),
        ]
        if ctypes.sizeof(ctypes.c_long) == 8:
            longs = memoryview(array("q", [1, 2])).cast("B")
            views += [("int64", longs.cast("l")), ("uint64", longs.cast("L"))]

        for element, view in views:
            with self.subTest(element=element, format=view.format):
                self.assertEqual(round_trip(element, view), [1, 2])


if __name__ == "__main__":
    unittest.main()
