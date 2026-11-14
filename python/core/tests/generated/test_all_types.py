"""The code generated from ``schemas/all_types.tafla``, of every type of field,
writes and reads tables with tafla."""

import unittest
from collections.abc import Buffer, Callable, Iterable
from dataclasses import FrozenInstanceError, astuple, fields, replace
from functools import partial
from typing import Any, NamedTuple

from hypothesis import example, given
from hypothesis.strategies import binary, integers, lists, one_of

from tafla import ByteBuffer, DecodeError, TableLayout
from tests.generated import SETTINGS
from tests.generated.all_types.all_types import (
    Arrays,
    ArraysFields,
    Documented,
    Keywords,
    KeywordsFields,
    Nested,
    NestedFields,
    Packed,
    Point,
    Reserved,
    ReservedFields,
    Scalars,
    ScalarsFields,
    Segment,
    SegmentFields,
    Structs,
    StructsFields,
)
from tests.generated.all_types_tables import (
    arrays_fields,
    describe_arrays,
    describe_documented,
    describe_nested,
    describe_packed,
    describe_point,
    describe_reserved,
    describe_scalars,
    describe_segment,
    describe_structs,
    documented_bytes,
    documented_structs,
    from_last,
    nested_fields,
    packed_bytes,
    packed_structs,
    point_structs,
    read_arrays,
    read_nested,
    read_reserved,
    read_scalars,
    read_structs,
    reserved_fields,
    scalars_fields,
    segment_bytes,
    segment_structs,
    structs_fields,
)
from tests.generated.byte_strategies import (
    Overwrite,
    Slice,
    Truncation,
    corruptions,
    slices,
)
from tests.generated.expectations import (
    assert_same_fields,
    assert_same_structs,
    assert_same_table,
    assert_values_or_decode_error,
    fields_or_decode_error,
    plain,
)
from tests.tables import Description, build_table


class TestRoundTrip(unittest.TestCase):
    @SETTINGS
    @given(scalars_fields, slices)
    def test_reads_back_a_scalar_of_every_type_wherever_its_bytes_lie(
        self, fields: ScalarsFields, slice: Slice
    ) -> None:
        data = slice.of(Scalars.serialize(**fields))

        assert_same_table(
            self,
            describe_scalars(read_scalars(Scalars(data))),
            describe_scalars(fields),
        )

    @SETTINGS
    @given(arrays_fields, slices)
    def test_reads_back_an_array_of_every_type_wherever_its_bytes_lie(
        self, fields: ArraysFields, slice: Slice
    ) -> None:
        data = slice.of(Arrays.serialize(**fields))

        assert_same_table(
            self, describe_arrays(read_arrays(Arrays(data))), describe_arrays(fields)
        )

    @SETTINGS
    @given(nested_fields, slices)
    def test_reads_back_tables_nested_three_deep_and_vectors_of_trees(
        self, fields: NestedFields, slice: Slice
    ) -> None:
        data = slice.of(Nested.serialize(**fields))

        assert_same_table(
            self, describe_nested(read_nested(Nested(data))), describe_nested(fields)
        )

    @SETTINGS
    @given(reserved_fields, slices)
    def test_reads_back_fields_named_after_keywords(
        self, fields: ReservedFields, slice: Slice
    ) -> None:
        data = slice.of(Reserved.serialize(**fields))

        assert_same_table(
            self,
            describe_reserved(read_reserved(Reserved(data))),
            describe_reserved(fields),
        )

    @SETTINGS
    @given(structs_fields, slices)
    def test_reads_back_structs_as_fields_in_arrays_and_within_structs(
        self, fields: StructsFields, slice: Slice
    ) -> None:
        data = slice.of(Structs.serialize(**fields))

        assert_same_table(
            self,
            describe_structs(read_structs(Structs(data))),
            describe_structs(fields),
        )


