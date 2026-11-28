package io.github.snovader.tafla.test;

import io.github.snovader.tafla.DecodeException;
import io.github.snovader.tafla.OffsetType;
import io.github.snovader.tafla.TableReader;
import io.github.snovader.tafla.test.AllTypesTables.ViewOrder;
import io.github.snovader.tafla.test.generated.all_types.*;
import net.jqwik.api.*;
import net.jqwik.api.constraints.*;

import java.util.List;

import static io.github.snovader.tafla.test.DecodeAssertions.*;
import static io.github.snovader.tafla.test.GeneratedDescriptions.*;
import static io.github.snovader.tafla.test.GivenBytesAssertions.*;
import static org.junit.jupiter.api.Assertions.*;

/**
 * The vectors of strings of {@code schemas/all_types.tafla}, whose getters return the runtime's
 * {@code StringVector}, and whose {@code Fields} records hold a {@code List} of the strings
 */
public class StringVectorsTest {
    /** The layout of {@code NarrowStringVectors}, of {@code uint16} offsets */
    private static final LayoutDescription NARROW_LAYOUT = LayoutDescription.headerless(OffsetType.UINT16);

    // Round trips

    @Property
    void string_vectors_reads_back_its_vectors_of_strings_in_a_field_a_nested_table_and_vector_items(
        @ForAll("stringVectors") StringVectors.Fields fields,
        @ForAll ViewOrder order
    ) {
        var bytes = StringVectors.serialize(fields);

        assertEquals(fields, readStringVectors(new StringVectors(bytes), order));
    }

    @Property
    void narrow_string_vectors_of_uint16_offsets_reads_back_its_strings_and_the_scalar_after_them(
        @ForAll("narrowStringVectors") NarrowStringVectors.Fields fields,
        @ForAll ViewOrder order
    ) {
        var bytes = NarrowStringVectors.serialize(fields);

        assertEquals(fields, readNarrowStringVectors(new NarrowStringVectors(bytes), order));
    }

    // Tables built by hand

    @Property
    void string_vectors_is_written_as_TableBuilder_writes_it(@ForAll("stringVectors") StringVectors.Fields fields) {
        var byHand = Tables.build(describeStringVectors(fields)).bytes();

        assertArrayEquals(byHand, StringVectors.serialize(fields), () -> describeStringVectors(fields).toString());
    }

    @Property
    void narrow_string_vectors_is_written_as_TableBuilder_writes_it(
        @ForAll("narrowStringVectors") NarrowStringVectors.Fields fields
    ) {
        var table = message(NARROW_LAYOUT, stringVector(fields.names()), uint8(fields.count()));

        assertArrayEquals(Tables.build(table).bytes(), NarrowStringVectors.serialize(fields), table::toString);
    }

    // Writing into bytes that the caller gives, which may hold anything

    @Property
    void string_vectors_written_into_longer_bytes_holds_at_their_start_what_serialize_writes_and_reads_back(
        @ForAll("stringVectors") StringVectors.Fields fields,
        @ForAll @IntRange(max = 16) int extra,
        @ForAll byte fill
    ) {
        var expected = StringVectors.serialize(fields);
        var bytes = new byte[expected.length + extra];

        assertWrittenInto(expected, longer -> StringVectors.serializeInto(fields, longer), extra, fill);
        StringVectors.serializeInto(fields, bytes);
        assertEquals(fields, readStringVectors(new StringVectors(bytes), ViewOrder.ITERATED));
    }

    // Tables that writers reject

    @Example
    void narrow_string_vectors_too_long_for_uint16_offsets_throws_IllegalArgumentException() {
        var fields = new NarrowStringVectors.Fields(List.of("a".repeat(0x8000), "b".repeat(0x8000)), (short) 0);

        var exception = assertThrows(IllegalArgumentException.class, () -> NarrowStringVectors.serialize(fields));
        // Two offsets, the count and two ends, the strings and the uint8
        assertEquals(
            "The table's fields make it 65547 bytes, too long for uint16 offsets",
            exception.getMessage()
        );
    }

    // Decode exceptions

