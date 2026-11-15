package io.github.snovader.tafla.test;

import io.github.snovader.tafla.*;
import io.github.snovader.tafla.test.FieldDescription.*;
import net.jqwik.api.*;
import net.jqwik.api.constraints.*;
import net.jqwik.api.statistics.*;

import java.nio.charset.*;
import java.util.*;
import java.util.function.*;

import static org.junit.jupiter.api.Assertions.*;

public class TableBuilderTest {
    private static final TableLayout SIZED = TableLayout.builder().sized(true).build();

    // Tables of up to 64 KiB, so fewer of them
    @Property(tries = 250)
    void a_field_too_long_for_the_offset_type_throws_from_its_setter_and_leaves_the_builder_as_it_was(
        @ForAll("narrowTables") TableDescription described,
        @ForAll GrowingField growing,
        @ForAll Source from,
        @ForAll @IntRange(min = 1, max = 100) int excess,
        @ForAll boolean inPlace
    ) {
        // With every nested table written in place if `inPlace`, where its own fields make it, and
        // so its parent, too long
        UnaryOperator<TableDescription> shaped = inPlace ? Tables::nestedInPlace : UnaryOperator.identity();
        var table = shaped.apply(described);
        var offsetType = table.layout().offsetType();
        var max = offsetType.maxValue();
        var empty = shaped.apply(described.withField(growing.ofLength(0, from)));
        Assume.that(Descriptions.maxLength(empty) <= max);

        // The longest field that fits, and one longer by `excess` elements
        var longest = (int) (max - Tables.build(empty).bytes().length) / growing.unit(offsetType);
        var longestTable = shaped.apply(described.withField(growing.ofLength(longest, from)));
        var fits = longestTable.fields().getLast();
        var tooLong = shaped.apply(described.withField(growing.ofLength(longest + excess, from))).fields().getLast();

        var builder = Tables.createBuilder(longestTable);
        var setters = new InPlaceTables();
        for (var field : table.fields()) {
            setters.setField(builder, field, offsetType);
        }

        var exception = assertThrows(IllegalArgumentException.class, () -> setters.setField(builder, tooLong, offsetType));
        var message = String.valueOf(exception.getMessage());
        assertTrue(message.endsWith(String.format(" bytes, too long for %s offsets", offsetType)), message);
        setters.setField(builder, fits, offsetType);

        // The longest table reads back, and the rejected field left no trace in it
        var bytes = builder.bytes();
        assertArrayEquals(Tables.build(longestTable).bytes(), bytes);
        assertEquals(longestTable, TableReader.strict(bytes, table.layout()).readTable(longestTable));
    }

    @Provide
    Arbitrary<TableDescription> narrowTables() {
        return Descriptions.narrowTables();
    }

    /** A field of any length, which grows by the same bytes with each element */
    enum GrowingField {
        INT8_ARRAY,
        FLOAT64_ARRAY,
        STRING,
        STRUCT_ARRAY,
        NESTED_TABLE,
        VECTOR_ITEMS,
        VECTOR_ITEM;

        /** The scalars of the structs of {@link #STRUCT_ARRAY}: 3 bytes, aligned to 2 */
        private static final List<ScalarType> STRUCT = List.of(ScalarType.UINT16, ScalarType.UINT8);

        /** An empty table, which takes no bytes and is read in place */
        private static final TableDescription EMPTY = new TableDescription(
            LayoutDescription.headerless(OffsetType.UINT32),
            OptionalLong.empty(),
            List.of()
        );

        /** {@return the bytes that each element adds in a table of offsets of {@code offsetType}} */
        int unit(OffsetType offsetType) {
            return switch (this) {
                case FLOAT64_ARRAY -> Double.BYTES;
                case STRUCT_ARRAY -> StructBytes.size(STRUCT);
                // Each item's end
                case VECTOR_ITEMS -> offsetType.size();
                default -> Byte.BYTES;
            };
        }

