"""The enums of ``schemas/all_types.tafla``: names for some of the integers of
a type, which the generated code stores as that type, so a field of an enum
reads any integer of it, named or not, as an ``int``."""

import unittest
from collections.abc import Callable, Iterator
from dataclasses import fields as dataclass_fields, is_dataclass
from enum import IntEnum
from functools import partial
from typing import Any

from hypothesis import given
from hypothesis.strategies import binary, integers, lists, one_of

from tafla import DecodeError, TableLayout, TableReader
from tests.generated import SETTINGS
from tests.generated.all_types.all_types import (
    Change,
    Direction,
    Enums,
    EnumsFields,
    Extent,
    Level,
    Mark,
    Mask,
    Sign,
    Status,
    Turn,
    Unit,
    Word,
)
from tests.generated.all_types_tables import from_last
from tests.generated.byte_strategies import (
    Overwrite,
    Slice,
    Truncation,
    corruptions,
    slices,
)
from tests.generated.enums_tables import (
    ENUM_VALUES,
    describe_enums,
    describe_mark,
    enums_fields,
    mark_bytes,
    mark_structs,
    member_fields,
    member_marks,
    read_enums,
    read_step,
)
from tests.generated.expectations import (
    assert_same_fields,
    assert_same_structs,
    assert_same_table,
    assert_values_or_decode_error,
    fields_or_decode_error,
    plain,
)
from tests.tables import FORMATS, build_table


class TestValues(unittest.TestCase):
    def test_names_the_integers_that_the_schema_names_in_its_order(self) -> None:
        for enum, kind, values in ENUM_VALUES:
            with self.subTest(enum.__name__, kind=kind):
                members = [(member.name, member.value) for member in enum]

                self.assertEqual(members, list(values.items()))

    def test_holds_each_value_as_a_member_that_is_the_int(self) -> None:
        for enum, _, values in ENUM_VALUES:
            for name, value in values.items():
                with self.subTest(enum.__name__, name=name):
                    member = enum[name]

                    self.assertIsInstance(member, enum)
                    self.assertIsInstance(member, int)
                    self.assertEqual(member, value)
                    self.assertEqual(hash(member), hash(value))
                    self.assertIs(enum(value), member)


class TestRoundTrip(unittest.TestCase):
    @SETTINGS
    @given(enums_fields, slices)
    def test_reads_back_any_integer_of_an_enum_named_or_not_wherever_its_bytes_lie(
        self, fields: EnumsFields, slice: Slice
    ) -> None:
        data = slice.of(Enums.serialize(**fields))

        assert_same_table(
            self, describe_enums(read_enums(Enums(data))), describe_enums(fields)
        )

    @SETTINGS
    @given(enums_fields, slices)
    def test_reads_back_structs_and_tables_of_enums_by_index(
        self, fields: EnumsFields, slice: Slice
    ) -> None:
        data = slice.of(Enums.serialize(**fields))

        assert_same_table(
            self,
            describe_enums(read_enums(Enums(data), from_last)),
            describe_enums(fields),
        )

    @SETTINGS
    @given(enums_fields)
    def test_reads_an_array_of_enums_as_a_view_of_their_integer_type(
        self, fields: EnumsFields
    ) -> None:
        enums = Enums(Enums.serialize(**fields))
        arrays = {
            "uint8": enums.levels(),
            "int8": enums.directions(),
            "uint16": enums.units(),
            "int16": enums.changes(),
            "uint32": enums.statuses(),
            "int32": enums.signs(),
            "uint64": enums.masks(),
            "int64": enums.extents(),
        }

        for kind, array in arrays.items():
            self.assertIsInstance(array, memoryview, kind)
            self.assertEqual(array.format, FORMATS[kind], kind)

    @SETTINGS
    @given(mark_structs, binary(max_size=8), binary(max_size=8))
    def test_reads_back_a_struct_of_enums_at_an_offset_in_longer_data(
        self, mark: Mark, before: bytes, after: bytes
    ) -> None:
        data = before + mark.serialize() + after

        read = Mark.read(data, len(before))
        assert_same_fields(self, describe_mark(read), describe_mark(mark))

    @SETTINGS
    @given(lists(mark_structs, max_size=8))
    def test_reads_back_every_struct_of_enums_that_serialize_array_writes(
        self, marks: list[Mark]
    ) -> None:
        data = Mark.serialize_array(marks)

        self.assertEqual(data, b"".join(mark.serialize() for mark in marks))
        assert_same_structs(self, Mark.read_array(data), marks, describe_mark)

    @SETTINGS
    @given(mark_structs)
    def test_reads_and_writes_a_struct_of_enums_values_as_its_dataclass_holds_them(
        self, mark: Mark
    ) -> None:
        turn = mark.turn
        values = (turn.direction, turn.mask)

        self.assertEqual(Turn.read_values(turn.serialize()), values)
        self.assertEqual(Turn.serialize_values(*values), turn.serialize())
        self.assertEqual(Turn.read_array_values(turn.serialize() * 2), [values] * 2)


