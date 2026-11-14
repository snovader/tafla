package io.github.snovader.tafla.test;

import io.github.snovader.tafla.ByteBuffer;
import io.github.snovader.tafla.DecodeException;
import io.github.snovader.tafla.test.generated.all_types.*;
import net.jqwik.api.*;
import net.jqwik.api.constraints.*;

import java.util.List;
import java.util.NoSuchElementException;

import static io.github.snovader.tafla.test.AllTypesTables.*;
import static io.github.snovader.tafla.test.DecodeAssertions.*;
import static io.github.snovader.tafla.test.GeneratedDescriptions.*;
import static io.github.snovader.tafla.test.GivenBytesAssertions.*;
import static org.junit.jupiter.api.Assertions.*;

/**
 * The code generated from {@code schemas/all_types.tafla}, of every type of field, writes and reads
 * tables with tafla. Values read back compare with the values written by their descriptions, which
 * compare floats by their bits and arrays by their elements.
 */
public class AllTypesTest {
    // Round trips

    @Property
    void scalars_read_back_a_value_of_every_scalar_type(@ForAll("scalars") Scalars.Fields fields) {
        var bytes = Scalars.serialize(fields);

        assertEquals(describeScalars(fields), describeScalars(readScalars(new Scalars(bytes))));
    }

    @Property
    void arrays_read_back_an_array_of_every_scalar_type(@ForAll("arrays") Arrays.Fields fields) {
        var bytes = Arrays.serialize(fields);

        assertEquals(describeArrays(fields), describeArrays(readArrays(new Arrays(bytes))));
    }

    @Property
    void a_nested_message_reads_back_its_tables_three_levels_deep_and_its_trees_of_trees(
        @ForAll("nesteds") Nested.Fields fields
    ) {
        var bytes = Nested.serialize(fields);

        assertEquals(describeNested(fields), describeNested(readNested(new Nested(bytes))));
    }

    @Property
    void a_reserved_message_reads_back_its_fields_named_after_keywords(@ForAll("reserveds") Reserved.Fields fields) {
        var bytes = Reserved.serialize(fields);

        assertEquals(describeReserved(fields), describeReserved(readReserved(new Reserved(bytes))));
    }

    @Property
    void structs_read_back_structs_as_fields_in_arrays_within_structs_and_in_a_nested_table(
        @ForAll("structs") Structs.Fields fields
    ) {
        var bytes = Structs.serialize(fields);

        assertEquals(describeStructs(fields), describeStructs(readStructs(new Structs(bytes))));
    }

    @Example
    void unsigned_integers_read_back_widened_and_a_uint64_as_the_bits_of_a_long() {
        // The largest value of each unsigned type
        var fields = new Scalars.Fields((byte) 0, (short) 0xff, (short) 0, 0xffff, 0, 0xffff_ffffL, 0, -1L, 0, 0, false);

        var scalars = new Scalars(Scalars.serialize(fields));
        var packed = Packed.read(packedOf(fields).serialize(), 0);

        assertEquals(255, scalars.uint8());
        assertEquals(65_535, scalars.uint16());
        assertEquals(4_294_967_295L, scalars.uint32());
        assertEquals("18446744073709551615", Long.toUnsignedString(scalars.uint64()));

        assertEquals(255, packed.uint8());
        assertEquals(65_535, packed.uint16());
        assertEquals(4_294_967_295L, packed.uint32());
        assertEquals("18446744073709551615", Long.toUnsignedString(packed.uint64()));
    }

    @Property
    void a_bool_reads_any_byte_but_0_as_true(
        @ForAll("scalars") Scalars.Fields fields,
        @ForAll @IntRange(min = 2, max = 0xff) int value
    ) {
        var bytes = Scalars.serialize(withBool(fields, false));
        bytes[new ByteBuffer(bytes).rootOffset(10)] = (byte) value;

        assertEquals(describeScalars(withBool(fields, true)), describeScalars(readScalars(new Scalars(bytes))));
    }

    // Tables built by hand

    @Property
    void scalars_are_written_as_TableBuilder_writes_them(@ForAll("scalars") Scalars.Fields fields) {
        var byHand = Tables.build(describeScalars(fields)).bytes();

        assertArrayEquals(byHand, Scalars.serialize(fields), () -> describeScalars(fields).toString());
    }

