package io.github.snovader.tafla.test;

import io.github.snovader.tafla.*;
import io.github.snovader.tafla.test.FieldDescription.*;
import net.jqwik.api.*;
import net.jqwik.api.constraints.*;

import java.nio.ByteOrder;
import java.nio.ReadOnlyBufferException;
import java.nio.charset.*;
import java.util.*;

import static org.junit.jupiter.api.Assertions.*;

/**
 * Tests of {@link ByteBuffer}'s reads, from bytes that the JDK's own little-endian buffer writes as
 * {@code SPEC.md} lays them out, so that they don't depend on tafla's writers.
 */
public class ByteBufferTest {
    @Property
    void a_buffer_reads_back_any_value_written_little_endian_at_its_position(
        @ForAll("values") FieldDescription field,
        @ForAll("layouts") LayoutDescription layout
    ) {
        assertReadsBack(field, layout);
    }

    @Property(generation = GenerationMode.EXHAUSTIVE)
    void a_buffer_reads_back_the_limits_of_every_type_as_scalars_and_in_arrays(@ForAll("limits") ScalarField limit) {
        var layout = LayoutDescription.headerless(OffsetType.UINT32);

        assertReadsBack(limit, layout);
        assertReadsBack(new ArrayField(limit.type(), new long[] {limit.value()}), layout);
    }

    private static void assertReadsBack(FieldDescription field, LayoutDescription layout) {
        var bytes = new byte[(int) Descriptions.maxLength(field, layout.width())];
        put(littleEndian(bytes), field, layout.width());
        var buffer = new ByteBuffer(bytes, layout.tableLayout());

        assertEquals(field, readAtStart(buffer, field));

        // After a count of 1 or 2 bytes, the elements aren't aligned
        if (field instanceof ArrayField array) {
            assertArrayEquals(array.values(), array.type().readArrayView(buffer, 0));
        }
    }

    /**
     * {@return the scalar, array, string, struct or array of structs of the field's type at the
     * buffer's start}
     */
    private static FieldDescription readAtStart(ByteBuffer buffer, FieldDescription field) {
        return switch (field) {
            case ScalarField scalar -> new ScalarField(scalar.type(), scalar.type().read(buffer, 0));
            case ArrayField array -> new ArrayField(array.type(), array.type().readArray(buffer, 0));
            case StringField ignored -> new StringField(buffer.getString(0));
            case StructField struct -> StructBytes.read(buffer, 0, struct.types());
            case StructArrayField array -> StructBytes.readArray(buffer, 0, array.fields());
            default -> throw new IllegalArgumentException("Not a value: " + field);
        };
    }

    @Property
    void a_buffer_reads_every_item_of_any_vector(
        @ForAll @Size(max = 10) List<@Size(max = 20) byte[]> items,
        @ForAll OffsetType offsetType
    ) {
        // At most 244 bytes, which even uint8 ends hold
        var width = offsetType.size();
        var byteLength = (items.size() + 1) * width + items.stream().mapToInt(item -> item.length).sum();

        var bytes = new byte[byteLength];
        putVector(littleEndian(bytes), items, width);
        var buffer = new ByteBuffer(bytes, TableLayout.builder().offsetType(offsetType).build());

        var vector = items.toArray(byte[][]::new);
        int[] offsets = buffer.getVectorOffsets(0);
        for (int i = 0; i < vector.length; i++) {
            assertArrayEquals(vector[i], Arrays.copyOfRange(buffer.bytes(), offsets[i], offsets[i] + vector[i].length));
            assertEquals(offsets[i], buffer.getVectorOffset(0, i));
        }

        // In order, each item starting at the end of the one before it
        var start = buffer.getVectorItemsStart(0, buffer.getVectorLength(0));
        for (int i = 0; i < vector.length; i++) {
            assertEquals(offsets[i], start, "Item " + i);
            start = buffer.getVectorItemEnd(0, i, start);
            assertEquals(offsets[i] + vector[i].length, start, "The end of item " + i);
        }
        assertEquals(vector.length, buffer.getVectorLength(0));
        assertArrayEquals(vector, buffer.getVector(0));
    }

    // Values as SPEC.md lays them out, put with the JDK's buffer, with counts, offsets and ends of
    // `width` bytes

    /** {@return the JDK's little-endian buffer over the bytes, to put values into from their start} */
    private static java.nio.ByteBuffer littleEndian(byte[] bytes) {
        return java.nio.ByteBuffer.wrap(bytes).order(ByteOrder.LITTLE_ENDIAN);
    }

    /** Puts a scalar, array, string, struct or array of structs */
    private static void put(java.nio.ByteBuffer bytes, FieldDescription field, int width) {
        switch (field) {
            case ScalarField scalar -> scalar.type().put(bytes, scalar.value());
            case ArrayField array -> {
                putUnsigned(bytes, array.values().length, width);
                for (var value : array.values()) {
                    array.type().put(bytes, value);
                }
            }
            case StringField string -> {
                var utf8 = string.value().getBytes(StandardCharsets.UTF_8);
                putUnsigned(bytes, utf8.length, width);
                bytes.put(utf8);
            }
            case StructField struct -> bytes.put(StructBytes.pack(struct.types(), List.of(struct.values())));
            case StructArrayField array -> {
                putUnsigned(bytes, array.items().size(), width);
                bytes.put(StructBytes.pack(array.fields(), array.items()));
            }
            default -> throw new IllegalArgumentException("Not a value: " + field);
        }
    }

