package io.github.snovader.tafla.test;

import io.github.snovader.tafla.*;
import io.github.snovader.tafla.test.FieldDescription.*;
import net.jqwik.api.*;
import net.jqwik.api.constraints.*;
import org.junit.jupiter.api.function.*;

import java.nio.ByteOrder;
import java.nio.CharBuffer;
import java.nio.charset.*;
import java.util.*;
import java.util.function.*;

import static org.junit.jupiter.api.Assertions.*;

/**
 * {@link TableSizer} counts each table as {@link TableBuilder} counts it, padding included, and
 * {@link TableWriter} writes the same bytes from the test's calls as from the builder's, which
 * writes its tables with a {@code TableWriter} too. They fail where the builder fails. They write
 * nested tables in place, so the descriptions that they write have every nested table and vector
 * item from its builder, without a header, and with the root's offset type, as
 * {@link Tables#nestedInPlace} makes them.
 */
public class TableWriterTest {
    @Property
    void a_table_sized_and_written_from_any_description_is_as_long_as_TableBuilder_counts_it_and_holds_the_same_bytes(
        @ForAll("inPlaceTables") TableDescription table
    ) {
        assertArrayEquals(Tables.build(table).bytes(), Tables.write(table));
    }

    @Example
    void a_struct_array_a_nested_table_and_a_vector_are_padded_as_TableBuilder_counts_their_padding() {
        // A uint8 leaves the uint32[] 3 bytes short of its alignment, the array leaves the structs
        // 2 bytes short of theirs, and the structs leave the nested table of float64s 6 bytes short
        var layout = new LayoutDescription(true, true, OffsetType.UINT16);
        var nested = new TableDescription(
            LayoutDescription.headerless(OffsetType.UINT16),
            OptionalLong.empty(),
            List.of(new ArrayField(ScalarType.FLOAT64, new long[] {Double.doubleToLongBits(6)}))
        );
        var item = new TableDescription(
            LayoutDescription.headerless(OffsetType.UINT16),
            OptionalLong.empty(),
            List.of(new StringField("🦀"))
        );
        var table = new TableDescription(
            layout,
            OptionalLong.of(7),
            List.of(
                new ScalarField(ScalarType.UINT8, 1),
                new ArrayField(ScalarType.UINT32, new long[] {2, 3}),
                new StructArrayField(
                    List.of(ScalarType.UINT16, ScalarType.FLOAT64),
                    List.of(List.of(4L, Double.doubleToLongBits(5)))
                ),
                new TableField(Source.BUILDER, nested, OptionalInt.empty()),
                new VectorField(List.of(new NestedTable(Source.BUILDER, item)))
            )
        );

        var bytes = Tables.write(table);
        assertArrayEquals(Tables.build(table).bytes(), bytes);
        assertEquals(table, TableReader.strict(bytes, layout).readTable(table));
    }

    @Property
    void structs_written_in_place_hold_the_bytes_of_the_array_written_from_their_bytes(
        @ForAll("inPlaceTables") TableDescription table,
        @ForAll @IntRange(max = 7) int length,
        @ForAll("structArrays") FieldDescription field
    ) {
        // The bytes before the structs leave them 0 to 7 bytes short of any alignment
        var before = table.withField(bytesField(length));
        var structs = (StructArrayField) field;
        var whole = before.withField(structs);

        assertEquals(outcomeOf(() -> Tables.write(whole)), outcomeOf(() -> writeInPlace(before, structs)));
    }

    @Property
    void any_string_is_written_as_the_JDKs_encoder_encodes_it_with_each_lone_surrogate_as_U_FFFD(
        @ForAll("utf16Strings") String string
    ) throws CharacterCodingException {
        var table = new TableDescription(
            LayoutDescription.headerless(OffsetType.UINT32),
            OptionalLong.empty(),
            List.of(new StringField(string))
        );

        // The field's offset, after the one offset, and then the string's length and bytes
        var utf8 = utf8(string);
        var expected = java.nio.ByteBuffer.allocate(2 * Integer.BYTES + utf8.length)
            .order(ByteOrder.LITTLE_ENDIAN)
            .putInt(Integer.BYTES)
            .putInt(utf8.length)
            .put(utf8)
            .array();

        assertArrayEquals(expected, Tables.write(table), string::toString);
        assertArrayEquals(expected, Tables.build(table).bytes(), string::toString);
    }

