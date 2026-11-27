package io.github.snovader.tafla.test;

import io.github.snovader.tafla.*;
import net.jqwik.api.*;
import net.jqwik.api.constraints.*;

import java.nio.charset.*;
import java.util.*;
import java.util.function.*;

import static org.junit.jupiter.api.Assertions.*;

/**
 * The vector of strings, which {@link TableBuilder}, {@link TableSizer} and {@link TableWriter}
 * write, and {@link TableReader} and {@link StringVector} read, every string at once, one from its
 * ends alone, or each in order
 */
public class StringVectorTest {
    /** A table of {@link TableLayout#DEFAULT} whose only field is a vector of {@code strings} */
    private static byte[] tableOf(List<String> strings) {
        var builder = new TableBuilder(1);
        builder.setStringVector(strings);

        return builder.bytes();
    }

    private static StringVector viewOf(byte[] bytes) {
        var reader = new TableReader(bytes);
        return new StringVector(reader, reader.rootOffset(0));
    }

    private static byte[][] utf8Of(List<String> strings) {
        return strings.stream().map(string -> string.getBytes(StandardCharsets.UTF_8)).toArray(byte[][]::new);
    }

    // Round trips and the bytes written

    @Property
    void should_read_back_every_string_at_once_each_from_its_ends_alone_and_in_order(
        @ForAll("vectors") List<String> strings
    ) {
        var bytes = tableOf(strings);
        var reader = new TableReader(bytes);
        var view = viewOf(bytes);

        var each = new ArrayList<String>();
        for (int index = 0; index < view.size(); index++) {
            each.add(view.get(index));
        }

        assertEquals(strings.size(), view.size());
        assertEquals(strings, List.of(reader.getStringVector(reader.rootOffset(0))));
        assertEquals(strings, each);
        assertEquals(strings, List.copyOf(view));
    }

    @Property
    void should_take_the_bytes_of_a_vector_whose_items_are_the_strings_utf8(@ForAll("vectors") List<String> strings) {
        var builder = new TableBuilder(1);
        builder.setVector(utf8Of(strings));

        assertArrayEquals(builder.bytes(), tableOf(strings));
    }

    @Property
    void should_take_the_end_of_each_string_and_a_count_beside_the_strings_utf8(
        @ForAll("vectors") List<String> strings
    ) {
        var utf8 = Arrays.stream(utf8Of(strings)).mapToInt(string -> string.length).sum();

        // The table's one offset, the count and the ends, of 4 bytes each
        assertEquals(4 + 4 + 4 * strings.size() + utf8, tableOf(strings).length);
    }

    @Property
    void should_be_written_by_a_sizer_and_its_writer_as_by_a_builder(@ForAll("vectors") List<String> strings) {
        var sizer = new TableSizer(1);
        sizer.addStringVector(strings);
        var writer = sizer.writer();
        writer.setStringVector(0, strings);

        assertArrayEquals(tableOf(strings), writer.bytes());
    }

    @Example
    void should_write_a_lone_surrogate_as_U_FFFD_as_a_string_field_does() {
        var view = viewOf(tableOf(List.of("a\uD800", "\uDC00b", "🌍")));

        assertEquals(List.of("a�", "�b", "🌍"), view);
    }

    @Example
    void should_write_and_read_strings_that_writers_encode_once_among_shorter_ones() {
        // Strings of at least 64 chars are encoded once rather than counted
        var strings = List.of("a".repeat(63), "é".repeat(64), "東".repeat(300), "", "b".repeat(4096), "c");
        var sizer = new TableSizer(1);
        sizer.addStringVector(strings);
        var writer = sizer.writer();
        writer.setStringVector(0, strings);

        assertEquals(strings, viewOf(tableOf(strings)));
        assertEquals(strings.get(2), viewOf(tableOf(strings)).get(2));
        assertArrayEquals(tableOf(strings), writer.bytes());
    }

    @Example
    void should_write_long_strings_other_than_those_sized_when_they_fill_the_sized_bytes() {
        var sized = List.of("x".repeat(100), "y".repeat(70));
        var written = List.of("y".repeat(100), "x".repeat(70));
        var sizer = new TableSizer(1);
        sizer.addStringVector(sized);
        var writer = sizer.writer();
        writer.setStringVector(0, written);

        assertArrayEquals(tableOf(written), writer.bytes());
    }

