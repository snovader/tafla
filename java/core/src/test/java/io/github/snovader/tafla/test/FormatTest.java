package io.github.snovader.tafla.test;

import io.github.snovader.tafla.test.FieldDescription.*;
import net.jqwik.api.*;

import java.nio.*;
import java.nio.charset.*;
import java.util.*;

import static org.junit.jupiter.api.Assertions.*;

/**
 * Tables follow the format's rules as {@code SPEC.md} states them. The properties read the bytes
 * that a builder writes with the JDK's little-endian buffer rather than tafla's reader, and check
 * the fields of the root table. A nested table holds the bytes that its builder writes on its own,
 * so the rules hold for nested tables as they do for the roots of the same descriptions.
 */
public class FormatTest {
    @Property
    void a_sized_tables_size_slot_holds_its_byte_length(@ForAll("sizedTables") TableDescription table) {
        var bytes = Tables.build(table).bytes();

        assertEquals(bytes.length, unsigned(bytes, 0, table.layout().width()));
    }

    @Property
    void a_versioned_tables_version_slot_holds_its_version(@ForAll("versionedTables") TableDescription table) {
        var bytes = Tables.build(table).bytes();
        var layout = table.layout();
        var slot = layout.sized() ? layout.width() : 0;

        assertEquals(table.version().orElseThrow(), unsigned(bytes, slot, layout.width()));
    }

    @Property
    void root_offsets_lie_within_the_table_in_field_order(@ForAll("tables") TableDescription table) {
        var bytes = Tables.build(table).bytes();
        var layout = table.layout();
        var fields = table.fields().size();

        // A table nested with no fields and no header has no bytes, so it can start where the table ends
        long previous = layout.headerSize() + (long) fields * layout.width();
        for (int index = 0; index < fields; index++) {
            var offset = rootOffset(bytes, layout, index);

            assertTrue(
                previous <= offset && offset <= bytes.length,
                String.format("Offset %d of field %d isn't within [%d, %d]", offset, index, previous, bytes.length)
            );
            previous = offset;
        }
    }

    @Property
    void every_array_starts_at_a_multiple_of_its_alignment_from_the_start_of_its_table(
        @ForAll("tables") TableDescription table
    ) {
        var bytes = Tables.build(table).bytes();
        var layout = table.layout();

        for (int index = 0; index < table.fields().size(); index++) {
            var alignment = Tables.arrayAlignment(table.fields().get(index));
            if (alignment.isPresent()) {
                var elements = rootOffset(bytes, layout, index) + layout.width();
                assertEquals(0, elements % alignment.getAsInt(), "The elements of field " + index);
            }
        }
    }

    @Property
    void multi_byte_values_are_little_endian(@ForAll("tables") TableDescription table) {
        var bytes = Tables.build(table).bytes();
        var layout = table.layout();

        for (int index = 0; index < table.fields().size(); index++) {
            var field = table.fields().get(index);
            var offset = rootOffset(bytes, layout, index);

            littleEndian(field, layout.width()).ifPresent(
                expected -> assertArrayEquals(expected, slice(bytes, offset, expected.length), field.toString())
            );
        }
    }

    // Tables of many nested tables, so fewer of them
    @Property(tries = 250)
    void a_nested_table_holds_the_bytes_of_its_builder_at_a_multiple_of_its_alignment(
        @ForAll("nestingTables") TableDescription table
    ) {
        var bytes = Tables.build(table).bytes();
        var layout = table.layout();

        for (int index = 0; index < table.fields().size(); index++) {
            if (table.fields().get(index) instanceof TableField field) {
                var offset = rootOffset(bytes, layout, index);
                var nested = Tables.build(field.table()).bytes();

                assertEquals(0, offset % Tables.alignmentOf(field), "The offset of field " + index);
                assertArrayEquals(nested, slice(bytes, offset, nested.length), "The bytes of field " + index);
            }
        }
    }

    // Tables of many nested tables, so fewer of them
    @Property(tries = 250)
    void a_vector_holds_its_item_count_then_each_items_end_and_then_the_items_bytes(
        @ForAll("nestingTables") TableDescription table
    ) {
        var bytes = Tables.build(table).bytes();
        var layout = table.layout();

        for (int index = 0; index < table.fields().size(); index++) {
            if (table.fields().get(index) instanceof VectorField vector) {
                assertVector(bytes, rootOffset(bytes, layout, index), layout.width(), vector);
            }
        }
    }

    /**
     * Asserts that the vector at {@code position} holds its count, then the end of each item from
     * that position, and then each item's bytes, starting where the item before it ends
     */
    private static void assertVector(byte[] bytes, int position, int width, VectorField vector) {
        var items = vector.items();
        assertEquals(items.size(), unsigned(bytes, position, width), "The count of " + vector);

        // Item 0 starts after the count and the ends
        var start = (items.size() + 1) * width;
        for (int i = 0; i < items.size(); i++) {
            var item = items.get(i);
            var itemBytes = Tables.build(item.table()).bytes();
            var end = start + itemBytes.length;

            assertEquals(end, unsigned(bytes, position + (i + 1) * width, width), "The end of " + item);
            assertArrayEquals(itemBytes, slice(bytes, position + start, itemBytes.length), "The bytes of " + item);
            start = end;
        }
    }

    @Provide
    Arbitrary<TableDescription> tables() {
        return Descriptions.tables();
    }

    @Provide
    Arbitrary<TableDescription> nestingTables() {
        return Descriptions.nestingTables();
    }