class TestMembers(unittest.TestCase):
    # A member of an enum is an int, so the generated code writes it wherever
    # it writes the enum's integer type, and reads the integer back as an int
    # that equals the member, rather than looking the member up

    def test_writes_the_values_that_an_enum_names(self) -> None:
        turn = Turn(Direction.BACKWARD, Mask.BEYOND_FLOAT64)
        mark = Mark(Level.HIGH, Change.LEAST, 0.5, Extent.GREATEST, turn)

        data = Enums.serialize(
            level=Level.MEDIUM,
            levels=[Level.LOW, Level.HIGH],
            direction=Direction.FURTHEST_BACK,
            directions=[Direction.FORWARD, Direction.BACKWARD],
            unit=Unit.LAST,
            units=[Unit.V_2, Unit.FIRST],
            change=Change.GREATEST,
            changes=[Change.LEAST],
            status=Status.NOT_FOUND,
            statuses=[Status.GREATEST, Status.OK],
            sign=Sign.LEAST,
            signs=[Sign.GREATEST],
            mask=Mask.ALL,
            masks=[Mask.ONE, Mask.ALL],
            extent=Extent.LEAST,
            extents=[Extent.ZERO, Extent.GREATEST],
            word=Word.TRUE,
            mark=mark,
            marks=[mark],
            steps=[{"level": Level.HIGH, "mark": mark, "name": "step"}],
        )

        read_mark = Mark(2, -32_768, 0.5, 2**63 - 1, Turn(-1, 9_007_199_254_740_993))
        self.assertEqual(
            read_enums(Enums(data)),
            {
                "level": 1,
                "levels": [0, 2],
                "direction": -128,
                "directions": [1, -1],
                "unit": 65_535,
                "units": [2, 0],
                "change": 32_767,
                "changes": [-32_768],
                "status": 404,
                "statuses": [4_294_967_295, 0],
                "sign": -2_147_483_648,
                "signs": [2_147_483_647],
                "mask": 2**64 - 1,
                "masks": [1, 2**64 - 1],
                "extent": -(2**63),
                "extents": [0, 2**63 - 1],
                "word": 6,
                "mark": read_mark,
                "marks": [read_mark],
                "steps": [{"level": 2, "mark": read_mark, "name": "step"}],
            },
        )

    @SETTINGS
    @given(member_fields)
    def test_writes_an_enums_members_as_the_integers_that_they_are(
        self, members: EnumsFields
    ) -> None:
        integers: EnumsFields = _as_integers(members)

        self.assertEqual(
            bytes(Enums.serialize(**members)), bytes(Enums.serialize(**integers))
        )

    @SETTINGS
    @given(lists(member_marks, min_size=1, max_size=8))
    def test_writes_a_structs_members_as_the_integers_that_they_are(
        self, members: list[Mark]
    ) -> None:
        integers: list[Mark] = _as_integers(members)
        turn, turn_integers = members[0].turn, integers[0].turn

        self.assertEqual(members[0].serialize(), integers[0].serialize())
        self.assertEqual(Mark.serialize_array(members), Mark.serialize_array(integers))
        self.assertEqual(
            Turn.serialize_values(turn.direction, turn.mask),
            Turn.serialize_values(turn_integers.direction, turn_integers.mask),
        )

    @SETTINGS
    @given(member_fields)
    def test_reads_an_integer_that_an_enum_names_as_an_int_equal_to_its_member(
        self, members: EnumsFields
    ) -> None:
        data = Enums.serialize(**members)
        enums = Enums(data)
        steps = enums.steps()

        reads: dict[str, object] = {
            "getters": read_enums(enums),
            "fields()": enums.fields(),
            "field_values()": enums.field_values(),
            "read_values()": Enums.read_values(data),
            "a vector's fields()": steps.fields(),
            "a vector's field_values()": steps.field_values(),
            "a struct's read_values()": Turn.read_values(members["mark"].serialize()),
        }
        for name, read in reads.items():
            types = {type(value) for value in _scalars(read)}

            self.assertLessEqual(types, {int, float, str}, name)

        integers: EnumsFields = _as_integers(members)
        assert_same_table(
            self, describe_enums(read_enums(enums)), describe_enums(integers)
        )


