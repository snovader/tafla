package io.github.snovader.tafla.test;

import io.github.snovader.tafla.*;
import net.jqwik.api.*;
import net.jqwik.api.constraints.*;

import java.nio.ByteOrder;
import java.nio.charset.*;
import java.util.*;

import static io.github.snovader.tafla.test.DecodeAssertions.*;
import static org.junit.jupiter.api.Assertions.*;

/** Reading untrusted bytes returns values or throws {@link DecodeException}, and nothing else */
public class DecodeExceptionTest {
    // Every length of each table, so fewer of them
    @Property(tries = 250)
    void reading_a_truncated_table_returns_the_described_values_or_throws_DecodeException(
        @ForAll("tables") TableDescription table
    ) {
        var bytes = Tables.build(table).bytes();
        var fields = table.fields();

        for (int length = 0; length < bytes.length; length++) {
            // A copy, so that no read can reach the bytes after the cut
            var reader = TableReader.lenient(Arrays.copyOf(bytes, length), table.layout());

            assertValuesOrDecodeException(() -> assertEquals(table.version(), reader.readHeader()));
            for (int index = 0; index < fields.size(); index++) {
                var field = fields.get(index);
                var fieldIndex = index;

                assertValuesOrDecodeException(() -> assertEquals(field, reader.readRootField(fieldIndex, field)));
            }
        }
    }

    @Property
    void reading_a_table_with_overwritten_bytes_returns_values_or_throws_DecodeException(
        @ForAll("tables") TableDescription table,
        @ForAll @IntRange(min = 0, max = Integer.MAX_VALUE) int at,
        @ForAll @Size(min = 1, max = 4) byte[] overwrite
    ) {
        var bytes = Tables.build(table).bytes();
        Assume.that(bytes.length > 0);

        // Any position in the table, and as many of the bytes as fit after it
        var position = at % bytes.length;
        System.arraycopy(overwrite, 0, bytes, position, Math.min(overwrite.length, bytes.length - position));

        var reader = TableReader.lenient(bytes, table.layout());
        var fields = table.fields();

        assertValuesOrDecodeException(reader::readHeader);
        for (int index = 0; index < fields.size(); index++) {
            var fieldIndex = index;
            assertValuesOrDecodeException(() -> reader.readRootField(fieldIndex, fields.get(fieldIndex)));
        }
    }

    @Property
    void reading_any_bytes_with_any_getter_returns_a_value_or_throws_DecodeException(
        @ForAll @Size(max = 64) byte[] bytes,
        @ForAll("layouts") LayoutDescription layout,
        @ForAll("getters") Getter getter,
        @ForAll("positions") Position position
    ) {
        var buffer = new ByteBuffer(bytes, layout.tableLayout());

        assertValuesOrDecodeException(buffer::getSize);
        if (layout.versioned()) {
            assertValuesOrDecodeException(buffer::getVersion);
        }

        assertValuesOrDecodeException(() -> getter.read().at(buffer, position.of(buffer)));
    }

    @Property
    void finding_a_vector_item_from_any_count_finds_it_within_the_bytes_or_throws_DecodeException(
        @ForAll @Size(max = 64) byte[] bytes,
        @ForAll("layouts") LayoutDescription layout,
        @ForAll @IntRange(min = 0, max = 72) int position,
        @ForAll @IntRange(min = 1) int count,
        @ForAll @IntRange(min = 0, max = 20) int at
    ) {
        // A count other than the vector's reads other bytes as the item's ends
        var buffer = new ByteBuffer(bytes, layout.tableLayout());
        var index = Math.min(at, count - 1);

        assertValuesOrDecodeException(() -> {
            var item = buffer.getVectorItemOffset(position, count, index);
            assertTrue(item >= position && item <= bytes.length, () -> "Item " + index + " at " + item);
        });
    }

    @Property
    void a_scalar_read_outside_the_bytes_throws_DecodeException_naming_the_read_and_the_bytes_available(
        @ForAll @Size(max = 16) byte[] bytes,
        @ForAll ScalarType type,
        @ForAll("nearPositions") int position
    ) {
        Assume.that(position < 0 || (long) position + type.size() > bytes.length);

        var buffer = new ByteBuffer(bytes);
        var exception = assertThrows(DecodeException.class, () -> type.read(buffer, position));

        assertEquals(
            String.format("Read exceeds the supplied buffer: %d bytes at %d of %d", type.size(), position, bytes.length),
            exception.getMessage()
        );
    }