    @Provide
    Arbitrary<TableDescription> sizedTables() {
        return Descriptions.tables(LayoutDescription::sized);
    }

    @Provide
    Arbitrary<TableDescription> versionedTables() {
        return Descriptions.tables(LayoutDescription::versioned);
    }

    /**
     * {@return the bytes of a scalar, bool, array, array of bools, string, struct, array of structs
     * or vector of strings, as the JDK writes them little-endian}, or nothing for a nested table or
     * vector
     */
    private static Optional<byte[]> littleEndian(FieldDescription field, int width) {
        return switch (field) {
            case ScalarField scalar -> Optional.of(littleEndian(scalar));
            case BoolField bool -> Optional.of(new byte[] {(byte) bool.uint8()});
            case ArrayField array -> Optional.of(littleEndian(array, width));
            case BoolArrayField array -> Optional.of(littleEndian(array, width));
            case StringField string -> Optional.of(littleEndian(string, width));
            case StructField struct -> Optional.of(littleEndian(struct));
            case StructArrayField array -> Optional.of(littleEndian(array, width));
            case TableField ignored -> Optional.empty();
            case VectorField ignored -> Optional.empty();
            case StringVectorField vector -> Optional.of(littleEndian(vector, width));
        };
    }

    private static byte[] littleEndian(ScalarField scalar) {
        var bytes = new byte[scalar.type().size()];
        scalar.type().put(littleEndianView(bytes), scalar.value());

        return bytes;
    }

    /** {@return an array's count and then its elements} */
    private static byte[] littleEndian(ArrayField array, int width) {
        var bytes = new byte[width + array.values().length * array.type().size()];

        var view = littleEndianView(bytes);
        putUnsigned(view, width, array.values().length);
        for (var value : array.values()) {
            array.type().put(view, value);
        }

        return bytes;
    }

    /** {@return an array of bools' count and then a byte of 1 or 0 for each, unpadded} */
    private static byte[] littleEndian(BoolArrayField array, int width) {
        var bytes = new byte[width + array.values().length];

        var view = littleEndianView(bytes);
        putUnsigned(view, width, array.values().length);
        for (var value : array.values()) {
            view.put((byte) (value ? 1 : 0));
        }

        return bytes;
    }

    /** {@return a string's count and then its UTF-8} */
    private static byte[] littleEndian(StringField string, int width) {
        var utf8 = string.value().getBytes(StandardCharsets.UTF_8);
        var bytes = new byte[width + utf8.length];

        var view = littleEndianView(bytes);
        putUnsigned(view, width, utf8.length);
        view.put(utf8);

        return bytes;
    }

    /**
     * {@return a vector of strings' count, then the end of each string, counted from the vector's
     * position, and then the strings' UTF-8, one after another}
     */
    private static byte[] littleEndian(StringVectorField vector, int width) {
        var strings = vector.values().stream().map(string -> string.getBytes(StandardCharsets.UTF_8)).toList();
        var ends = (strings.size() + 1) * width;
        var bytes = new byte[ends + strings.stream().mapToInt(utf8 -> utf8.length).sum()];

        var view = littleEndianView(bytes);
        putUnsigned(view, width, strings.size());
        var end = ends;
        for (var utf8 : strings) {
            end += utf8.length;
            putUnsigned(view, width, end);
        }
        strings.forEach(view::put);

        return bytes;
    }

    /** {@return a struct's scalars, one after another} */
    private static byte[] littleEndian(StructField struct) {
        var bytes = new byte[StructBytes.size(struct.types())];

        var view = littleEndianView(bytes);
        for (var scalar : struct.fields()) {
            scalar.type().put(view, scalar.value());
        }

        return bytes;
    }

    /** {@return an array of structs' count and then each struct's scalars, one after another} */
    private static byte[] littleEndian(StructArrayField array, int width) {
        var types = array.fields();
        var bytes = new byte[width + array.items().size() * StructBytes.size(types)];

        var view = littleEndianView(bytes);
        putUnsigned(view, width, array.items().size());
        for (var values : array.items()) {
            for (int i = 0; i < types.size(); i++) {
                types.get(i).put(view, values.get(i));
            }
        }

        return bytes;
    }

    private static ByteBuffer littleEndianView(byte[] bytes) {
        return ByteBuffer.wrap(bytes).order(ByteOrder.LITTLE_ENDIAN);
    }

    private static void putUnsigned(ByteBuffer bytes, int width, int value) {
        switch (width) {
            case Byte.BYTES -> bytes.put((byte) value);
            case Short.BYTES -> bytes.putShort((short) value);
            default -> bytes.putInt(value);
        }
    }

    /** {@return the offset of a root field, read as the format describes it} */
    private static int rootOffset(byte[] bytes, LayoutDescription layout, int index) {
        return (int) unsigned(bytes, layout.headerSize() + index * layout.width(), layout.width());
    }

    /** {@return the little-endian unsigned integer of {@code width} bytes at {@code position}} */
    private static long unsigned(byte[] bytes, int position, int width) {
        var view = littleEndianView(bytes);

        return switch (width) {
            case Byte.BYTES -> Byte.toUnsignedLong(view.get(position));
            case Short.BYTES -> Short.toUnsignedLong(view.getShort(position));
            default -> Integer.toUnsignedLong(view.getInt(position));
        };
    }

    /** {@return a copy of {@code length} bytes from {@code position}, which must be within the bytes} */
    private static byte[] slice(byte[] bytes, int position, int length) {
        Objects.checkFromIndexSize(position, length, bytes.length);
        return Arrays.copyOfRange(bytes, position, position + length);
    }
}
