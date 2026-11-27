package io.github.snovader.tafla.test;

import io.github.snovader.tafla.*;
import io.github.snovader.tafla.test.FieldDescription.*;
import io.github.snovader.tafla.test.generated.presence.*;
// Rather than java.lang.Record, which every class imports too
import io.github.snovader.tafla.test.generated.presence.Record;
import net.jqwik.api.*;
import org.jspecify.annotations.Nullable;

import java.nio.DoubleBuffer;
import java.util.*;
import java.util.function.*;

import static io.github.snovader.tafla.test.FieldValues.*;
import static io.github.snovader.tafla.test.GeneratedDescriptions.*;
import static org.junit.jupiter.api.Assertions.*;

/**
 * The code generated from {@code typescript/core/test/presence.tafla}, whose optional fields its
 * writers leave out with an offset of 0, and whose readers find the fields that a later version of
 * a message or a later revision of a table appends missing from the tables that don't hold them:
 * how tests read its messages back, describe their tables in the conformance suite's model, and
 * generate their fields.
 *
 * <p>A table read back describes each field that a getter finds missing, absent or of a later
 * version or revision, as an {@link AbsentField}, so that it equals the description of the table
 * written, whose absent fields are those left out. Reading a scalar that may be missing also
 * checks that its getter returns the schema's default where {@code has<Field>()} doesn't find it,
 * and reading an array, that its view is {@code null} exactly where the array is.</p>
 */
final class PresenceTables {
    /** The layout of a {@code Record} and a {@code RecordV1}, from their attributes: sized and versioned */
    static final LayoutDescription RECORD_LAYOUT = new LayoutDescription(true, true, OffsetType.UINT32);

    // The defaults that the schema gives the scalars that may be missing
    static final BoolField FLAG_DEFAULT = new BoolField(true);
    static final ScalarField TOTAL_DEFAULT = int64(-1);
    static final ScalarField Z_DEFAULT = float64(-0.5);
    static final ScalarField WEIGHT_DEFAULT = float32(0.1f);
    static final ScalarField COUNT_DEFAULT = uint16(7);

    private static final AbsentField ABSENT = new AbsentField();

    private PresenceTables() {}

    // Reading every field, nested tables and vector items included

    /** {@return the fields of a record, as {@link #describeRecord} describes the fields written} */
    static List<FieldDescription> readRecord(Record record) {
        var extent = record.extent();
        var label = record.label();

        var fields = new ArrayList<>(readFirstVersion(
            record.id(),
            record.name(),
            record.values(),
            record.valuesView(),
            scalar(record.hasFlag(), new BoolField(record.flag()), FLAG_DEFAULT),
            scalar(record.hasTotal(), int64(record.total()), TOTAL_DEFAULT)
        ));
        fields.set(3, nullable(record.origin(), PresenceTables::readPoint));
        fields.set(5, nullable(record.entries(), entries -> vector(entries.stream().map(PresenceTables::readEntry).toList())));
        fields.add(label == null ? ABSENT : string(label));
        fields.add(extent == null ? ABSENT : describeExtent(extent));

        return fields;
    }

    /** {@return the fields of a record of the first version, as {@link #describeRecordV1} describes them} */
    static List<FieldDescription> readRecordV1(RecordV1 record) {
        var fields = new ArrayList<>(readFirstVersion(
            record.id(),
            record.name(),
            record.values(),
            record.valuesView(),
            scalar(record.hasFlag(), new BoolField(record.flag()), FLAG_DEFAULT),
            scalar(record.hasTotal(), int64(record.total()), TOTAL_DEFAULT)
        ));
        fields.set(3, nullable(record.origin(), point -> table(float64(point.x()), float64(point.y()))));
        fields.set(5, nullable(record.entries(), entries -> vector(entries.stream().map(PresenceTables::readEntryV1).toList())));

        return fields;
    }

    /**
     * {@return the fields that both versions of a record hold, with the nested table and the vector
     * left for each version to read}, after checking that the view of the values is {@code null}
     * exactly where they are, and holds them otherwise
     */
    private static List<FieldDescription> readFirstVersion(
        long id,
        @Nullable String name,
        double @Nullable [] values,
        @Nullable DoubleBuffer valuesView,
        FieldDescription flag,
        FieldDescription total
    ) {
        assertEquals(values == null, valuesView == null, "The view of the values is null as they are");
        if (values != null && valuesView != null) {
            var viewed = new double[valuesView.remaining()];
            valuesView.get(viewed);
            assertEquals(float64Array(values), float64Array(viewed), "The view of the values");
        }

        return List.of(
            uint32(id),
            nullable(name, GeneratedDescriptions::string),
            nullable(values, GeneratedDescriptions::float64Array),
            ABSENT,
            flag,
            ABSENT,
            total
        );
    }

    private static TableField readPoint(Point point) {
        return table(
            float64(point.x()),
            float64(point.y()),
            scalar(point.hasZ(), float64(point.z()), Z_DEFAULT),
            nullable(point.label(), GeneratedDescriptions::string)
        );
    }

    private static NestedTable readEntry(Entry entry) {
        return vectorItem(
            uint16(entry.key()),
            nullable(entry.note(), GeneratedDescriptions::string),
            scalar(entry.hasWeight(), float32(entry.weight()), WEIGHT_DEFAULT)
        );
    }

    private static NestedTable readEntryV1(EntryV1 entry) {
        return vectorItem(uint16(entry.key()), nullable(entry.note(), GeneratedDescriptions::string));
    }

    /** {@return the fields of a sample, its count absent where {@code hasCount()} doesn't find it} */
    static List<FieldDescription> readSample(Sample sample) {
        return List.of(uint32(sample.key()), scalar(sample.hasCount(), uint16(sample.count()), COUNT_DEFAULT));
    }