    /**
     * {@return the string's UTF-8 bytes from the JDK's encoder}, which replaces each lone surrogate
     * with U+FFFD, as TypeScript's {@code TextEncoder} does, rather than with {@code '?'}, as
     * {@link String#getBytes} does
     */
    private static byte[] utf8(String string) throws CharacterCodingException {
        var encoder = StandardCharsets.UTF_8.newEncoder()
            .onMalformedInput(CodingErrorAction.REPLACE)
            .replaceWith("\uFFFD".getBytes(StandardCharsets.UTF_8));
        var encoded = encoder.encode(CharBuffer.wrap(string));

        var bytes = new byte[encoded.remaining()];
        encoded.get(bytes);
        return bytes;
    }

    @Property
    void strings_in_nested_tables_and_vector_items_are_written_in_order_as_TableBuilder_writes_them(
        @ForAll("utf16Strings") String first,
        @ForAll("utf16Strings") String nested,
        @ForAll @Size(max = 3) List<@From("utf16Strings") String> items
    ) {
        var layout = LayoutDescription.headerless(OffsetType.UINT32);
        Function<String, TableDescription> tableOf = string ->
            new TableDescription(layout, OptionalLong.empty(), List.of(new StringField(string)));
        var vector = items.stream().map(item -> new NestedTable(Source.BUILDER, tableOf.apply(item))).toList();
        var table = new TableDescription(
            layout,
            OptionalLong.empty(),
            List.of(
                new StringField(first),
                new TableField(Source.BUILDER, tableOf.apply(nested), OptionalInt.empty()),
                new VectorField(vector)
            )
        );

        assertArrayEquals(Tables.build(table).bytes(), Tables.write(table));
    }

    @Property
    void a_string_other_than_the_one_sized_is_written_as_itself_when_it_fills_the_sized_bytes(
        @ForAll("utf16Strings") String sized
    ) {
        // The same units in reverse, which take as many bytes unless lone surrogates pair up
        var written = new StringBuilder(sized).reverse().toString();
        var sizer = new TableSizer(1);
        sizer.addString(sized);
        var writer = sizer.writer();
        var builder = new TableBuilder(1);
        builder.setString(written);
        var expected = builder.bytes();

        if (expected.length == sizer.byteLength()) {
            writer.setString(0, written);
            assertArrayEquals(expected, writer.bytes());
        } else {
            assertThrows(RuntimeException.class, () -> {
                writer.setString(0, written);
                writer.bytes();
            });
        }
    }

    // Tables longer than their offsets address

    @Property
    void a_table_too_long_for_its_offsets_throws_IllegalArgumentException_as_TableBuilder_does(
        @ForAll("narrowInPlaceTables") TableDescription table,
        @ForAll @IntRange(max = 300) int length
    ) {
        var longer = table.withField(bytesField(length));

        assertEquals(outcomeOf(() -> Tables.build(longer).bytes()), outcomeOf(() -> Tables.write(longer)));
    }

    // Each try writes and builds tables of up to 64 KiB, so fewer of them
    @Property(tries = 50)
    void the_longest_table_of_uint8_and_uint16_offsets_is_written_and_one_a_byte_longer_throws_IllegalArgumentException(
        @ForAll("narrowInPlaceTables") TableDescription table,
        @ForAll("unpaddedFields") FieldDescription last
    ) {
        var offsetType = table.layout().offsetType();
        var max = offsetType.maxValue();
        IntFunction<TableDescription> withBytes = length -> table.withField(bytesField(length)).withField(last);

        // The bytes before the last field grow the table by as many bytes, without padding
        var shortest = withBytes.apply(0);
        Assume.that(Descriptions.fitsItsLayout(shortest));
        var room = (int) (max - Tables.build(shortest).bytes().length);

        var longest = withBytes.apply(room);
        var bytes = Tables.write(longest);
        assertEquals(max, bytes.length);
        assertArrayEquals(Tables.build(longest).bytes(), bytes);

        var tooLong = withBytes.apply(room + 1);
        var exception = assertThrows(IllegalArgumentException.class, () -> Tables.write(tooLong));
        assertEquals(
            String.format(
                "The table's fields make it %d bytes, too long for %s offsets",
                max + 1,
                offsetType
            ),
            exception.getMessage()
        );
        assertThrows(IllegalArgumentException.class, () -> Tables.build(tooLong));
    }

