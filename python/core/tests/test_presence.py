"""The properties of absent fields, whose offsets are 0, and of the fields that
a reader of a later revision of a table finds missing, as SPEC.md describes
them. The tables are those of the conformance suite's model, with fields left
out; ``test_conformance.py`` reads the suite's cases of presence."""

import unittest
from itertools import count

from hypothesis import assume, given
from hypothesis.strategies import (
    DrawFn,
    SearchStrategy,
    booleans,
    composite,
    integers,
    lists,
)

from tafla import DecodeError, TableBuilder
from tests.descriptions import tables
from tests.generated.byte_strategies import Overwrite, Slice, Truncation, corruptions
from tests.tables import (
    Description,
    MismatchError,
    build_table,
    build_table_in_place,
    comparable_table,
    layout_of,
    nested_in_place,
    read_table,
    write_table,
)

ABSENT: Description = {"type": "absent"}


def with_absences(table: Description, absences: list[bool]) -> Description:
    """The table with fields left out at every depth: each field in turn,
    a nested table's after the field that holds it, is absent when the next
    of ``absences`` says so"""
    fields = count()

    def within(table: Description) -> Description:
        return {**table, "fields": [field(each) for each in table["fields"]]}

    def field(description: Description) -> Description:
        if absences[next(fields) % len(absences)]:
            return ABSENT

        if description["type"] == "table":
            return {**description, "table": within(description["table"])}

        if description["type"] == "vector":
            items = [
                {**item, "table": within(item["table"])}
                for item in description["items"]
            ]
            return {**description, "items": items}

        return description

    return within(table)


@composite
def presence_tables(draw: DrawFn) -> Description:
    """Tables of at least one field, some of which, at every depth, are
    absent. Leaving a field out never lengthens a table, since the padding
    after it aligns from an earlier position, so the tables fit their
    offset types as those of ``tables()`` do."""
    table = draw(tables())
    assume(table["fields"])
    absences = draw(lists(booleans(), min_size=1, max_size=24))

    return with_absences(table, absences)


def revised_tables() -> SearchStrategy[Description]:
    """Tables whose first field is present, as a table of later revisions
    needs, since a reader finds the fields that the table holds from where
    its first value starts"""
    return presence_tables().filter(lambda table: table["fields"][0] != ABSENT)


def fits(table: Description) -> bool:
    """Whether a table and those nested in it fit their offset types, as their
    builders judge"""
    try:
        build_table(table).bytes()
    except ValueError:
        return False

    return True


class TestAbsentFields(unittest.TestCase):
    @given(presence_tables(), integers(0, 7))
    def test_reads_back_the_values_of_a_table_with_absent_fields_at_any_alignment(
        self, table: Description, misalignment: int
    ) -> None:
        backing = bytearray(misalignment) + build_table(table).bytes()

        read = read_table(memoryview(backing)[misalignment:], table)

        self.assertEqual(comparable_table(read), comparable_table(table))

    @given(presence_tables())
    def test_writes_0_as_the_offset_of_an_absent_field_and_every_other_after_the_offsets(
        self, table: Description
    ) -> None:
        reader = build_table(table).finalize()
        layout = layout_of(table["layout"])
        first_value = layout.header_size() + len(table["fields"]) * layout.offset_size()

        for index, field in enumerate(table["fields"]):
            offset = reader.root_offset(index)

            if field == ABSENT:
                self.assertEqual(offset, 0)
            else:
                self.assertGreaterEqual(offset, first_value)

    @given(presence_tables())
    def test_writes_the_bytes_that_table_builder_writes_with_table_writer_and_in_place(
        self, table: Description
    ) -> None:
        table = nested_in_place(table)
        assume(fits(table))
        built = bytes(build_table(table).bytes())

        self.assertEqual(bytes(write_table(table)), built)
        self.assertEqual(bytes(build_table_in_place(table).bytes()), built)

    def test_counts_an_absent_field_as_a_field_of_the_builder(self) -> None:
        builder = TableBuilder(1)
        builder.set_absent()

        with self.assertRaises(ValueError):
            builder.set_absent()
        self.assertEqual(bytes(builder.bytes()), bytes(4))
        with self.assertRaises(TypeError):
            builder.set_absent()

    def test_records_an_absent_field_of_a_table_nested_in_place(self) -> None:
        def fill(nested: TableBuilder) -> None:
            nested.set_absent()
            nested.set_uint8(7)

        builder = TableBuilder(2)
        builder.set_absent()
        builder.set_table_in_place(fill, fields=2)
        reader = builder.finalize()

        table = reader.root_offset(1)
        self.assertEqual(reader.root_offset(0), 0)
        self.assertEqual(reader.field_offset(table, 0), table)
        self.assertEqual(reader.get_uint8(reader.field_offset(table, 1)), 7)


class TestFieldsOfLaterRevisions(unittest.TestCase):
    @given(revised_tables(), integers(1, 4))
    def test_finds_every_field_after_the_tables_own_missing(
        self, table: Description, missing: int
    ) -> None:
        described = {**table, "missing": missing}

        read = read_table(build_table(table).bytes(), described)

        self.assertEqual(comparable_table(read), comparable_table(described))

    @given(revised_tables(), integers(min_value=0))
    def test_reads_an_earlier_revision_from_a_table_that_holds_more_and_finds_the_rest_unless_absent(
        self, table: Description, split: int
    ) -> None:
        known = 1 + split % len(table["fields"])
        later = table["fields"][known:]
        earlier = {**table, "fields": table["fields"][:known], "missing": len(later)}

        read = read_table(build_table(table).bytes(), earlier)

        absent = sum(1 for field in later if field == ABSENT)
        expected = {**earlier, "missing": absent}
        self.assertEqual(comparable_table(read), comparable_table(expected))


class TestUntrustedBytes(unittest.TestCase):
    @given(
        revised_tables(),
        integers(0, 4),
        lists(corruptions, min_size=1, max_size=3),
        integers(0, 15),
    )
    def test_reads_corrupted_bytes_as_values_or_decode_error_within_them(
        self,
        table: Description,
        missing: int,
        corrupted: list[Truncation | Overwrite],
        offset: int,
    ) -> None:
        """The bytes around them, which differ, read alike, so no read leaves
        them"""
        data = bytes(build_table(table).bytes())
        for corruption in corrupted:
            data = corruption.apply(data)
        described = {**table, "missing": missing}

        self.assertEqual(
            read_outcome(Slice(offset, 0x00).of(data), described),
            read_outcome(Slice(offset, 0xFF).of(data), described),
        )


def read_outcome(data: memoryview[int], table: Description) -> object:
    """What reading the table as its description sets it out returns: the
    description read, or the mismatch of bytes that read as another shape,
    or ``DecodeError``, by its message. Any other error fails the test."""
    try:
        return comparable_table(read_table(data, table))
    except (DecodeError, MismatchError) as error:
        return f"{type(error).__name__}: {error}"


if __name__ == "__main__":
    unittest.main()