    /**
     * {@return a scalar that may be missing, or an absent field where the table doesn't hold it},
     * after checking that its getter returned its default then
     */
    private static FieldDescription scalar(boolean has, FieldDescription value, FieldDescription defaultValue) {
        if (has) {
            return value;
        }

        assertEquals(defaultValue, value, "The default of a scalar that the table doesn't hold");
        return ABSENT;
    }

    /** {@return the description of a value that may be {@code null}, or an absent field} */
    private static <T> FieldDescription nullable(@Nullable T value, Function<T, ? extends FieldDescription> describe) {
        return value == null ? ABSENT : describe.apply(value);
    }

    // The tables that the generated code writes, in the conformance suite's model

    static TableDescription describeRecord(Record.Fields record) {
        var origin = record.origin();
        var entries = record.entries();

        return message(
            RECORD_LAYOUT,
            Record.FORMAT_VERSION,
            uint32(record.id()),
            nullable(record.name(), GeneratedDescriptions::string),
            nullable(record.values(), GeneratedDescriptions::float64Array),
            origin == null ? ABSENT : describePoint(origin),
            nullable(record.flag(), BoolField::new),
            entries == null ? ABSENT : vector(entries.stream().map(PresenceTables::describeEntry).toList()),
            nullable(record.total(), GeneratedDescriptions::int64),
            string(record.label()),
            nullable(record.extent(), PresenceTables::describeExtent)
        );
    }

    private static TableField describePoint(Point.Fields point) {
        return table(
            float64(point.x()),
            float64(point.y()),
            float64(point.z()),
            nullable(point.label(), GeneratedDescriptions::string)
        );
    }

    private static NestedTable describeEntry(Entry.Fields entry) {
        return vectorItem(
            uint16(entry.key()),
            nullable(entry.note(), GeneratedDescriptions::string),
            float32(entry.weight())
        );
    }

    private static StructField describeExtent(Extent extent) {
        return struct(List.of(float32(extent.width()), float32(extent.height())));
    }

    static TableDescription describeRecordV1(RecordV1.Fields record) {
        var origin = record.origin();
        var entries = record.entries();

        return message(
            RECORD_LAYOUT,
            RecordV1.FORMAT_VERSION,
            uint32(record.id()),
            nullable(record.name(), GeneratedDescriptions::string),
            nullable(record.values(), GeneratedDescriptions::float64Array),
            origin == null ? ABSENT : table(float64(origin.x()), float64(origin.y())),
            nullable(record.flag(), BoolField::new),
            entries == null ? ABSENT : vector(entries.stream().map(PresenceTables::describeEntryV1).toList()),
            nullable(record.total(), GeneratedDescriptions::int64)
        );
    }

    private static NestedTable describeEntryV1(EntryV1.Fields entry) {
        return vectorItem(uint16(entry.key()), nullable(entry.note(), GeneratedDescriptions::string));
    }

    /**
     * {@return the fields of a record of the first version as a reader of the second reads them}:
     * the fields that the second version added, to the record and to its nested tables, missing
     */
    static List<FieldDescription> describeFromFirstVersion(RecordV1.Fields record) {
        var origin = record.origin();
        var entries = record.entries();

        var fields = new ArrayList<>(describeRecordV1(record).fields());
        fields.set(3, origin == null ? ABSENT : table(float64(origin.x()), float64(origin.y()), ABSENT, ABSENT));
        fields.set(5, entries == null ? ABSENT : vector(entries.stream().map(PresenceTables::describeEntryFromV1).toList()));
        fields.add(ABSENT);
        fields.add(ABSENT);

        return fields;
    }

    private static NestedTable describeEntryFromV1(EntryV1.Fields entry) {
        return vectorItem(uint16(entry.key()), nullable(entry.note(), GeneratedDescriptions::string), ABSENT);
    }

    /** {@return the fields of a record of the first version, without those that the second added} */
    static RecordV1.Fields firstVersionOf(Record.Fields record) {
        var origin = record.origin();
        var entries = record.entries();

        return new RecordV1.Fields(
            record.id(),
            record.name(),
            record.values(),
            origin == null ? null : new PointV1.Fields(origin.x(), origin.y()),
            record.flag(),
            entries == null ? null : entries.stream().map(entry -> new EntryV1.Fields(entry.key(), entry.note())).toList(),
            record.total()
        );
    }

    // Arbitrary values of the fields, each of those that may be left out null half the time

    static Arbitrary<Record.Fields> recordFields() {
        var points = Combinators.combine(float64s(), float64s(), float64s(), optional(strings())).as(Point.Fields::new);
        var entries = Combinators.combine(uint16s(), optional(strings()), float32s()).as(Entry.Fields::new);
        var extents = Combinators.combine(float32s(), float32s()).as(Extent::new);

        // jqwik combines at most 8 arbitraries with their types
        var heads = Combinators.combine(uint32s(), optional(strings()), optional(float64Arrays()), optional(points))
            .as(Head::new);

        return Combinators.combine(
            heads,
            optional(Arbitraries.of(true, false)),
            optional(vectorsOf(entries)),
            optional(int64s()),
            strings(),
            optional(extents)
        ).as((head, flag, entryList, total, label, extent) -> new Record.Fields(
            head.id(),
            head.name(),
            head.values(),
            head.origin(),
            flag,
            entryList,
            total,
            label,
            extent
        ));
    }

    /** The first fields of a record, which jqwik generates apart from the others */
    @SuppressWarnings("ArrayRecordComponent") // Written, not compared
    private record Head(long id, @Nullable String name, double @Nullable [] values, Point.@Nullable Fields origin) {}

    private static <T> Arbitrary<@Nullable T> optional(Arbitrary<T> values) {
        return values.injectNull(0.5);
    }
}
