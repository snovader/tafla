package io.github.snovader.tafla.test;

import io.github.snovader.tafla.*;
import io.github.snovader.tafla.test.FieldDescription.*;
import net.jqwik.api.*;

import java.nio.charset.*;
import java.util.*;
import java.util.function.*;
import java.util.stream.*;

/**
 * Arbitrary descriptions of tables in the shape of the conformance suite's, of every part of the
 * format: layouts of every offset type, with and without each header slot and with versions that
 * fit, every scalar and array type over its whole range, strings of any code points, structs and
 * arrays of structs of any scalars, and tables nested from builders and bytes, in fields and
 * vectors, down to a bounded depth.
 *
 * <p>Like the suite, they follow the rules that {@code SPEC.md} and {@link TableBuilder} set out:
 * every table fits its offset type, a table nested with {@code setTable()} has no header, so that
 * it's read in place when its offsets are of its parent's offset type, and vector items have any
 * layout.</p>
 */
final class Descriptions {
    /** How many levels of tables a table nests, in fields and vectors */
    private static final int MAX_DEPTH = 2;
    private static final int MAX_FIELDS = 6;
    private static final int MAX_ELEMENTS = 8;
    private static final int MAX_CODE_POINTS = 8;
    private static final int MAX_STRUCT_SCALARS = 4;
    /** How many structs an array of structs holds at most */
    static final int MAX_STRUCTS = 4;
    /** How many tables a vector holds at most */
    static final int MAX_ITEMS = 3;

    /** Every layout: with and without each header slot, and of every offset type */
    private static final List<LayoutDescription> LAYOUTS = layoutsOf(List.of(false, true), List.of(false, true));
    /** The layouts of tables nested with {@code setTable()} */
    private static final List<LayoutDescription> HEADERLESS_LAYOUTS = layoutsOf(List.of(false), List.of(false));

    private Descriptions() {}

    private static List<LayoutDescription> layoutsOf(List<Boolean> sized, List<Boolean> versioned) {
        var layouts = new ArrayList<LayoutDescription>();
        for (var isSized : sized) {
            for (var isVersioned : versioned) {
                for (var offsetType : OffsetType.values()) {
                    layouts.add(new LayoutDescription(isSized, isVersioned, offsetType));
                }
            }
        }

        return List.copyOf(layouts);
    }

    /** {@return tables of any layout} */
    static Arbitrary<TableDescription> tables() {
        return tables(LAYOUTS, MAX_DEPTH, Nesting.OCCASIONAL);
    }

    /** {@return tables of the layouts that pass {@code filter}, or of a wider offset type} */
    static Arbitrary<TableDescription> tables(Predicate<LayoutDescription> filter) {
        return tables(LAYOUTS.stream().filter(filter).toList(), MAX_DEPTH, Nesting.OCCASIONAL);
    }

    /**
     * {@return tables of any layout that nest tables in about half their fields}, for properties of
     * nested tables, which need tables nested in nested tables too
     */
    static Arbitrary<TableDescription> nestingTables() {
        return tables(LAYOUTS, MAX_DEPTH, Nesting.FREQUENT);
    }

    /** {@return tables of uint8 and uint16 offsets, whose limits a test can reach} */
    static Arbitrary<TableDescription> narrowTables() {
        // A table too long for uint16 offsets is widened to uint32
        return tables(layout -> layout.offsetType() != OffsetType.UINT32)
            .filter(table -> table.layout().offsetType() != OffsetType.UINT32);
    }

    /** {@return tables without a header, which can be nested with {@code setTable()}} */
    static Arbitrary<TableDescription> headerlessTables() {
        return tables(HEADERLESS_LAYOUTS, MAX_DEPTH, Nesting.OCCASIONAL);
    }

    static Arbitrary<LayoutDescription> layouts() {
        return Arbitraries.of(LAYOUTS);
    }

    /**
     * {@return scalars, arrays, strings, structs and arrays of structs: fields that hold values
     * rather than tables}
     */
    static Arbitrary<FieldDescription> values() {
        return Arbitraries.frequencyOf(
            Tuple.of(4, scalars()),
            Tuple.of(4, arrays()),
            Tuple.of(2, stringFields()),
            Tuple.of(2, structs()),
            Tuple.of(2, structArrays())
        );
    }

