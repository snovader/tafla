package io.github.snovader.tafla.test;

import io.github.snovader.tafla.*;
import io.github.snovader.tafla.test.FieldDescription.*;
import net.jqwik.api.*;
import net.jqwik.api.constraints.*;
import net.jqwik.api.statistics.*;

import java.util.*;
import java.util.function.*;

import static org.junit.jupiter.api.Assertions.*;

/**
 * Checks the prototype {@link DirectTableBuilder} against {@link TableBuilder}: the same calls
 * write the same bytes and throw the same exceptions, from the same setters, and a setter that
 * throws leaves the builder as it was. Each builder keeps runs of 1, 8 or 256 bytes or more as
 * segments, or none.
 */
public class DirectTableBuilderTest {
    @Provide
    Arbitrary<Integer> segmentLengths() {
        return Arbitraries.of(1, 8, 256, Integer.MAX_VALUE);
    }

    @Provide
    Arbitrary<ConformanceTest.Case> validCases() {
        return Arbitraries.of(ConformanceTest.cases("valid"));
    }

    @Property(generation = GenerationMode.EXHAUSTIVE, shrinking = ShrinkingMode.OFF)
    void should_rebuild_every_valid_conformance_table_byte_for_byte_from_builders_and_in_place(
        @ForAll("validCases") ConformanceTest.Case validCase,
        @ForAll("segmentLengths") int segmentLength
    ) {
        var table = ConformanceTest.tableOf(validCase);
        assertArrayEquals(validCase.bytes(), new DirectTables(segmentLength).build(table).bytes());

        var inPlace = Tables.nestedInPlace(table);
        var expected = new InPlaceTables().build(inPlace).bytes();
        assertArrayEquals(expected, new DirectTables(segmentLength).buildInPlace(inPlace).bytes());
    }

    /**
     * Each root field is set, and set again without the fault if it throws, which writes what
     * {@code TableBuilder} writes only if the failure left the builder as it was
     */
    @Property(tries = 3000)
    void a_direct_builder_writes_the_bytes_and_throws_the_exceptions_of_a_table_builder(
        @ForAll("anyTables") TableDescription described,
        @ForAll boolean inPlace,
        @ForAll @IntRange(min = -1, max = 1) int extraFields,
        @ForAll("segmentLengths") int segmentLength,
        @ForAll FillFaults.Kind fault,
        @ForAll @IntRange(min = -1, max = 5) int faultAt
    ) {
        var table = inPlace ? Tables.nestedInPlace(described) : described;
        var fields = table.fields().size() + extraFields;
        Assume.that(fields >= 0);

        var expected = todayOutcomes(table, fields, inPlace, new FillFaults(fault, faultAt));
        var actual = directOutcomes(table, fields, inPlace, segmentLength, new FillFaults(fault, faultAt));

        var setters = expected.subList(0, expected.size() - 1);
        var setterFailed = setters.stream().anyMatch(outcome -> !outcome.equals("set"));
        Statistics.label("setter").collect(setterFailed ? "failed" : "succeeded");
        Statistics.label("table").collect(expected.getLast().startsWith("java") ? "exception" : "bytes");
        Statistics.label("setter").coverage(coverage -> coverage.check("failed").percentage(p -> p > 20));
        Statistics.label("table").coverage(coverage -> coverage.check("bytes").percentage(p -> p > 50));

        assertEquals(expected, actual);
    }

    @Provide
    Arbitrary<TableDescription> anyTables() {
        // Tables that fit their layout, and their fields in tables of uint8 offsets, which they
        // often make too long, from a field of a nested table or item too
        var nesting = Descriptions.nestingTables();
        return Arbitraries.oneOf(
            Descriptions.tables(),
            nesting,
            nesting.map(DirectTableBuilderTest::withUint8Offsets),
            Descriptions.narrowTables()
        );
    }

    private static TableDescription withUint8Offsets(TableDescription table) {
        var layout = new LayoutDescription(table.layout().sized(), false, OffsetType.UINT8);
        return new TableDescription(layout, OptionalLong.empty(), table.fields());
    }

