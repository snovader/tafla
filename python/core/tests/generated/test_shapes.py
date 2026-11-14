"""The code generated from ``schemas/shapes.tafla`` writes and reads tables
with tafla."""

import unittest
from collections.abc import Callable
from functools import partial

from hypothesis import given
from hypothesis.strategies import binary, integers, one_of

from tafla import ByteBuffer, DecodeError
from tests.generated import SETTINGS
from tests.generated.byte_strategies import (
    Overwrite,
    Slice,
    Truncation,
    corruptions,
    slices,
)
from tests.generated.expectations import (
    assert_same_fields,
    assert_same_table,
    assert_values_or_decode_error,
    fields_or_decode_error,
    plain,
)
from tests.generated.shapes.shapes import (
    Document,
    DocumentFields,
    Item,
    ItemFields,
    ItemList,
    ItemListFields,
    ItemVector,
    Point,
    PointFields,
    Sample,
)
from tests.generated.shapes_tables import (
    DOCUMENT_VERSION,
    describe_document,
    describe_item_list,
    describe_point,
    describe_sample,
    document_fields,
    item_list_fields,
    point_fields,
    read_document,
    read_item,
    read_item_list,
    read_point,
    sample_bytes,
    samples,
)
from tests.tables import build_table

_VERSIONS = integers(0, 0xFFFF_FFFF)
_OTHER_VERSIONS = _VERSIONS.filter(lambda version: version != DOCUMENT_VERSION)

# Integers of more digits than Python converts to a string by default, 4,300
_BEYOND_THE_DIGIT_LIMIT = integers(2**15000, 2**15000 + 16)


class TestRoundTrip(unittest.TestCase):
    @SETTINGS
    @given(point_fields, slices)
    def test_reads_back_the_fields_of_a_point_wherever_its_bytes_lie(
        self, fields: PointFields, slice: Slice
    ) -> None:
        data = slice.of(Point.serialize(**fields))

        assert_same_table(
            self, describe_point(read_point(Point(data))), describe_point(fields)
        )

    @SETTINGS
    @given(document_fields, slices)
    def test_reads_back_a_document_and_its_nested_columns_wherever_its_bytes_lie(
        self, fields: DocumentFields, slice: Slice
    ) -> None:
        data = slice.of(Document.serialize(**fields))

        assert_same_table(
            self,
            describe_document(read_document(Document(data))),
            describe_document(fields),
        )

    @SETTINGS
    @given(item_list_fields, slices)
    def test_reads_back_the_items_of_an_item_list_and_their_pairs_wherever_its_bytes_lie(
        self, fields: ItemListFields, slice: Slice
    ) -> None:
        data = slice.of(ItemList.serialize(**fields))

        assert_same_table(
            self,
            describe_item_list(read_item_list(ItemList(data))),
            describe_item_list(fields),
        )


class TestTablesBuiltByHand(unittest.TestCase):
    @SETTINGS
    @given(point_fields)
    def test_writes_the_bytes_of_a_point_that_table_builder_writes(
        self, fields: PointFields
    ) -> None:
        by_hand = bytes(build_table(describe_point(fields)).bytes())

        self.assertEqual(bytes(Point.serialize(**fields)), by_hand)

    @SETTINGS
    @given(document_fields)
    def test_writes_the_bytes_of_a_sized_versioned_document_that_table_builder_writes(
        self, fields: DocumentFields
    ) -> None:
        by_hand = bytes(build_table(describe_document(fields)).bytes())

        self.assertEqual(bytes(Document.serialize(**fields)), by_hand)

    @SETTINGS
    @given(item_list_fields)
    def test_writes_the_bytes_of_an_item_list_that_table_builder_writes(
        self, fields: ItemListFields
    ) -> None:
        by_hand = bytes(build_table(describe_item_list(fields)).bytes())

        self.assertEqual(bytes(ItemList.serialize(**fields)), by_hand)