    @Example
    void should_copy_the_list_so_that_later_changes_to_it_dont_change_the_table() {
        var strings = new ArrayList<>(List.of("a", "b".repeat(64)));
        var builder = new TableBuilder(1);
        builder.setStringVector(strings);
        strings.set(0, "changed");
        strings.add("added");

        assertEquals(List.of("a", "b".repeat(64)), viewOf(builder.bytes()));
    }

    @Example
    void a_view_is_an_unmodifiable_random_access_list() {
        var view = viewOf(tableOf(List.of("a", "b")));

        assertInstanceOf(RandomAccess.class, view);
        assertThrows(UnsupportedOperationException.class, () -> view.set(0, "c"));
        assertThrows(UnsupportedOperationException.class, () -> view.add("c"));
        assertEquals(List.of("a", "b"), view);
        assertArrayEquals(new String[] {"a", "b"}, view.toArray(String[]::new));
    }

    // Reading every string at once reads what reading each string reads

    @Property
    void reading_every_string_reads_what_reading_each_string_reads_and_throws_where_any_does(
        @ForAll("vectors") List<String> strings,
        @ForAll("overwrites") List<Tuple.Tuple2<Integer, Byte>> overwrites
    ) {
        var bytes = tableOf(strings);
        for (var overwrite : overwrites) {
            // Past the table's offset, so that the vector is found
            bytes[Integer.BYTES + overwrite.get1() % (bytes.length - Integer.BYTES)] = overwrite.get2();
        }

        assertReadAlike(new TableReader(bytes), Integer.BYTES);
    }

    @Property
    void reading_any_bytes_every_string_at_once_reads_what_reading_each_string_reads(
        @ForAll @Size(max = 64) byte[] bytes,
        @ForAll OffsetType offsetType,
        @ForAll @IntRange(min = -2, max = 64) int position
    ) {
        var reader = new TableReader(bytes, TableLayout.builder().offsetType(offsetType).build());

        assertReadAlike(reader, position);
    }

    /**
     * Asserts that reading every string of the vector at {@code position} with the reader, each
     * string by index, and each in order, read the same strings, or throw a
     * {@link DecodeException} with the same message, that of the first string that doesn't read
     */
    private static void assertReadAlike(TableReader reader, int position) {
        StringVector view;
        try {
            view = new StringVector(reader, position);
        } catch (DecodeException expected) {
            assertThrows(DecodeException.class, () -> reader.getStringVector(position));
            return;
        }

        var each = outcomeOf(() -> {
            var strings = new ArrayList<String>();
            for (int index = 0; index < view.size(); index++) {
                strings.add(view.get(index));
            }

            return strings;
        });

        assertEquals(each, outcomeOf(() -> List.of(reader.getStringVector(position))), "Every string at once");
        assertEquals(each, outcomeOf(() -> List.copyOf(view)), "Each string in order");
    }

    /** {@return the strings that {@code read} reads, or the message of the decode exception that it throws} */
    private static Object outcomeOf(Supplier<List<String>> read) {
        try {
            return read.get();
        } catch (DecodeException exception) {
            return "DecodeException: " + exception.getMessage();
        }
    }

    // Vectors that readers reject

    @Property(generation = GenerationMode.EXHAUSTIVE)
    void should_reject_a_string_outside_the_vector_with_IndexOutOfBoundsException(
        @ForAll @IntRange(min = -2, max = 4) int index
    ) {
        Assume.that(index < 0 || index >= 2);
        var view = viewOf(tableOf(List.of("a", "b")));

        var exception = assertThrows(IndexOutOfBoundsException.class, () -> view.get(index));
        assertEquals(String.format("Item %d is outside the vector of 2 at 4", index), exception.getMessage());
    }

    @Example
    void should_reject_an_impossible_count_before_anything_allocates_for_it() {
        var reader = new TableReader(new byte[] {-1, -1, -1, -1});

        assertThrows(DecodeException.class, () -> reader.getStringVector(0));
        assertThrows(DecodeException.class, () -> new StringVector(reader, 0));
    }

