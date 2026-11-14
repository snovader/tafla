"""A message of only fixed fields, which the code generated from
``schemas/fixed_fields.tafla`` and ``schemas/shapes.tafla`` writes in one
call into bytes of its own, and whose ``read_values()`` reads its values in
one call, without a reader, as ``field_values()`` reads them through one."""

import unittest
from collections.abc import Buffer
from functools import partial
from typing import Any

from hypothesis import given
from hypothesis.strategies import (
    SearchStrategy,
    binary,
    builds,
    floats,
    integers,
    one_of,
    sampled_from,
)

from tafla import ByteBuffer, DecodeError, TableLayout
from tests.descriptions import STRINGS
from tests.generated import SETTINGS
from tests.generated.byte_strategies import (
    Overwrite,
    Slice,
    Truncation,
    corruptions,
    slices,
)
from tests.generated.expectations import fields_or_decode_error, plain
from tests.generated.fixed_fields.fixed_fields import Box, Pixel, Tagged, Tiny, Vec2
from tests.generated.shapes.shapes import Point
from tests.generated.shapes_tables import point_fields

_FLOAT32S = floats(width=32)
_VEC2S = builds(Vec2, _FLOAT32S, _FLOAT32S)

# Each message, with a strategy of its fields' values, as serialize_values()
# and field_values() hold them
_MESSAGES: list[tuple[Any, SearchStrategy[dict[str, Any]]]] = [
    (
        Tiny,
        builds(dict, id=integers(0, 2**16 - 1), kind=integers(0, 255)),
    ),
    (
        Tagged,
        builds(
            dict,
            id=integers(0, 2**32 - 1),
            position=_VEC2S,
            label=STRINGS.filter(lambda label: len(label.encode()) < 64),
        ),
    ),
    (Pixel, builds(dict, position=_VEC2S, color=integers(0, 2**32 - 1))),
    (
        Box,
        builds(
            dict,
            min=_VEC2S,
            max=_VEC2S,
            range=builds(
                dict, start=integers(0, 2**32 - 1), end=integers(0, 2**32 - 1)
            ),
        ),
    ),
]

_MESSAGE_FIELDS = one_of(
    *(
        fields.map(partial(lambda message, each: (message, each), message))
        for message, fields in _MESSAGES
    )
)


class TestWholeMessages(unittest.TestCase):
    @SETTINGS
    @given(_MESSAGE_FIELDS, slices)
    def test_writes_bytes_that_the_reader_and_read_values_read_back(
        self, message_fields: tuple[Any, dict[str, Any]], slice: Slice
    ) -> None:
        message, fields = message_fields
        data = message.serialize(**fields)
        self.assertIs(type(data), bytes)

        read = slice.of(data)
        self.assertEqual(
            fields_or_decode_error(partial(_plain_read_values, message, read)),
            fields_or_decode_error(lambda: plain(message(read).field_values())),
        )

    @SETTINGS
    @given(_MESSAGE_FIELDS, corruptions)
    def test_reads_corrupted_values_as_the_reader_does(
        self,
        message_fields: tuple[Any, dict[str, Any]],
        corruption: Truncation | Overwrite,
    ) -> None:
        message, fields = message_fields
        data = corruption.apply(message.serialize(**fields))

        self.assertEqual(
            fields_or_decode_error(partial(_plain_read_values, message, data)),
            fields_or_decode_error(partial(_values_by_reader, message, data)),
        )

    @SETTINGS
    @given(sampled_from([message for message, _ in _MESSAGES]), binary(max_size=48))
    def test_reads_arbitrary_bytes_as_the_reader_does(
        self, message: Any, data: bytes
    ) -> None:
        self.assertEqual(
            fields_or_decode_error(partial(_plain_read_values, message, data)),
            fields_or_decode_error(partial(_values_by_reader, message, data)),
        )

    @SETTINGS
    @given(point_fields, slices)
    def test_writes_a_point_in_one_call_and_reads_it_back(
        self, fields: Any, slice: Slice
    ) -> None:
        data = Point.serialize(**fields)
        self.assertIs(type(data), bytes)

        values = Point.read_values(slice.of(data))
        self.assertEqual(bytes(Point.serialize_values(*values)), data)

    def test_raises_decode_error_for_another_version_as_the_reader_does(
        self,
    ) -> None:
        data = bytearray(Tagged.serialize(id=1, position=Vec2(0.5, 1.5), label="a"))
        layout = TableLayout(sized=True, versioned=True, offset_type="uint8")
        self.assertEqual(ByteBuffer(data, layout).get_version(), 3)
        # The version slot follows the length's, of one byte each
        data[1] = 4

        with self.assertRaisesRegex(DecodeError, "version 3 of Tagged, got 4"):
            Tagged.read_values(data)

    def test_raises_type_error_for_bytes_that_arent_c_contiguous(self) -> None:
        data = Tiny.serialize(id=1, kind=2)
        strided = memoryview(data * 2)[::2]

        with self.assertRaises(TypeError):
            Tiny.read_values(strided)

    def test_raises_value_error_for_a_value_that_doesnt_fit_its_field(self) -> None:
        for call in [
            partial(Tiny.serialize, id=2**16, kind=0),
            partial(Tagged.serialize, id=-1, position=Vec2(0.0, 0.0), label=""),
            partial(Tagged.serialize, id=0, position=Vec2(0.0, 0.0), label="é" * 200),
        ]:
            with self.subTest(call=call), self.assertRaises(ValueError):
                call()


def _plain_read_values(message: Any, data: Buffer) -> object:
    return plain(message.read_values(data))


def _values_by_reader(message: Any, data: Buffer) -> object:
    return plain(message(data).field_values())


if __name__ == "__main__":
    unittest.main()
