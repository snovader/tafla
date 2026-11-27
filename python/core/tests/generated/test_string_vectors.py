"""The vectors of strings of the code generated from
``schemas/all_types.tafla``, whose getters return a ``StringVector``, and
whose ``fields()``, ``field_values()`` and writers take lists of strings."""

import unittest

from hypothesis import given
from hypothesis.strategies import (
    SearchStrategy,
    builds,
    integers,
    lists,
    text,
)

from tafla import DecodeError, TableReader
from tests.descriptions import STRINGS
from tests.generated import SETTINGS
from tests.generated.all_types.all_types import (
    Entry,
    EntryFields,
    Labels,
    NarrowStringVectors,
    NarrowStringVectorsFields,
    StringVectors,
    StringVectorsFields,
)
from tests.generated.byte_strategies import (
    Overwrite,
    Slice,
    Truncation,
    corruptions,
    slices,
)
from tests.generated.descriptions import layout, message, scalar, string, table, vector
from tests.generated.expectations import assert_values_or_decode_error
from tests.tables import Description, build_table

STRING_LISTS: SearchStrategy[list[str]] = lists(STRINGS, max_size=6)

ENTRY_FIELDS: SearchStrategy[EntryFields] = builds(
    EntryFields, key=STRINGS, aliases=STRING_LISTS
)

STRING_VECTORS_FIELDS: SearchStrategy[StringVectorsFields] = builds(
    StringVectorsFields,
    id=integers(0, 2**32 - 1),
    tags=STRING_LISTS,
    labels=builds(dict, values=STRING_LISTS),
    entries=lists(ENTRY_FIELDS, max_size=3),
)

# Names of short strings, which fit in a table of uint16 offsets
NARROW_FIELDS: SearchStrategy[NarrowStringVectorsFields] = builds(
    NarrowStringVectorsFields,
    names=lists(text(max_size=8), max_size=6),
    count=integers(0, 255),
)


def string_vector(strings: list[str]) -> Description:
    return {"type": "string[]", "value": strings}


def read_entry(entry: Entry) -> EntryFields:
    """Reads an ``Entry`` through its getters, each string of it from its ends"""
    aliases = entry.aliases()
    return {
        "key": entry.key(),
        "aliases": [aliases[index] for index in range(len(aliases))],
    }


def read_string_vectors(table: StringVectors) -> StringVectorsFields:
    """Reads every field of a ``StringVectors`` through its getters and views"""
    return {
        "id": table.id(),
        "tags": table.tags().tolist(),
        "labels": {"values": list(table.labels().values())},
        "entries": [read_entry(entry) for entry in table.entries()],
    }


def describe_string_vectors(fields: StringVectorsFields) -> Description:
    """The description of the table that a ``StringVectors`` writes"""
    entries = [
        [string(entry["key"]), string_vector(list(entry["aliases"]))]
        for entry in fields["entries"]
    ]
    return message(
        layout(),
        [
            scalar("uint32", fields["id"]),
            string_vector(list(fields["tags"])),
            table([string_vector(list(fields["labels"]["values"]))]),
            vector(entries),
        ],
    )


def describe_narrow(fields: NarrowStringVectorsFields) -> Description:
    return message(
        layout(offset_type="uint16"),
        [string_vector(list(fields["names"])), scalar("uint8", fields["count"])],
    )


