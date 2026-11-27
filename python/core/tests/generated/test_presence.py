"""The code generated from ``typescript/core/test/presence.tafla``: optional
fields, which its writers leave out with an offset of 0, and fields that a
later version of a message or a later revision of a table appends, which its
readers return as ``None``, or a scalar's default, from the tables that don't
hold them."""

import unittest
from collections.abc import Callable
from functools import partial
from struct import pack_into

from hypothesis import given
from hypothesis.strategies import binary, integers, lists

from tafla import DecodeError, TableReader
from tests.generated import SETTINGS
from tests.generated.byte_strategies import (
    Overwrite,
    Slice,
    Truncation,
    corruptions,
    slices,
)
from tests.generated.expectations import (
    assert_values_or_decode_error,
    fields_or_decode_error,
)
from tests.generated.presence.presence import (
    Entry,
    Point,
    Record,
    RecordFields,
    RecordV1,
    Sample,
    SampleV1,
)
from tests.generated.presence_tables import (
    COUNT_DEFAULT,
    FLAG_DEFAULT,
    TOTAL_DEFAULT,
    WEIGHT_DEFAULT,
    Z_DEFAULT,
    describe_record,
    expected_from_first_version,
    expected_record,
    first_version_of,
    read_record,
    record_fields,
)
from tests.tables import build_table

_UINT16S = integers(0, 0xFFFF)
_UINT32S = integers(0, 0xFFFF_FFFF)

# The position of a Record's version in its header, after its length
_VERSION_SLOT = 4


def _same_reads(test: unittest.TestCase, actual: object, expected: object) -> None:
    """Asserts that two reads hold the same values, with floats compared by
    their bits, so that NaN equals NaN and -0.0 differs from 0.0"""
    test.assertEqual(
        fields_or_decode_error(lambda: actual), fields_or_decode_error(lambda: expected)
    )


class TestOptionalFields(unittest.TestCase):
    @SETTINGS
    @given(record_fields(), slices)
    def test_reads_back_every_field_of_a_record_and_none_or_its_default_for_each_absent_one(
        self, fields: RecordFields, slice: Slice
    ) -> None:
        data = slice.of(Record.serialize(**fields))

        _same_reads(self, read_record(Record(data)), expected_record(fields))

    @SETTINGS
    @given(record_fields())
    def test_writes_an_absent_field_as_table_builder_set_absent_does(
        self, fields: RecordFields
    ) -> None:
        by_hand = bytes(build_table(describe_record(fields)).bytes())

        self.assertEqual(bytes(Record.serialize(**fields)), by_hand)

    @SETTINGS
    @given(record_fields())
    def test_writes_an_optional_field_given_as_none_as_one_left_out(
        self, fields: RecordFields
    ) -> None:
        left_out: RecordFields = {"id": fields["id"], "label": fields["label"]}
        with_none: RecordFields = {
            **left_out,
            "name": None,
            "values": None,
            "origin": None,
            "flag": None,
            "entries": None,
            "total": None,
            "extent": None,
        }

        self.assertEqual(
            bytes(Record.serialize(**with_none)), bytes(Record.serialize(**left_out))
        )

    def test_reads_the_defaults_and_has_methods_of_scalars_left_out(self) -> None:
        record = Record(
            Record.serialize(id=1, label="", origin={"x": 1, "y": 2, "z": 3})
        )

        self.assertIs(record.flag(), FLAG_DEFAULT)
        self.assertFalse(record.has_flag())
        self.assertEqual(record.total(), TOTAL_DEFAULT)
        self.assertFalse(record.has_total())
        self.assertIsNone(record.name())
        self.assertIsNone(record.entries())

        origin = record.origin()
        assert origin is not None
        self.assertEqual(origin.z(), 3)
        self.assertTrue(origin.has_z())
        self.assertIsNone(origin.label())