def _as_integers(value: Any) -> Any:
    """Fields, structs or values with each member of an enum as the plain int
    that it equals"""
    if isinstance(value, IntEnum):
        return int(value)

    if isinstance(value, dict):
        return {key: _as_integers(item) for key, item in value.items()}

    if isinstance(value, list):
        return [_as_integers(item) for item in value]

    if is_dataclass(value) and not isinstance(value, type):
        attributes = {
            field.name: _as_integers(getattr(value, field.name))
            for field in dataclass_fields(value)
        }
        return type(value)(**attributes)

    return value


def _scalars(value: object) -> Iterator[object]:
    """The scalars and strings that a reader returned, of its structs, arrays
    and nested fields too, but not of a view's items, which it hasn't read"""
    value = plain(value)

    if isinstance(value, dict):
        value = list(value.values())

    if isinstance(value, (list, tuple)):
        for item in value:
            yield from _scalars(item)
    else:
        yield value


class TestTablesBuiltByHand(unittest.TestCase):
    @SETTINGS
    @given(enums_fields)
    def test_writes_the_bytes_of_enums_that_table_builder_writes_of_their_integer_types(
        self, fields: EnumsFields
    ) -> None:
        by_hand = bytes(build_table(describe_enums(fields)).bytes())

        self.assertEqual(bytes(Enums.serialize(**fields)), by_hand)

    @SETTINGS
    @given(mark_structs)
    def test_writes_a_struct_of_enums_little_endian_at_fixed_offsets_in_24_bytes(
        self, mark: Mark
    ) -> None:
        self.assertEqual(Mark.SIZE, 24)
        self.assertEqual(mark.serialize(), mark_bytes(mark))

    @SETTINGS
    @given(enums_fields, integers(0, 0xFF))
    def test_reads_any_byte_of_an_enum_of_uint8_as_its_value(
        self, fields: EnumsFields, byte: int
    ) -> None:
        data = bytearray(Enums.serialize(**fields))
        # The level is Enums' field 0, in a table of the default layout
        position = TableReader(data, TableLayout.DEFAULT).root_offset(0)
        data[position] = byte
        enums = Enums(data)

        self.assertEqual(enums.level(), byte)
        self.assertEqual(enums.fields()["level"], byte)
        self.assertEqual(enums.field_values()[0], byte)

    @SETTINGS
    @given(mark_structs, integers(0, 0xFF))
    def test_reads_any_byte_of_a_structs_enum_of_uint8_as_its_value(
        self, mark: Mark, byte: int
    ) -> None:
        data = bytearray(mark.serialize())
        # The level is a Mark's first byte
        data[0] = byte

        self.assertEqual(Mark.read(data).level, byte)
        self.assertEqual(Mark.read_array(data)[0].level, byte)