    @Example
    void should_throw_DecodeException_for_invalid_strings() {
        var invalid = List.of(
            new byte[] {(byte) 0x80}, // A lone continuation byte
            new byte[] {(byte) 0xE2, (byte) 0x82}, // A truncated sequence
            new byte[] {(byte) 0xC0, (byte) 0xAF}, // An overlong encoding
            new byte[] {(byte) 0xED, (byte) 0xA0, (byte) 0x80}, // An encoded surrogate
            new byte[] {(byte) 0xEF, (byte) 0xBF, (byte) 0xBD, (byte) 0xFF} // U+FFFD, then an invalid byte
        );

        for (var utf8 : invalid) {
            // The string's length, and then its bytes
            var bytes = java.nio.ByteBuffer.allocate(Integer.BYTES + utf8.length)
                .order(ByteOrder.LITTLE_ENDIAN)
                .putInt(utf8.length)
                .put(utf8)
                .array();
            var buffer = new ByteBuffer(bytes);

            var exception = assertThrows(DecodeException.class, () -> buffer.getString(0));
            assertInstanceOf(CharacterCodingException.class, exception.getCause());
        }
    }

    @Example
    void should_throw_DecodeException_for_a_vector_item_that_ends_before_it_starts() {
        // A count of 2 puts the first item after the ends, at 12, but it ends at 11
        var buffer = new ByteBuffer(new byte[] {2, 0, 0, 0, 11, 0, 0, 0, 12, 0, 0, 0});

        var exception = assertThrows(DecodeException.class, () -> buffer.getVector(0));
        assertEquals("The vector item at 12 ends before it starts, at 11", exception.getMessage());
        assertThrows(DecodeException.class, () -> buffer.getVectorOffsets(0));
        assertThrows(DecodeException.class, () -> buffer.getVectorOffset(0, 0));
    }

    @Example
    void should_throw_DecodeException_for_a_vector_item_that_ends_past_the_bytes() {
        // A vector of one item, which would end at 9 from the vector's position, of 8 bytes
        var buffer = new ByteBuffer(new byte[] {1, 0, 0, 0, 9, 0, 0, 0});

        var exception = assertThrows(DecodeException.class, () -> buffer.getVectorOffset(0, 0));
        assertEquals("Read exceeds the supplied buffer: 1 bytes at 8 of 8", exception.getMessage());
        assertThrows(DecodeException.class, () -> buffer.getVectorOffsets(0));
        assertThrows(DecodeException.class, () -> buffer.getVector(0));
    }

    @Example
    void should_throw_DecodeException_for_a_struct_or_structs_past_the_bytes() {
        // A count of 2 and then 5 bytes, which hold one struct of 3 bytes but not two
        var buffer = new ByteBuffer(new byte[] {2, 0, 0, 0, 1, 2, 3, 4, 5});

        var exception = assertThrows(DecodeException.class, () -> buffer.getStructArray(0, 3));
        assertEquals("Read exceeds the supplied buffer: 6 bytes at 4 of 9", exception.getMessage());
        exception = assertThrows(DecodeException.class, () -> buffer.getStruct(4, 6));
        assertEquals("Read exceeds the supplied buffer: 6 bytes at 4 of 9", exception.getMessage());
        assertThrows(DecodeException.class, () -> buffer.getStruct(-1, 1));
        assertThrows(DecodeException.class, () -> buffer.getStruct(Integer.MAX_VALUE, 1));
        assertEquals(5, buffer.getStruct(4, 5).remaining());
    }

    @Example
    void should_throw_DecodeException_for_an_offset_or_count_beyond_the_largest_int() {
        // 2^32 - 1, which once threw an AssertionError as an offset, and then four bytes
        var bytes = new byte[] {-1, -1, -1, -1, 0, 0, 0, 0};

        assertThrows(DecodeException.class, () -> new ByteBuffer(bytes).rootOffset(0));
        assertThrows(DecodeException.class, () -> new ByteBuffer(bytes).fieldOffset(0, 0));
        assertThrows(DecodeException.class, () -> new ByteBuffer(bytes).getInt8Array(0));
        assertEquals(4294967295L, new ByteBuffer(bytes, TableLayout.builder().sized(true).build()).getSize());
    }