class TestVersions(unittest.TestCase):
    @SETTINGS
    @given(document_fields, _VERSIONS)
    def test_reads_a_documents_version_from_its_bytes_of_any_version(
        self, fields: DocumentFields, version: int
    ) -> None:
        """An application reads the version to choose the reader of it"""
        data = build_table(describe_document(fields, version)).bytes()

        self.assertEqual(Document.version_of(data), version)

    @SETTINGS
    @given(document_fields, integers(0, 7))
    def test_raises_decode_error_for_the_version_of_data_without_a_header(
        self, fields: DocumentFields, length: int
    ) -> None:
        data = Document.serialize(**fields)[:length]

        with self.assertRaises(DecodeError):
            Document.version_of(data)


class TestDecodeErrors(unittest.TestCase):
    @SETTINGS
    @given(document_fields, _OTHER_VERSIONS)
    def test_rejects_a_document_of_another_version(
        self, fields: DocumentFields, version: int
    ) -> None:
        data = build_table(describe_document(fields, version)).bytes()

        with self.assertRaises(DecodeError):
            Document(data)

    @SETTINGS
    @given(point_fields, corruptions)
    def test_reads_a_corrupted_point_as_values_or_raises_decode_error(
        self, fields: PointFields, corruption: Truncation | Overwrite
    ) -> None:
        data = corruption.apply(bytes(Point.serialize(**fields)))

        assert_values_or_decode_error(lambda: read_point(Point(data)))

    @SETTINGS
    @given(document_fields, corruptions)
    def test_reads_a_corrupted_document_as_values_or_raises_decode_error(
        self, fields: DocumentFields, corruption: Truncation | Overwrite
    ) -> None:
        data = corruption.apply(bytes(Document.serialize(**fields)))

        assert_values_or_decode_error(lambda: read_document(Document(data)))

    @SETTINGS
    @given(item_list_fields, corruptions)
    def test_reads_a_corrupted_item_list_as_values_or_raises_decode_error(
        self, fields: ItemListFields, corruption: Truncation | Overwrite
    ) -> None:
        data = corruption.apply(bytes(ItemList.serialize(**fields)))

        assert_values_or_decode_error(lambda: read_item_list(ItemList(data)))

    @SETTINGS
    @given(item_list_fields, corruptions)
    def test_reads_each_item_of_a_corrupted_item_list_by_index_as_its_offset_finds_it(
        self, fields: ItemListFields, corruption: Truncation | Overwrite
    ) -> None:
        """The view reads each item from the count that it checked once, which
        reads the same values, DecodeError or IndexError as reading the count
        for each item"""
        data = corruption.apply(bytes(ItemList.serialize(**fields)))

        self.assertEqual(_items_by_index(data), _items_by_offset(data))

    @SETTINGS
    @given(binary(max_size=64))
    def test_reads_each_item_of_arbitrary_bytes_by_index_as_its_offset_finds_it(
        self, data: bytes
    ) -> None:
        self.assertEqual(_items_by_index(data), _items_by_offset(data))

    @SETTINGS
    @given(binary(max_size=64))
    def test_reads_arbitrary_bytes_as_any_message_or_raises_decode_error(
        self, data: bytes
    ) -> None:
        assert_values_or_decode_error(lambda: read_point(Point(data)))
        assert_values_or_decode_error(lambda: read_document(Document(data)))
        assert_values_or_decode_error(lambda: read_item_list(ItemList(data)))

    def test_rejects_data_that_isnt_c_contiguous_with_type_error(self) -> None:
        """The readers read views of the data, which a copy of it wouldn't be"""
        strided = memoryview(bytes(64))[::2]

        for message in [Point, Document, ItemList]:
            with self.subTest(message=message.__name__), self.assertRaises(TypeError):
                message(strided)


