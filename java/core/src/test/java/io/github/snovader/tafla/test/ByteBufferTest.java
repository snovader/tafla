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

public class ByteBufferTest {
    @Property
    void a_buffer_reads_back_the_value_that_its_setter_writes(
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

    @Property
    void unsigned_arrays_write_low_bits_and_preserve_the_following_field(
        @ForAll @Size(max = 65) long[] values,
        @ForAll OffsetType offsetType
    ) {
        var layout = TableLayout.builder().offsetType(offsetType).build();
        for (var type : List.of(ScalarType.UINT8, ScalarType.UINT16, ScalarType.UINT32)) {
            var length = 1 + offsetType.size() + values.length * type.size() + Integer.BYTES;
            var buffer = new ByteBuffer(new byte[length], layout);
            buffer.setInt8((byte) 42);
            type.writeArray(buffer, values);
            buffer.setInt32(0x12345678);

            var expectedBytes = java.nio.ByteBuffer.allocate(length).order(ByteOrder.LITTLE_ENDIAN);
            expectedBytes.put((byte) 42);
            for (int b = 0; b < offsetType.size(); b++) {
                expectedBytes.put((byte) (values.length >>> (b * Byte.SIZE)));
            }

            var expectedValues = new long[values.length];
            var mask = (1L << (type.size() * Byte.SIZE)) - 1;
            for (int i = 0; i < values.length; i++) {
                type.put(expectedBytes, values[i]);
                expectedValues[i] = values[i] & mask;
            }
            expectedBytes.putInt(0x12345678);

            assertArrayEquals(expectedBytes.array(), buffer.bytes());
            assertArrayEquals(expectedValues, type.readArray(buffer, 1));
        }
    }

    private static void assertReadsBack(FieldDescription field, LayoutDescription layout) {
        var bytes = new byte[(int) Descriptions.maxLength(field, layout.width())];
        var buffer = new ByteBuffer(bytes, layout.tableLayout());

        write(buffer, field);
        assertEquals(field, readAtStart(buffer, field));

        // After a count of 1 or 2 bytes, the elements aren't aligned
        if (field instanceof ArrayField array) {
            assertArrayEquals(array.values(), array.type().readArrayView(buffer, 0));
        }
    }

    /**
     * Writes a scalar, array, string, struct or array of structs from the buffer's position with
     * the buffer's own setter
     */
    private static void write(ByteBuffer buffer, FieldDescription field) {
        switch (field) {
            case ScalarField scalar -> scalar.type().write(buffer, scalar.value());
            case ArrayField array -> array.type().writeArray(buffer, array.values());
            case StringField string -> buffer.setString(string.value().getBytes(StandardCharsets.UTF_8));
            case StructField struct -> buffer.setBytes(StructBytes.pack(struct.types(), List.of(struct.values())));
            case StructArrayField array -> buffer.setStructArray(
                StructBytes.pack(array.fields(), array.items()),
                StructBytes.size(array.fields())
            );
            default -> throw new IllegalArgumentException("Not a value: " + field);
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

    // The write setters, which write fixed-size records, as the README and the benchmarks do.
    // TableBuilder writes through TableWriter, so these are their only writes.

    @Property
    void each_setter_writes_its_value_little_endian_after_the_previous_one(
        @ForAll OffsetType offsetType,
        @ForAll @Size(max = 8) List<@From("writes") Write> writes
    ) {
        // Every count, offset and length of the writes fits in a uint8
        var expected = java.nio.ByteBuffer.allocate(4096).order(ByteOrder.LITTLE_ENDIAN);
        for (var write : writes) {
            write.put(expected, offsetType.size());
        }

        // Bytes that start as 0xa5, so that padding and anything past the writes show, and one more
        var length = expected.position();
        var bytes = new byte[length + 1];
        Arrays.fill(bytes, (byte) 0xa5);
        var buffer = new ByteBuffer(bytes, TableLayout.builder().offsetType(offsetType).build());
        for (var write : writes) {
            write.apply(buffer);
        }

        var expectedBytes = Arrays.copyOf(expected.array(), length + 1);
        expectedBytes[length] = (byte) 0xa5;
        assertArrayEquals(expectedBytes, bytes, writes::toString);
    }

    @Provide
    Arbitrary<Write> writes() {
        var items = Arbitraries.bytes().array(byte[].class).ofMaxSize(8).list().ofMaxSize(3);

        return Arbitraries.oneOf(
            Descriptions.values().map(Value::new),
            Arbitraries.integers().between(0, 7).map(Padding::new),
            Arbitraries.longs().between(0, 0xff).map(Offset::new),
            items.map(VectorOfBytes::new)
        );
    }

    /** A call of one of {@link ByteBuffer}'s write setters */
    private sealed interface Write {
        void apply(ByteBuffer buffer);

        /** Puts the bytes that the call writes, with counts, offsets and ends of {@code width} bytes */
        void put(java.nio.ByteBuffer expected, int width);
    }

    /** A scalar, array, string, struct or array of structs, written with its type's setter */
    private record Value(FieldDescription field) implements Write {
        @Override
        public void apply(ByteBuffer buffer) {
            write(buffer, field);
        }

        @Override
        public void put(java.nio.ByteBuffer expected, int width) {
            switch (field) {
                case ScalarField scalar -> scalar.type().put(expected, scalar.value());
                case ArrayField array -> {
                    putUnsigned(expected, array.values().length, width);
                    for (var value : array.values()) {
                        array.type().put(expected, value);
                    }
                }
                case StringField string -> {
                    var utf8 = string.value().getBytes(StandardCharsets.UTF_8);
                    putUnsigned(expected, utf8.length, width);
                    expected.put(utf8);
                }
                case StructField struct -> putStruct(expected, struct.types(), struct.values());
                case StructArrayField array -> {
                    putUnsigned(expected, array.items().size(), width);
                    for (var item : array.items()) {
                        putStruct(expected, array.fields(), item);
                    }
                }
                default -> throw new IllegalArgumentException("Not a value: " + field);
            }
        }

        private static void putStruct(java.nio.ByteBuffer expected, List<ScalarType> types, List<Long> values) {
            for (int i = 0; i < types.size(); i++) {
                types.get(i).put(expected, values.get(i));
            }
        }
    }

    private record Padding(int length) implements Write {
        @Override
        public void apply(ByteBuffer buffer) {
            buffer.setPadding(length);
        }

        @Override
        public void put(java.nio.ByteBuffer expected, int width) {
            expected.put(new byte[length]);
        }
    }

    private record Offset(long offset) implements Write {
        @Override
        public void apply(ByteBuffer buffer) {
            buffer.setOffset((int) offset);
        }

        @Override
        public void put(java.nio.ByteBuffer expected, int width) {
            putUnsigned(expected, offset, width);
        }
    }

    private record VectorOfBytes(List<byte[]> items) implements Write {
        @Override
        public void apply(ByteBuffer buffer) {
            buffer.setVector(items.toArray(byte[][]::new));
        }

        @Override
        public void put(java.nio.ByteBuffer expected, int width) {
            putUnsigned(expected, items.size(), width);

            // Each end counts from the count
            long end = (items.size() + 1L) * width;
            for (var item : items) {
                end += item.length;
                putUnsigned(expected, end, width);
            }

            for (var item : items) {
                expected.put(item);
            }
        }

        @Override
        public String toString() {
            return "VectorOfBytes" + items.stream().map(Arrays::toString).toList();
        }
    }

    /** Puts an unsigned count, offset or end of {@code width} bytes */
    private static void putUnsigned(java.nio.ByteBuffer expected, long value, int width) {
        switch (width) {
            case Byte.BYTES -> expected.put((byte) value);
            case Short.BYTES -> expected.putShort((short) value);
            default -> expected.putInt((int) value);
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

    @Property
    void should_round_trip_vector(@ForAll @Size(max = 10) List<byte[]> items) {
        var vector = items.toArray(byte[][]::new);
        var byteLength = Integer.BYTES + items.stream().mapToInt(item -> Integer.BYTES + item.length).sum();
        var buffer = ByteBuffer.allocate(byteLength);
        buffer.setVector(vector);

        int[] offsets = buffer.getVectorOffsets(0);
        for (int i = 0; i < vector.length; i++) {
            assertArrayEquals(vector[i], Arrays.copyOfRange(buffer.bytes(), offsets[i], offsets[i] + vector[i].length));
            assertEquals(offsets[i], buffer.getVectorOffset(0, i));
        }
        assertEquals(vector.length, buffer.getVectorLength(0));
        assertArrayEquals(vector, buffer.getVector(0));
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
        var layout = TableLayout.builder().offsetType(OffsetType.UINT16).build();
        var buffer = new ByteBuffer(new byte[Short.BYTES + 6], layout);
        buffer.setStructArray(new byte[] {1, 0, 2, 3, 0, 4}, 3);

        assertArrayEquals(new byte[] {2, 0, 1, 0, 2, 3, 0, 4}, buffer.bytes());

        // Structs of a uint16 and a uint8
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
    void should_reject_a_struct_size_below_1_or_bytes_that_arent_whole_structs_and_write_nothing() {
        var buffer = ByteBuffer.allocate(Integer.BYTES + 4);

        var exception = assertThrows(IllegalArgumentException.class, () -> buffer.setStructArray(new byte[5], 2));
        assertEquals("5 bytes aren't a whole number of 2-byte structs", exception.getMessage());
        exception = assertThrows(IllegalArgumentException.class, () -> buffer.setStructArray(new byte[4], 0));
        assertEquals("A struct's size must be positive, not 0", exception.getMessage());
        assertThrows(IllegalArgumentException.class, () -> buffer.setStructArray(new byte[4], -2));
        assertThrows(IndexOutOfBoundsException.class, () -> buffer.setStructArray(new byte[6], 2));
        assertArrayEquals(new byte[8], buffer.bytes());

        assertThrows(IllegalArgumentException.class, () -> buffer.getStructArray(0, 0));
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
        var buffer = ByteBuffer.allocate(Integer.BYTES + 2 * Float.BYTES);
        buffer.setFloat32Array(new float[] {1, 2});
        var view = buffer.getFloat32ArrayView(0);

        assertTrue(view.isReadOnly());
        assertThrows(ReadOnlyBufferException.class, () -> view.put(0, 3));

        // 1.0f is 0x3f800000, and 0x40800000 is 4.0f
        buffer.bytes()[Integer.BYTES + 3] = 0x40;
        assertEquals(4, view.get(0));
        assertEquals(2, view.get(1));
    }

    @Example
    void should_reject_a_uint16_size_beyond_65535_and_write_nothing() {
        var layout = TableLayout.builder().offsetType(OffsetType.UINT16).build();
        var buffer = new ByteBuffer(new byte[Short.BYTES + 65536], layout);

        var exception = assertThrows(IllegalArgumentException.class, () -> buffer.setInt8Array(new byte[65536]));
        assertEquals(
            "Can't write 65536 at 0: a uint16 count, length or item end holds at most 65535",
            exception.getMessage()
        );
        assertThrows(IllegalArgumentException.class, () -> buffer.setVector(new byte[][] {new byte[65536]}));

        buffer.setInt8Array(new byte[] {7});
        assertArrayEquals(new byte[] {1, 0, 7}, Arrays.copyOf(buffer.bytes(), 3));
    }

    @Example
    void should_reject_writes_past_the_end_and_write_nothing() {
        var buffer = ByteBuffer.allocate(Integer.BYTES + 2 * Float.BYTES);

        assertThrows(IndexOutOfBoundsException.class, () -> buffer.setFloat32Array(new float[3]));
        assertThrows(IndexOutOfBoundsException.class, () -> buffer.setUint8Array(new short[9]));
        assertThrows(IndexOutOfBoundsException.class, () -> buffer.setUint16Array(new int[5]));
        assertThrows(IndexOutOfBoundsException.class, () -> buffer.setUint32Array(new long[3]));
        assertThrows(IndexOutOfBoundsException.class, () -> buffer.setString(new byte[9]));
        assertThrows(IndexOutOfBoundsException.class, () -> buffer.setVector(new byte[][] {new byte[5]}));
        assertThrows(IndexOutOfBoundsException.class, () -> buffer.setPadding(13));
        assertArrayEquals(new byte[12], buffer.bytes());

        buffer.setFloat32Array(new float[] {1, 2});
        assertArrayEquals(new float[] {1, 2}, buffer.getFloat32Array(0));
        assertThrows(IndexOutOfBoundsException.class, () -> buffer.setInt8((byte) 1));
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