    @Example
    void a_table_of_uint32_offsets_is_counted_up_to_the_longest_java_array_and_rejected_beyond_before_allocating() {
        // The longest array that the JDK allocates, after the table's offset and the array's count
        var longestArray = Integer.MAX_VALUE - 8;
        var longest = new TableSizer(1);
        longest.addInt8Array(longestArray - 2 * Integer.BYTES);
        assertEquals(longestArray, longest.byteLength());

        var tooLong = new TableSizer(1);
        tooLong.addInt8Array(longestArray - 2 * Integer.BYTES + 1);
        var exception = assertThrows(IllegalArgumentException.class, tooLong::writer);
        assertEquals(
            "The table's fields make it 2147483640 bytes, longer than a Java array can be",
            exception.getMessage()
        );

        // Past the 4 GiB that uint32 offsets address, in a count that an int can't hold
        var past4GiB = new TableSizer(3);
        for (int i = 0; i < 3; i++) {
            past4GiB.addFloat64Array(Integer.MAX_VALUE);
        }
        assertThrows(IllegalArgumentException.class, past4GiB::writer);
    }

    // The header

    @Property
    void a_header_longer_than_the_offsets_address_is_rejected_as_TableBuilder_rejects_it(
        @ForAll("narrowTypes") OffsetType offsetType,
        @ForAll boolean sized
    ) {
        var layout = TableLayout.builder().sized(sized).offsetType(offsetType).build();
        var fields = (int) (offsetType.maxValue() / offsetType.size()) - (sized ? 1 : 0);

        assertEquals(layout.headerSize() + fields * offsetType.size(), new TableSizer(fields, layout).byteLength());
        assertRejectedAlike(() -> new TableBuilder(fields + 1, layout), () -> new TableSizer(fields + 1, layout));
    }

    @Example
    void versions_and_field_counts_are_checked_as_TableBuilder_checks_them() {
        var versioned = TableLayout.builder().versioned(true).offsetType(OffsetType.UINT8).build();

        assertRejectedAlike(
            () -> new TableBuilder(0, TableLayout.DEFAULT, 1),
            () -> new TableSizer(0, TableLayout.DEFAULT, 1)
        );
        assertRejectedAlike(() -> new TableBuilder(0, versioned), () -> new TableSizer(0, versioned));
        assertRejectedAlike(() -> new TableBuilder(0, versioned, 256), () -> new TableSizer(0, versioned, 256));
        assertRejectedAlike(() -> new TableBuilder(0, versioned, -1), () -> new TableSizer(0, versioned, -1));
        assertRejectedAlike(() -> new TableBuilder(-1), () -> new TableSizer(-1));
    }

    // Fields that differ from those sized

    @Property
    void fields_that_differ_from_those_sized_are_written_when_they_fill_the_sized_bytes_and_otherwise_throw(
        @ForAll("inPlaceTables") TableDescription sized,
        @ForAll("values") FieldDescription other,
        @ForAll @IntRange(max = 100) int at
    ) {
        Assume.that(!sized.fields().isEmpty());
        var fields = new ArrayList<>(sized.fields());
        fields.set(at % fields.size(), other);
        var written = new TableDescription(sized.layout(), sized.version(), fields);
        Assume.that(Descriptions.fitsItsLayout(written));

        var expected = Tables.build(written).bytes();
        var writer = Tables.size(sized).writer();
        if (expected.length == Tables.build(sized).bytes().length) {
            assertArrayEquals(expected, Tables.write(writer, written));
        } else {
            var exception = assertThrows(RuntimeException.class, () -> Tables.write(writer, written));
            assertTrue(
                exception instanceof IndexOutOfBoundsException || exception instanceof IllegalStateException,
                exception::toString
            );
        }
    }

    @Example
    void a_string_longer_than_the_one_sized_throws_IndexOutOfBoundsException() {
        var sizer = new TableSizer(1);
        sizer.addString("a");
        var writer = sizer.writer();

        assertThrows(IndexOutOfBoundsException.class, () -> writer.setString(0, "ab"));
    }