class TestFields(unittest.TestCase):
    # fields() reads a fixed-layout table's fields at once, or through the
    # getters for bytes that its writer didn't write, so it returns what the
    # getters read or raises DecodeError as they do

    @SETTINGS
    @given(point_fields, slices)
    def test_reads_a_points_fields_as_its_getters_read_them_and_writes_them_back(
        self, fields: PointFields, where: Slice
    ) -> None:
        data = Point.serialize(**fields)
        point = Point(where.of(data))

        self.assertEqual(
            fields_or_decode_error(point.fields),
            fields_or_decode_error(partial(read_point, point)),
        )
        self.assertEqual(bytes(Point.serialize(**point.fields())), bytes(data))

    @SETTINGS
    @given(item_list_fields)
    def test_reads_each_items_fields_as_its_getters_read_them(
        self, fields: ItemListFields
    ) -> None:
        for item in ItemList(ItemList.serialize(**fields)).items():
            self.assertEqual(
                fields_or_decode_error(item.fields),
                fields_or_decode_error(partial(read_item, item)),
            )

    @SETTINGS
    @given(point_fields, corruptions)
    def test_reads_a_corrupted_points_fields_as_its_getters_do(
        self, fields: PointFields, corruption: Truncation | Overwrite
    ) -> None:
        point = Point(corruption.apply(bytes(Point.serialize(**fields))))

        self.assertEqual(
            fields_or_decode_error(point.fields),
            fields_or_decode_error(partial(read_point, point)),
        )

    @SETTINGS
    @given(item_list_fields, corruptions)
    def test_reads_the_fields_of_each_item_of_a_corrupted_item_list_as_its_getters_do(
        self, fields: ItemListFields, corruption: Truncation | Overwrite
    ) -> None:
        data = corruption.apply(bytes(ItemList.serialize(**fields)))
        try:
            view = ItemList(data).items()
        except DecodeError:
            return

        for index in range(len(view)):
            with self.subTest(index=index):
                self.assertEqual(
                    fields_or_decode_error(partial(_item_fields, view, index)),
                    fields_or_decode_error(partial(_read_item_at, view, index)),
                )

    @SETTINGS
    @given(binary(max_size=64))
    def test_reads_the_fields_of_arbitrary_bytes_as_the_getters_do(
        self, data: bytes
    ) -> None:
        point = Point(data)

        self.assertEqual(
            fields_or_decode_error(point.fields),
            fields_or_decode_error(partial(read_point, point)),
        )


class TestEveryField(unittest.TestCase):
    # A document's fields() and field_values() read its leading fields of fixed
    # size in one call, and the rest, its nested columns included, as the
    # getters do, so they return what the getters read or raise DecodeError as
    # they do

    @SETTINGS
    @given(document_fields, slices)
    def test_reads_a_documents_fields_and_values_as_its_getters_and_writes_them_back(
        self, fields: DocumentFields, where: Slice
    ) -> None:
        data = Document.serialize(**fields)
        document = Document(where.of(data))

        _assert_document_as_its_getters(self, document)
        written = Document.serialize(**document.fields())
        self.assertEqual(bytes(written), bytes(data))

    @SETTINGS
    @given(document_fields, corruptions)
    def test_reads_a_corrupted_documents_fields_and_values_as_its_getters_do(
        self, fields: DocumentFields, corruption: Truncation | Overwrite
    ) -> None:
        data = corruption.apply(bytes(Document.serialize(**fields)))
        try:
            document = Document(data)
        except DecodeError:
            return

        _assert_document_as_its_getters(self, document)

    @SETTINGS
    @given(binary(max_size=64))
    def test_reads_the_fields_and_values_of_arbitrary_bytes_as_the_getters_do(
        self, data: bytes
    ) -> None:
        try:
            document = Document(data)
        except DecodeError:
            return

        _assert_document_as_its_getters(self, document)


def _assert_document_as_its_getters(
    test: unittest.TestCase, document: Document
) -> None:
    test.assertEqual(
        fields_or_decode_error(lambda: plain(document.fields())),
        fields_or_decode_error(lambda: plain(read_document(document))),
    )
    test.assertEqual(
        fields_or_decode_error(lambda: plain(document.field_values())),
        fields_or_decode_error(lambda: plain(_document_values(document))),
    )


def _document_values(document: Document) -> tuple[object, ...]:
    """A document's values, read in the fields' order, as the getters read them"""
    x, y, name, columns = (
        document.x(),
        document.y(),
        document.name(),
        document.columns(),
    )
    nested = (columns.base(), columns.keys(), columns.a(), columns.b())
    return (x, y, name, nested)