    @Property
    void arrays_are_written_aligned_as_TableBuilder_writes_them(@ForAll("arrays") Arrays.Fields fields) {
        var byHand = Tables.build(describeArrays(fields)).bytes();

        assertArrayEquals(byHand, Arrays.serialize(fields), () -> describeArrays(fields).toString());
    }

    @Property
    void a_nested_message_is_written_as_TableBuilder_writes_it(@ForAll("nesteds") Nested.Fields fields) {
        var byHand = Tables.build(describeNested(fields)).bytes();

        assertArrayEquals(byHand, Nested.serialize(fields), () -> describeNested(fields).toString());
    }

    @Property
    void a_reserved_message_is_written_as_TableBuilder_writes_it(@ForAll("reserveds") Reserved.Fields fields) {
        var byHand = Tables.build(describeReserved(fields)).bytes();

        assertArrayEquals(byHand, Reserved.serialize(fields), () -> describeReserved(fields).toString());
    }

    @Property
    void structs_are_written_with_arrays_aligned_to_their_structs_as_TableBuilder_writes_them(
        @ForAll("structs") Structs.Fields fields
    ) {
        var byHand = Tables.build(describeStructs(fields)).bytes();

        assertArrayEquals(byHand, Structs.serialize(fields), () -> describeStructs(fields).toString());
    }

    // Writing into bytes that the caller gives, which may hold anything

    @Property
    void every_message_written_into_longer_bytes_holds_at_their_start_what_serialize_writes(
        @ForAll("scalars") Scalars.Fields scalars,
        @ForAll("arrays") Arrays.Fields arrays,
        @ForAll("nesteds") Nested.Fields nested,
        @ForAll("reserveds") Reserved.Fields reserved,
        @ForAll("structs") Structs.Fields structs,
        @ForAll @IntRange(max = 16) int extra,
        @ForAll byte fill
    ) {
        assertWrittenInto(Scalars.serialize(scalars), bytes -> Scalars.serializeInto(scalars, bytes), extra, fill);
        assertWrittenInto(Arrays.serialize(arrays), bytes -> Arrays.serializeInto(arrays, bytes), extra, fill);
        assertWrittenInto(Nested.serialize(nested), bytes -> Nested.serializeInto(nested, bytes), extra, fill);
        assertWrittenInto(Reserved.serialize(reserved), bytes -> Reserved.serializeInto(reserved, bytes), extra, fill);
        assertWrittenInto(Structs.serialize(structs), bytes -> Structs.serializeInto(structs, bytes), extra, fill);
    }

    // Decode exceptions

    @Property
    void corrupted_scalars_read_as_values_or_throw_DecodeException(
        @ForAll("scalars") Scalars.Fields fields,
        @ForAll("corruptions") Corruption corruption
    ) {
        var bytes = corruption.applyTo(Scalars.serialize(fields));

        assertValuesOrDecodeException(() -> readScalars(new Scalars(bytes)));
    }

    @Property
    void corrupted_arrays_read_as_values_or_throw_DecodeException(
        @ForAll("arrays") Arrays.Fields fields,
        @ForAll("corruptions") Corruption corruption
    ) {
        var bytes = corruption.applyTo(Arrays.serialize(fields));

        assertValuesOrDecodeException(() -> readArrays(new Arrays(bytes)));
    }

    @Property
    void a_corrupted_nested_message_reads_as_values_or_throws_DecodeException(
        @ForAll("nesteds") Nested.Fields fields,
        @ForAll("corruptions") Corruption corruption
    ) {
        var bytes = corruption.applyTo(Nested.serialize(fields));

        assertValuesOrDecodeException(() -> readNested(new Nested(bytes)));
    }

    @Property
    void a_corrupted_reserved_message_reads_as_values_or_throws_DecodeException(
        @ForAll("reserveds") Reserved.Fields fields,
        @ForAll("corruptions") Corruption corruption
    ) {
        var bytes = corruption.applyTo(Reserved.serialize(fields));

        assertValuesOrDecodeException(() -> readReserved(new Reserved(bytes)));
    }