        /** {@return the field with {@code length} elements, nesting tables from {@code from}} */
        FieldDescription ofLength(int length, Source from) {
            return switch (this) {
                case INT8_ARRAY -> new ArrayField(ScalarType.INT8, new long[length]);
                case FLOAT64_ARRAY -> new ArrayField(ScalarType.FLOAT64, new long[length]);
                case STRING -> new StringField("a".repeat(length));
                case STRUCT_ARRAY -> new StructArrayField(STRUCT, Collections.nCopies(length, List.of(1L, 2L)));
                case NESTED_TABLE -> new TableField(from, tableOfBytes(length), OptionalInt.empty());
                case VECTOR_ITEMS -> new VectorField(Collections.nCopies(length, new NestedTable(from, EMPTY)));
                case VECTOR_ITEM -> new VectorField(List.of(new NestedTable(from, tableOfBytes(length))));
            };
        }

        /** {@return a table of an array of {@code length} bytes} */
        private static TableDescription tableOfBytes(int length) {
            var bytes = new ArrayField(ScalarType.INT8, new long[length]);
            return new TableDescription(EMPTY.layout(), OptionalLong.empty(), List.of(bytes));
        }
    }

    @Property
    void a_complete_builder_writes_the_same_table_wherever_it_is_nested_and_when_written_itself(
        @ForAll("headerlessTables") TableDescription table
    ) {
        var nested = Tables.build(table);

        // Copied into its parent before it's written itself, and from its bytes after
        var beforeWritten = parentOf(nested);
        var written = nested.bytes();
        var afterWritten = parentOf(nested);

        assertArrayEquals(Tables.build(table).bytes(), written);
        assertArrayEquals(beforeWritten, afterWritten);
    }

    private static byte[] parentOf(TableBuilder nested) {
        var parent = new TableBuilder(2);
        parent.setTable(nested);
        parent.setVector(List.of(nested, nested));

        return parent.bytes();
    }

    @Provide
    Arbitrary<TableDescription> headerlessTables() {
        return Descriptions.headerlessTables();
    }

    @Property
    void tables_written_in_place_write_the_bytes_and_throw_the_exceptions_of_nested_builders(
        @ForAll("tablesToWriteInPlace") TableDescription described
    ) {
        var table = Tables.nestedInPlace(described);
        var inPlace = new InPlaceTables();

        var inPlaceBytes = outcome(() -> inPlace.build(table).bytes());
        var fromBuilders = outcome(() -> Tables.build(table).bytes());

        // Both outcomes, so that the property compares exceptions too
        Statistics.collect(fromBuilders.startsWith("java.") ? "exception" : "bytes");
        Statistics.coverage(coverage -> {
            coverage.check("bytes").percentage(percentage -> percentage > 50);
            coverage.check("exception").percentage(percentage -> percentage > 5);
        });

        // Except that a vector whose items' ends and offsets alone make its table too long fails
        // before its items are written, with the length of those rather than of the whole vector
        if (!inPlaceBytes.equals(fromBuilders) && inPlace.failedBeforeItems()) {
            var tooLong = String.format(" bytes, too long for %s offsets", table.layout().offsetType());
            assertTrue(inPlaceBytes.endsWith(tooLong), inPlaceBytes);
            assertTrue(fromBuilders.endsWith(tooLong), fromBuilders);
        } else {
            assertEquals(fromBuilders, inPlaceBytes);
        }
    }

    @Provide
    Arbitrary<TableDescription> tablesToWriteInPlace() {
        // Tables that fit their layout, and their fields in tables of uint8 offsets, which they
        // often make too long, from a field of a nested table or item too
        var tables = Descriptions.nestingTables();
        return Arbitraries.oneOf(tables, tables.map(TableBuilderTest::withUint8Offsets));
    }

    private static TableDescription withUint8Offsets(TableDescription table) {
        var layout = new LayoutDescription(table.layout().sized(), false, OffsetType.UINT8);
        return new TableDescription(layout, OptionalLong.empty(), table.fields());
    }

    /** {@return the table's bytes in hexadecimal, or the exception that building them threw} */
    private static String outcome(Supplier<byte[]> build) {
        try {
            return HexFormat.of().formatHex(build.get());
        } catch (IllegalArgumentException | IllegalStateException e) {
            return e.toString();
        }
    }