class TestStringVectors(unittest.TestCase):
    @SETTINGS
    @given(STRING_VECTORS_FIELDS, slices)
    def test_reads_back_the_vectors_of_strings_wherever_the_bytes_lie(
        self, fields: StringVectorsFields, slice: Slice
    ) -> None:
        data = slice.of(StringVectors.serialize(**fields))

        self.assertEqual(read_string_vectors(StringVectors(data)), fields)

    @SETTINGS
    @given(STRING_VECTORS_FIELDS)
    def test_writes_the_bytes_of_table_builder(
        self, fields: StringVectorsFields
    ) -> None:
        self.assertEqual(
            bytes(StringVectors.serialize(**fields)),
            bytes(build_table(describe_string_vectors(fields)).bytes()),
        )

    @SETTINGS
    @given(STRING_VECTORS_FIELDS)
    def test_reads_every_value_with_field_values_and_read_values(
        self, fields: StringVectorsFields
    ) -> None:
        data = StringVectors.serialize(**fields)
        expected = (fields["id"], fields["tags"], (fields["labels"]["values"],))

        for values in [
            StringVectors(data).field_values(),
            StringVectors.read_values(data),
        ]:
            self.assertEqual(values[:3], expected)
            self.assertEqual([entry.fields() for entry in values[3]], fields["entries"])

    @SETTINGS
    @given(ENTRY_FIELDS)
    def test_reads_back_an_entry_and_labels_with_fields_and_field_values(
        self, fields: EntryFields
    ) -> None:
        data = StringVectors.serialize(
            id=0, tags=[], labels={"values": fields["aliases"]}, entries=[fields]
        )
        table = StringVectors(data)
        entries = table.entries()

        self.assertEqual(entries[0].fields(), fields)
        self.assertEqual(
            entries.item_field_values(0), (fields["key"], fields["aliases"])
        )
        self.assertEqual(entries.field_values(), [(fields["key"], fields["aliases"])])
        self.assertEqual(table.labels().fields(), {"values": fields["aliases"]})
        self.assertEqual(table.labels().field_values(), (fields["aliases"],))

    @SETTINGS
    @given(NARROW_FIELDS, slices)
    def test_reads_back_every_field_of_narrow_string_vectors_of_uint16_offsets(
        self, fields: NarrowStringVectorsFields, slice: Slice
    ) -> None:
        data = slice.of(NarrowStringVectors.serialize(**fields))
        values = (fields["names"], fields["count"])

        self.assertEqual(NarrowStringVectors(data).fields(), fields)
        self.assertEqual(NarrowStringVectors(data).field_values(), values)
        self.assertEqual(NarrowStringVectors.read_values(data), values)
        self.assertEqual(
            bytes(NarrowStringVectors.serialize_values(*values)), bytes(data)
        )
        self.assertEqual(
            bytes(data), bytes(build_table(describe_narrow(fields)).bytes())
        )

    def test_raises_value_error_for_narrow_string_vectors_too_long_for_uint16(
        self,
    ) -> None:
        names = ["a" * 0x8000, "b" * 0x8000]

        with self.assertRaisesRegex(ValueError, "too long for uint16 offsets"):
            NarrowStringVectors.serialize(names=names, count=0)
        with self.assertRaisesRegex(ValueError, "too long for uint16 offsets"):
            NarrowStringVectors.serialize_values(names, 0)

    @SETTINGS
    @given(STRING_VECTORS_FIELDS, lists(corruptions, min_size=1, max_size=3))
    def test_reads_corrupted_bytes_as_values_or_raises_decode_error(
        self,
        fields: StringVectorsFields,
        corrupted_by: list[Truncation | Overwrite],
    ) -> None:
        data = bytes(StringVectors.serialize(**fields))
        for corruption in corrupted_by:
            data = corruption.apply(data)

        assert_values_or_decode_error(lambda: read_string_vectors(StringVectors(data)))
        assert_values_or_decode_error(lambda: StringVectors(data).field_values())
        assert_values_or_decode_error(lambda: StringVectors(data).labels().fields())
        assert_values_or_decode_error(lambda: StringVectors.read_values(data))

    def test_raises_decode_error_for_a_tag_that_isnt_valid_utf8_and_reads_the_others(
        self,
    ) -> None:
        data = bytearray(
            StringVectors.serialize(
                id=1, tags=["ok", "bad"], labels={"values": []}, entries=[]
            )
        )
        # The second tag's first byte, after the count, two ends and "ok"
        reader = TableReader(bytes(data))
        data[reader.root_offset(1) + 3 * 4 + 2] = 0xC0
        tags = StringVectors(data).tags()

        self.assertEqual(tags[0], "ok")
        with self.assertRaises(DecodeError):
            tags[1]
        with self.assertRaises(DecodeError):
            tags.tolist()
        with self.assertRaises(DecodeError):
            StringVectors(data).field_values()

    def test_reads_a_string_vector_through_labels_as_its_view(self) -> None:
        data = StringVectors.serialize(
            id=1, tags=["x"], labels={"values": ["a", "東"]}, entries=[]
        )
        labels: Labels = StringVectors(data).labels()

        self.assertEqual(len(labels.values()), 2)
        self.assertEqual(labels.values()[-1], "東")


if __name__ == "__main__":
    unittest.main()
