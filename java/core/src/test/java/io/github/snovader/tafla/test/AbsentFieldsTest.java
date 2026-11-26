package io.github.snovader.tafla.test;

import io.github.snovader.tafla.*;
import io.github.snovader.tafla.test.FieldDescription.*;
import net.jqwik.api.*;
import net.jqwik.api.constraints.*;

import java.util.*;
import java.util.function.*;

import static io.github.snovader.tafla.test.DecodeAssertions.*;
import static org.junit.jupiter.api.Assertions.*;

/**
 * Absent fields, whose offsets are 0 and which have no bytes, as {@link TableBuilder#setAbsent()}
 * and {@link TableWriter#setAbsent(int)} write them, and the fields that a reader of a later
 * revision of a table finds missing from it, as {@code SPEC.md} describes them. The tables are those
 * of {@link Descriptions}, with fields left out at every depth.
 */
public class AbsentFieldsTest {
    private static final AbsentField ABSENT = new AbsentField();

    // Absent fields

    @Property
    void a_table_with_absent_fields_reads_back_its_values(@ForAll("presenceTables") TableDescription table) {
        var bytes = Tables.build(table).bytes();

        assertEquals(table, DescriptionReader.strict(bytes, table.layout()).readTable(table));
    }

    @Property
    void an_absent_field_has_the_offset_0_and_every_other_field_an_offset_after_the_offsets(
        @ForAll("presenceTables") TableDescription table
    ) {
        var layout = table.layout();
        var reader = new TableReader(Tables.build(table).bytes(), layout.tableLayout());
        var firstValue = layout.headerSize() + table.fields().size() * layout.width();

        for (int index = 0; index < table.fields().size(); index++) {
            var offset = reader.rootOffset(index);

            if (table.fields().get(index) instanceof AbsentField) {
                assertEquals(0, offset, "The offset of field " + index);
            } else {
                assertTrue(offset >= firstValue, "Field " + index + " lies at " + offset);
            }
        }
    }

    @Property
    void a_table_with_absent_fields_is_written_by_a_sizer_and_writer_and_in_place_as_TableBuilder_writes_it(
        @ForAll("writtenTables") TableDescription table,
        @ForAll @IntRange(max = 16) int extra
    ) {
        var built = Tables.build(table).bytes();

        assertArrayEquals(built, Tables.write(table));
        assertArrayEquals(built, new InPlaceTables().build(table).bytes());

        // Bytes that aren't zero, so the writer writes each absent field's offset
        GivenBytesAssertions.assertWrittenInto(built, bytes -> writeInto(table, bytes), extra, (byte) 0xff);
    }

    /** {@return the length of the table, written into the start of {@code bytes}} */
    private static int writeInto(TableDescription table, byte[] bytes) {
        var writer = Tables.size(table).writer(bytes);
        Tables.writeFields(writer, table);

        return writer.byteLength();
    }

    @Example
    void an_absent_field_counts_as_one_of_the_builders_fields() {
        var builder = new TableBuilder(1);
        builder.setAbsent();

        assertThrows(IllegalStateException.class, builder::setAbsent);
        assertArrayEquals(new byte[4], builder.bytes());
        assertThrows(IllegalStateException.class, builder::setAbsent);
    }

    @Example
    void an_absent_field_takes_no_room_and_the_array_after_it_is_padded_from_the_value_before_it() {
        var builder = new TableBuilder(3);
        builder.setUint8((short) 7);
        builder.setAbsent();
        builder.setFloat64Array(new double[] {0.5});

        var bytes = builder.bytes();
        var reader = new TableReader(bytes);

        // The uint8 after the 12 bytes of offsets ends at 13, and the array's count at 20, padded so
        // that its elements start at 24
        assertEquals(12, reader.rootOffset(0));
        assertEquals(0, reader.rootOffset(1));
        assertEquals(20, reader.rootOffset(2));
        assertArrayEquals(new double[] {0.5}, reader.getFloat64Array(reader.rootOffset(2)));
        assertEquals(32, bytes.length);
    }

    // Fields of a later revision