    /**
     * Arrays, strings and arrays of structs of 256 bytes or more, and tables nested as bytes as
     * long, are written into the table by {@code bytes()} rather than copied when they're set, from
     * builders, from bytes and in place
     */
    @Property(tries = 300)
    void a_table_of_long_arrays_and_strings_reads_back_and_holds_the_bytes_that_a_table_writer_writes(
        @ForAll("anyTables") TableDescription described,
        @ForAll @IntRange(min = 1, max = 64) int times
    ) {
        var table = Descriptions.lengthened(described, times);
        Statistics.collect(longestRun(table) >= 256 ? "a run of 256 bytes or more" : "shorter runs");
        Statistics.coverage(coverage -> coverage.check("a run of 256 bytes or more").percentage(p -> p > 30));

        var bytes = Tables.build(table).bytes();
        assertEquals(table, TableReader.strict(bytes, table.layout()).readTable(table));

        var inPlace = Tables.nestedInPlace(table);
        var written = Tables.write(inPlace);
        assertArrayEquals(written, Tables.build(inPlace).bytes());
        assertArrayEquals(written, new InPlaceTables().build(inPlace).bytes());
    }

    /** {@return the bytes of the table's longest array, string or array of structs, nested ones too} */
    private static long longestRun(TableDescription table) {
        long longest = 0;
        for (var field : table.fields()) {
            longest = Math.max(longest, switch (field) {
                case ArrayField array -> (long) array.values().length * array.type().size();
                case StringField string -> string.value().getBytes(StandardCharsets.UTF_8).length;
                case StructArrayField array -> (long) array.items().size() * StructBytes.size(array.fields());
                case TableField nested -> longestRun(nested.table());
                case VectorField vector -> vector.items().stream().mapToLong(item -> longestRun(item.table())).max().orElse(0);
                case ScalarField ignored -> 0;
                case BoolField ignored -> 0;
                case StructField ignored -> 0;
            });
        }

        return longest;
    }

    /**
     * Each root field is set, in place where it can be, and set again without the fault if it
     * throws. The table then holds what a builder whose fills never failed holds only if each
     * failure left the builder as it was: fills that throw, leave a field unset or set one too
     * many, in tables and vectors written in place at any depth, in tables of every offset type,
     * with runs of 256 bytes or more too, and tables declared with a field more or fewer than set.
     */
    @Property(tries = 1000)
    void a_fill_that_fails_leaves_the_builder_as_it_was(
        @ForAll("tablesToFail") TableDescription described,
        @ForAll @IntRange(min = -1, max = 1) int extraFields,
        @ForAll FillFaults.Kind fault,
        @ForAll @IntRange(max = 3) int faultAt
    ) {
        var table = Tables.nestedInPlace(described);
        var fields = table.fields().size() + extraFields;
        Assume.that(fields >= 0);

        var expected = settings(table, fields, FillFaults.none());
        var actual = settings(table, fields, new FillFaults(fault, faultAt));

        Statistics.label("fill").collect(actual.failures() > expected.failures() ? "failed" : "didn't fail");
        Statistics.label("table").collect(expected.outcomes().getLast().startsWith("java") ? "exception" : "bytes");
        Statistics.label("fill").coverage(coverage -> coverage.check("failed").percentage(p -> p > 15));
        Statistics.label("table").coverage(coverage -> coverage.check("bytes").percentage(p -> p > 30));

        assertEquals(expected.outcomes(), actual.outcomes());
    }

    @Provide
    Arbitrary<TableDescription> tablesToFail() {
        // Tables that fit their layout, with long runs too, and their fields in tables of uint8
        // offsets, which they often make too long, from a field of a nested table or item too
        var tables = Descriptions.nestingTables();
        return Arbitraries.oneOf(
            tables,
            tables.map(table -> Descriptions.lengthened(table, 40)),
            tables.map(TableBuilderTest::withUint8Offsets),
            Descriptions.narrowTables()
        );
    }

    /**
     * What setting each of a table's fields did, "set" or the exception that it threw, set again
     * without faults if it threw, followed by the table's bytes in hexadecimal or the exception
     * that writing them threw, and how many fields threw the first time
     */
    private record Settings(List<String> outcomes, int failures) {}