class TestVersions(unittest.TestCase):
    @SETTINGS
    @given(record_fields())
    def test_reads_a_record_of_the_first_version_with_none_or_the_default_for_each_field_that_the_second_added(
        self, fields: RecordFields
    ) -> None:
        """In its nested tables and vector items too, which are of their first
        revisions"""
        first = first_version_of(fields)
        data = RecordV1.serialize(**first)

        _same_reads(self, read_record(Record(data)), expected_from_first_version(first))

    @SETTINGS
    @given(_UINT32S, record_fields())
    def test_rejects_a_record_of_a_version_before_the_first_or_after_its_own(
        self, version: int, fields: RecordFields
    ) -> None:
        data = bytearray(Record.serialize(**fields))
        pack_into("<I", data, _VERSION_SLOT, version)

        if Record.FIRST_VERSION <= version <= Record.FORMAT_VERSION:
            Record(data)
        else:
            with self.assertRaises(DecodeError):
                Record(data)
        self.assertEqual(Record.version_of(data), version)

    def test_keeps_rejecting_other_versions_with_a_reader_of_one_version(
        self,
    ) -> None:
        data = Record.serialize(id=1, label="")

        with self.assertRaises(DecodeError):
            RecordV1(data)

    @SETTINGS
    @given(_UINT32S, _UINT16S)
    def test_reads_a_sample_of_the_first_revision_with_the_default_for_the_field_that_the_second_appended(
        self, key: int, count: int
    ) -> None:
        first = SampleV1.serialize(key=key)
        second = Sample.serialize(key=key, count=count)

        self.assertEqual(Sample(first).key(), key)
        self.assertEqual(Sample(first).count(), COUNT_DEFAULT)
        self.assertFalse(Sample(first).has_count())
        self.assertEqual(Sample(second).count(), count)
        self.assertTrue(Sample(second).has_count())
        # A reader of the first revision reads its fields of a later one
        self.assertEqual(SampleV1(second).key(), key)


class TestUntrustedBytes(unittest.TestCase):
    @SETTINGS
    @given(record_fields(), lists(corruptions, min_size=1, max_size=3))
    def test_reads_a_corrupted_record_as_values_none_defaults_or_decode_error(
        self, fields: RecordFields, corrupted: list[Truncation | Overwrite]
    ) -> None:
        data = bytes(Record.serialize(**fields))
        for corruption in corrupted:
            data = corruption.apply(data)

        assert_values_or_decode_error(partial(_read_corrupted_record, self, data))

    @SETTINGS
    @given(binary(max_size=48))
    def test_reads_arbitrary_bytes_as_a_sample_as_values_or_decode_error(
        self, data: bytes
    ) -> None:
        sample = Sample(data)

        assert_values_or_decode_error(sample.key)
        assert_values_or_decode_error(
            partial(_check_default, self, sample.count, sample.has_count, COUNT_DEFAULT)
        )

    @SETTINGS
    @given(binary(max_size=48), integers(0, 48))
    def test_reads_arbitrary_bytes_as_a_nested_table_as_values_or_decode_error(
        self, data: bytes, position: int
    ) -> None:
        reader = TableReader(data)
        point = Point(reader, position)
        entry = Entry(reader, position)

        assert_values_or_decode_error(
            partial(_check_default, self, point.z, point.has_z, Z_DEFAULT)
        )
        assert_values_or_decode_error(
            partial(
                _check_default, self, entry.weight, entry.has_weight, WEIGHT_DEFAULT
            )
        )
        assert_values_or_decode_error(point.label)
        assert_values_or_decode_error(entry.note)


def _read_corrupted_record(test: unittest.TestCase, data: bytes) -> None:
    """Reads every field of a record whose bytes may be corrupted, as values,
    ``None`` or the defaults, and checks that each scalar that ``has_<field>()``
    doesn't find reads as its default"""
    record = Record(data)
    read_record(record)

    _check_default(test, record.flag, record.has_flag, FLAG_DEFAULT)
    _check_default(test, record.total, record.has_total, TOTAL_DEFAULT)

    origin = record.origin()
    if origin is not None:
        _check_default(test, origin.z, origin.has_z, Z_DEFAULT)

    for entry in record.entries() or []:
        _check_default(test, entry.weight, entry.has_weight, WEIGHT_DEFAULT)


def _check_default(
    test: unittest.TestCase,
    read: Callable[[], object],
    has: Callable[[], bool],
    default: object,
) -> None:
    """Asserts that a scalar that ``has`` doesn't find reads as its default"""
    if not has():
        test.assertEqual(read(), default)


if __name__ == "__main__":
    unittest.main()