    @Property
    void corrupted_structs_read_as_values_or_throw_DecodeException(
        @ForAll("structs") Structs.Fields fields,
        @ForAll("corruptions") Corruption corruption
    ) {
        var bytes = corruption.applyTo(Structs.serialize(fields));

        assertValuesOrDecodeException(() -> readStructs(new Structs(bytes)));
    }

    @Property
    void any_bytes_read_as_any_message_or_throw_DecodeException(@ForAll @Size(max = 64) byte[] bytes) {
        assertValuesOrDecodeException(() -> readScalars(new Scalars(bytes)));
        assertValuesOrDecodeException(() -> readArrays(new Arrays(bytes)));
        assertValuesOrDecodeException(() -> readNested(new Nested(bytes)));
        assertValuesOrDecodeException(() -> readReserved(new Reserved(bytes)));
        assertValuesOrDecodeException(() -> readStructs(new Structs(bytes)));
    }

    // Views

    @Property
    void a_nested_message_reads_back_its_trees_item_by_item_through_their_views(
        @ForAll("nesteds") Nested.Fields fields,
        @ForAll ViewOrder order
    ) {
        var bytes = Nested.serialize(fields);

        assertEquals(describeNested(fields), describeNested(readNestedViews(new Nested(bytes), order)));
    }

    @Property
    void structs_read_back_their_arrays_struct_by_struct_through_their_views(
        @ForAll("structs") Structs.Fields fields,
        @ForAll ViewOrder order
    ) {
        var bytes = Structs.serialize(fields);

        assertEquals(describeStructs(fields), describeStructs(readStructsViews(new Structs(bytes), order)));
    }

    @Property
    void arrays_read_back_every_element_through_their_views(@ForAll("arrays") Arrays.Fields fields) {
        var bytes = Arrays.serialize(fields);

        assertEquals(describeArrays(fields), describeArrays(readArraysViews(new Arrays(bytes))));
    }

    @Property
    void corrupted_arrays_read_through_their_views_as_values_or_throw_DecodeException(
        @ForAll("arrays") Arrays.Fields fields,
        @ForAll("corruptions") Corruption corruption
    ) {
        var bytes = corruption.applyTo(Arrays.serialize(fields));

        assertValuesOrDecodeException(() -> readArraysViews(new Arrays(bytes)));
    }

    @Property
    void a_corrupted_nested_message_reads_through_its_views_as_values_or_throws_DecodeException(
        @ForAll("nesteds") Nested.Fields fields,
        @ForAll("corruptions") Corruption corruption,
        @ForAll ViewOrder order
    ) {
        var bytes = corruption.applyTo(Nested.serialize(fields));

        assertValuesOrDecodeException(() -> readNestedViews(new Nested(bytes), order));
    }

    @Property
    void corrupted_structs_read_through_their_views_as_values_or_throw_DecodeException(
        @ForAll("structs") Structs.Fields fields,
        @ForAll("corruptions") Corruption corruption,
        @ForAll ViewOrder order
    ) {
        var bytes = corruption.applyTo(Structs.serialize(fields));

        assertValuesOrDecodeException(() -> readStructsViews(new Structs(bytes), order));
    }

    @Property
    void any_bytes_read_through_the_views_or_throw_DecodeException(
        @ForAll @Size(max = 64) byte[] bytes,
        @ForAll ViewOrder order
    ) {
        assertValuesOrDecodeException(() -> readNestedViews(new Nested(bytes), order));
        assertValuesOrDecodeException(() -> readStructsViews(new Structs(bytes), order));
        assertValuesOrDecodeException(() -> readArraysViews(new Arrays(bytes)));
    }

    @Property
    void a_view_throws_IndexOutOfBoundsException_for_an_item_outside_it(
        @ForAll("nesteds") Nested.Fields nestedFields,
        @ForAll("structs") Structs.Fields structsFields,
        @ForAll @IntRange(min = 0, max = 1000) int past
    ) {
        var nested = new Nested(Nested.serialize(nestedFields));
        var structs = new Structs(Structs.serialize(structsFields));

        for (var view : List.<List<?>>of(nested.trees(), structs.documented())) {
            assertThrows(IndexOutOfBoundsException.class, () -> view.get(view.size() + past));
            assertThrows(IndexOutOfBoundsException.class, () -> view.get(-1 - past));
        }
    }