    @Example
    void should_reject_the_strings_on_either_side_of_an_end_that_splits_a_character_and_read_the_others() {
        var bytes = tableOf(List.of("a", "東", "京", "b"));
        // The end of the second string, after the table's offset, the count and the first end
        bytes[4 + 4 + 4]--;
        var view = viewOf(bytes);

        assertEquals("a", view.get(0));
        assertThrows(DecodeException.class, () -> view.get(1));
        assertThrows(DecodeException.class, () -> view.get(2));
        assertEquals("b", view.get(3));
        assertThrows(DecodeException.class, () -> new TableReader(bytes).getStringVector(4));

        var strings = view.iterator();
        assertEquals("a", strings.next());
        assertThrows(DecodeException.class, strings::next);
        assertThrows(DecodeException.class, strings::next);
        assertEquals("b", strings.next());
        assertFalse(strings.hasNext());
        assertThrows(NoSuchElementException.class, strings::next);
    }

    @Property(generation = GenerationMode.EXHAUSTIVE)
    void should_name_the_position_of_a_string_that_isnt_valid_UTF_8(@ForAll @IntRange(min = 1, max = 70) int length) {
        var bytes = tableOf(List.of("a", "b".repeat(length)));
        // The second string starts after the offset, the count, two ends and the first string
        bytes[17] = (byte) 0xff;
        var view = viewOf(bytes);

        var exception = assertThrows(DecodeException.class, () -> view.get(1));
        assertEquals("The string at 17 isn't valid UTF-8", exception.getMessage());
        assertInstanceOf(CharacterCodingException.class, exception.getCause());
        assertEquals("a", view.get(0));
    }

    // Vectors that writers reject

    @Property(generation = GenerationMode.EXHAUSTIVE)
    void should_fill_a_table_of_narrow_offsets_and_reject_a_byte_more_leaving_the_builder_as_it_was(
        @ForAll("narrowTypes") OffsetType offsetType
    ) {
        var layout = TableLayout.builder().offsetType(offsetType).build();
        var max = (int) offsetType.maxValue();
        // Two offsets, a uint8, and the vector's count and its one end
        var room = max - 4 * offsetType.size() - 1;
        var builder = new TableBuilder(2, layout);
        builder.setUint8((short) 7);

        var exception = assertThrows(
            IllegalArgumentException.class,
            () -> builder.setStringVector(List.of("a".repeat(room + 1)))
        );
        assertEquals(
            String.format("Field 2 makes the table %d bytes, too long for %s offsets", max + 1, offsetType),
            exception.getMessage()
        );
        builder.setStringVector(List.of("a".repeat(room)));

        var bytes = builder.bytes();
        var reader = new TableReader(bytes, layout);
        assertEquals(max, bytes.length);
        assertEquals(7, reader.getUint8(reader.rootOffset(0)));
        assertArrayEquals(new String[] {"a".repeat(room)}, reader.getStringVector(reader.rootOffset(1)));
    }

    @Example
    void should_reject_strings_longer_than_those_sized_with_IndexOutOfBoundsException() {
        var sizer = new TableSizer(1);
        sizer.addStringVector(List.of("a", "b"));

        assertThrows(IndexOutOfBoundsException.class, () -> sizer.writer().setStringVector(0, List.of("a", "bc")));
        assertThrows(IndexOutOfBoundsException.class, () -> sizer.writer().setStringVector(0, List.of("a", "b", "")));
    }

    @Example
    void should_reject_strings_shorter_than_those_sized_with_IllegalStateException_from_bytes() {
        var sizer = new TableSizer(1);
        sizer.addStringVector(List.of("ab"));
        var writer = sizer.writer();
        writer.setStringVector(0, List.of("a"));

        var exception = assertThrows(IllegalStateException.class, writer::bytes);
        assertEquals("The fields wrote 13 of the table's 14 bytes", exception.getMessage());
    }

    @Provide
    Arbitrary<List<String>> vectors() {
        return Descriptions.vectorStrings().list().ofMaxSize(8);
    }

    /** {@return one to four bytes to write over a table, each at a position modulo its length} */
    @Provide
    Arbitrary<List<Tuple.Tuple2<Integer, Byte>>> overwrites() {
        var positions = Arbitraries.integers().greaterOrEqual(0);
        return Combinators.combine(positions, Arbitraries.bytes()).as(Tuple::of).list().ofMinSize(1).ofMaxSize(4);
    }

    @Provide
    Arbitrary<OffsetType> narrowTypes() {
        return Arbitraries.of(OffsetType.UINT8, OffsetType.UINT16);
    }
}