class TestTablesBuiltByHand(unittest.TestCase):
    @SETTINGS
    @given(scalars_fields)
    def test_writes_the_bytes_of_scalars_that_table_builder_writes(
        self, fields: ScalarsFields
    ) -> None:
        by_hand = bytes(build_table(describe_scalars(fields)).bytes())

        self.assertEqual(bytes(Scalars.serialize(**fields)), by_hand)

    @SETTINGS
    @given(arrays_fields)
    def test_writes_the_bytes_of_aligned_arrays_that_table_builder_writes(
        self, fields: ArraysFields
    ) -> None:
        by_hand = bytes(build_table(describe_arrays(fields)).bytes())

        self.assertEqual(bytes(Arrays.serialize(**fields)), by_hand)

    @SETTINGS
    @given(nested_fields)
    def test_writes_the_bytes_of_nested_tables_and_vectors_that_table_builder_writes(
        self, fields: NestedFields
    ) -> None:
        by_hand = bytes(build_table(describe_nested(fields)).bytes())

        self.assertEqual(bytes(Nested.serialize(**fields)), by_hand)

    @SETTINGS
    @given(reserved_fields)
    def test_writes_the_bytes_of_fields_named_after_keywords_that_table_builder_writes(
        self, fields: ReservedFields
    ) -> None:
        by_hand = bytes(build_table(describe_reserved(fields)).bytes())

        self.assertEqual(bytes(Reserved.serialize(**fields)), by_hand)

    @SETTINGS
    @given(structs_fields)
    def test_writes_the_bytes_of_structs_and_aligned_arrays_that_table_builder_writes(
        self, fields: StructsFields
    ) -> None:
        by_hand = bytes(build_table(describe_structs(fields)).bytes())

        self.assertEqual(bytes(Structs.serialize(**fields)), by_hand)

    @SETTINGS
    @given(scalars_fields, integers(2, 0xFF))
    def test_reads_a_bool_of_any_byte_but_0_as_true(
        self, fields: ScalarsFields, byte: int
    ) -> None:
        """The generated code writes a true bool as 1, but reads any byte other
        than 0 as true"""
        data = bytearray(Scalars.serialize(**fields))
        # The bool is Scalars' field 10, in a table of the default layout
        position = ByteBuffer(data, TableLayout.DEFAULT).root_offset(10)
        data[position] = byte

        self.assertIs(Scalars(data).bool_(), True)


class TestDecodeErrors(unittest.TestCase):
    @SETTINGS
    @given(scalars_fields, corruptions)
    def test_reads_corrupted_scalars_as_values_or_raises_decode_error(
        self, fields: ScalarsFields, corruption: Truncation | Overwrite
    ) -> None:
        data = corruption.apply(bytes(Scalars.serialize(**fields)))

        assert_values_or_decode_error(lambda: read_scalars(Scalars(data)))

    @SETTINGS
    @given(arrays_fields, corruptions)
    def test_reads_corrupted_arrays_as_values_or_raises_decode_error(
        self, fields: ArraysFields, corruption: Truncation | Overwrite
    ) -> None:
        data = corruption.apply(bytes(Arrays.serialize(**fields)))

        assert_values_or_decode_error(lambda: read_arrays(Arrays(data)))

    @SETTINGS
    @given(nested_fields, corruptions)
    # Overwrites the last 4 of the 216 bytes, the count of the last tree's
    # empty vector of children, with 2.5 billion, which list() allocated for
    # when the view didn't check its count against the bytes
    @example(
        fields={
            "label": "000\x80\x80\x80𐀀𐀀",
            "outer": {"count": 0, "inner": {"name": "0\x80\x80𐀀", "values": [0.0] * 8}},
            "trees": [{"value": 0, "children": []}] * 3,
        },
        corruption=Overwrite(312302300, b"\x00\x00\x00\x95"),
    )
    def test_reads_corrupted_nested_tables_and_trees_as_values_or_raises_decode_error(
        self, fields: NestedFields, corruption: Truncation | Overwrite
    ) -> None:
        data = corruption.apply(bytes(Nested.serialize(**fields)))

        assert_values_or_decode_error(lambda: read_nested(Nested(data)))

    @SETTINGS
    @given(reserved_fields, corruptions)
    def test_reads_corrupted_keywords_as_values_or_raises_decode_error(
        self, fields: ReservedFields, corruption: Truncation | Overwrite
    ) -> None:
        data = corruption.apply(bytes(Reserved.serialize(**fields)))

        assert_values_or_decode_error(lambda: read_reserved(Reserved(data)))

    @SETTINGS
    @given(structs_fields, corruptions)
    def test_reads_corrupted_structs_as_values_or_raises_decode_error(
        self, fields: StructsFields, corruption: Truncation | Overwrite
    ) -> None:
        data = corruption.apply(bytes(Structs.serialize(**fields)))

        assert_values_or_decode_error(lambda: read_structs(Structs(data)))

    @SETTINGS
    @given(binary(max_size=64))
    def test_reads_arbitrary_bytes_as_any_message_or_raises_decode_error(
        self, data: bytes
    ) -> None:
        assert_values_or_decode_error(lambda: read_scalars(Scalars(data)))
        assert_values_or_decode_error(lambda: read_arrays(Arrays(data)))
        assert_values_or_decode_error(lambda: read_nested(Nested(data)))
        assert_values_or_decode_error(lambda: read_reserved(Reserved(data)))
        assert_values_or_decode_error(lambda: read_structs(Structs(data)))