    @Property
    void a_vectors_iterator_reads_the_items_that_get_reads_and_then_throws_NoSuchElementException(
        @ForAll("nesteds") Nested.Fields fields
    ) {
        var trees = new Nested(Nested.serialize(fields)).trees();
        var iterator = trees.iterator();

        for (var i = 0; i < trees.size(); i++) {
            assertTrue(iterator.hasNext());
            var tree = iterator.next();
            assertEquals(trees.get(i).value(), tree.value(), "Item " + i);
            assertEquals(trees.get(i).children().size(), tree.children().size(), "Item " + i);
        }
        assertFalse(iterator.hasNext());
        assertThrows(NoSuchElementException.class, iterator::next);
    }

    @Example
    void views_of_empty_vectors_and_arrays_of_structs_hold_no_items() {
        var point = new Point(0, 0);
        var nested = new Nested(
            Nested.serialize(new Nested.Fields("", new Outer.Fields(0, new Inner.Fields("", new double[0])), List.of()))
        );
        var structs = new Structs(
            Structs.serialize(
                new Structs.Fields(
                    new Packed((byte) 0, (short) 0, (short) 0, 0, 0, 0, 0, 0, 0, 0, false),
                    new Documented[0],
                    new Segment(point, point),
                    new Segment[0],
                    new Path.Fields(point, new Point[0])
                )
            )
        );

        var views = List.<List<?>>of(
            nested.trees(),
            structs.documented(),
            structs.segments(),
            structs.path().points()
        );
        for (var view : views) {
            assertTrue(view.isEmpty());
            assertThrows(IndexOutOfBoundsException.class, () -> view.get(0));
        }
    }

    // Structs

    @Property
    void a_packed_is_written_in_its_43_bytes_little_endian_at_fixed_offsets(@ForAll("packeds") Packed packed) {
        var bytes = packed.serialize();

        assertEquals(43, Packed.SIZE);
        assertArrayEquals(structBytes(describePacked(packed)), bytes);
    }

    @Property
    void a_packed_reads_any_byte_of_its_bool_but_0_as_true(
        @ForAll("scalars") Scalars.Fields fields,
        @ForAll @IntRange(min = 2, max = 0xff) int value
    ) {
        var bytes = packedOf(withBool(fields, false)).serialize();
        bytes[42] = (byte) value;

        assertEquals(describePacked(packedOf(withBool(fields, true))), describePacked(Packed.read(bytes, 0)));
    }

    @Property
    void a_packed_reads_back_at_any_offset_in_longer_bytes(
        @ForAll("packeds") Packed packed,
        @ForAll @Size(max = 16) byte[] before,
        @ForAll @Size(max = 8) byte[] after
    ) {
        var bytes = java.nio.ByteBuffer.allocate(before.length + Packed.SIZE + after.length)
            .put(before)
            .put(packed.serialize())
            .put(after)
            .array();

        assertEquals(describePacked(packed), describePacked(Packed.read(bytes, before.length)));
    }

    @Property
    void reading_a_packed_throws_DecodeException_where_its_bytes_end_past_the_bytes(
        @ForAll @Size(max = 64) byte[] bytes,
        @ForAll("offsets") int offset
    ) {
        if (offset >= 0 && (long) offset + Packed.SIZE <= bytes.length) {
            assertDoesNotThrow(() -> Packed.read(bytes, offset));
        } else {
            assertThrows(DecodeException.class, () -> Packed.read(bytes, offset));
        }
    }

    @Property
    void a_documented_is_written_in_its_7_bytes_little_endian_at_fixed_offsets(
        @ForAll("documenteds") Documented documented
    ) {
        var bytes = documented.serialize();

        assertEquals(7, Documented.SIZE);
        assertArrayEquals(structBytes(describeDocumented(documented)), bytes);
    }

    @Property
    void a_documented_reads_back_at_any_offset_in_longer_bytes(
        @ForAll("documenteds") Documented documented,
        @ForAll @Size(max = 16) byte[] before,
        @ForAll @Size(max = 8) byte[] after
    ) {
        var bytes = java.nio.ByteBuffer.allocate(before.length + Documented.SIZE + after.length)
            .put(before)
            .put(documented.serialize())
            .put(after)
            .array();

        assertEquals(describeDocumented(documented), describeDocumented(Documented.read(bytes, before.length)));
    }

