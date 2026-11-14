"""Checks every prototype against ``tafla.TableBuilder``, with whichever
implementation ``tafla`` uses: the conformance suite's tables and the
comparison's workloads byte for byte, generated tables in every offset type
call by call, errors included, and ``TableBuilder``'s own tests, which
pass but for the behaviours that writing at set time changes."""

import unittest
from array import array
from collections.abc import Callable, Generator, Iterator
from contextlib import contextmanager
from typing import Any
from unittest.mock import patch

from hypothesis import given, settings
from hypothesis.strategies import (
    DrawFn,
    booleans,
    composite,
    integers,
    none,
    one_of,
    sampled_from,
)
from tafla_prototype import direct_table_builder as pure
from tafla_prototype.builders import NATIVE, PURE, Builder, using

import tafla.elements
import tests.test_table_builder as table_builder_tests
from tafla import TableBuilder
from tafla_benchmarks.codecs import tafla as codec
from tafla_benchmarks.workloads import RECORD, document_input, list_input
from tests import tables
from tests.byte_order import big_endian_host
from tests.descriptions import tables as described_tables
from tests.tables import Description
from tests.test_conformance import SUITE, read_bytes
from tests.test_table_builder import with_offset_type


class _AllSegments(pure.DirectTableBuilder):
    """Keeps every run as a segment, so that segments meet every part of the
    format, which generated tables are too short to reach otherwise"""

    __slots__ = ()
    _segment_bytes = 1


class _EightByteSegments(pure.DirectTableBuilder):
    __slots__ = ()
    _segment_bytes = 8


class _AllSegmentsPreallocated(pure.PreallocatedTableBuilder):
    __slots__ = ()
    _segment_bytes = 1


PROTOTYPES: tuple[Builder, ...] = (
    *PURE,
    Builder("pure, +=, all segments", _AllSegments),
    Builder("pure, +=, 8-byte segments", _EightByteSegments),
    Builder("pure, preallocated, all segments", _AllSegmentsPreallocated),
    *NATIVE,
    *((Builder("native, all segments", NATIVE[0].cls, (1, False)),) if NATIVE else ()),
)


@contextmanager
def building_with(builder: Builder) -> Generator[Any]:
    """Makes the core tests' helpers, and their tests of TableBuilder, build
    with ``builder``'s class"""
    with (
        using(builder) as cls,
        patch.object(tables, "TableBuilder", cls),
        patch.object(table_builder_tests, "TableBuilder", cls),
    ):
        yield cls


@contextmanager
def big_endian_everywhere() -> Iterator[None]:
    """Treats the host as big-endian for the native classes, as
    ``big_endian_host()`` does with the native implementation, and for the
    pure-Python prototypes, which work with either"""
    with big_endian_host(), patch.object(tafla.elements, "IS_LITTLE_ENDIAN", False):
        yield


class _Raised(Exception):
    """What a fill raises, which no builder raises itself"""