class TestDecodeErrors(unittest.TestCase):
    @SETTINGS
    @given(enums_fields, corruptions)
    def test_reads_corrupted_enums_as_values_or_raises_decode_error(
        self, fields: EnumsFields, corruption: Truncation | Overwrite
    ) -> None:
        data = corruption.apply(bytes(Enums.serialize(**fields)))

        assert_values_or_decode_error(lambda: read_enums(Enums(data)))
        assert_values_or_decode_error(lambda: read_enums(Enums(data), from_last))

    @SETTINGS
    @given(binary(max_size=64))
    def test_reads_arbitrary_bytes_as_enums_or_raises_decode_error(
        self, data: bytes
    ) -> None:
        assert_values_or_decode_error(lambda: read_enums(Enums(data)))
        assert_values_or_decode_error(lambda: read_enums(Enums(data), from_last))

    @SETTINGS
    @given(binary(max_size=32), integers(1, Mark.SIZE))
    def test_raises_decode_error_when_the_data_ends_before_a_struct_of_enums(
        self, data: bytes, missing: int
    ) -> None:
        # Where the struct would end ``missing`` bytes after the data, or at
        # the start of data shorter than a struct
        offset = max(len(data) + missing - Mark.SIZE, 0)

        with self.assertRaises(DecodeError):
            Mark.read(data, offset)


class TestEveryField(unittest.TestCase):
    # fields() and field_values() return what the getters read, or raise
    # DecodeError as they do, and so do those of a vector's view

    @SETTINGS
    @given(enums_fields, one_of(corruptions, binary(max_size=0)))
    def test_reads_enums_fields_and_values_as_their_getters_do(
        self, fields: EnumsFields, corruption: Truncation | Overwrite | bytes
    ) -> None:
        data = bytes(Enums.serialize(**fields))
        if not isinstance(corruption, bytes):
            data = corruption.apply(data)
        enums = Enums(data)

        self.assert_same(enums.fields, partial(read_enums, enums))
        self.assert_same(enums.field_values, partial(_enums_values, enums))
        self.assert_same(partial(Enums.read_values, data), enums.field_values)

    @SETTINGS
    @given(enums_fields, one_of(corruptions, binary(max_size=0)))
    def test_reads_the_fields_and_values_of_a_vectors_tables_as_each_table_reads_them(
        self, fields: EnumsFields, corruption: Truncation | Overwrite | bytes
    ) -> None:
        data = bytes(Enums.serialize(**fields))
        if not isinstance(corruption, bytes):
            data = corruption.apply(data)
        enums = Enums(data)

        self.assert_same(
            lambda: enums.steps().fields(),
            lambda: [read_step(step) for step in enums.steps()],
        )
        self.assert_same(
            lambda: enums.steps().field_values(),
            lambda: [tuple(read_step(step).values()) for step in enums.steps()],
        )
        self.assert_same(
            lambda: list(enums.steps().iter_field_values()),
            lambda: [step.field_values() for step in enums.steps()],
        )

    def assert_same(
        self, read: Callable[[], object], expected: Callable[[], object]
    ) -> None:
        """Asserts that ``read`` returns what ``expected`` reads through the
        getters, or raises the same DecodeError"""
        self.assertEqual(
            fields_or_decode_error(lambda: plain(read())),
            fields_or_decode_error(lambda: plain(expected())),
        )


def _enums_values(enums: Enums) -> tuple[object, ...]:
    """The value of each field of an ``Enums`` as its getter returns it, in
    the fields' order, which its ``TypedDict`` declares them in"""
    return tuple(getattr(enums, name)() for name in EnumsFields.__annotations__)


if __name__ == "__main__":
    unittest.main()
