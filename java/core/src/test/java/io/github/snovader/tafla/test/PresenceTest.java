package io.github.snovader.tafla.test;

import io.github.snovader.tafla.*;
import io.github.snovader.tafla.test.generated.presence.*;
// Rather than java.lang.Record, which every class imports too
import io.github.snovader.tafla.test.generated.presence.Record;
import net.jqwik.api.*;
import net.jqwik.api.constraints.*;

import java.nio.ByteBuffer;
import java.nio.ByteOrder;

import static io.github.snovader.tafla.test.DecodeAssertions.*;
import static io.github.snovader.tafla.test.GivenBytesAssertions.*;
import static io.github.snovader.tafla.test.PresenceTables.*;
import static org.junit.jupiter.api.Assertions.*;

/**
 * The code generated from {@code typescript/core/test/presence.tafla}: optional fields, which its
 * writers leave out with an offset of 0, and fields that a later version of a message or a later
 * revision of a table appends, which its readers find missing from the tables that don't hold
 * them. Their getters return {@code null}, or a scalar's default, which {@code has<Field>()}
 * tells apart from a value that the table holds.
 */
public class PresenceTest {
    // Optional fields

    @Property
    void a_record_reads_back_its_fields_and_null_or_the_default_for_each_absent_one(
        @ForAll("records") Record.Fields fields
    ) {
        var bytes = Record.serialize(fields);

        assertEquals(describeRecord(fields).fields(), readRecord(new Record(bytes)));
    }

    @Property
    void a_record_is_written_with_its_absent_fields_as_TableBuilder_setAbsent_writes_them(
        @ForAll("records") Record.Fields fields
    ) {
        var byHand = Tables.build(describeRecord(fields)).bytes();

        assertArrayEquals(byHand, Record.serialize(fields), () -> describeRecord(fields).toString());
    }

    @Property
    void a_record_written_into_reused_bytes_holds_what_serialize_writes_absent_fields_included(
        @ForAll("records") Record.Fields fields,
        @ForAll @IntRange(max = 16) int extra,
        @ForAll byte fill
    ) {
        assertWrittenInto(Record.serialize(fields), bytes -> Record.serializeInto(fields, bytes), extra, fill);
    }

    @Example
    void a_record_of_absent_fields_is_its_header_offsets_and_required_fields() {
        var fields = new Record.Fields(7, null, null, null, null, null, null, "a", null);
        var record = new Record(Record.serialize(fields));

        assertNull(record.name());
        assertNull(record.values());
        assertNull(record.valuesView());
        assertNull(record.origin());
        assertTrue(record.flag());
        assertFalse(record.hasFlag());
        assertNull(record.entries());
        assertEquals(-1, record.total());
        assertFalse(record.hasTotal());
        assertEquals("a", record.label());
        assertNull(record.extent());

        // A sized and versioned header, nine offsets, the uint32 and the string
        assertEquals(8 + 9 * 4 + 4 + 4 + 1, Record.serialize(fields).length);
    }

    @Property
    void a_first_version_record_reads_back_its_fields(@ForAll("records") Record.Fields fields) {
        var first = firstVersionOf(fields);
        var bytes = RecordV1.serialize(first);

        assertEquals(describeRecordV1(first).fields(), readRecordV1(new RecordV1(bytes)));
        assertArrayEquals(Tables.build(describeRecordV1(first)).bytes(), bytes);
    }

    // Versions and revisions

    @Property
    void a_record_reads_a_first_version_table_with_the_fields_that_the_second_added_missing_in_nested_tables_too(
        @ForAll("records") Record.Fields fields
    ) {
        var first = firstVersionOf(fields);
        var bytes = RecordV1.serialize(first);

        assertEquals(describeFromFirstVersion(first), readRecord(new Record(bytes)));
    }

    @Property
    void a_record_reads_its_first_and_second_versions_and_rejects_every_other_with_DecodeException(
        @ForAll("records") Record.Fields fields,
        @ForAll @LongRange(max = 0xffff_ffffL) long version
    ) {
        var bytes = Record.serialize(fields);
        ByteBuffer.wrap(bytes).order(ByteOrder.LITTLE_ENDIAN).putInt(4, (int) version);

        if (version >= Record.FIRST_VERSION && version <= Record.FORMAT_VERSION) {
            assertDoesNotThrow(() -> new Record(bytes));
        } else {
            assertThrows(DecodeException.class, () -> new Record(bytes));
        }
        assertEquals(version, Record.versionOf(bytes));
    }

    @Property
    void a_reader_of_one_version_rejects_every_other(@ForAll("records") Record.Fields fields) {
        var bytes = Record.serialize(fields);

        assertThrows(DecodeException.class, () -> new RecordV1(bytes));
    }

    @Property
    void a_sample_without_a_version_reads_a_first_revision_table_with_the_default_for_the_field_that_the_second_appended(
        @ForAll("uint32s") long key,
        @ForAll("uint16s") int count
    ) {
        var first = SampleV1.serialize(new SampleV1.Fields(key));
        var second = Sample.serialize(new Sample.Fields(key, count));

        assertEquals(key, new Sample(first).key());
        assertEquals(COUNT_DEFAULT.value(), new Sample(first).count());
        assertFalse(new Sample(first).hasCount());
        assertEquals(count, new Sample(second).count());
        assertTrue(new Sample(second).hasCount());

        // A reader of the first revision reads its fields of a later one
        assertEquals(key, new SampleV1(second).key());
    }

    // Untrusted bytes

    @Property
    void a_corrupted_record_reads_as_values_null_or_defaults_or_throws_DecodeException(
        @ForAll("records") Record.Fields fields,
        @ForAll("corruptions") Corruption first,
        @ForAll("corruptions") Corruption second
    ) {
        var bytes = second.applyTo(first.applyTo(Record.serialize(fields)));

        assertValuesOrDecodeException(() -> readRecord(new Record(bytes)));
    }

    @Property
    void a_corrupted_first_version_record_reads_as_a_second_version_record_or_throws_DecodeException(
        @ForAll("records") Record.Fields fields,
        @ForAll("corruptions") Corruption corruption
    ) {
        var bytes = corruption.applyTo(RecordV1.serialize(firstVersionOf(fields)));

        assertValuesOrDecodeException(() -> readRecord(new Record(bytes)));
    }

    @Property
    void any_bytes_read_as_a_sample_without_a_version_or_a_record_or_throw_DecodeException(
        @ForAll @Size(max = 48) byte[] bytes
    ) {
        assertValuesOrDecodeException(() -> readSample(new Sample(bytes)));
        assertValuesOrDecodeException(() -> readRecord(new Record(bytes)));
    }

    @Provide
    Arbitrary<Record.Fields> records() {
        return recordFields();
    }

    @Provide
    Arbitrary<Long> uint32s() {
        return FieldValues.uint32s();
    }

    @Provide
    Arbitrary<Integer> uint16s() {
        return FieldValues.uint16s();
    }

    @Provide
    Arbitrary<Corruption> corruptions() {
        return Corruption.corruptions();
    }
}