    @Property
    void reading_a_documented_throws_DecodeException_where_its_bytes_end_past_the_bytes(
        @ForAll @Size(max = 32) byte[] bytes,
        @ForAll("offsets") int offset
    ) {
        if (offset >= 0 && (long) offset + Documented.SIZE <= bytes.length) {
            assertDoesNotThrow(() -> Documented.read(bytes, offset));
        } else {
            assertThrows(DecodeException.class, () -> Documented.read(bytes, offset));
        }
    }

    @Example
    void a_structs_size_is_the_sum_of_its_scalars_sizes_and_its_alignment_the_largest_of_them() {
        assertSizeAndAlignment(PACKED_TYPES, Packed.SIZE, Packed.ALIGNMENT);
        assertSizeAndAlignment(DOCUMENTED_TYPES, Documented.SIZE, Documented.ALIGNMENT);
        assertSizeAndAlignment(SEGMENT_TYPES, Segment.SIZE, Segment.ALIGNMENT);
        assertSizeAndAlignment(POINT_TYPES, Point.SIZE, Point.ALIGNMENT);

        // A Packed's float64s and int64s align it to 8, and the others' 32-bit scalars to 4
        assertEquals(
            List.of(8, 4, 4, 4),
            List.of(Packed.ALIGNMENT, Documented.ALIGNMENT, Segment.ALIGNMENT, Point.ALIGNMENT)
        );
    }

    private static void assertSizeAndAlignment(List<ScalarType> types, int size, int alignment) {
        assertEquals(StructBytes.size(types), size, () -> "The size of a struct of " + types);
        assertEquals(StructBytes.alignment(types), alignment, () -> "The alignment of a struct of " + types);
    }

    @Property
    void a_struct_reads_the_same_from_its_bytes_and_from_a_view_of_them_at_any_offset(
        @ForAll @Size(max = 16) byte[] before,
        @ForAll("packeds") Packed packed,
        @ForAll("documenteds") Documented documented,
        @ForAll("segments") Segment segment,
        @ForAll("points") Point point
    ) {
        // The structs one after another, unaligned after the bytes before them, which a struct array's
        // view holds as tafla's read-only, little-endian buffer
        var parts = List.of(before, packed.serialize(), documented.serialize(), segment.serialize(), point.serialize());
        var bytes = concatenated(parts);
        var view = new ByteBuffer(bytes).getStruct(0, bytes.length);

        var offset = before.length;
        assertEquals(describePacked(packed), describePacked(Packed.read(bytes, offset)));
        assertEquals(describePacked(packed), describePacked(Packed.read(view, offset)));

        offset += Packed.SIZE;
        assertEquals(describeDocumented(documented), describeDocumented(Documented.read(bytes, offset)));
        assertEquals(describeDocumented(documented), describeDocumented(Documented.read(view, offset)));

        offset += Documented.SIZE;
        assertEquals(describeSegment(segment), describeSegment(Segment.read(bytes, offset)));
        assertEquals(describeSegment(segment), describeSegment(Segment.read(view, offset)));

        offset += Segment.SIZE;
        assertEquals(describePoint(point), describePoint(Point.read(bytes, offset)));
        assertEquals(describePoint(point), describePoint(Point.read(view, offset)));
    }

    @Property
    void a_structs_static_write_of_its_values_writes_what_the_struct_writes(
        @ForAll("packeds") Packed packed,
        @ForAll("documenteds") Documented documented,
        @ForAll("segments") Segment segment,
        @ForAll("points") Point point,
        @ForAll @IntRange(max = 16) int offset,
        @ForAll byte fill
    ) {
        assertWritten(Packed.SIZE, offset, fill, packed::write, (bytes, at) -> Packed.write(
            bytes,
            at,
            packed.int8(),
            packed.uint8(),
            packed.int16(),
            packed.uint16(),
            packed.int32(),
            packed.uint32(),
            packed.int64(),
            packed.uint64(),
            packed.float32(),
            packed.float64(),
            packed.bool()
        ));
        assertWritten(Documented.SIZE, offset, fill, documented::write, (bytes, at) -> Documented.write(
            bytes,
            at,
            documented.one(),
            documented.two(),
            documented.undocumented()
        ));
        assertWritten(Segment.SIZE, offset, fill, segment::write, (bytes, at) -> Segment.write(
            bytes,
            at,
            segment.start(),
            segment.end()
        ));
        assertWritten(Point.SIZE, offset, fill, point::write, (bytes, at) -> Point.write(bytes, at, point.x(), point.y()));
    }