class TestEveryField(unittest.TestCase):
    # fields() and field_values() read nested tables in the same call, and
    # return what the getters read or raise DecodeError as they do. A nested
    # table of fixed size has its values in place of it, and vectors and
    # arrays of structs are their views in field_values().

    @SETTINGS
    @given(nested_fields, one_of(corruptions, binary(max_size=0)))
    def test_reads_nested_tables_values_as_their_getters_do(
        self, fields: NestedFields, corruption: Truncation | Overwrite | bytes
    ) -> None:
        nested = Nested(_corrupted(Nested.serialize(**fields), corruption))

        _assert_same(self, nested.field_values, partial(_nested_values, nested))
        _assert_same(
            self, lambda: nested.outer().fields(), partial(_outer_fields, nested)
        )

    @SETTINGS
    @given(nested_fields, one_of(corruptions, binary(max_size=0)))
    def test_iterates_over_the_values_of_trees_as_each_tree_reads_them(
        self, fields: NestedFields, corruption: Truncation | Overwrite | bytes
    ) -> None:
        nested = Nested(_corrupted(Nested.serialize(**fields), corruption))

        _assert_same(
            self,
            lambda: list(nested.trees().iter_field_values()),
            lambda: [tree.field_values() for tree in nested.trees()],
        )

    @SETTINGS
    @given(structs_fields, one_of(corruptions, binary(max_size=0)))
    def test_reads_structs_fields_and_values_as_their_getters_do(
        self, fields: StructsFields, corruption: Truncation | Overwrite | bytes
    ) -> None:
        structs = Structs(_corrupted(Structs.serialize(**fields), corruption))

        _assert_same(self, structs.fields, partial(read_structs, structs))
        _assert_same(self, structs.field_values, partial(_structs_values, structs))

    @SETTINGS
    @given(reserved_fields, one_of(corruptions, binary(max_size=0)))
    def test_reads_a_nested_table_of_fixed_size_in_place_as_its_getters_do(
        self, fields: ReservedFields, corruption: Truncation | Overwrite | bytes
    ) -> None:
        reserved = Reserved(_corrupted(Reserved.serialize(**fields), corruption))

        _assert_same(self, reserved.fields, partial(read_reserved, reserved))
        _assert_same(self, reserved.field_values, partial(_reserved_values, reserved))

    @SETTINGS
    @given(scalars_fields, arrays_fields, one_of(corruptions, binary(max_size=0)))
    def test_reads_scalars_and_arrays_values_as_their_getters_do(
        self,
        scalars_fields: ScalarsFields,
        arrays_fields: ArraysFields,
        corruption: Truncation | Overwrite | bytes,
    ) -> None:
        scalars = Scalars(_corrupted(Scalars.serialize(**scalars_fields), corruption))
        arrays = Arrays(_corrupted(Arrays.serialize(**arrays_fields), corruption))

        _assert_same(
            self, scalars.field_values, lambda: tuple(read_scalars(scalars).values())
        )
        _assert_same(self, arrays.fields, partial(read_arrays, arrays))
        _assert_same(
            self, arrays.field_values, lambda: tuple(read_arrays(arrays).values())
        )

    @SETTINGS
    @given(binary(max_size=64))
    def test_reads_arbitrary_bytes_values_as_the_getters_do(self, data: bytes) -> None:
        nested, structs = Nested(data), Structs(data)

        _assert_same(self, nested.field_values, partial(_nested_values, nested))
        _assert_same(self, structs.fields, partial(read_structs, structs))
        _assert_same(self, structs.field_values, partial(_structs_values, structs))


def _corrupted(data: Buffer, corruption: Truncation | Overwrite | bytes) -> bytes:
    """The bytes of a table, corrupted unless ``corruption`` is bytes"""
    if isinstance(corruption, bytes):
        return bytes(data)
    return corruption.apply(bytes(data))


def _assert_same(
    test: unittest.TestCase, read: Callable[[], object], expected: Callable[[], object]
) -> None:
    """Asserts that ``read`` returns what ``expected`` reads through the getters,
    or raises the same DecodeError"""
    test.assertEqual(
        fields_or_decode_error(lambda: plain(read())),
        fields_or_decode_error(lambda: plain(expected())),
    )