    /** {@return whether the table fits its offset type, with as much padding as it could need} */
    static boolean fitsItsLayout(TableDescription table) {
        return maxLength(table) <= table.layout().offsetType().maxValue();
    }

    /**
     * {@return the most bytes that the table can take, with as much padding as each field's
     * alignment could need}, to keep descriptions within their offset type without asking the
     * builder under test
     */
    static long maxLength(TableDescription table) {
        return maxLength(table.layout(), table.fields());
    }

    private static long maxLength(LayoutDescription layout, List<FieldDescription> fields) {
        var width = layout.width();

        long length = layout.headerSize() + (long) fields.size() * width;
        for (var field : fields) {
            length += maxLength(field, width);
        }

        return length;
    }

    /** {@return the most bytes that the field can take in a table of offsets {@code width} bytes wide} */
    static long maxLength(FieldDescription field, int width) {
        return switch (field) {
            case ScalarField scalar -> scalar.type().size();
            case BoolField ignored -> Byte.BYTES;
            case ArrayField array -> array.type().size() - 1 + width + (long) array.values().length * array.type().size();
            case StringField string -> width + (long) string.value().getBytes(StandardCharsets.UTF_8).length;
            case StructField struct -> StructBytes.size(struct.types());
            case StructArrayField array -> StructBytes.alignment(array.fields()) - 1 + width
                + (long) array.items().size() * StructBytes.size(array.fields());
            case TableField table -> Long.BYTES - 1 + maxLength(table.table());
            case VectorField vector -> width + vector.items().stream()
                .mapToLong(item -> width + maxLength(item.table()))
                .sum();
        };
    }

    /**
     * {@return tables of the layouts, with fields that nest tables {@code depth} levels deep}. The
     * fields and the header are generated apart, rather than one from the other with
     * {@code flatMap()}, which made generating them ten times slower.
     */
    private static Arbitrary<TableDescription> tables(List<LayoutDescription> layouts, int depth, Nesting nesting) {
        var fields = fields(depth, nesting).list().ofMaxSize(MAX_FIELDS);

        // The limits of the values are generated often already, and a table's edge cases would be
        // generated as often again, mostly of a field or two
        return Combinators.combine(headers(layouts), fields).as(Descriptions::fitted).withoutEdgeCases();
    }

    /** The layout and version of a table */
    private record Header(LayoutDescription layout, OptionalLong version) {}

    /** {@return the layouts, each with versions that fit its slot} */
    private static Arbitrary<Header> headers(List<LayoutDescription> layouts) {
        var headers = new ArrayList<Arbitrary<Header>>();
        for (var layout : layouts) {
            headers.add(versions(layout).map(version -> new Header(layout, version)));
        }

        return Arbitraries.oneOf(headers);
    }

    /** {@return versions that fit the layout's slot, often the smallest and the largest} */
    private static Arbitrary<OptionalLong> versions(LayoutDescription layout) {
        if (!layout.versioned()) {
            return Arbitraries.just(OptionalLong.empty());
        }

        var max = layout.offsetType().maxValue();
        return Arbitraries.frequencyOf(
            Tuple.of(1, Arbitraries.of(0L, max)),
            Tuple.of(4, Arbitraries.longs().between(0, max))
        ).map(OptionalLong::of);
    }

    /**
     * {@return a table of the header and fields}, with offsets of the header's offset type or,
     * when the fields don't fit them, the next wider type that they fit. A version still fits the
     * wider slot. Widening the offsets rather than rejecting the table keeps generating tables
     * fast, and small tables still take every offset type.
     */
    private static TableDescription fitted(Header header, List<FieldDescription> fields) {
        var layout = header.layout();
        while (maxLength(layout, fields) > layout.offsetType().maxValue()) {
            layout = new LayoutDescription(layout.sized(), layout.versioned(), wider(layout.offsetType()));
        }

        return new TableDescription(layout, header.version(), fields);
    }

    private static OffsetType wider(OffsetType offsetType) {
        return switch (offsetType) {
            case UINT8 -> OffsetType.UINT16;
            case UINT16 -> OffsetType.UINT32;
            case UINT32 -> throw new IllegalStateException("The fields don't fit in uint32 offsets");
        };
    }

    /** How often a field nests tables, in a table field or a vector, rather than holding a value */
    private enum Nesting {
        /** One field in twelve */
        OCCASIONAL(10),
        /** One field in two */
        FREQUENT(2);