@contextmanager
def fills_raising_at(call: int | None) -> Iterator[None]:
    """Makes the ``call``th fill of a table nested in place set half its
    fields and raise ``_Raised``"""
    fill = tables._set_fields_in_place
    calls = 0

    def raising(builder: Any, fields: list[Description], offset_type: Any) -> None:
        nonlocal calls
        calls += 1
        if calls == call:
            fill(builder, fields[: len(fields) // 2], offset_type)
            raise _Raised

        fill(builder, fields, offset_type)

    with patch.object(tables, "_set_fields_in_place", raising):
        yield


def attempt(call: Callable[[], object]) -> object:
    """What a call returns, or the type and message of what it raises"""
    try:
        return call()
    except (TypeError, ValueError, _Raised) as error:
        return type(error).__name__, str(error)


def outcomes(
    cls: Any,
    table: Description,
    *,
    extra_fields: int,
    in_place: bool,
    raise_at: int | None,
) -> list[object]:
    """Sets each field of ``table`` on a builder of ``cls`` whose header has
    ``extra_fields`` more fields than the table, carrying on past a field
    that raises, and then writes it. Returns what each call returned or
    raised."""
    layout = tables.layout_of(table["layout"])
    fields = len(table["fields"]) + extra_fields
    offset_type = table["layout"]["offsetType"]
    if table["layout"]["versioned"]:
        builder = cls(fields, layout, table["version"])
    else:
        builder = cls(fields, layout)

    def set_field(field: Description) -> None:
        if in_place:
            tables.set_field_in_place(builder, field, offset_type)
        else:
            tables.set_field(builder, field)

    results: list[object] = []
    with fills_raising_at(raise_at):
        for field in table["fields"]:
            results.append(attempt(lambda field=field: set_field(field)))
    results.append(attempt(lambda: bytes(builder.bytes())))

    return results


@composite
def long_vectors(draw: DrawFn) -> Description:
    """Tables of uint8 offsets whose last field is a vector of many copies of
    one table, which is written in place and often makes its parent too long
    for its offsets, after fields that start it at any offset"""
    item = draw(described_tables(1, header=False, offset_types=("uint8",)))
    before = draw(described_tables(0, header=False, offset_types=("uint8",)))
    count = draw(integers(0, 40))
    vector = {"type": "vector", "items": [{"from": "builder", "table": item}] * count}

    return {
        "layout": {"sized": False, "versioned": False, "offsetType": "uint8"},
        "fields": [*before["fields"], vector],
    }


@composite
def long_tables(draw: DrawFn) -> Description:
    """Tables of uint8 offsets whose last field is a table of the same fields
    several times over, which is written in place and often makes its parent
    too long for its offsets, though not itself"""
    fields = draw(described_tables(1, header=False, offset_types=("uint8",)))
    before = draw(described_tables(0, header=False, offset_types=("uint8",)))
    nested = {
        "layout": fields["layout"],
        "fields": fields["fields"] * draw(integers(1, 8)),
    }
    table = {"type": "table", "from": "builder", "table": nested}

    return {
        "layout": {"sized": False, "versioned": False, "offsetType": "uint8"},
        "fields": [*before["fields"], table],
    }


@composite
def tables_near_the_limit(draw: DrawFn) -> Description:
    """Tables of uint8 offsets that start with an array of 180 to 250 bytes,
    so that each kind of field after it is the one that makes the table too
    long, with its nested tables written in place or not"""
    table = draw(described_tables(1, offset_types=("uint8",)))
    array = {"type": "uint8[]", "value": [0] * draw(integers(180, 250))}
    table = {**table, "fields": [array, *table["fields"]]}

    return tables.nested_in_place(table) if draw(booleans()) else table


ANY_TABLES = one_of(
    described_tables(),
    described_tables().map(lambda table: with_offset_type(table, "uint8")),
    described_tables().map(lambda table: with_offset_type(table, "uint16")),
    described_tables().map(tables.nested_in_place),
    described_tables().map(
        lambda table: with_offset_type(tables.nested_in_place(table), "uint8")
    ),
    long_vectors(),
    long_tables(),
    tables_near_the_limit(),
)
"""Tables of any offset type, the same tables of short offsets, whose limits
their fields reach, and tables whose nested tables are all from builders
without a header, which are written in place more often, and too long for
short offsets more often, long vectors and tables written in place, and
fields that reach a short offset type's limit"""


class TestConformance(unittest.TestCase):
    def test_rebuilds_every_valid_table_byte_for_byte(self) -> None:
        for builder in PROTOTYPES:
            for build in (tables.build_table, tables.build_table_in_place):
                for case in SUITE["valid"]:
                    with (
                        self.subTest(
                            builder.name, build=build.__name__, case=case["name"]
                        ),
                        building_with(builder),
                    ):
                        built = bytes(build(case["table"]).bytes())
                        self.assertEqual(built, read_bytes(case["file"]))

    def test_writes_what_table_builder_writes_on_a_big_endian_host(self) -> None:
        for case in SUITE["valid"]:
            with big_endian_everywhere():
                expected = bytes(tables.build_table(case["table"]).bytes())
                for builder in PROTOTYPES:
                    with (
                        self.subTest(builder.name, case=case["name"]),
                        building_with(builder),
                    ):
                        built = tables.build_table_in_place(case["table"]).bytes()
                        self.assertEqual(bytes(built), expected)


class TestWorkloads(unittest.TestCase):
    def test_writes_the_comparisons_workloads_as_table_builder_does(self) -> None:
        workloads: list[tuple[str, Callable[[Any], Any], Any]] = [
            ("record", codec.serialize_record, RECORD),
            ("document of 100 rows", codec.serialize_document, document_input(100)),
            (
                "document of 100,000 rows",
                codec.serialize_document,
                document_input(100_000),
            ),
            ("vector of 100 nested tables", codec.serialize_list, list_input(100)),
            ("array of 100 structs", codec.serialize_structs, list_input(100)),
        ]

        for name, serialize, value in workloads:
            expected = bytes(serialize(value))
            for builder in PROTOTYPES:
                with (
                    self.subTest(builder.name, workload=name),
                    using(builder) as cls,
                    patch.object(codec, "TableBuilder", cls),
                ):
                    self.assertEqual(bytes(serialize(value)), expected)


class TestGeneratedTables(unittest.TestCase):
    @settings(max_examples=500, deadline=None)
    @given(
        ANY_TABLES,
        sampled_from([-1, 0, 1]),
        booleans(),
        one_of(none(), integers(1, 6)),
    )
    def test_each_call_returns_and_raises_what_table_builder_does(
        self,
        table: Description,
        extra_fields: int,
        in_place: bool,
        raise_at: int | None,
    ) -> None:
        """Every setter's error, a field more or fewer than the header holds,
        fields too long for short offsets, and fills that raise partway,
        after each of which a builder carries on as it was"""
        if len(table["fields"]) + extra_fields < 0:
            extra_fields = 0

        def outcome(cls: Any) -> list[object]:
            return outcomes(
                cls,
                table,
                extra_fields=extra_fields,
                in_place=in_place,
                raise_at=raise_at,
            )

        with patch.object(tables, "TableBuilder", TableBuilder):
            expected = outcome(TableBuilder)

        for builder in PROTOTYPES:
            with building_with(builder) as cls:
                self.assertEqual(outcome(cls), expected, builder.name)


def _small_table(fields: list[Description]) -> Description:
    return {"layout": DEFAULT_UINT8, "fields": fields}


DEFAULT_UINT8: Description = {
    "sized": False,
    "versioned": False,
    "offsetType": "uint8",
}

_PAIR = _small_table(
    [{"type": "float32", "value": 1.5}, {"type": "int16[]", "value": [1, -2]}]
)

KINDS_OF_FIELDS: list[Description] = [
    *({"type": kind, "value": 1} for kind in ("int8", "int16", "int32", "int64")),
    {"type": "float32", "value": 0.5},
    {"type": "float64", "value": -0.0},
    {"type": "string", "value": ""},
    {"type": "string", "value": "tafla"},
    {
        "type": "struct",
        "fields": [{"type": "uint8", "value": 1}, {"type": "int16", "value": -1}],
    },
    {"type": "int32[]", "value": [1, 2, 3]},
    {"type": "float64[]", "value": [0.5, 1.5]},
    {"type": "struct[]", "fields": ["int16", "uint32"], "items": [[1, 2], [3, 4]]},
    {"type": "table", "from": "bytes", "table": _PAIR},
    {"type": "table", "from": "builder", "table": _PAIR},
    {"type": "vector", "items": []},
    {"type": "vector", "items": [{"from": "builder", "table": _PAIR}] * 2},
    {"type": "vector", "items": [{"from": "bytes", "table": _PAIR}] * 2},
]
"""A field of each kind, of a few bytes, with nested tables and vectors that
are written in place or as bytes"""


class TestFieldsAtTheLimit(unittest.TestCase):
    def test_each_kind_of_field_at_every_position_near_the_limit(self) -> None:
        """After an array that starts it at every offset near the limit of
        uint8 offsets, each kind of field fits or raises as it does with
        TableBuilder"""
        for field in KINDS_OF_FIELDS:
            for length in range(200, 256):
                array = {"type": "uint8[]", "value": [0] * length}
                table = _small_table([array, field])
                for in_place in (False, True):
                    self.assertWritesAsTableBuilderDoes(table, in_place=in_place)

    def assertWritesAsTableBuilderDoes(
        self, table: Description, *, in_place: bool
    ) -> None:
        with patch.object(tables, "TableBuilder", TableBuilder):
            expected = outcomes(
                TableBuilder, table, extra_fields=0, in_place=in_place, raise_at=None
            )
        for builder in PROTOTYPES:
            with building_with(builder) as cls:
                built = outcomes(
                    cls, table, extra_fields=0, in_place=in_place, raise_at=None
                )
                self.assertEqual(built, expected, (builder.name, in_place))


# TableBuilder's tests of the behaviours that writing at set time changes,
# by the test's name: what they check, and what the prototypes do instead
CHANGED_BEHAVIOURS = {
    # A scalar that its type can't hold raises ValueError from its setter,
    # rather than from finalize()
    "test_rejects_a_scalar_that_doesnt_fit_its_type_from_finalize",
    # Buffers shorter than a segment are copied when they're set, so a change
    # after that, of their values or their length, is neither written nor
    # raised. Those tests' buffers are a few bytes long.
    "test_raises_value_error_for_any_buffer_whose_length_changed_the_tables",
    "test_writes_the_values_of_buffers_whose_lengths_changed_but_not_the_tables",
}
CHANGED_BEHAVIOURS_OF_SHORT_ARRAYS = {
    # An array of 2 float32s or 3 float64s, which a builder that keeps runs
    # of 8 bytes or more keeps
    "test_writes_values_as_they_are_when_finalizing",
    "test_writes_a_nested_builders_values_as_they_are_when_finalized",
    "test_raises_value_error_for_an_array_that_grew_or_shrank_and_writes_it_once_restored",
}


class TestTableBuildersTests(unittest.TestCase):
    def test_pass_but_for_the_behaviours_that_writing_at_set_time_changes(
        self,
    ) -> None:
        loader = unittest.TestLoader()
        for builder in PROTOTYPES:
            with self.subTest(builder.name), building_with(builder) as cls:
                suite = loader.loadTestsFromModule(table_builder_tests)
                result = unittest.TestResult()
                suite.run(result)

                # A subtest's failure names the test that it's part of
                failed = {
                    getattr(test, "test_case", test).id().rsplit(".", 1)[-1]
                    for test, _ in result.failures + result.errors
                }
                changed = set(CHANGED_BEHAVIOURS)
                if segment_bytes(builder, cls) > 8:
                    changed |= CHANGED_BEHAVIOURS_OF_SHORT_ARRAYS
                self.assertEqual(failed, changed)
                self.assertGreater(result.testsRun, len(CHANGED_BEHAVIOURS))


class TestChangedBehaviours(unittest.TestCase):
    """What the prototypes do where TableBuilder's tests expect otherwise"""

    def test_rejects_a_scalar_that_doesnt_fit_its_type_from_its_setter(self) -> None:
        for builder in PROTOTYPES:
            with self.subTest(builder.name), using(builder) as cls:
                table = cls(2)
                table.set_uint8(1)

                with self.assertRaisesRegex(ValueError, r"Can't write 4294967296 at 9"):
                    table.set_uint32(2**32)
                with self.assertRaisesRegex(ValueError, "Can't write 1e\\+300 at 9"):
                    table.set_float32(1e300)

                table.set_uint32(7)
                self.assertEqual(bytes(table.bytes()), bytes(_uint8_and_uint32(1, 7)))

    def test_writes_a_short_array_as_it_was_when_set_and_a_long_one_as_it_is(
        self,
    ) -> None:
        for builder in PROTOTYPES:
            short = array("f", [1, 2])
            long = array("f", range(64))
            with self.subTest(builder.name), using(builder) as cls:
                table = cls(2)
                table.set_float32_array(short)
                table.set_float32_array(long)
                short[0] = 3
                long[0] = 3

                buffer = table.finalize()
                written = [
                    buffer.get_float32_array(buffer.root_offset(field))[0]
                    for field in (0, 1)
                ]
                shortest = segment_bytes(builder, cls)
                self.assertEqual(
                    written, [3 if 8 >= shortest else 1, 3 if 256 >= shortest else 0]
                )

    def test_raises_value_error_from_bytes_for_a_kept_array_whose_length_changed(
        self,
    ) -> None:
        for builder in PROTOTYPES:
            values = array("f", range(64))
            with self.subTest(builder.name), using(builder) as cls:
                if segment_bytes(builder, cls) > 256:
                    continue

                table = cls(1)
                table.set_float32_array(values)
                values.append(64)

                with self.assertRaisesRegex(
                    ValueError, "The fields wrote 268 bytes rather than the 264"
                ):
                    table.bytes()

                values.pop()
                self.assertEqual(len(table.bytes()), 264)


def segment_bytes(builder: Builder, cls: Any) -> int:
    """The shortest run that a builder keeps as a segment"""
    if builder.native_defaults is not None:
        return builder.native_defaults[0]
    return cls._segment_bytes


def _uint8_and_uint32(first: int, second: int) -> memoryview:
    builder = TableBuilder(2)
    builder.set_uint8(first)
    builder.set_uint32(second)
    return builder.bytes()


if __name__ == "__main__":
    unittest.main()