def _outer_fields(nested: Nested) -> object:
    outer = nested.outer()
    count, inner = outer.count(), outer.inner()
    return {"count": count, "inner": {"name": inner.name(), "values": inner.values()}}


# The values of each table, read in the fields' order, as the getters read them


def _nested_values(nested: Nested) -> tuple[object, ...]:
    label, outer = nested.label(), nested.outer()
    count, inner = outer.count(), outer.inner()
    values = (count, (inner.name(), inner.values()))
    return (label, values, nested.trees())


def _structs_values(structs: Structs) -> tuple[object, ...]:
    packed, documented = structs.packed(), structs.documented()
    segment, segments, path = structs.segment(), structs.segments(), structs.path()
    return (packed, documented, segment, segments, (path.start(), path.points()))


def _reserved_values(reserved: Reserved) -> tuple[object, ...]:
    return tuple(read_reserved(reserved)["keywords"].values())


class TestViews(unittest.TestCase):
    # The getters of vectors and arrays of structs return views, which the
    # tests above iterate. These read their items by index instead, from the
    # last, as a reader that skips items would.

    @SETTINGS
    @given(nested_fields, slices)
    def test_reads_back_vectors_of_trees_by_index(
        self, fields: NestedFields, slice: Slice
    ) -> None:
        data = slice.of(Nested.serialize(**fields))

        assert_same_table(
            self,
            describe_nested(read_nested(Nested(data), from_last)),
            describe_nested(fields),
        )

    @SETTINGS
    @given(structs_fields, slices)
    def test_reads_back_arrays_of_structs_by_index(
        self, fields: StructsFields, slice: Slice
    ) -> None:
        data = slice.of(Structs.serialize(**fields))

        assert_same_table(
            self,
            describe_structs(read_structs(Structs(data), from_last)),
            describe_structs(fields),
        )

    @SETTINGS
    @given(nested_fields, corruptions)
    def test_reads_corrupted_trees_by_index_as_values_or_raises_decode_error(
        self, fields: NestedFields, corruption: Truncation | Overwrite
    ) -> None:
        data = corruption.apply(bytes(Nested.serialize(**fields)))

        assert_values_or_decode_error(lambda: read_nested(Nested(data), from_last))

    @SETTINGS
    @given(structs_fields, corruptions)
    def test_reads_corrupted_structs_by_index_as_values_or_raises_decode_error(
        self, fields: StructsFields, corruption: Truncation | Overwrite
    ) -> None:
        data = corruption.apply(bytes(Structs.serialize(**fields)))

        assert_values_or_decode_error(lambda: read_structs(Structs(data), from_last))

    @SETTINGS
    @given(binary(max_size=64))
    def test_reads_arbitrary_bytes_by_index_or_raises_decode_error(
        self, data: bytes
    ) -> None:
        assert_values_or_decode_error(lambda: read_nested(Nested(data), from_last))
        assert_values_or_decode_error(lambda: read_structs(Structs(data), from_last))

    # Formatting an index of more digits than Python converts to a string,
    # 4,300 by default, once raised ValueError
    @SETTINGS
    @given(
        nested_fields,
        structs_fields,
        one_of(integers(min_value=0), integers(2**15000, 2**15000 + 16)),
    )
    def test_raises_index_error_for_an_item_outside_a_view(
        self, nested_fields: NestedFields, structs_fields: StructsFields, past: int
    ) -> None:
        nested = Nested(Nested.serialize(**nested_fields))
        structs = Structs(Structs.serialize(**structs_fields))

        trees, documented = nested.trees(), structs.documented()
        reads: list[tuple[int, Callable[[int], object]]] = [
            (len(trees), trees.__getitem__),
            (len(documented), documented.__getitem__),
            (len(documented), documented.item_field_values),
        ]
        for length, read in reads:
            for index in [length + past, -length - 1 - past]:
                with self.assertRaises(IndexError):
                    read(index)

    def test_raises_decode_error_for_a_vector_whose_count_the_bytes_cant_hold(
        self,
    ) -> None:
        """A view checks its count against the bytes before ``len()`` gives it
        to a caller that allocates for it"""
        data = bytearray(
            Nested.serialize(
                label="",
                outer={"count": 0, "inner": {"name": "", "values": []}},
                trees=[{"value": 0, "children": []}],
            )
        )
        # The tree's empty vector of children, its count alone, ends the bytes
        data[-4:] = (2**31).to_bytes(4, "little")
        tree = Nested(data).trees()[0]

        with self.assertRaises(DecodeError):
            tree.children()

    def test_views_empty_vectors_and_arrays_of_structs(self) -> None:
        point = Point(0.0, 0.0)
        nested = Nested(
            Nested.serialize(
                label="",
                outer={"count": 0, "inner": {"name": "", "values": []}},
                trees=[],
            )
        )
        structs = Structs(
            Structs.serialize(
                packed=Packed(0, 0, 0, 0, 0, 0, 0, 0, 0.0, 0.0, False),
                documented=[],
                segment=Segment(point, point),
                segments=[],
                path={"start": point, "points": []},
            )
        )

        views = [
            nested.trees(),
            structs.documented(),
            structs.segments(),
            structs.path().points(),
        ]
        for view in views:
            self.assertEqual(len(view), 0)
            self.assertEqual(list(view), [])
            with self.assertRaises(IndexError):
                view[0]