class TestValues(unittest.TestCase):
    # field_values() reads the fields of a table of scalars and a string into a
    # tuple, as fields() reads them into a dict, and serialize_values() writes
    # a message from them. A view's fields() reads every item's fields as each
    # item's fields() does.

    @SETTINGS
    @given(point_fields, slices)
    def test_reads_a_points_values_as_its_getters_read_them_and_writes_them_back(
        self, fields: PointFields, where: Slice
    ) -> None:
        data = Point.serialize(**fields)
        point = Point(where.of(data))

        self.assertEqual(
            fields_or_decode_error(point.field_values),
            fields_or_decode_error(partial(_point_values, point)),
        )
        written = Point.serialize_values(*point.field_values())
        self.assertEqual(bytes(written), bytes(data))

    @SETTINGS
    @given(point_fields, corruptions)
    def test_reads_a_corrupted_points_values_as_its_getters_do(
        self, fields: PointFields, corruption: Truncation | Overwrite
    ) -> None:
        point = Point(corruption.apply(bytes(Point.serialize(**fields))))

        self.assertEqual(
            fields_or_decode_error(point.field_values),
            fields_or_decode_error(partial(_point_values, point)),
        )

    @SETTINGS
    @given(binary(max_size=64))
    def test_reads_the_values_of_arbitrary_bytes_as_the_getters_do(
        self, data: bytes
    ) -> None:
        point = Point(data)

        self.assertEqual(
            fields_or_decode_error(point.field_values),
            fields_or_decode_error(partial(_point_values, point)),
        )

    @SETTINGS
    @given(item_list_fields, one_of(corruptions, binary(max_size=0)))
    def test_reads_every_items_fields_and_pairs_values_as_each_item_does(
        self, fields: ItemListFields, corruption: Truncation | Overwrite | bytes
    ) -> None:
        data = bytes(ItemList.serialize(**fields))
        if not isinstance(corruption, bytes):
            data = corruption.apply(data)
        try:
            view = ItemList(data).items()
        except DecodeError:
            return

        self.assertEqual(
            fields_or_decode_error(view.fields),
            fields_or_decode_error(partial(_fields_of_each, view)),
        )
        self.assertEqual(
            fields_or_decode_error(view.field_values),
            fields_or_decode_error(lambda: [_item_getters(item) for item in view]),
        )
        for index in range(len(view)):
            item = partial(_item_getters_at, view, index)
            self.assertEqual(
                fields_or_decode_error(partial(view.item_field_values, index)),
                fields_or_decode_error(item),
                f"item {index}",
            )
            self.assertEqual(
                fields_or_decode_error(lambda index=index: view[index].field_values()),
                fields_or_decode_error(item),
                f"item {index}",
            )
            self.assertEqual(
                fields_or_decode_error(partial(_pair_values, view, index)),
                fields_or_decode_error(partial(_pair_getters, view, index)),
                f"item {index}",
            )

    @SETTINGS
    @given(item_list_fields, slices)
    def test_writes_an_item_list_from_the_values_of_its_items_that_its_view_reads(
        self, fields: ItemListFields, where: Slice
    ) -> None:
        data = ItemList.serialize(**fields)
        items = ItemList(where.of(data)).items().field_values()

        self.assertEqual(bytes(ItemList.serialize_values(items)), bytes(data))

    @SETTINGS
    @given(item_list_fields)
    def test_reads_back_every_items_fields_as_they_were_written(
        self, fields: ItemListFields
    ) -> None:
        view = ItemList(ItemList.serialize(**fields)).items()

        self.assertEqual(
            fields_or_decode_error(view.fields),
            fields_or_decode_error(lambda: list(fields["items"])),
        )

    @SETTINGS
    @given(samples, binary(max_size=8), binary(max_size=8))
    def test_reads_and_writes_a_samples_values_as_the_sample_holds_them(
        self, sample: Sample, before: bytes, after: bytes
    ) -> None:
        data = before + sample.serialize() + after
        values = (sample.key, sample.a, sample.b)

        self.assertEqual(
            fields_or_decode_error(partial(Sample.read_values, data, len(before))),
            fields_or_decode_error(lambda: values),
        )
        self.assertEqual(Sample.serialize_values(*values), sample.serialize())

    @SETTINGS
    @given(binary(max_size=40), one_of(integers(-40, 40), _BEYOND_THE_DIGIT_LIMIT))
    def test_reads_a_samples_values_or_raises_as_read_does(
        self, data: bytes, offset: int
    ) -> None:
        self.assertEqual(
            fields_or_decode_error(partial(Sample.read_values, data, offset)),
            fields_or_decode_error(partial(_sample_values, data, offset)),
        )
        self.assertEqual(
            fields_or_decode_error(partial(Sample.read_array_values, data)),
            fields_or_decode_error(partial(_array_values, data)),
        )

    def test_raises_type_error_and_value_error_as_read_and_serialize_do(
        self,
    ) -> None:
        strided = memoryview(Sample(1, 0.5, -0.5).serialize() * 2)[::2]
        with self.assertRaises(TypeError):
            Sample.read_values(strided)
        with self.assertRaises(TypeError):
            Sample.read_array_values(strided)

        for values in [(2**32, 0.0, 0.0), (-1, 0.0, 0.0), (0, 1e300, 0.0)]:
            with self.subTest(values=values), self.assertRaises(ValueError):
                Sample.serialize_values(*values)