    @Example
    void structs_that_end_past_the_sized_bytes_throw_IndexOutOfBoundsException() {
        var sizer = new TableSizer(1);
        sizer.addStructArray(1, Integer.BYTES, Integer.BYTES);
        var writer = sizer.writer();

        Integer[] structs = {1, 2};
        TableWriter.StructWrite<Integer> write = (struct, bytes, offset) -> bytes[offset] = struct.byteValue();
        assertThrows(
            IndexOutOfBoundsException.class,
            () -> writer.setStructs(0, structs, Integer.BYTES, Integer.BYTES, write)
        );
    }

    @Example
    void fields_shorter_than_those_sized_throw_IllegalStateException_from_bytes() {
        var sizer = new TableSizer(1);
        sizer.addString("ab");
        var writer = sizer.writer();
        writer.setString(0, "a");

        var exception = assertThrows(IllegalStateException.class, writer::bytes);
        assertEquals("The fields wrote 9 of the table's 10 bytes", exception.getMessage());
    }

    @Example
    void negative_counts_struct_sizes_below_1_and_alignments_other_than_1_2_4_or_8_are_rejected_and_change_nothing() {
        var sizer = new TableSizer(1);
        assertThrows(IllegalArgumentException.class, () -> sizer.startTable(-1));
        assertThrows(IllegalArgumentException.class, () -> sizer.addFloat32Array(-1));
        assertThrows(IllegalArgumentException.class, () -> sizer.addStruct(-1));
        assertThrows(IllegalArgumentException.class, () -> sizer.addStructArray(-1, 4, 4));
        assertThrows(IllegalArgumentException.class, () -> sizer.addStructArray(1, 0, 1));
        assertThrows(IllegalArgumentException.class, () -> sizer.addStructArray(1, 4, 16));
        assertThrows(IllegalArgumentException.class, () -> sizer.addTable(3));
        assertThrows(IllegalArgumentException.class, () -> sizer.addVector(-1));
        sizer.addVector(0);

        var writer = sizer.writer();
        assertThrows(IllegalArgumentException.class, () -> writer.startTable(-1));
        assertThrows(IllegalArgumentException.class, () -> writer.setStructArray(0, new byte[5], 2, 1));
        assertThrows(IllegalArgumentException.class, () -> writer.setStructArray(0, new byte[4], 0, 1));
        assertThrows(
            IllegalArgumentException.class,
            () -> writer.setStructs(0, new Integer[] {1}, 4, 3, (struct, bytes, offset) -> {})
        );
        assertThrows(IllegalArgumentException.class, () -> writer.setTable(0, 3));
        assertThrows(IllegalArgumentException.class, () -> writer.setVector(0, -1));
        writer.setVector(0, 0);

        // The table's offset and the vector's count
        assertArrayEquals(new byte[] {4, 0, 0, 0, 0, 0, 0, 0}, writer.bytes());
    }

    // Indices and item ends outside their table, which leave the table's bytes undefined but within
    // the writer's

    @Property
    void any_index_of_a_field_or_item_of_an_end_writes_within_the_bytes_or_throws_IndexOutOfBoundsException(
        @ForAll("inPlaceTables") TableDescription table,
        @ForAll @IntRange(max = 1_000) int call,
        @ForAll("positions") int position
    ) {
        var length = Tables.write(table).length;
        var counter = new IndexReplacer(-1, 0);
        Tables.write(Tables.size(table).writer(), table, counter);
        Assume.that(counter.calls() > 0);

        var replacer = new IndexReplacer(call % counter.calls(), position);
        var writer = Tables.size(table).writer();
        byte[] bytes;
        try {
            bytes = Tables.write(writer, table, replacer);
        } catch (IndexOutOfBoundsException exception) {
            return; // A slot past the bytes
        }

        assertEquals(length, bytes.length);
    }

    @Provide
    Arbitrary<Integer> positions() {
        return Arbitraries.oneOf(Arbitraries.integers().between(-4, 64), Arbitraries.integers());
    }

    /**
     * Passes each index of a field and item of an end through, but replaces that of call
     * {@code call} of them by {@code value}, as a caller other than the generated code might
     */
    private static final class IndexReplacer implements IntUnaryOperator {
        private final int call;
        private final int value;
        private int calls = 0;