class TestStructs(unittest.TestCase):
    def test_gives_the_size_and_alignment_of_each_struct(self) -> None:
        """An array of structs is aligned to the size of its structs' largest
        scalar"""
        cases: list[tuple[str, int, int, int, int]] = [
            ("Packed", Packed.SIZE, Packed.ALIGNMENT, 43, 8),
            ("Documented", Documented.SIZE, Documented.ALIGNMENT, 7, 4),
            ("Segment", Segment.SIZE, Segment.ALIGNMENT, 16, 4),
            ("Point", Point.SIZE, Point.ALIGNMENT, 8, 4),
        ]

        for name, size, alignment, expected_size, expected_alignment in cases:
            with self.subTest(name):
                self.assertEqual((size, alignment), (expected_size, expected_alignment))

    @SETTINGS
    @given(packed_structs, segment_structs)
    def test_constructs_structs_by_keyword_as_frozen_dataclasses_of_their_fields(
        self, packed: Packed, segment: Segment
    ) -> None:
        """A struct takes its fields by keyword as by position, hashes and
        matches by them, copies with ``replace()``, and is frozen"""
        with self.subTest("Packed"):
            self.assert_frozen_dataclass_of_its_fields(packed, describe_packed)
        with self.subTest("Segment"):
            self.assert_frozen_dataclass_of_its_fields(segment, describe_segment)

    def assert_frozen_dataclass_of_its_fields[Struct: (Packed, Segment)](
        self, struct: Struct, describe: Callable[[Struct], list[Description]]
    ) -> None:
        """Compares the structs by their fields' bits rather than with ``==``,
        which a dataclass applies to each field, so that NaN differs from
        itself"""
        values = {field.name: getattr(struct, field.name) for field in fields(struct)}
        rebuilt = type(struct)(**values)

        assert_same_structs(
            self, [rebuilt, replace(struct)], [struct, struct], describe
        )
        self.assertEqual(hash(rebuilt), hash(struct))
        self.assertEqual(type(struct).__match_args__, tuple(values))
        with self.assertRaises(FrozenInstanceError):
            setattr(struct, next(iter(values)), None)

    @SETTINGS
    @given(segment_structs)
    def test_writes_a_segment_as_the_scalars_of_its_points_one_after_another(
        self, segment: Segment
    ) -> None:
        points = segment.start.serialize() + segment.end.serialize()

        self.assertEqual(segment.serialize(), segment_bytes(segment))
        self.assertEqual(segment.serialize(), points)

    @SETTINGS
    @given(packed_structs)
    def test_writes_a_packed_little_endian_at_fixed_offsets_in_43_bytes(
        self, packed: Packed
    ) -> None:
        self.assertEqual(Packed.SIZE, 43)
        self.assertEqual(packed.serialize(), packed_bytes(packed))

    @SETTINGS
    @given(packed_structs, integers(2, 0xFF))
    def test_reads_a_packed_bool_of_any_byte_but_0_as_true(
        self, packed: Packed, byte: int
    ) -> None:
        """A ``Packed`` writes a true bool as 1, but reads any byte other than
        0 as true"""
        data = bytearray(packed.serialize())
        # The bool is a Packed's last byte, at offset 42
        data[Packed.SIZE - 1] = byte

        self.assertIs(Packed.read(data).bool_, True)
        self.assertIs(Packed.read_array(data)[0].bool_, True)

    @SETTINGS
    @given(documented_structs)
    def test_writes_a_documented_little_endian_at_fixed_offsets_in_7_bytes(
        self, documented: Documented
    ) -> None:
        self.assertEqual(Documented.SIZE, 7)
        self.assertEqual(documented.serialize(), documented_bytes(documented))

    @SETTINGS
    @given(packed_structs, binary(max_size=8), binary(max_size=8))
    def test_reads_a_packed_back_at_an_offset_in_longer_data(
        self, packed: Packed, before: bytes, after: bytes
    ) -> None:
        data = before + packed.serialize() + after

        read = Packed.read(data, len(before))
        assert_same_fields(self, describe_packed(read), describe_packed(packed))

    @SETTINGS
    @given(documented_structs, binary(max_size=8), binary(max_size=8))
    def test_reads_a_documented_back_at_an_offset_in_longer_data(
        self, documented: Documented, before: bytes, after: bytes
    ) -> None:
        data = before + documented.serialize() + after

        self.assertEqual(Documented.read(data, len(before)), documented)

    @SETTINGS
    @given(binary(max_size=64), integers(1, Packed.SIZE))
    def test_raises_decode_error_when_the_data_ends_before_a_packed(
        self, data: bytes, missing: int
    ) -> None:
        # Where the struct would end ``missing`` bytes after the data, or at
        # the start of data shorter than a struct
        offset = max(len(data) + missing - Packed.SIZE, 0)

        with self.assertRaises(DecodeError):
            Packed.read(data, offset)

    @SETTINGS
    @given(binary(max_size=16), integers(1, Documented.SIZE))
    def test_raises_decode_error_when_the_data_ends_before_a_documented(
        self, data: bytes, missing: int
    ) -> None:
        offset = max(len(data) + missing - Documented.SIZE, 0)

        with self.assertRaises(DecodeError):
            Documented.read(data, offset)

    # serialize_array() writes each struct's serialize() one after another,
    # and read_array() reads them back, but not a part of one more

    @SETTINGS
    @given(structs_fields)
    def test_writes_arrays_of_structs_from_any_objects_of_their_fields(
        self, fields: StructsFields
    ) -> None:
        """An array of structs reads its items' attributes, which the struct's
        protocol of fields types, so objects of the caller's own write the
        bytes that the structs write"""
        documented = [
            _DocumentedTuple(item.one, item.two, item.undocumented)
            for item in fields["documented"]
        ]
        segments = [_segment_tuple(item) for item in fields["segments"]]

        self.assertEqual(
            bytes(
                Structs.serialize(
                    packed=fields["packed"],
                    documented=documented,
                    segment=fields["segment"],
                    segments=segments,
                    path=fields["path"],
                )
            ),
            bytes(Structs.serialize(**fields)),
        )
        self.assertEqual(
            Segment.serialize_array(segments),
            Segment.serialize_array(fields["segments"]),
        )

    @SETTINGS
    @given(
        lists(packed_structs, max_size=8), binary(min_size=1, max_size=Packed.SIZE - 1)
    )
    def test_reads_back_every_packed_that_serialize_array_writes(
        self, items: list[Packed], partial: bytes
    ) -> None:
        data = Packed.serialize_array(items)

        self.assertEqual(data, b"".join(item.serialize() for item in items))
        assert_same_structs(self, Packed.read_array(data), items, describe_packed)
        with self.assertRaises(DecodeError):
            Packed.read_array(data + partial)

    @SETTINGS
    @given(
        lists(documented_structs, max_size=8),
        binary(min_size=1, max_size=Documented.SIZE - 1),
    )
    def test_reads_back_every_documented_that_serialize_array_writes(
        self, items: list[Documented], partial: bytes
    ) -> None:
        data = Documented.serialize_array(items)

        self.assertEqual(data, b"".join(item.serialize() for item in items))
        assert_same_structs(
            self, Documented.read_array(data), items, describe_documented
        )
        with self.assertRaises(DecodeError):
            Documented.read_array(data + partial)

    @SETTINGS
    @given(
        lists(segment_structs, max_size=8),
        binary(min_size=1, max_size=Segment.SIZE - 1),
    )
    def test_reads_back_every_segment_that_serialize_array_writes(
        self, items: list[Segment], partial: bytes
    ) -> None:
        data = Segment.serialize_array(items)

        self.assertEqual(data, b"".join(item.serialize() for item in items))
        assert_same_structs(self, Segment.read_array(data), items, describe_segment)
        with self.assertRaises(DecodeError):
            Segment.read_array(data + partial)

    @SETTINGS
    @given(
        lists(point_structs, max_size=8), binary(min_size=1, max_size=Point.SIZE - 1)
    )
    def test_reads_back_every_point_that_serialize_array_writes(
        self, items: list[Point], partial: bytes
    ) -> None:
        data = Point.serialize_array(items)

        self.assertEqual(data, b"".join(item.serialize() for item in items))
        assert_same_structs(self, Point.read_array(data), items, describe_point)
        with self.assertRaises(DecodeError):
            Point.read_array(data + partial)