    // Tables of up to 64 KiB, so fewer of them
    @Property(tries = 250)
    void a_field_too_long_for_the_offset_type_throws_as_from_a_table_builder_and_leaves_the_builder_as_it_was(
        @ForAll("narrowTables") TableDescription table,
        @ForAll TableBuilderTest.GrowingField growing,
        @ForAll Source from,
        @ForAll @IntRange(min = 1, max = 100) int excess,
        @ForAll("segmentLengths") int segmentLength,
        @ForAll boolean inPlace
    ) {
        var offsetType = table.layout().offsetType();
        Assume.that(Descriptions.maxLength(table.withField(growing.ofLength(0, from))) <= offsetType.maxValue());

        // The longest field that fits, and one longer by `excess` elements, which the root takes
        // one of: the field that's too long, set again after it throws, and then the one that fits
        var empty = Tables.build(table.withField(growing.ofLength(0, from))).bytes();
        var longest = (int) (offsetType.maxValue() - empty.length) / growing.unit(offsetType);
        var described = table.withField(growing.ofLength(longest + excess, from)).withField(growing.ofLength(longest, from));
        var fields = table.fields().size() + 1;

        var both = inPlace ? Tables.nestedInPlace(described) : described;
        var expected = todayOutcomes(both, fields, inPlace, FillFaults.none());
        var actual = directOutcomes(both, fields, inPlace, segmentLength, FillFaults.none());
        assertEquals(expected, actual);

        if (!inPlace) {
            var tooLong = String.format(" bytes, too long for %s offsets", offsetType);
            assertTrue(expected.get(table.fields().size()).endsWith(tooLong), expected::toString);
            assertFalse(expected.getLast().startsWith("java"), expected::toString);
        }
    }

    @Provide
    Arbitrary<TableDescription> narrowTables() {
        return Descriptions.narrowTables();
    }

    /**
     * A string of fewer than 64 chars, which the direct builder encodes before it knows its length,
     * at the end of a table of bytes, which it fits or makes too long, and nested in place
     */
    @Property(tries = 500)
    void a_short_string_at_the_end_of_a_table_throws_as_from_a_table_builder(
        @ForAll("limitedTypes") OffsetType offsetType,
        @ForAll @IntRange(max = 200) int room,
        @ForAll @StringLength(max = 63) String string,
        @ForAll boolean nested,
        @ForAll("segmentLengths") int segmentLength
    ) {
        var width = offsetType.size();
        var bytes = (int) offsetType.maxValue() - 4 * width - room;
        Assume.that(bytes >= 0);

        var layout = LayoutDescription.headerless(offsetType);
        List<FieldDescription> fields = List.of(new ArrayField(ScalarType.INT8, new long[bytes]), new StringField(string));
        if (nested) {
            var table = new TableDescription(layout, OptionalLong.empty(), fields);
            fields = List.of(new TableField(Source.BUILDER, table, OptionalInt.empty()), new BoolField(true));
        }
        var table = new TableDescription(layout, OptionalLong.empty(), fields);

        var expected = todayOutcomes(table, fields.size(), true, FillFaults.none());
        var actual = directOutcomes(table, fields.size(), true, segmentLength, FillFaults.none());
        assertEquals(expected, actual);
    }

    @Provide
    Arbitrary<OffsetType> limitedTypes() {
        return Arbitraries.of(OffsetType.UINT8, OffsetType.UINT16);
    }

    @Property
    void a_table_nested_as_a_fresh_builder_a_written_builder_or_its_bytes_writes_the_parent_of_a_table_builder(
        @ForAll("anyTables") TableDescription table,
        @ForAll @IntRange(max = 7) int before,
        @ForAll("segmentLengths") int segmentLength
    ) {
        Assume.that(Descriptions.fitsItsLayout(table));
        var tables = new DirectTables(segmentLength);
        var bytes = Tables.build(table).bytes();
        var layout = TableLayout.DEFAULT;

        // Of a parent of uint32 offsets, which holds any of them
        var expected = parentOf(Tables.build(table), before, layout);
        var fresh = parentOf(tables.build(table), before, layout, segmentLength);
        var written = tables.build(table);
        written.bytes();
        var fromWritten = parentOf(written, before, layout, segmentLength);
        var fromBytes = parentOf(bytes, before, layout, segmentLength);

        assertArrayEquals(expected, fresh);
        assertArrayEquals(expected, fromWritten);
        assertArrayEquals(expected, fromBytes);
    }