    @Property
    void a_reader_of_a_later_revision_finds_every_field_after_the_tables_own_missing(
        @ForAll("revisedTables") TableDescription table,
        @ForAll @IntRange(min = 1, max = 4) int missing
    ) {
        var bytes = Tables.build(table).bytes();
        var later = table.withMissing(missing);

        assertEquals(later, DescriptionReader.strict(bytes, table.layout()).readTable(later));
    }

    @Property
    void a_reader_of_an_earlier_revision_reads_its_fields_of_a_table_that_holds_more_and_finds_the_others_unless_absent(
        @ForAll("revisedTables") TableDescription table,
        @ForAll @IntRange(min = 0, max = Integer.MAX_VALUE) int split
    ) {
        var fields = table.fields();
        var known = 1 + split % fields.size();
        var later = fields.subList(known, fields.size());
        var earlier = new TableDescription(table.layout(), table.version(), fields.subList(0, known));

        var bytes = Tables.build(table).bytes();
        var read = DescriptionReader.strict(bytes, table.layout()).readTable(earlier.withMissing(later.size()));

        var absent = (int) later.stream().filter(AbsentField.class::isInstance).count();
        assertEquals(earlier.withMissing(absent), read);
    }

    // Untrusted bytes

    @Property
    void a_corrupted_table_with_absent_and_missing_fields_reads_as_values_or_throws_DecodeException(
        @ForAll("revisedTables") TableDescription table,
        @ForAll @IntRange(max = 4) int missing,
        @ForAll("corruptions") List<Corruption> corruptions
    ) {
        var bytes = Tables.build(table).bytes();
        for (var corruption : corruptions) {
            bytes = corruption.applyTo(bytes);
        }

        var corrupted = bytes;
        assertValuesOrDecodeException(
            () -> DescriptionReader.lenient(corrupted, table.layout()).readTable(table.withMissing(missing))
        );
    }

    /**
     * Tables of the conformance suite's model with fields left out, at every depth: each field in
     * turn is absent when the next of a list of booleans says so
     */
    @Provide
    Arbitrary<TableDescription> presenceTables() {
        var absences = Arbitraries.of(true, false).list().ofMinSize(1).ofMaxSize(24);

        return Combinators.combine(Descriptions.tables(), absences)
            .as(AbsentFieldsTest::withAbsences)
            .filter(table -> !table.fields().isEmpty());
    }

    /** The tables that a {@link TableSizer} and a {@link TableWriter} write too */
    @Provide
    Arbitrary<TableDescription> writtenTables() {
        return presenceTables().map(Tables::nestedInPlace).filter(Descriptions::fitsItsLayout);
    }

    /** Tables whose first field is present, as a table with later revisions needs */
    @Provide
    Arbitrary<TableDescription> revisedTables() {
        return presenceTables().filter(table -> !(table.fields().getFirst() instanceof AbsentField));
    }

    @Provide
    Arbitrary<List<Corruption>> corruptions() {
        return Corruption.corruptions().list().ofMinSize(1).ofMaxSize(3);
    }

    /** {@return the table with each field absent when the next of {@code absences} says so} */
    private static TableDescription withAbsences(TableDescription table, List<Boolean> absences) {
        var next = new int[1];
        BooleanSupplier isAbsent = () -> absences.get(next[0]++ % absences.size());

        return within(table, isAbsent);
    }

    private static TableDescription within(TableDescription table, BooleanSupplier isAbsent) {
        var fields = table.fields().stream().map(field -> absentOr(field, isAbsent)).toList();
        return new TableDescription(table.layout(), table.version(), fields);
    }

    private static FieldDescription absentOr(FieldDescription field, BooleanSupplier isAbsent) {
        if (isAbsent.getAsBoolean()) {
            return ABSENT;
        }

        return switch (field) {
            case TableField table -> new TableField(table.from(), within(table.table(), isAbsent), table.alignment());
            case VectorField vector -> new VectorField(
                vector.items().stream()
                    .map(item -> new NestedTable(item.from(), within(item.table(), isAbsent)))
                    .toList()
            );
            default -> field;
        };
    }
}