        IndexReplacer(int call, int value) {
            this.call = call;
            this.value = value;
        }

        /** {@return the indices and items passed through} */
        int calls() {
            return calls;
        }

        @Override
        public int applyAsInt(int index) {
            return calls++ == call ? value : index;
        }
    }

    @Example
    void a_struct_of_0_bytes_is_sized_and_written_as_TableBuilder_builds_it() {
        var sizer = new TableSizer(2);
        sizer.addStruct(0);
        sizer.addUint8();
        var writer = sizer.writer();
        writer.setStruct(0, new byte[0]);
        writer.setUint8(1, (short) 7);

        var builder = new TableBuilder(2);
        builder.setStruct(new byte[0]);
        builder.setUint8((short) 7);
        assertArrayEquals(builder.bytes(), writer.bytes());
    }

    @Example
    void values_convert_as_the_package_documentations_table_states() {
        // An unsigned setter writes its value's low bits, as a cast would
        assertConverted(TableSizer::addUint8, writer -> writer.setUint8(0, (short) 300),
            builder -> builder.setUint8((short) 300), 44);
        assertConverted(TableSizer::addUint8, writer -> writer.setUint8(0, (short) -1),
            builder -> builder.setUint8((short) -1), 0xff);
        assertConverted(TableSizer::addUint32, writer -> writer.setUint32(0, 1L << 32 | 7),
            builder -> builder.setUint32(1L << 32 | 7), 7, 0, 0, 0);
        assertConverted(sizer -> sizer.addUint8Array(2), writer -> writer.setUint8Array(0, new short[] {300, -1}),
            builder -> builder.setUint8Array(new short[] {300, -1}), 2, 0, 0, 0, 44, 0xff);

        // The cast to float makes a double too large for a float32 infinity
        assertConverted(TableSizer::addFloat32, writer -> writer.setFloat32(0, (float) 1e300),
            builder -> builder.setFloat32((float) 1e300), 0, 0, 0x80, 0x7f);

        // A lone surrogate is written as U+FFFD
        assertConverted(sizer -> sizer.addString("\ud800"), writer -> writer.setString(0, "\ud800"),
            builder -> builder.setString("\ud800"), 3, 0, 0, 0, 0xef, 0xbf, 0xbd);
    }

    /**
     * Asserts that a table of one field, sized and written by the writer and built by the builder,
     * holds {@code expected} after its offset
     */
    private static void assertConverted(
        Consumer<TableSizer> add,
        Consumer<TableWriter> set,
        Consumer<TableBuilder> build,
        int... expected
    ) {
        var sizer = new TableSizer(1);
        add.accept(sizer);
        var writer = sizer.writer();
        set.accept(writer);

        var builder = new TableBuilder(1);
        build.accept(builder);
        var bytes = builder.bytes();
        assertArrayEquals(bytes, writer.bytes());

        var value = new byte[expected.length];
        for (var i = 0; i < expected.length; i++) {
            value[i] = (byte) expected[i];
        }
        assertArrayEquals(value, Arrays.copyOfRange(bytes, Integer.BYTES, bytes.length));
    }

    @Property
    void unsigned_arrays_are_written_as_their_elements_low_bits_before_the_next_field(
        @ForAll @Size(max = 40) long[] values,
        @ForAll OffsetType offsetType
    ) {
        var layout = TableLayout.builder().offsetType(offsetType).build();
        for (var type : List.of(ScalarType.UINT8, ScalarType.UINT16, ScalarType.UINT32)) {
            var sizer = new TableSizer(2, layout);
            type.addArray(sizer, values.length);
            sizer.addInt32();
            var writer = sizer.writer();
            type.setArray(writer, 0, values);
            writer.setInt32(1, 0x12345678);
            var bytes = writer.bytes();

            // Each element's low bytes, as the JDK's buffer puts them
            var expected = java.nio.ByteBuffer.allocate(values.length * type.size()).order(ByteOrder.LITTLE_ENDIAN);
            for (var value : values) {
                type.put(expected, value);
            }

            var buffer = new ByteBuffer(bytes, layout);
            var elements = buffer.rootOffset(0) + offsetType.size();
            assertArrayEquals(expected.array(), Arrays.copyOfRange(bytes, elements, elements + expected.capacity()));
            assertEquals(0x12345678, buffer.getInt32(buffer.rootOffset(1)));
        }
    }