    private static byte[] parentOf(TableBuilder table, int before, TableLayout layout) {
        var parent = new TableBuilder(3, layout);
        parent.setUint8Array(new short[before]);
        parent.setTable(table, Long.BYTES);
        parent.setVector(List.of(table, table));

        return parent.bytes();
    }

    private static byte[] parentOf(DirectTableBuilder table, int before, TableLayout layout, int segmentLength) {
        var parent = new DirectTableBuilder(3, layout).segmentsFrom(segmentLength);
        parent.setUint8Array(new short[before]);
        parent.setTable(table, Long.BYTES);
        parent.setVector(List.of(table, table));

        return parent.bytes();
    }

    private static byte[] parentOf(byte[] table, int before, TableLayout layout, int segmentLength) {
        var parent = new DirectTableBuilder(3, layout).segmentsFrom(segmentLength);
        parent.setUint8Array(new short[before]);
        parent.setTable(table, Long.BYTES);
        parent.setVector(new byte[][] {table, table});

        return parent.bytes();
    }

    /**
     * The one difference from {@code TableBuilder} that a caller can see, which neither builder's
     * contract allows: an array changed after it's set
     */
    @Example
    void should_copy_a_short_array_when_its_set_and_a_long_one_when_the_table_is_written() {
        var short_ = new float[] {1, 2};
        var long_ = new float[64];
        var builder = new DirectTableBuilder(2);
        builder.setFloat32Array(short_);
        builder.setFloat32Array(long_);
        short_[0] = 3;
        long_[0] = 3;

        var buffer = new ByteBuffer(builder.bytes());
        assertEquals(1, buffer.getFloat32Array(buffer.rootOffset(0))[0]);
        assertEquals(3, buffer.getFloat32Array(buffer.rootOffset(1))[0]);
    }

    @Example
    void should_reject_a_vector_whose_items_ends_and_offsets_are_too_long_before_writing_any_item() {
        var layout = TableLayout.builder().offsetType(OffsetType.UINT8).build();

        var builder = new DirectTableBuilder(1, layout);
        var exception = assertThrows(IllegalArgumentException.class, () -> builder.setVector(100, 2, (item, index) -> {
            throw new AssertionError("Item " + index + " was written");
        }));
        assertEquals("Field 1 makes the table 302 bytes, too long for uint8 offsets", exception.getMessage());

        builder.setVector(0, 0, (item, index) -> {});
        assertArrayEquals(new byte[] {1, 0}, builder.bytes());
    }

    @Example
    void should_reject_writing_the_builder_within_a_fill() {
        var builder = new DirectTableBuilder(1);

        assertThrows(IllegalStateException.class, () -> builder.setTable(0, table -> builder.bytes()));

        builder.setTable(0, table -> {});
        assertArrayEquals(new byte[] {4, 0, 0, 0}, builder.bytes());
    }

    @Example
    void should_reject_a_table_of_uint32_offsets_longer_than_a_java_array_before_allocating_it() {
        // One mebibyte, repeated past 2 GiB, as segments and copied
        var item = new byte[1 << 20];
        var vector = new byte[2048][];
        Arrays.fill(vector, item);

        var expected = assertThrows(IllegalArgumentException.class, () -> new TableBuilder(1).setVector(vector));
        for (var segmentLength : List.of(256, Integer.MAX_VALUE)) {
            var builder = new DirectTableBuilder(1).segmentsFrom(segmentLength);
            var exception = assertThrows(IllegalArgumentException.class, () -> builder.setVector(vector));
            assertEquals(expected.getMessage(), exception.getMessage());

            builder.setVector(new byte[][] {{1}});
            var buffer = new ByteBuffer(builder.bytes());
            assertArrayEquals(new byte[][] {{1}}, buffer.getVector(buffer.rootOffset(0)));
        }
    }