    @Property
    void a_corrupted_string_vectors_reads_as_values_or_throws_DecodeException(
        @ForAll("stringVectors") StringVectors.Fields fields,
        @ForAll("corruptions") Corruption corruption,
        @ForAll ViewOrder order
    ) {
        var bytes = corruption.applyTo(StringVectors.serialize(fields));

        assertValuesOrDecodeException(() -> readStringVectors(new StringVectors(bytes), order));
    }

    @Property
    void any_bytes_read_as_vectors_of_strings_or_throw_DecodeException(
        @ForAll @Size(max = 64) byte[] bytes,
        @ForAll ViewOrder order
    ) {
        assertValuesOrDecodeException(() -> readStringVectors(new StringVectors(bytes), order));
        assertValuesOrDecodeException(() -> readNarrowStringVectors(new NarrowStringVectors(bytes), order));
    }

    @Example
    void a_tag_that_isnt_valid_UTF_8_throws_DecodeException_and_leaves_the_others_to_read() {
        var bytes = StringVectors.serialize(
            new StringVectors.Fields(1, List.of("ok", "bad", "fine"), new Labels.Fields(List.of()), List.of())
        );
        var table = new StringVectors(bytes);
        // The second tag's first byte, after the count, three ends and "ok"
        bytes[new TableReader(bytes).rootOffset(1) + 4 * 4 + 2] = (byte) 0xc0;

        var tags = table.tags();
        assertEquals(3, tags.size());
        assertEquals("ok", tags.get(0));
        assertThrows(DecodeException.class, () -> tags.get(1));
        assertEquals("fine", tags.get(2));
        assertThrows(DecodeException.class, () -> List.copyOf(tags));
        assertEquals(1, table.id());
    }

    // Reading every field, the strings of each vector in an order

    private static StringVectors.Fields readStringVectors(StringVectors table, ViewOrder order) {
        var entries = order.read(table.entries()).stream().map(entry -> readEntry(entry, order)).toList();

        return new StringVectors.Fields(
            table.id(),
            order.read(table.tags()),
            new Labels.Fields(order.read(table.labels().values())),
            entries
        );
    }

    private static Entry.Fields readEntry(Entry entry, ViewOrder order) {
        return new Entry.Fields(entry.key(), order.read(entry.aliases()));
    }

    private static NarrowStringVectors.Fields readNarrowStringVectors(NarrowStringVectors table, ViewOrder order) {
        return new NarrowStringVectors.Fields(order.read(table.names()), table.count());
    }

    /** {@return the table that a {@code StringVectors} writes, as the conformance suite describes it} */
    private static TableDescription describeStringVectors(StringVectors.Fields fields) {
        var entries = fields.entries().stream()
            .map(entry -> vectorItem(string(entry.key()), stringVector(entry.aliases())))
            .toList();

        return message(
            DEFAULT_LAYOUT,
            uint32(fields.id()),
            stringVector(fields.tags()),
            table(stringVector(fields.labels().values())),
            vector(entries)
        );
    }

    // Arbitrary fields

    private static Arbitrary<List<String>> stringLists() {
        return Descriptions.vectorStrings().list().ofMaxSize(6);
    }

    @Provide
    Arbitrary<StringVectors.Fields> stringVectors() {
        var entries = Combinators.combine(Descriptions.strings(), stringLists())
            .as(Entry.Fields::new)
            .list()
            .ofMaxSize(3);

        return Combinators.combine(
            Arbitraries.longs().between(0, 0xffff_ffffL),
            stringLists(),
            stringLists().map(Labels.Fields::new),
            entries
        ).as(StringVectors.Fields::new);
    }

    /** {@return names of short strings, which fit in a table of {@code uint16} offsets} */
    @Provide
    Arbitrary<NarrowStringVectors.Fields> narrowStringVectors() {
        var names = Descriptions.strings().list().ofMaxSize(6);
        var counts = Arbitraries.shorts().between((short) 0, (short) 0xff);

        return Combinators.combine(names, counts).as(NarrowStringVectors.Fields::new);
    }

    @Provide
    Arbitrary<Corruption> corruptions() {
        return Corruption.corruptions();
    }
}
