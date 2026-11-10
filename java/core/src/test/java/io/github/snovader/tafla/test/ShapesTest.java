package io.github.snovader.tafla.test;

import io.github.snovader.tafla.*;
import io.github.snovader.tafla.test.generated.shapes.*;
import net.jqwik.api.*;
import net.jqwik.api.constraints.*;

import java.time.Duration;
import java.util.Arrays;
import java.util.LinkedList;
import java.util.List;

import static io.github.snovader.tafla.test.DecodeAssertions.*;
import static io.github.snovader.tafla.test.GeneratedDescriptions.*;
import static io.github.snovader.tafla.test.GivenBytesAssertions.*;
import static io.github.snovader.tafla.test.ShapesTables.*;
import static org.junit.jupiter.api.Assertions.*;

/**
 * The code generated from {@code schemas/shapes.tafla} writes and reads tables with tafla. Values
 * read back compare with the values written by their descriptions, which compare floats by their
 * bits and arrays by their elements.
 */
public class ShapesTest {
    // Round trips

    @Property
    void a_point_reads_back_its_fields(@ForAll("points") Point.Fields fields) {
        var bytes = Point.serialize(fields);

        assertEquals(describePoint(fields), describePoint(readPoint(new Point(bytes))));
    }

    @Property
    void a_document_reads_back_its_fields_and_those_of_its_nested_columns(@ForAll("documents") Document.Fields fields) {
        var bytes = Document.serialize(fields);

        assertEquals(describeDocument(fields), describeDocument(readDocument(new Document(bytes))));
    }

    @Property
    void an_item_list_reads_back_its_items_and_their_pairs(@ForAll("itemLists") ItemList.Fields fields) {
        var bytes = ItemList.serialize(fields);

        assertEquals(describeItemList(fields), describeItemList(readItemList(new ItemList(bytes))));
    }

    // Tables built by hand

    @Property
    void a_point_is_written_as_TableBuilder_writes_it(@ForAll("points") Point.Fields fields) {
        var byHand = Tables.build(describePoint(fields)).bytes();

        assertArrayEquals(byHand, Point.serialize(fields), () -> describePoint(fields).toString());
    }

    @Property
    void a_document_is_written_sized_and_versioned_as_TableBuilder_writes_it(
        @ForAll("documents") Document.Fields fields
    ) {
        var byHand = Tables.build(describeDocument(fields)).bytes();

        assertArrayEquals(byHand, Document.serialize(fields), () -> describeDocument(fields).toString());
    }

    @Property
    void an_item_list_is_written_as_TableBuilder_writes_it(@ForAll("itemLists") ItemList.Fields fields) {
        var byHand = Tables.build(describeItemList(fields)).bytes();

        assertArrayEquals(byHand, ItemList.serialize(fields), () -> describeItemList(fields).toString());
    }

    // Writing into bytes that the caller gives, which may hold anything

    @Property
    void every_message_written_into_longer_bytes_holds_at_their_start_what_serialize_writes(
        @ForAll("points") Point.Fields point,
        @ForAll("documents") Document.Fields document,
        @ForAll("itemLists") ItemList.Fields itemList,
        @ForAll @IntRange(max = 16) int extra,
        @ForAll byte fill
    ) {
        assertWrittenInto(Point.serialize(point), bytes -> Point.serializeInto(point, bytes), extra, fill);
        assertWrittenInto(Document.serialize(document), bytes -> Document.serializeInto(document, bytes), extra, fill);
        assertWrittenInto(ItemList.serialize(itemList), bytes -> ItemList.serializeInto(itemList, bytes), extra, fill);
    }