    private static Settings settings(TableDescription table, int fields, FillFaults faults) {
        var builder = Tables.createBuilder(table, fields);
        var setters = new InPlaceTables(faults);
        var offsetType = table.layout().offsetType();

        var outcomes = new ArrayList<String>();
        var failures = 0;
        for (var field : table.fields()) {
            Supplier<String> set = () -> settingOutcome(() -> setters.setField(builder, field, offsetType));

            var outcome = set.get();
            if (!outcome.equals("set")) {
                failures++;
                outcome = faults.paused(set);
            }
            outcomes.add(outcome);
        }
        outcomes.add(outcome(builder::bytes));

        return new Settings(outcomes, failures);
    }

    private static String settingOutcome(Runnable set) {
        try {
            set.run();
            return "set";
        } catch (RuntimeException e) {
            return e.toString();
        }
    }

    /**
     * A string of fewer than 64 chars, which the builder encodes before it knows its length, at the
     * end of a table of bytes, or of one written in place, which it fits or makes too long by up to
     * 200 bytes. A string that doesn't fit throws from its setter, and an empty one takes its place.
     */
    @Property(tries = 500)
    void a_short_string_at_the_end_of_a_table_is_written_if_it_fits_and_otherwise_leaves_the_builder_as_it_was(
        @ForAll("limitedTypes") OffsetType offsetType,
        @ForAll @IntRange(max = 200) int room,
        @ForAll @StringLength(max = 63) String string,
        @ForAll boolean nested
    ) {
        var table = endingIn(offsetType, room, string, nested);
        var empty = endingIn(offsetType, room, "", nested);

        var builder = Tables.createBuilder(table);
        var setters = new InPlaceTables();
        var fits = true;
        for (int i = 0; i < table.fields().size(); i++) {
            try {
                setters.setField(builder, table.fields().get(i), offsetType);
            } catch (IllegalArgumentException exception) {
                var message = String.valueOf(exception.getMessage());
                assertTrue(message.endsWith(String.format(" bytes, too long for %s offsets", offsetType)), message);

                setters.setField(builder, empty.fields().get(i), offsetType);
                fits = false;
            }
        }

        Statistics.collect(fits ? "fits" : "too long");
        Statistics.coverage(coverage -> {
            coverage.check("fits").percentage(p -> p > 10);
            coverage.check("too long").percentage(p -> p > 10);
        });

        // The writer counts the string first, and writes it only if it fits
        assertEquals(fits, !outcome(() -> Tables.write(table)).startsWith("java."));
        assertArrayEquals(Tables.write(fits ? table : empty), builder.bytes());
    }

    /**
     * {@return a table of {@code offsetType} offsets of bytes and then {@code string}, or of a bool
     * and then that table nested in place}, whose bytes leave {@code room} bytes for the string's
     * UTF-8 within the offset type's reach
     */
    private static TableDescription endingIn(OffsetType offsetType, int room, String string, boolean nested) {
        var width = offsetType.size();
        // The offsets, counts and bool besides the bytes and the string's UTF-8
        var others = nested ? 6 * width + 1 : 4 * width;
        var bytes = new ArrayField(ScalarType.INT8, new long[(int) offsetType.maxValue() - others - room]);

        var layout = LayoutDescription.headerless(offsetType);
        var table = new TableDescription(layout, OptionalLong.empty(), List.of(bytes, new StringField(string)));
        if (!nested) {
            return table;
        }

        var nestedTable = new TableField(Source.BUILDER, table, OptionalInt.empty());
        return new TableDescription(layout, OptionalLong.empty(), List.of(new BoolField(true), nestedTable));
    }

    @Example
    void should_pass_on_what_a_fill_throws_and_leave_the_builder_as_it_was() {
        var failure = new IllegalStateException("The fill failed");
        var builder = uint8Before();

        // From the second item, once it and the first were written, with a table nested in each
        var thrown = assertThrows(IllegalStateException.class, () -> builder.setVector(2, 2, (item, index) -> {
            item.setFloat32(index);
            item.setTable(1, pair -> pair.setFloat32(index));
            if (index == 1) {
                throw failure;
            }
        }));
        assertSame(failure, thrown);

        thrown = assertThrows(IllegalStateException.class, () -> builder.setTable(1, table -> {
            table.setFloat32(1);
            throw failure;
        }));
        assertSame(failure, thrown);

        setItems(builder);
        assertArrayEquals(uint8AndItems(), builder.bytes());
    }