    /** Puts a vector's count, the end of each item, counted from the count, and then the items */
    private static void putVector(java.nio.ByteBuffer bytes, List<byte[]> items, int width) {
        putUnsigned(bytes, items.size(), width);

        long end = (items.size() + 1L) * width;
        for (var item : items) {
            end += item.length;
            putUnsigned(bytes, end, width);
        }

        for (var item : items) {
            bytes.put(item);
        }
    }

    /** Puts an unsigned count, offset or end of {@code width} bytes */
    private static void putUnsigned(java.nio.ByteBuffer bytes, long value, int width) {
        switch (width) {
            case Byte.BYTES -> bytes.put((byte) value);
            case Short.BYTES -> bytes.putShort((short) value);
            default -> bytes.putInt((int) value);
        }
    }

    @Provide
    Arbitrary<FieldDescription> values() {
        return Descriptions.values();
    }

    @Provide
    Arbitrary<LayoutDescription> layouts() {
        return Descriptions.layouts();
    }

    @Provide
    Arbitrary<ScalarField> limits() {
        var limits = new ArrayList<ScalarField>();
        for (var type : ScalarType.values()) {
            for (var value : Descriptions.limits(type)) {
                limits.add(new ScalarField(type, value));
            }
        }

        return Arbitraries.of(limits);
    }

    @Example
    void should_find_each_vector_item_from_its_ends_as_reading_every_item_finds_it() {
        var builder = new TableBuilder(1);
        builder.setVector(new byte[][] {{1}, {}, {2, 3}});
        var buffer = new ByteBuffer(builder.bytes());
        var vector = buffer.rootOffset(0);

        // The count at 4 and the three ends, 17, 17 and 19 from there, then the items from 20
        var countAndEnds = new byte[] {3, 0, 0, 0, 17, 0, 0, 0, 17, 0, 0, 0, 19, 0, 0, 0};
        assertArrayEquals(countAndEnds, Arrays.copyOfRange(buffer.bytes(), vector, 20));

        var offsets = new int[] {20, 21, 21};
        assertArrayEquals(offsets, buffer.getVectorOffsets(vector));
        for (int i = 0; i < offsets.length; i++) {
            assertEquals(offsets[i], buffer.getVectorOffset(vector, i), "Item " + i);
            assertEquals(offsets[i], buffer.getVectorItemOffset(vector, 3, i), "Item " + i + " of 3");
        }
        assertEquals(3, buffer.getVectorLength(vector));
        assertArrayEquals(new byte[][] {{1}, {}, {2, 3}}, buffer.getVector(vector));
    }

    @Property(generation = GenerationMode.EXHAUSTIVE)
    void should_reject_an_item_outside_a_vector_with_IndexOutOfBoundsException(
        @ForAll @IntRange(min = -2, max = 4) int index
    ) {
        Assume.that(index < 0 || index >= 2);

        var builder = new TableBuilder(1);
        builder.setVector(new byte[][] {{1}, {2, 3}});
        var buffer = new ByteBuffer(builder.bytes());

        var exception = assertThrows(IndexOutOfBoundsException.class, () -> buffer.getVectorOffset(4, index));
        assertEquals(String.format("Item %d is outside the vector of 2 at 4", index), exception.getMessage());
        exception = assertThrows(IndexOutOfBoundsException.class, () -> buffer.getVectorItemOffset(4, 2, index));
        assertEquals(String.format("Item %d is outside the vector of 2 at 4", index), exception.getMessage());
    }

    @Property(generation = GenerationMode.EXHAUSTIVE)
    void should_reject_a_vector_count_that_the_bytes_cant_hold_before_the_index(
        @ForAll @IntRange(min = 0, max = 3) int index
    ) {
        // A count of 3, whose ends need 12 bytes after it, where 4 are left
        var buffer = new ByteBuffer(new byte[] {3, 0, 0, 0, 8, 0, 0, 0});

        assertThrows(DecodeException.class, () -> buffer.getVectorOffset(0, index));
    }

    @Example
    void struct_views_are_read_only_little_endian_and_start_at_the_struct() {
        // A uint8 field, and then a struct of a uint16 and a float32
        var builder = new TableBuilder(2);
        builder.setUint8((short) 7);
        builder.setStruct(new byte[] {0x34, 0x12, 0, 0, (byte) 0xc0, 0x3f});
        var buffer = new ByteBuffer(builder.bytes());

        var struct = buffer.getStruct(buffer.rootOffset(1), 6);
        assertTrue(struct.isReadOnly());
        assertEquals(ByteOrder.LITTLE_ENDIAN, struct.order());
        assertEquals(6, struct.remaining());
        assertEquals(0x1234, struct.getShort(0));
        assertEquals(1.5f, struct.getFloat(2));
        assertThrows(ReadOnlyBufferException.class, () -> struct.put(0, (byte) 1));
    }