    @Property
    void bytes_shorter_than_a_message_throw_IllegalArgumentException_before_any_is_written(
        @ForAll("documents") Document.Fields document,
        @ForAll("itemLists") ItemList.Fields itemList,
        @ForAll @IntRange(min = 1, max = 16) int shorter,
        @ForAll byte fill
    ) {
        var itemListLength = ItemList.serialize(itemList).length;
        Assume.that(shorter <= itemListLength);

        // A document's header and offsets alone take 24 bytes
        assertTooShort(Document.serialize(document).length, bytes -> Document.serializeInto(document, bytes), shorter, fill);
        assertTooShort(itemListLength, bytes -> ItemList.serializeInto(itemList, bytes), shorter, fill);
    }

    // A LinkedList, without random access, is what these tests write from
    @Property
    @SuppressWarnings("JdkObsolete")
    void an_item_list_is_written_from_a_linked_list_as_from_any_other_list(
        @ForAll("itemLists") ItemList.Fields fields
    ) {
        var linked = new ItemList.Fields(new LinkedList<>(fields.items()));

        assertArrayEquals(ItemList.serialize(fields), ItemList.serialize(linked));
    }

    @Example
    @SuppressWarnings("JdkObsolete")
    void an_item_list_is_written_from_a_long_linked_list_in_time_proportional_to_its_length() {
        // Getting each item by its index would walk the list for each, which
        // would take minutes for these items rather than milliseconds
        var items = new LinkedList<Item.Fields>();
        for (int i = 0; i < 300_000; i++) {
            items.add(new Item.Fields(i, new Pair.Fields(i, -i)));
        }

        var bytes = assertTimeoutPreemptively(
            Duration.ofSeconds(5),
            () -> ItemList.serialize(new ItemList.Fields(items))
        );
        assertEquals(300_000, new ItemList(bytes).items().size());
    }

    // Versions

    @Property
    void a_documents_version_is_read_from_its_bytes_of_any_version_to_choose_its_reader(
        @ForAll("documents") Document.Fields fields,
        @ForAll @LongRange(max = 0xffff_ffffL) long version
    ) {
        var bytes = Tables.build(describeDocument(fields).withVersion(version)).bytes();

        assertEquals(version, Document.versionOf(bytes));
    }

    @Property
    void the_version_of_bytes_without_a_documents_header_throws_DecodeException(
        @ForAll("documents") Document.Fields fields,
        @ForAll @IntRange(max = 7) int length
    ) {
        var bytes = Arrays.copyOf(Document.serialize(fields), length);

        assertThrows(DecodeException.class, () -> Document.versionOf(bytes));
    }

    // Decode exceptions

    @Property
    void a_document_of_another_version_throws_DecodeException(
        @ForAll("documents") Document.Fields fields,
        @ForAll @LongRange(max = 0xffff_ffffL) long version
    ) {
        Assume.that(version != DOCUMENT_VERSION);
        var bytes = Tables.build(describeDocument(fields).withVersion(version)).bytes();

        assertThrows(DecodeException.class, () -> new Document(bytes));
    }

    @Property
    void a_corrupted_point_reads_as_values_or_throws_DecodeException(
        @ForAll("points") Point.Fields fields,
        @ForAll("corruptions") Corruption corruption
    ) {
        var bytes = corruption.applyTo(Point.serialize(fields));

        assertValuesOrDecodeException(() -> readPoint(new Point(bytes)));
    }

    @Property
    void a_corrupted_document_reads_as_values_or_throws_DecodeException(
        @ForAll("documents") Document.Fields fields,
        @ForAll("corruptions") Corruption corruption
    ) {
        var bytes = corruption.applyTo(Document.serialize(fields));

        assertValuesOrDecodeException(() -> readDocument(new Document(bytes)));
    }

    @Property
    void a_corrupted_item_list_reads_as_values_or_throws_DecodeException(
        @ForAll("itemLists") ItemList.Fields fields,
        @ForAll("corruptions") Corruption corruption
    ) {
        var bytes = corruption.applyTo(ItemList.serialize(fields));

        assertValuesOrDecodeException(() -> readItemList(new ItemList(bytes)));
    }