    @Example
    void should_reject_an_item_or_a_table_in_it_with_fields_left_to_set_and_leave_the_builder_as_it_was() {
        var builder = uint8Before();

        var exception = assertThrows(IllegalArgumentException.class, () -> builder.setVector(2, 2, (item, index) -> {
            item.setFloat32(index);
            if (index == 0) {
                item.setTable(1, pair -> pair.setFloat32(index));
            }
        }));
        assertEquals("Only 1 of the nested table's 2 fields were set", exception.getMessage());

        exception = assertThrows(IllegalArgumentException.class, () -> builder.setVector(2, 2, (item, index) -> {
            item.setFloat32(index);
            item.setTable(2, pair -> pair.setFloat32(index));
        }));
        assertEquals("Only 1 of the nested table's 2 fields were set", exception.getMessage());

        exception = assertThrows(IllegalArgumentException.class, () -> builder.setTable(1, table -> {}));
        assertEquals("Only 0 of the nested table's 1 fields were set", exception.getMessage());

        setItems(builder);
        assertArrayEquals(uint8AndItems(), builder.bytes());
    }

    @Example
    void should_reject_a_field_past_an_items_count_and_leave_the_builder_as_it_was() {
        var builder = uint8Before();

        var exception = assertThrows(IllegalStateException.class, () -> builder.setVector(1, 1, (item, index) -> {
            item.setFloat32(1);
            item.setFloat32(2);
        }));
        assertEquals("Can't add field 2 to a table of 1", exception.getMessage());

        setItems(builder);
        assertArrayEquals(uint8AndItems(), builder.bytes());
    }

    @Example
    void should_reject_a_vector_whose_items_ends_and_offsets_are_too_long_before_writing_any_item() {
        var layout = TableLayout.builder().offsetType(OffsetType.UINT8).build();

        // The field's offset, then the count and 100 ends, and 2 offsets in each item: 302 bytes
        var builder = new TableBuilder(1, layout);
        var exception = assertThrows(IllegalArgumentException.class, () -> builder.setVector(100, 2, (item, index) -> {
            throw new AssertionError("Item " + index + " was written");
        }));

        var offsets = new TableBuilder(1, layout);
        var expected = assertThrows(IllegalArgumentException.class, () -> offsets.setVector(new byte[100][2]));
        assertEquals("Field 1 makes the table 302 bytes, too long for uint8 offsets", expected.getMessage());
        assertEquals(expected.getMessage(), exception.getMessage());

        assertThrows(IllegalArgumentException.class, () -> builder.setVector(-1, 0, (item, index) -> {}));
        assertThrows(IllegalArgumentException.class, () -> builder.setVector(0, -1, (item, index) -> {}));
        assertThrows(IllegalArgumentException.class, () -> builder.setTable(-1, table -> {}));
        exception = assertThrows(IllegalArgumentException.class, () -> builder.setTable(256, table -> {}));
        assertEquals("A header of 256 fields doesn't fit in uint8 offsets", exception.getMessage());

        builder.setVector(0, 0, (item, index) -> {});
        assertArrayEquals(new byte[] {1, 0}, builder.bytes());
    }

    @Example
    void should_reject_writing_the_builder_within_a_fill() {
        var builder = new TableBuilder(1);

        assertThrows(IllegalStateException.class, () -> builder.setTable(0, table -> builder.bytes()));

        builder.setTable(0, table -> {});
        assertArrayEquals(new byte[] {4, 0, 0, 0}, builder.bytes());
    }

    /** {@return a builder of a uint8 and then a vector} */
    private static TableBuilder uint8Before() {
        var builder = new TableBuilder(2);
        builder.setUint8((short) 1);

        return builder;
    }

    /** Records a vector of two items in place, each a float32 and a table of a float32 */
    private static void setItems(TableBuilder builder) {
        builder.setVector(2, 2, (item, index) -> {
            item.setFloat32(index);
            item.setTable(1, pair -> pair.setFloat32(index + 0.5f));
        });
    }