def _point_values(point: Point) -> tuple[float, float, int, str]:
    return (point.x(), point.y(), point.id(), point.name())


def _fields_of_each(view: ItemVector) -> list[ItemFields]:
    return [item.fields() for item in view]


def _item_getters(item: Item) -> tuple[float, float, float]:
    pair = item.pair()
    return (item.value(), pair.first(), pair.second())


def _item_getters_at(view: ItemVector, index: int) -> tuple[float, float, float]:
    return _item_getters(view[index])


def _pair_values(view: ItemVector, index: int) -> tuple[float, float]:
    return view[index].pair().field_values()


def _pair_getters(view: ItemVector, index: int) -> tuple[float, float]:
    pair = view[index].pair()
    return (pair.first(), pair.second())


def _sample_values(data: bytes, offset: int) -> tuple[int, float, float]:
    sample = Sample.read(data, offset)
    return (sample.key, sample.a, sample.b)


def _array_values(data: bytes) -> list[tuple[int, float, float]]:
    return [(sample.key, sample.a, sample.b) for sample in Sample.read_array(data)]


class TestReadingInPlace(unittest.TestCase):
    def test_reads_an_item_in_place_past_its_end_within_the_bytes_of_its_item_list(
        self,
    ) -> None:
        item: ItemFields = {"value": 1.5, "pair": {"first": 2.5, "second": 3.5}}
        data = bytearray(ItemList.serialize(items=[item]))
        # The vector at 4 holds its count and the item's end, 36, and then the
        # item at 12: its two offsets, its value at 20 and its Pair at 24. An
        # end of 16 leaves the item only its offsets, which point past it.
        self.assertEqual(data[8:12], (36).to_bytes(4, "little"))
        data[8:12] = (16).to_bytes(4, "little")
        item_list = ItemList(data)

        self.assertEqual(read_item_list(item_list), {"items": [item]})
        self.assertEqual(read_item(item_list.items()[0]), item)

    def test_raises_decode_error_for_a_corrupted_item_when_its_read_not_before(
        self,
    ) -> None:
        items: list[ItemFields] = [
            {"value": 1.5, "pair": {"first": 2.5, "second": 3.5}},
            {"value": 4.5, "pair": {"first": 5.5, "second": 6.5}},
        ]
        data = bytearray(ItemList.serialize(items=items))
        # The vector at 4 holds its count and the items' ends, the second's at
        # 12, which is made to end past the bytes
        data[12:16] = (len(data) - 3).to_bytes(4, "little")

        # The getter reads the count and checks that the ends lie within the
        # bytes, and the view reads the first item alone
        view = ItemList(data).items()
        self.assertEqual(len(view), 2)
        self.assertEqual(read_item(view[0]), items[0])
        with self.assertRaises(DecodeError):
            view[1]
        # Iterating reads every item's end before it reads any
        with self.assertRaises(DecodeError):
            list(view)