    @Provide
    Arbitrary<TableDescription> inPlaceTables() {
        return Arbitraries.oneOf(Descriptions.tables(), Descriptions.nestingTables())
            .map(Tables::nestedInPlace)
            .filter(Descriptions::fitsItsLayout);
    }

    @Provide
    Arbitrary<TableDescription> narrowInPlaceTables() {
        return Descriptions.narrowTables().map(Tables::nestedInPlace).filter(Descriptions::fitsItsLayout);
    }

    @Provide
    Arbitrary<FieldDescription> values() {
        return Descriptions.values();
    }

    @Provide
    Arbitrary<FieldDescription> structArrays() {
        return Descriptions.structArrays();
    }

    /** {@return fields after which a table's length doesn't depend on where they start} */
    @Provide
    Arbitrary<FieldDescription> unpaddedFields() {
        return Descriptions.values().filter(field -> Tables.arrayAlignment(field).orElse(1) == 1);
    }

    @Provide
    Arbitrary<OffsetType> narrowTypes() {
        return Arbitraries.of(OffsetType.UINT8, OffsetType.UINT16);
    }

    /**
     * {@return strings of any UTF-16 code units}: ASCII, the rest of the Basic Multilingual Plane,
     * and surrogates, which pair up or stand alone, and strings of ASCII alone, which Java stores
     * as Latin-1. They're up to 128 units long, past the length from which the sizer encodes a
     * string rather than counting it.
     */
    @Provide
    Arbitrary<String> utf16Strings() {
        var units = Arbitraries.frequencyOf(
            Tuple.of(2, Arbitraries.chars().range('\u0000', '\u007f')),
            Tuple.of(2, Arbitraries.chars().range('\u0080', '￿')),
            Tuple.of(1, Arbitraries.chars().range(Character.MIN_HIGH_SURROGATE, Character.MAX_HIGH_SURROGATE)),
            Tuple.of(1, Arbitraries.chars().range(Character.MIN_LOW_SURROGATE, Character.MAX_LOW_SURROGATE))
        );

        return Arbitraries.oneOf(
            units.list().ofMaxSize(128).map(TableWriterTest::stringOf),
            Arbitraries.strings().ascii().ofMaxLength(128)
        );
    }

    private static String stringOf(List<Character> units) {
        var string = new StringBuilder(units.size());
        units.forEach(string::append);

        return string.toString();
    }

    /**
     * {@return the bytes of the table's fields and then the structs}, written with
     * {@link TableWriter#setStructs} from each struct's values, as the generated code writes them
     */
    private static byte[] writeInPlace(TableDescription table, StructArrayField structs) {
        var writer = Tables.size(table.withField(structs)).writer();
        Tables.writeFields(writer, table);

        // Each struct is its index in the items, whose values the writer of a struct writes
        var indices = new Integer[structs.items().size()];
        Arrays.setAll(indices, i -> i);
        writer.setStructs(
            table.fields().size(),
            indices,
            StructBytes.size(structs.fields()),
            StructBytes.alignment(structs.fields()),
            (index, bytes, offset) -> {
                StructBytes.write(structs.fields(), structs.items().get(index), bytes, offset);
            }
        );

        return writer.bytes();
    }

    /** {@return a {@code uint8[]} of {@code length} zeros} */
    private static FieldDescription bytesField(int length) {
        return new ArrayField(ScalarType.UINT8, new long[length]);
    }

    /** {@return the bytes that {@code write} returns, in hex, or the class of the exception that it throws} */
    private static String outcomeOf(Supplier<byte[]> write) {
        try {
            return HexFormat.of().formatHex(write.get());
        } catch (RuntimeException exception) {
            return exception.getClass().getName();
        }
    }

    /** Asserts that both throw an {@link IllegalArgumentException} with the same message */
    private static void assertRejectedAlike(Executable builder, Executable sizer) {
        var expected = assertThrows(IllegalArgumentException.class, builder);
        var actual = assertThrows(IllegalArgumentException.class, sizer);

        assertEquals(expected.getMessage(), actual.getMessage());
    }
}