    /** {@return the bytes of {@link #uint8Before} with the items of {@link #setItems}, from builders} */
    private static byte[] uint8AndItems() {
        var items = new ArrayList<TableBuilder>();
        for (int index = 0; index < 2; index++) {
            var pair = new TableBuilder(1);
            pair.setFloat32(index + 0.5f);

            var item = new TableBuilder(2);
            item.setFloat32(index);
            item.setTable(pair);
            items.add(item);
        }

        var builder = uint8Before();
        builder.setVector(items);
        return builder.bytes();
    }

    @Property
    void a_table_nested_as_a_fresh_builder_a_written_builder_or_its_bytes_writes_the_same_parent(
        @ForAll("anyTables") TableDescription table,
        @ForAll @IntRange(max = 7) int before
    ) {
        var bytes = Tables.build(table).bytes();
        // Of the table's offset type when the parent fits in it, and otherwise of uint32 offsets
        var offsetType = table.layout().offsetType();
        if (longestParent(offsetType, before, bytes.length) > offsetType.maxValue()) {
            offsetType = OffsetType.UINT32;
        }
        var layout = TableLayout.builder().offsetType(offsetType).build();

        // A fresh builder is copied from the bytes it has written so far, of any layout, and a
        // written one from its bytes
        var fresh = parentOf(Tables.build(table), before, layout);
        var written = Tables.build(table);
        written.bytes();
        var fromWritten = parentOf(written, before, layout);
        var fromBytes = parentOf(bytes, before, layout);

        assertArrayEquals(fromBytes, fresh);
        assertArrayEquals(fromBytes, fromWritten);

        var buffer = new ByteBuffer(fromBytes, layout);
        var nested = buffer.rootOffset(1);
        assertEquals(0, nested % 8);
        assertArrayEquals(bytes, Arrays.copyOfRange(fromBytes, nested, nested + bytes.length));
        assertArrayEquals(new byte[][] {bytes, bytes}, buffer.getVector(buffer.rootOffset(2)));
    }

    @Provide
    Arbitrary<TableDescription> anyTables() {
        return Arbitraries.oneOf(Descriptions.tables(), Descriptions.nestingTables());
    }

    /**
     * {@return a parent of {@code before} bytes in a {@code uint8[]}, the table at alignment 8,
     * and a vector of the table twice}
     */
    private static byte[] parentOf(TableBuilder table, int before, TableLayout layout) {
        var parent = parentBefore(before, layout);
        parent.setTable(table, Long.BYTES);
        parent.setVector(List.of(table, table));

        return parent.bytes();
    }

    private static byte[] parentOf(byte[] table, int before, TableLayout layout) {
        var parent = parentBefore(before, layout);
        parent.setTable(table, Long.BYTES);
        parent.setVector(new byte[][] {table, table});

        return parent.bytes();
    }

    private static TableBuilder parentBefore(int before, TableLayout layout) {
        var parent = new TableBuilder(3, layout);
        parent.setUint8Array(new short[before]);

        return parent;
    }

    /** {@return the most bytes that a parent of offsets of {@code offsetType} takes} */
    private static long longestParent(OffsetType offsetType, int before, int length) {
        long width = offsetType.size();
        var offsets = 3 * width;
        var array = width + before;
        var table = (Long.BYTES - 1) + (long) length;
        var vector = 3 * width + 2L * length;

        return offsets + array + table + vector;
    }

    @Property(generation = GenerationMode.EXHAUSTIVE)
    void a_version_is_written_up_to_the_largest_that_its_slot_holds_and_rejected_beyond(
        @ForAll OffsetType offsetType
    ) {
        var layout = versioned(offsetType);
        var max = offsetType.maxValue();

        var buffer = new ByteBuffer(new TableBuilder(0, layout, max).bytes(), layout);
        assertEquals(max, buffer.getVersion());

        var exception = assertThrows(IllegalArgumentException.class, () -> new TableBuilder(0, layout, max + 1));
        assertEquals(String.format("Version %d doesn't fit in a %s slot", max + 1, offsetType), exception.getMessage());
        assertThrows(IllegalArgumentException.class, () -> new TableBuilder(0, layout, -1));
    }