        /** The weight of fields that hold values, against 1 each for table fields and vectors */
        private final int valueWeight;

        Nesting(int valueWeight) {
            this.valueWeight = valueWeight;
        }
    }

    private static Arbitrary<FieldDescription> fields(int depth, Nesting nesting) {
        if (depth == 0) {
            return values();
        }

        return Arbitraries.frequencyOf(
            Tuple.of(nesting.valueWeight, values()),
            Tuple.of(1, tableFields(depth - 1, nesting)),
            Tuple.of(1, vectors(depth - 1, nesting))
        );
    }

    private static Arbitrary<FieldDescription> scalars() {
        return scalarFields().map(scalar -> scalar);
    }

    private static Arbitrary<ScalarField> scalarFields() {
        var scalars = new ArrayList<Arbitrary<ScalarField>>();
        for (var type : ScalarType.values()) {
            scalars.add(values(type).map(value -> new ScalarField(type, value)));
        }

        return Arbitraries.oneOf(scalars);
    }

    /** {@return structs of one to a few scalars of any types} */
    private static Arbitrary<FieldDescription> structs() {
        return scalarFields().list().ofMinSize(1).ofMaxSize(MAX_STRUCT_SCALARS).map(StructField::new);
    }

    /** {@return arrays of structs of one to a few scalars of any types, empty too} */
    static Arbitrary<FieldDescription> structArrays() {
        var types = Arbitraries.of(ScalarType.class).list().ofMinSize(1).ofMaxSize(MAX_STRUCT_SCALARS);
        return types.flatMap(Descriptions::structArraysOf);
    }

    /** {@return arrays of structs of scalars of the types, empty too} */
    private static Arbitrary<FieldDescription> structArraysOf(List<ScalarType> types) {
        return structValues(types).list().ofMaxSize(MAX_STRUCTS).map(items -> new StructArrayField(types, items));
    }

    /** {@return the values of a struct of scalars of the types} */
    private static Arbitrary<List<Long>> structValues(List<ScalarType> types) {
        return Combinators.combine(types.stream().map(Descriptions::values).toList()).as(values -> values);
    }

    private static Arbitrary<FieldDescription> arrays() {
        var arrays = new ArrayList<Arbitrary<FieldDescription>>();
        for (var type : ScalarType.values()) {
            arrays.add(arrays(type).map(array -> new ArrayField(type, array)));
        }

        return Arbitraries.oneOf(arrays);
    }

    /** {@return arrays of values of the type as {@link ScalarType} holds them, empty too} */
    static Arbitrary<long[]> arrays(ScalarType type) {
        return values(type).array(long[].class).ofMaxSize(MAX_ELEMENTS);
    }

    /**
     * {@return values of the type as {@link ScalarType} holds them, over its whole range and often
     * at its limits}
     */
    static Arbitrary<Long> values(ScalarType type) {
        return Arbitraries.frequencyOf(Tuple.of(1, Arbitraries.of(limits(type))), Tuple.of(4, range(type)));
    }

    /**
     * {@return the limits of the type's range, as {@link ScalarType} holds them}: an integer's
     * smallest and largest value and zero, and a float's zeros, NaN, infinities, smallest
     * subnormal and normal values, and largest values. A NaN is the quiet NaN, the one NaN that
     * {@code cases.json} names.
     */
    static List<Long> limits(ScalarType type) {
        return switch (type) {
            case INT8 -> List.of((long) Byte.MIN_VALUE, 0L, (long) Byte.MAX_VALUE);
            case UINT8 -> List.of(0L, 0xffL);
            case INT16 -> List.of((long) Short.MIN_VALUE, 0L, (long) Short.MAX_VALUE);
            case UINT16 -> List.of(0L, 0xffffL);
            case INT32 -> List.of((long) Integer.MIN_VALUE, 0L, (long) Integer.MAX_VALUE);
            case UINT32 -> List.of(0L, 0xffff_ffffL);
            // A uint64 is held as its bits, so -1 is its largest value and Long.MIN_VALUE is 2^63
            case INT64, UINT64 -> List.of(Long.MIN_VALUE, -1L, 0L, 1L, Long.MAX_VALUE);
            case FLOAT32 -> Stream.of(
                0.0f,
                -0.0f,
                Float.NaN,
                Float.POSITIVE_INFINITY,
                Float.NEGATIVE_INFINITY,
                Float.MIN_VALUE,
                Float.MIN_NORMAL,
                Float.MAX_VALUE,
                -Float.MAX_VALUE
            ).map(value -> (long) Float.floatToIntBits(value)).toList();
            case FLOAT64 -> Stream.of(
                0.0,
                -0.0,
                Double.NaN,
                Double.POSITIVE_INFINITY,
                Double.NEGATIVE_INFINITY,
                Double.MIN_VALUE,
                Double.MIN_NORMAL,
                Double.MAX_VALUE,
                -Double.MAX_VALUE
            ).map(Double::doubleToLongBits).toList();
        };
    }