    @Provide
    Arbitrary<TableDescription> tables() {
        return Descriptions.tables();
    }

    @Provide
    Arbitrary<LayoutDescription> layouts() {
        return Descriptions.layouts();
    }

    /** A getter of {@link ByteBuffer}'s, which shows as its type when a property fails */
    record Getter(String type, Read read) {
        @Override
        public String toString() {
            return type;
        }
    }

    @FunctionalInterface
    interface Read {
        void at(ByteBuffer buffer, int position);
    }

    @Provide
    Arbitrary<Getter> getters() {
        var getters = new ArrayList<Getter>();
        for (var type : ScalarType.values()) {
            getters.add(new Getter(type.jsonName(), type::read));
            getters.add(new Getter(type.jsonName() + "[]", type::readArray));
        }

        getters.add(new Getter("string", ByteBuffer::getString));
        getters.add(new Getter("struct of 9 bytes", (buffer, position) -> buffer.getStruct(position, 9)));
        getters.add(new Getter("structs of 3 bytes", (buffer, position) -> buffer.getStructArray(position, 3)));
        getters.add(new Getter("vector", ByteBuffer::getVector));
        getters.add(new Getter("vector length", ByteBuffer::getVectorLength));
        getters.add(new Getter("vector offsets", ByteBuffer::getVectorOffsets));
        getters.add(new Getter("last vector offset", DecodeExceptionTest::getLastVectorOffset));
        getters.add(new Getter("vector offsets from the count", DecodeExceptionTest::getEachVectorItemOffset));

        return Arbitraries.of(getters);
    }

    /** Finds the last item of a vector, which reads the ends before and after it, if it has any */
    private static void getLastVectorOffset(ByteBuffer buffer, int position) {
        var length = buffer.getVectorLength(position);
        if (length > 0) {
            buffer.getVectorOffset(position, length - 1);
        }
    }

    /**
     * Finds each item of a vector from its count, read once, as a view of the vector does, and each
     * on its own, so that an item that throws leaves the others to find
     */
    private static void getEachVectorItemOffset(ByteBuffer buffer, int position) {
        var count = buffer.getVectorLength(position);
        for (int index = 0; index < count; index++) {
            var itemIndex = index;
            assertValuesOrDecodeException(() -> buffer.getVectorItemOffset(position, count, itemIndex));
        }
    }

    /** Where a getter reads: at a root field's offset, at the offset of a field of a nested table, or anywhere */
    sealed interface Position {
        int of(ByteBuffer buffer);

        record RootOffset(int index) implements Position {
            @Override
            public int of(ByteBuffer buffer) {
                return buffer.rootOffset(index);
            }
        }

        /** A field of the table at root field {@code tableIndex}, read in place */
        record FieldOffset(int tableIndex, int index) implements Position {
            @Override
            public int of(ByteBuffer buffer) {
                return buffer.fieldOffset(buffer.rootOffset(tableIndex), index);
            }
        }

        record At(int position) implements Position {
            @Override
            public int of(ByteBuffer buffer) {
                return position;
            }
        }
    }

    @Provide
    Arbitrary<Position> positions() {
        var indices = Arbitraries.frequencyOf(
            Tuple.of(4, Arbitraries.integers().between(0, 16)),
            Tuple.of(1, Arbitraries.integers())
        );

        return Arbitraries.oneOf(
            indices.map(Position.RootOffset::new),
            Combinators.combine(indices, indices).as(Position.FieldOffset::new),
            nearPositions().map(Position.At::new)
        );
    }

    /** {@return positions mostly within or around a few dozen bytes, and sometimes any int} */
    @Provide
    Arbitrary<Integer> nearPositions() {
        return Arbitraries.frequencyOf(
            Tuple.of(4, Arbitraries.integers().between(-8, 72)),
            Tuple.of(1, Arbitraries.integers())
        );
    }
}