    @Property(generation = GenerationMode.EXHAUSTIVE)
    void an_alignment_other_than_1_2_4_or_8_is_rejected(@ForAll @IntRange(min = -16, max = 64) int alignment) {
        Assume.that(alignment != 1 && alignment != 2 && alignment != 4 && alignment != 8);
        var builder = new TableBuilder(1);

        assertThrows(IllegalArgumentException.class, () -> builder.setTable(new byte[0], alignment));
        assertThrows(IllegalArgumentException.class, () -> builder.setTable(new TableBuilder(0), alignment));
        assertThrows(IllegalArgumentException.class, () -> builder.setStructArray(new byte[0], 1, alignment));
    }

    @Example
    void should_reject_a_struct_size_below_1_or_bytes_that_arent_whole_structs_and_leave_the_builder_as_it_was() {
        var builder = new TableBuilder(1);

        var exception = assertThrows(IllegalArgumentException.class, () -> builder.setStructArray(new byte[5], 2, 1));
        assertEquals("5 bytes aren't a whole number of 2-byte structs", exception.getMessage());
        exception = assertThrows(IllegalArgumentException.class, () -> builder.setStructArray(new byte[4], 0, 1));
        assertEquals("A struct's size must be positive, not 0", exception.getMessage());
        assertThrows(IllegalArgumentException.class, () -> builder.setStructArray(new byte[4], -1, 1));

        builder.setStructArray(new byte[] {1, 2, 3, 4}, 2, 2);
        assertArrayEquals(new byte[] {4, 0, 0, 0, 2, 0, 0, 0, 1, 2, 3, 4}, builder.bytes());
    }

    @Example
    void should_count_structs_and_pad_so_that_they_start_at_a_multiple_of_their_alignment() {
        var builder = new TableBuilder(2);
        builder.setUint8((short) 7);
        builder.setStructArray(new byte[16], 8, 8);

        // The count goes at 12, after the uint8 at 8 and 3 bytes of padding, so that the structs
        // start at 16
        var buffer = new ByteBuffer(builder.bytes());
        assertEquals(12, buffer.rootOffset(1));
        assertEquals(2, buffer.getUint32(12));
        assertEquals(16, buffer.getStructArray(12, 8).remaining());
    }

    @Example
    void should_write_a_struct_unpadded() {
        var builder = new TableBuilder(2);
        builder.setUint8((short) 7);
        builder.setStruct(new byte[] {1, 2, 3});

        assertArrayEquals(new byte[] {8, 0, 0, 0, 9, 0, 0, 0, 7, 1, 2, 3}, builder.bytes());
    }

    @Example
    void should_pad_a_builder_to_the_alignment_of_its_arrays_of_structs() {
        var nested = new TableBuilder(1);
        nested.setStructArray(new byte[9], 9, 8);

        var parent = new TableBuilder(2);
        parent.setUint8((short) 1);
        parent.setTable(nested);

        // After the uint8 at 8, padded to 16
        assertEquals(16, new ByteBuffer(parent.bytes()).rootOffset(1));
    }

    @Example
    void should_reject_a_field_beyond_the_number_the_table_holds() {
        var builder = new TableBuilder(1);
        builder.setUint8((short) 1);

        assertThrows(IllegalStateException.class, () -> builder.setUint8((short) 2));
        assertArrayEquals(new byte[] {4, 0, 0, 0, 1}, builder.bytes());
    }

    @Property
    void should_reject_writing_before_every_field_is_set(@ForAll @IntRange(max = 4) int set) {
        var builder = new TableBuilder(set + 1);
        for (int i = 0; i < set; i++) {
            builder.setFloat64(i);
        }

        var exception = assertThrows(IllegalStateException.class, builder::bytes);
        assertEquals(
            String.format("Only %d of the table's %d fields were set", set, set + 1),
            exception.getMessage()
        );
    }

    @Example
    void should_reject_setting_a_field_after_writing() {
        var builder = new TableBuilder(0);
        builder.bytes();

        assertThrows(IllegalStateException.class, () -> builder.setInt8((byte) 1));
    }

    @Example
    void should_write_once_and_return_the_same_bytes() {
        var builder = new TableBuilder(1);
        builder.setString("once");

        assertSame(builder.bytes(), builder.bytes());
    }