class TestFields(unittest.TestCase):
    # fields() reads a fixed-layout table's fields at once, or through the
    # getters for bytes that its writer didn't write

    @SETTINGS
    @given(scalars_fields, reserved_fields)
    def test_reads_scalars_and_keywords_fields_as_their_getters_and_writes_them_back(
        self, scalars: ScalarsFields, reserved: ReservedFields
    ) -> None:
        scalars_data = Scalars.serialize(**scalars)
        written = Scalars(scalars_data)
        keywords = Reserved(Reserved.serialize(**reserved)).keywords()

        self.assertEqual(
            fields_or_decode_error(written.fields),
            fields_or_decode_error(partial(read_scalars, written)),
        )
        self.assertEqual(
            bytes(Scalars.serialize(**written.fields())), bytes(scalars_data)
        )
        self.assertEqual(
            fields_or_decode_error(keywords.fields),
            fields_or_decode_error(lambda: read_reserved_keywords(keywords)),
        )

    @SETTINGS
    @given(scalars_fields, reserved_fields, corruptions)
    def test_reads_corrupted_scalars_and_keywords_fields_as_their_getters_do(
        self,
        scalars: ScalarsFields,
        reserved: ReservedFields,
        corruption: Truncation | Overwrite,
    ) -> None:
        written = Scalars(corruption.apply(bytes(Scalars.serialize(**scalars))))
        corrupted = Reserved(corruption.apply(bytes(Reserved.serialize(**reserved))))

        self.assertEqual(
            fields_or_decode_error(written.fields),
            fields_or_decode_error(partial(read_scalars, written)),
        )
        self.assertEqual(
            fields_or_decode_error(lambda: corrupted.keywords().fields()),
            fields_or_decode_error(lambda: read_reserved(corrupted)["keywords"]),
        )