    /**
     * {@return any value of the type}, with floats from uniformly distributed bits, which span
     * every exponent. {@code floatToIntBits()} and {@code doubleToLongBits()}, rather than the raw
     * bits, make every NaN the quiet NaN.
     */
    private static Arbitrary<Long> range(ScalarType type) {
        return switch (type) {
            case INT8 -> Arbitraries.longs().between(Byte.MIN_VALUE, Byte.MAX_VALUE);
            case UINT8 -> Arbitraries.longs().between(0, 0xff);
            case INT16 -> Arbitraries.longs().between(Short.MIN_VALUE, Short.MAX_VALUE);
            case UINT16 -> Arbitraries.longs().between(0, 0xffff);
            case INT32 -> Arbitraries.longs().between(Integer.MIN_VALUE, Integer.MAX_VALUE);
            case UINT32 -> Arbitraries.longs().between(0, 0xffff_ffffL);
            case INT64, UINT64 -> Arbitraries.longs();
            case FLOAT32 -> Arbitraries.integers()
                .withDistribution(RandomDistribution.uniform())
                .map(bits -> (long) Float.floatToIntBits(Float.intBitsToFloat(bits)));
            case FLOAT64 -> Arbitraries.longs()
                .withDistribution(RandomDistribution.uniform())
                .map(bits -> Double.doubleToLongBits(Double.longBitsToDouble(bits)));
        };
    }

    private static Arbitrary<FieldDescription> stringFields() {
        return strings().map(StringField::new);
    }

    /** {@return strings of code points of every UTF-8 length, empty too} */
    static Arbitrary<String> strings() {
        return codePoints().list().ofMaxSize(MAX_CODE_POINTS).map(Descriptions::stringOf);
    }

    private static String stringOf(List<Integer> codePoints) {
        var string = new StringBuilder();
        codePoints.forEach(string::appendCodePoint);

        return string.toString();
    }

    /**
     * {@return code points of every UTF-8 length, without the surrogates, which a string can't
     * hold alone}
     */
    private static Arbitrary<Integer> codePoints() {
        return Arbitraries.frequencyOf(
            // ASCII, NUL included
            Tuple.of(4, Arbitraries.integers().between(0, 0x7f)),
            Tuple.of(2, Arbitraries.integers().between(0x80, 0xffff).filter(codePoint -> !isSurrogate(codePoint))),
            // Outside the Basic Multilingual Plane, as a surrogate pair in Java
            Tuple.of(2, Arbitraries.integers().between(0x1_0000, Character.MAX_CODE_POINT)),
            // U+FEFF, which is part of a string rather than a byte order mark, and U+FFFD, which a
            // reader must tell from invalid bytes that it would replace
            Tuple.of(1, Arbitraries.of(0xfeff, 0xfffd))
        );
    }

    private static boolean isSurrogate(int codePoint) {
        return codePoint >= Character.MIN_SURROGATE && codePoint <= Character.MAX_SURROGATE;
    }

    private static Arbitrary<FieldDescription> tableFields(int depth, Nesting nesting) {
        var alignments = Arbitraries.oneOf(
            Arbitraries.just(OptionalInt.empty()),
            Arbitraries.of(1, 2, 4, 8).map(OptionalInt::of)
        );

        return Combinators.combine(Arbitraries.of(Source.class), tables(HEADERLESS_LAYOUTS, depth, nesting), alignments)
            .as(TableField::new);
    }

    private static Arbitrary<FieldDescription> vectors(int depth, Nesting nesting) {
        return Combinators.combine(Arbitraries.of(Source.class), tables(LAYOUTS, depth, nesting))
            .as(NestedTable::new)
            .list()
            .ofMaxSize(MAX_ITEMS)
            .map(VectorField::new);
    }
}