    @Example
    void should_need_a_version_exactly_when_the_layout_has_a_slot_for_it() {
        var exception = assertThrows(
            IllegalArgumentException.class,
            () -> new TableBuilder(1, versioned(OffsetType.UINT32))
        );
        assertEquals("A versioned layout needs a version", exception.getMessage());

        exception = assertThrows(IllegalArgumentException.class, () -> new TableBuilder(1, SIZED, 1));
        assertEquals("The layout has no version slot", exception.getMessage());
    }

    @Example
    void should_write_a_lone_surrogate_as_the_replacement_character() {
        var builder = new TableBuilder(1);
        builder.setString("a\uD800b\uDC00\uD83D\uDE00");

        var buffer = new ByteBuffer(builder.bytes());
        assertEquals("a\uFFFDb\uFFFD\uD83D\uDE00", buffer.getString(buffer.rootOffset(0)));
    }

    @Property
    void should_reject_a_header_longer_than_the_offsets_address(
        @ForAll("limitedTypes") OffsetType offsetType,
        @ForAll boolean sized
    ) {
        var layout = TableLayout.builder().sized(sized).offsetType(offsetType).build();
        var fields = (int) (offsetType.maxValue() / offsetType.size()) - (sized ? 1 : 0);

        assertDoesNotThrow(() -> new TableBuilder(fields, layout));
        var exception = assertThrows(IllegalArgumentException.class, () -> new TableBuilder(fields + 1, layout));
        assertEquals(
            String.format("A header of %d fields doesn't fit in %s offsets", fields + 1, offsetType),
            exception.getMessage()
        );
    }

    @Provide
    Arbitrary<OffsetType> limitedTypes() {
        return Arbitraries.of(OffsetType.UINT8, OffsetType.UINT16);
    }

    @Example
    void should_reject_a_table_of_uint32_offsets_longer_than_a_java_array_before_allocating_it() {
        // One mebibyte, repeated past 2 GiB
        var item = new byte[1 << 20];
        var vector = new byte[2048][];
        Arrays.fill(vector, item);

        var builder = new TableBuilder(1);
        assertThrows(IllegalArgumentException.class, () -> builder.setVector(vector));

        builder.setVector(new byte[][] {{1}});
        var buffer = new ByteBuffer(builder.bytes());
        assertArrayEquals(new byte[][] {{1}}, buffer.getVector(buffer.rootOffset(0)));
    }

    @Example
    void should_pad_a_builder_without_arrays_by_nothing() {
        var nested = new TableBuilder(1);
        nested.setFloat64(1.5);

        var parent = new TableBuilder(2);
        parent.setUint8((short) 1);
        parent.setTable(nested);

        assertEquals(2 * Integer.BYTES + 1, new ByteBuffer(parent.bytes()).rootOffset(1));
    }

    @Example
    void should_reject_a_builder_with_fields_left_to_set_and_leave_the_table_as_it_was() {
        var nested = new TableBuilder(2);
        nested.setUint8((short) 1);

        var parent = new TableBuilder(2);
        var exception = assertThrows(IllegalArgumentException.class, () -> parent.setTable(nested));
        assertEquals("Only 1 of the nested table's 2 fields were set", exception.getMessage());
        assertThrows(IllegalArgumentException.class, () -> parent.setVector(List.of(nested)));

        nested.setUint8((short) 2);
        parent.setTable(nested);
        parent.setVector(List.of(nested));

        var buffer = new ByteBuffer(parent.bytes());
        var table = buffer.rootOffset(0);
        assertEquals(2, buffer.getUint8(buffer.fieldOffset(table, 1)));
        assertArrayEquals(new byte[][] {nested.bytes()}, buffer.getVector(buffer.rootOffset(1)));
    }

    @Example
    void should_write_a_vector_as_it_was_set() {
        var items = new ArrayList<TableBuilder>();
        items.add(new TableBuilder(0));

        var parent = new TableBuilder(1);
        parent.setVector(items);
        items.add(new TableBuilder(0));

        var buffer = new ByteBuffer(parent.bytes());
        assertEquals(1, buffer.getVectorOffsets(buffer.rootOffset(0)).length);
    }

    private static TableLayout versioned(OffsetType offsetType) {
        return TableLayout.builder().versioned(true).offsetType(offsetType).build();
    }
}