class TestValues(unittest.TestCase):
    # field_values() reads the fields of a table of scalars and a string into a
    # tuple, as fields() reads them into a dict, and serialize_values() writes
    # a message from them. Structs of scalars read and write their values as
    # tuples, as their dataclasses hold them, and their arrays' views read
    # every struct's.

    @SETTINGS
    @given(scalars_fields, reserved_fields, one_of(corruptions, binary(max_size=0)))
    def test_reads_scalars_and_keywords_values_as_their_getters_and_writes_them_back(
        self,
        scalars: ScalarsFields,
        reserved: ReservedFields,
        corruption: Truncation | Overwrite | bytes,
    ) -> None:
        scalars_data = bytes(Scalars.serialize(**scalars))
        reserved_data = bytes(Reserved.serialize(**reserved))
        if not isinstance(corruption, bytes):
            scalars_data = corruption.apply(scalars_data)
            reserved_data = corruption.apply(reserved_data)
        written = Scalars(scalars_data)
        keywords = Reserved(reserved_data)

        self.assertEqual(
            fields_or_decode_error(written.field_values),
            fields_or_decode_error(lambda: tuple(read_scalars(written).values())),
        )
        self.assertEqual(
            fields_or_decode_error(lambda: keywords.keywords().field_values()),
            fields_or_decode_error(
                lambda: tuple(read_reserved(keywords)["keywords"].values())
            ),
        )
        if isinstance(corruption, bytes):
            values = written.field_values()
            self.assertEqual(bytes(Scalars.serialize_values(*values)), scalars_data)

    @SETTINGS
    @given(packed_structs, documented_structs, point_structs, binary(max_size=8))
    def test_reads_and_writes_each_structs_values_as_its_dataclass_holds_them(
        self, packed: Packed, documented: Documented, point: Point, before: bytes
    ) -> None:
        for struct in [packed, documented, point]:
            _assert_struct_values(self, struct, before)

    @SETTINGS
    @given(binary(max_size=60), integers(-60, 60))
    def test_reads_each_structs_values_or_raises_as_read_does(
        self, data: bytes, offset: int
    ) -> None:
        for struct in [Packed, Documented, Point]:
            self.assertEqual(
                fields_or_decode_error(partial(struct.read_values, data, offset)),
                fields_or_decode_error(
                    lambda struct=struct: astuple(struct.read(data, offset))
                ),
                struct.__name__,
            )
            self.assertEqual(
                fields_or_decode_error(partial(struct.read_array_values, data)),
                fields_or_decode_error(
                    lambda struct=struct: _astuples(struct.read_array(data))
                ),
                struct.__name__,
            )

    @SETTINGS
    @given(structs_fields, one_of(corruptions, binary(max_size=0)))
    def test_reads_every_structs_values_through_a_view_as_its_structs_hold_them(
        self, fields: StructsFields, corruption: Truncation | Overwrite | bytes
    ) -> None:
        data = bytes(Structs.serialize(**fields))
        if not isinstance(corruption, bytes):
            data = corruption.apply(data)
        structs = Structs(data)

        self.assertEqual(
            fields_or_decode_error(lambda: structs.documented().field_values()),
            fields_or_decode_error(lambda: _astuples(structs.documented())),
        )
        self.assertEqual(
            fields_or_decode_error(lambda: structs.path().points().field_values()),
            fields_or_decode_error(lambda: _astuples(structs.path().points())),
        )
        self.assertEqual(
            fields_or_decode_error(
                lambda: list(structs.documented().iter_field_values())
            ),
            fields_or_decode_error(lambda: _astuples(structs.documented())),
        )
        self.assertEqual(
            fields_or_decode_error(
                lambda: list(structs.path().points().iter_field_values())
            ),
            fields_or_decode_error(lambda: _astuples(structs.path().points())),
        )

    @SETTINGS
    @given(structs_fields, one_of(corruptions, binary(max_size=0)))
    def test_reads_each_structs_values_by_index_as_the_struct_holds_them(
        self, fields: StructsFields, corruption: Truncation | Overwrite | bytes
    ) -> None:
        data = bytes(Structs.serialize(**fields))
        if not isinstance(corruption, bytes):
            data = corruption.apply(data)
        try:
            views = [Structs(data).documented(), Structs(data).path().points()]
        except DecodeError:
            return

        for view in views:
            every = _astuples(view)
            # Every index, from the end too, and one past each end
            for index in range(-len(view) - 1, len(view) + 1):
                with self.subTest(view=type(view).__name__, index=index):
                    if -len(view) <= index < len(view):
                        # By their repr, which tells NaN and -0 apart
                        expected = repr(every[index])
                        self.assertEqual(repr(view.item_field_values(index)), expected)
                        self.assertEqual(repr(astuple(view[index])), expected)
                        continue

                    with self.assertRaises(IndexError):
                        view.item_field_values(index)
                    with self.assertRaises(IndexError):
                        view[index]