    /**
     * Asserts that two writes of a struct at {@code offset} in bytes of {@code fill}, longer than it,
     * write the same bytes
     */
    private static void assertWritten(int size, int offset, byte fill, StructWrite expected, StructWrite actual) {
        var expectedBytes = new byte[offset + size + 8];
        java.util.Arrays.fill(expectedBytes, fill);
        expected.write(expectedBytes, offset);

        var actualBytes = new byte[offset + size + 8];
        java.util.Arrays.fill(actualBytes, fill);
        actual.write(actualBytes, offset);

        assertArrayEquals(expectedBytes, actualBytes);
    }

    /** A write of a struct at {@code offset} in {@code bytes} */
    @FunctionalInterface
    private interface StructWrite {
        void write(byte[] bytes, int offset);
    }

    @Property
    void serializeArray_writes_each_structs_serialized_bytes_one_after_another(
        @ForAll @Size(max = 4) List<@From("packeds") Packed> packeds,
        @ForAll @Size(max = 4) List<@From("documenteds") Documented> documenteds,
        @ForAll @Size(max = 4) List<@From("segments") Segment> segments,
        @ForAll @Size(max = 4) List<@From("points") Point> points
    ) {
        assertArrayEquals(
            concatenated(packeds.stream().map(Packed::serialize).toList()),
            Packed.serializeArray(packeds.toArray(Packed[]::new))
        );
        assertArrayEquals(
            concatenated(documenteds.stream().map(Documented::serialize).toList()),
            Documented.serializeArray(documenteds.toArray(Documented[]::new))
        );
        assertArrayEquals(
            concatenated(segments.stream().map(Segment::serialize).toList()),
            Segment.serializeArray(segments.toArray(Segment[]::new))
        );
        assertArrayEquals(
            concatenated(points.stream().map(Point::serialize).toList()),
            Point.serializeArray(points.toArray(Point[]::new))
        );
    }

    /** {@return the fields with another bool} */
    private static Scalars.Fields withBool(Scalars.Fields fields, boolean bool) {
        return new Scalars.Fields(
            fields.int8(),
            fields.uint8(),
            fields.int16(),
            fields.uint16(),
            fields.int32(),
            fields.uint32(),
            fields.int64(),
            fields.uint64(),
            fields.float32(),
            fields.float64(),
            bool
        );
    }

    /** {@return the bytes of the parts, one after another} */
    private static byte[] concatenated(List<byte[]> parts) {
        var bytes = java.nio.ByteBuffer.allocate(parts.stream().mapToInt(part -> part.length).sum());
        parts.forEach(bytes::put);

        return bytes.array();
    }

    @Provide
    Arbitrary<Scalars.Fields> scalars() {
        return scalarsFields();
    }

    @Provide
    Arbitrary<Arrays.Fields> arrays() {
        return arraysFields();
    }

    @Provide
    Arbitrary<Nested.Fields> nesteds() {
        return nestedFields();
    }

    @Provide
    Arbitrary<Reserved.Fields> reserveds() {
        return reservedFields();
    }

    @Provide
    Arbitrary<Packed> packeds() {
        return packedFields();
    }

    @Provide
    Arbitrary<Documented> documenteds() {
        return documentedFields();
    }

    @Provide
    Arbitrary<Structs.Fields> structs() {
        return structsFields();
    }

    @Provide
    Arbitrary<Segment> segments() {
        return segmentFields();
    }

    @Provide
    Arbitrary<Point> points() {
        return pointFields();
    }

    @Provide
    Arbitrary<Corruption> corruptions() {
        return Corruption.corruptions();
    }

    /** {@return offsets mostly within or around a few dozen bytes, and sometimes any int} */
    @Provide
    Arbitrary<Integer> offsets() {
        return Arbitraries.frequencyOf(
            Tuple.of(4, Arbitraries.integers().between(-8, 72)),
            Tuple.of(1, Arbitraries.integers())
        );
    }
}