    @Example
    void struct_array_views_hold_every_struct_after_the_count() {
        // A uint16 count of 2, and then structs of a uint16 and a uint8
        var layout = TableLayout.builder().offsetType(OffsetType.UINT16).build();
        var buffer = new ByteBuffer(new byte[] {2, 0, 1, 0, 2, 3, 0, 4}, layout);

        var structs = buffer.getStructArray(0, 3);
        assertTrue(structs.isReadOnly());
        assertEquals(ByteOrder.LITTLE_ENDIAN, structs.order());
        assertEquals(2, structs.remaining() / 3);
        assertEquals(1, structs.getShort(0));
        assertEquals(2, structs.get(2));
        assertEquals(3, structs.getShort(3));
        assertEquals(4, structs.get(5));
    }

    @Example
    void should_reject_a_struct_size_below_1_or_below_0_for_a_single_struct() {
        var buffer = new ByteBuffer(new byte[Integer.BYTES + 4]);

        var exception = assertThrows(IllegalArgumentException.class, () -> buffer.getStructArray(0, 0));
        assertEquals("A struct's size must be positive, not 0", exception.getMessage());
        assertThrows(IllegalArgumentException.class, () -> buffer.getStructArray(0, -2));
        assertThrows(IllegalArgumentException.class, () -> buffer.getStruct(0, -1));
        assertEquals(0, buffer.getStruct(8, 0).remaining());
    }

    @Example
    void should_reject_a_size_beyond_the_buffer_before_allocating_for_it() {
        var buffer = new ByteBuffer(withSize(Integer.MAX_VALUE, 4));

        assertThrows(DecodeException.class, () -> buffer.getUint8Array(0));
        assertThrows(DecodeException.class, () -> buffer.getUint16Array(0));
        assertThrows(DecodeException.class, () -> buffer.getUint32Array(0));
        assertThrows(DecodeException.class, () -> buffer.getUint8ArrayView(0));
        assertThrows(DecodeException.class, () -> buffer.getUint32ArrayView(0));
        assertThrows(DecodeException.class, () -> buffer.getString(0));
        assertThrows(DecodeException.class, () -> buffer.getStructArray(0, 1));
        assertThrows(DecodeException.class, () -> buffer.getVectorLength(0));
        assertThrows(DecodeException.class, () -> buffer.getVectorOffset(0, 0));
        assertThrows(DecodeException.class, () -> buffer.getVectorOffsets(0));
        assertThrows(DecodeException.class, () -> buffer.getVector(0));
    }

    @Example
    void should_reject_a_size_whose_byte_length_overflows_an_int() {
        // 2^29 + 1 doubles take 2^32 + 8 bytes, which an int wraps around to 8
        var buffer = new ByteBuffer(withSize((1 << 29) + 1, Double.BYTES));

        assertThrows(DecodeException.class, () -> buffer.getFloat64Array(0));
        assertThrows(DecodeException.class, () -> buffer.getFloat64ArrayView(0));
        assertThrows(DecodeException.class, () -> buffer.getStructArray(0, Double.BYTES));
    }

    @Example
    void array_views_are_read_only_and_show_later_writes_to_the_bytes() {
        var bytes = new byte[Integer.BYTES + 2 * Float.BYTES];
        littleEndian(bytes).putInt(2).putFloat(1).putFloat(2);
        var buffer = new ByteBuffer(bytes);
        var view = buffer.getFloat32ArrayView(0);

        assertTrue(view.isReadOnly());
        assertThrows(ReadOnlyBufferException.class, () -> view.put(0, 3));

        // 1.0f is 0x3f800000, and 0x40800000 is 4.0f
        bytes[Integer.BYTES + 3] = 0x40;
        assertEquals(4, view.get(0));
        assertEquals(2, view.get(1));
    }

    @Example
    void should_read_the_size_of_a_sized_table_and_the_length_of_another() {
        var layout = TableLayout.builder().sized(true).build();
        var builder = new TableBuilder(1, layout);
        builder.setFloat64(1.5);
        var bytes = Arrays.copyOf(builder.bytes(), 20);

        assertEquals(16, new ByteBuffer(bytes, layout).getSize());
        assertEquals(20, new ByteBuffer(bytes).getSize());
    }

    @Example
    void should_reject_reading_the_version_of_an_unversioned_table() {
        var layout = TableLayout.builder().sized(true).build();
        var buffer = new ByteBuffer(new TableBuilder(0, layout).bytes(), layout);

        assertThrows(IllegalStateException.class, buffer::getVersion);
    }

    // A 32-bit size followed by `length` bytes
    private static byte[] withSize(int size, int length) {
        var bytes = new byte[Integer.BYTES + length];
        java.nio.ByteBuffer.wrap(bytes).order(ByteOrder.LITTLE_ENDIAN).putInt(0, size);
        return bytes;
    }
}