    @Property(generation = GenerationMode.EXHAUSTIVE)
    void an_alignment_other_than_1_2_4_or_8_is_rejected(@ForAll @IntRange(min = -16, max = 64) int alignment) {
        Assume.that(alignment != 1 && alignment != 2 && alignment != 4 && alignment != 8);
        var builder = new DirectTableBuilder(1);

        assertThrows(IllegalArgumentException.class, () -> builder.setTable(new byte[0], alignment));
        assertThrows(IllegalArgumentException.class, () -> builder.setTable(new DirectTableBuilder(0), alignment));
        assertThrows(IllegalArgumentException.class, () -> builder.setStructArray(new byte[0], 1, alignment));

        builder.setTable(new byte[0], 1);
        assertArrayEquals(new byte[] {4, 0, 0, 0}, builder.bytes());
    }

    @Example
    void should_reject_a_struct_size_below_1_or_bytes_that_arent_whole_structs() {
        var builder = new DirectTableBuilder(1);

        var exception = assertThrows(IllegalArgumentException.class, () -> builder.setStructArray(new byte[5], 2, 1));
        assertEquals("5 bytes aren't a whole number of 2-byte structs", exception.getMessage());
        assertThrows(IllegalArgumentException.class, () -> builder.setStructArray(new byte[4], 0, 1));

        builder.setStructArray(new byte[] {1, 2, 3, 4}, 2, 2);
        assertArrayEquals(new byte[] {4, 0, 0, 0, 2, 0, 0, 0, 1, 2, 3, 4}, builder.bytes());
    }

    @Example
    void should_write_once_and_reject_fields_after() {
        var builder = new DirectTableBuilder(1);
        builder.setString("once");

        assertSame(builder.bytes(), builder.bytes());
        var exception = assertThrows(IllegalStateException.class, () -> builder.setInt8((byte) 1));
        assertEquals("The table was already written", exception.getMessage());
    }

    /** {@return what each of the table's fields, set on a {@code TableBuilder}, and its bytes do} */
    private static List<String> todayOutcomes(TableDescription table, int fields, boolean inPlace, FillFaults faults) {
        var offsetType = table.layout().offsetType();
        var tables = new InPlaceTables(faults);
        var layout = table.layout().tableLayout();
        var version = table.version();
        var builder = version.isPresent()
            ? new TableBuilder(fields, layout, version.getAsLong())
            : new TableBuilder(fields, layout);

        BiConsumer<TableBuilder, FieldDescription> set = inPlace
            ? (target, field) -> tables.setField(target, field, offsetType)
            : Tables::setField;
        return outcomes(builder, table.fields(), set, TableBuilder::bytes, faults);
    }

    /** {@return what each of the table's fields, set on a {@code DirectTableBuilder}, and its bytes do} */
    private static List<String> directOutcomes(
        TableDescription table,
        int fields,
        boolean inPlace,
        int segmentLength,
        FillFaults faults
    ) {
        var offsetType = table.layout().offsetType();
        var tables = new DirectTables(segmentLength, faults);
        var builder = tables.createBuilder(table, fields);

        BiConsumer<DirectTableBuilder, FieldDescription> set = inPlace
            ? (target, field) -> tables.setFieldInPlace(target, field, offsetType)
            : tables::setField;
        return outcomes(builder, table.fields(), set, DirectTableBuilder::bytes, faults);
    }

    /**
     * {@return what setting each field does, and then writing the table}: "set", or the exception
     * that it threw, followed by what setting it again without the fault does, and then the table's
     * bytes in hexadecimal, or the exception that writing them threw
     */
    private static <B> List<String> outcomes(
        B builder,
        List<FieldDescription> fields,
        BiConsumer<B, FieldDescription> set,
        Function<B, byte[]> bytes,
        FillFaults faults
    ) {
        var outcomes = new ArrayList<String>();
        for (var field : fields) {
            var outcome = outcome(() -> set.accept(builder, field));
            outcomes.add(outcome);

            if (!outcome.equals("set")) {
                faults.paused(() -> outcomes.add(outcome(() -> set.accept(builder, field))));
            }
        }

        try {
            outcomes.add(HexFormat.of().formatHex(bytes.apply(builder)));
        } catch (RuntimeException e) {
            outcomes.add(e.toString());
        }

        return outcomes;
    }

    private static String outcome(Runnable set) {
        try {
            set.run();
            return "set";
        } catch (RuntimeException e) {
            return e.toString();
        }
    }
}