    @Property
    void any_bytes_read_as_any_message_or_throw_DecodeException(@ForAll @Size(max = 64) byte[] bytes) {
        assertValuesOrDecodeException(() -> readPoint(new Point(bytes)));
        assertValuesOrDecodeException(() -> readDocument(new Document(bytes)));
        assertValuesOrDecodeException(() -> readItemList(new ItemList(bytes)));
    }

    // Reading in place

    @Example
    void an_item_is_read_in_place_past_its_end_within_the_bytes_of_its_item_list() {
        var item = new Item.Fields(1.5f, new Pair.Fields(2.5f, 3.5f));
        var bytes = ItemList.serialize(new ItemList.Fields(List.of(item)));
        // The vector at 4 holds its count and the item's end, 36, and then the item at 12: its two
        // offsets, its value at 20 and its Pair at 24. An end of 16 leaves the item only its
        // offsets, which point past it.
        assertArrayEquals(new byte[] {36, 0, 0, 0}, Arrays.copyOfRange(bytes, 8, 12));
        bytes[8] = 16;
        var itemList = new ItemList(bytes);

        assertEquals(new ItemList.Fields(List.of(item)), readItemList(itemList));
        assertEquals(item, readItem(itemList.itemsView().get(0)));
    }

    // The struct

    @Property
    void a_sample_is_written_in_its_12_bytes_little_endian_at_fixed_offsets(@ForAll("samples") Sample sample) {
        var bytes = sample.serialize();

        assertEquals(12, Sample.SIZE);
        assertArrayEquals(structBytes(describeSample(sample)), bytes);
    }

    @Property
    void a_sample_reads_back_at_any_offset_in_longer_bytes(
        @ForAll("samples") Sample sample,
        @ForAll @Size(max = 16) byte[] before,
        @ForAll @Size(max = 8) byte[] after
    ) {
        var bytes = java.nio.ByteBuffer.allocate(before.length + Sample.SIZE + after.length)
            .put(before)
            .put(sample.serialize())
            .put(after)
            .array();

        assertEquals(describeSample(sample), describeSample(Sample.read(bytes, before.length)));
    }

    @Property
    void reading_a_sample_throws_DecodeException_where_its_bytes_end_past_the_bytes_or_the_view(
        @ForAll @Size(max = 32) byte[] bytes,
        @ForAll("offsets") int offset
    ) {
        // The view ends where `bytes` do, within longer bytes, as a view of a struct array does
        var view = java.nio.ByteBuffer.wrap(java.util.Arrays.copyOf(bytes, bytes.length + Sample.SIZE))
            .slice(0, bytes.length)
            .order(java.nio.ByteOrder.LITTLE_ENDIAN);

        if (offset >= 0 && (long) offset + Sample.SIZE <= bytes.length) {
            assertDoesNotThrow(() -> Sample.read(bytes, offset));
            assertDoesNotThrow(() -> Sample.read(view, offset));
        } else {
            assertThrows(DecodeException.class, () -> Sample.read(bytes, offset));
            assertThrows(DecodeException.class, () -> Sample.read(view, offset));
        }
    }

    @Provide
    Arbitrary<Point.Fields> points() {
        return pointFields();
    }

    @Provide
    Arbitrary<Document.Fields> documents() {
        return documentFields();
    }

    @Provide
    Arbitrary<ItemList.Fields> itemLists() {
        return itemListFields();
    }

    @Provide
    Arbitrary<Sample> samples() {
        return sampleFields();
    }

    @Provide
    Arbitrary<Corruption> corruptions() {
        return Corruption.corruptions();
    }

    /** {@return offsets mostly within or around a few dozen bytes, and sometimes any int} */
    @Provide
    Arbitrary<Integer> offsets() {
        return Arbitraries.frequencyOf(
            Tuple.of(4, Arbitraries.integers().between(-8, 40)),
            Tuple.of(1, Arbitraries.integers())
        );
    }
}