def _assert_struct_values(test: unittest.TestCase, struct: Any, before: bytes) -> None:
    """Asserts that a struct of scalars reads and writes the values that its
    dataclass holds, alone and in an array"""
    values = astuple(struct)
    data = before + struct.serialize()
    array = type(struct).serialize_array([struct, struct])

    test.assertEqual(
        fields_or_decode_error(partial(type(struct).read_values, data, len(before))),
        fields_or_decode_error(lambda: values),
    )
    test.assertEqual(type(struct).serialize_values(*values), struct.serialize())
    test.assertEqual(
        fields_or_decode_error(partial(type(struct).read_array_values, array)),
        fields_or_decode_error(lambda: [values, values]),
    )


def _astuples(structs: Iterable[Any]) -> list[tuple[Any, ...]]:
    return [astuple(struct) for struct in structs]


class _PointTuple(NamedTuple):
    x: float
    y: float


class _SegmentTuple(NamedTuple):
    start: _PointTuple
    end: _PointTuple


class _DocumentedTuple(NamedTuple):
    one: int
    two: int
    undocumented: int


def read_reserved_keywords(keywords: Keywords) -> KeywordsFields:
    """A table of keywords' fields, as read_reserved() reads them"""
    return {
        "class_": keywords.class_(),
        "default": keywords.default(),
        "in_": keywords.in_(),
        "match_": keywords.match_(),
        "self": keywords.self(),
        "type_": keywords.type_(),
    }


def _segment_tuple(segment: SegmentFields) -> _SegmentTuple:
    return _SegmentTuple(
        _PointTuple(segment.start.x, segment.start.y),
        _PointTuple(segment.end.x, segment.end.y),
    )


if __name__ == "__main__":
    unittest.main()