class TestSample(unittest.TestCase):
    @SETTINGS
    @given(samples)
    def test_writes_its_fields_little_endian_at_fixed_offsets_in_12_bytes(
        self, sample: Sample
    ) -> None:
        self.assertEqual(Sample.SIZE, 12)
        self.assertEqual(sample.serialize(), sample_bytes(sample))

    @SETTINGS
    @given(samples, binary(max_size=8), binary(max_size=8))
    def test_reads_a_sample_back_at_an_offset_in_longer_data(
        self, sample: Sample, before: bytes, after: bytes
    ) -> None:
        data = before + sample.serialize() + after

        read = Sample.read(data, len(before))
        assert_same_fields(self, describe_sample(read), describe_sample(sample))

    @SETTINGS
    @given(binary(max_size=24), integers(1, Sample.SIZE))
    def test_raises_decode_error_when_the_data_ends_before_the_sample(
        self, data: bytes, missing: int
    ) -> None:
        # Where the sample would end ``missing`` bytes after the data, or at
        # the start of data shorter than a sample
        offset = max(len(data) + missing - Sample.SIZE, 0)

        with self.assertRaises(DecodeError):
            Sample.read(data, offset)

    # Struct.unpack_from() would read a negative offset from the end of the
    # data, so Sample.read(data, -12) once read the last sample. Formatting
    # one of more digits than Python converts to a string once raised
    # ValueError.
    @SETTINGS
    @given(samples, one_of(integers(1, 64), _BEYOND_THE_DIGIT_LIMIT))
    def test_raises_decode_error_at_a_negative_offset(
        self, sample: Sample, distance: int
    ) -> None:
        data = sample.serialize() * 4

        with self.assertRaises(DecodeError):
            Sample.read(data, -distance)

    # Struct.unpack_from() takes the offset as a C ssize_t, and raises
    # OverflowError for a larger one
    @SETTINGS
    @given(
        samples,
        one_of(integers(min_value=2**63 - Sample.SIZE), _BEYOND_THE_DIGIT_LIMIT),
    )
    def test_raises_decode_error_at_an_offset_however_large(
        self, sample: Sample, offset: int
    ) -> None:
        data = sample.serialize() * 4

        with self.assertRaises(DecodeError):
            Sample.read(data, offset)

    def test_raises_type_error_for_data_that_isnt_c_contiguous(self) -> None:
        strided = memoryview(Sample(1, 0.5, -0.5).serialize() * 2)[::2]

        with self.assertRaises(TypeError):
            Sample.read(strided)
        with self.assertRaises(TypeError):
            Sample.read_array(strided)

    def test_raises_value_error_for_a_value_that_doesnt_fit_its_field(self) -> None:
        for sample in [
            Sample(2**32, 0.0, 0.0),
            Sample(-1, 0.0, 0.0),
            Sample(0, 1e300, 0.0),
        ]:
            with self.subTest(sample=sample), self.assertRaises(ValueError):
                sample.serialize()


def _item_fields(view: ItemVector, index: int) -> ItemFields:
    return view[index].fields()


def _read_item_at(view: ItemVector, index: int) -> ItemFields:
    return read_item(view[index])


def _items_by_index(data: bytes) -> list[str]:
    """Each item of an item list, read through its view by index, the last
    one again by -1, and one past it"""
    try:
        view = ItemList(data).items()
    except DecodeError as error:
        return [f"DecodeError: {error}"]

    def read(index: int) -> ItemFields:
        return read_item(view[index])

    indices = [*range(len(view)), -1, len(view)]
    return [_outcome(partial(read, index)) for index in indices]


def _items_by_offset(data: bytes) -> list[str]:
    """Each item of an item list, as ``_items_by_index()`` reads them, but
    found by ``get_vector_offset()``, which reads the count for each"""
    buffer = ByteBuffer(data)
    try:
        vector = buffer.root_offset(0)
        count = buffer.get_vector_length(vector)
    except DecodeError as error:
        return [f"DecodeError: {error}"]

    def read(index: int) -> ItemFields:
        return read_item(Item(buffer, buffer.get_vector_offset(vector, index)))

    indices = [*range(count), count - 1, count]
    return [_outcome(partial(read, index)) for index in indices]


def _outcome(read: Callable[[], ItemFields]) -> str:
    """The fields that ``read`` returns, as their repr, which tells NaN and -0
    apart, or its DecodeError or IndexError, which no other error may be"""
    try:
        return repr(read())
    except (DecodeError, IndexError) as error:
        return f"{type(error).__name__}: {error}"


if __name__ == "__main__":
    unittest.main()
