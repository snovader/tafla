package io.github.snovader.tafla.test;

import io.github.snovader.tafla.OffsetType;
import io.github.snovader.tafla.TableBuilder;
import io.github.snovader.tafla.test.FieldDescription.*;

import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import java.util.*;

/**
 * The fields of the messages, tables and structs that tafla-gen generates, in the conformance
 * suite's description model, so that {@link Tables#build} writes the same tables by hand with
 * {@link TableBuilder}, as the suite builds its tables, for tests to compare with the bytes that
 * the generated code writes.
 *
 * <p>Descriptions hold values as {@link ScalarType} does, floats as their raw bits, and compare
 * arrays by their elements, so describing the fields that were written and the fields that were
 * read back compares them by their bits, unlike the generated {@code Fields} records, whose
 * equality compares arrays by identity.</p>
 */
final class GeneratedDescriptions {
    /** The layout of tables and of messages without attributes */
    static final LayoutDescription DEFAULT_LAYOUT = LayoutDescription.headerless(OffsetType.UINT32);

    private GeneratedDescriptions() {}

    // Scalars, in the Java types that the generated code takes and returns them as: unsigned
    // integers widened, and a uint64 as a long of its bits

    static ScalarField int8(byte value) {
        return new ScalarField(ScalarType.INT8, value);
    }

    static ScalarField uint8(short value) {
        return new ScalarField(ScalarType.UINT8, value);
    }

    static ScalarField int16(short value) {
        return new ScalarField(ScalarType.INT16, value);
    }

    static ScalarField uint16(int value) {
        return new ScalarField(ScalarType.UINT16, value);
    }

    static ScalarField int32(int value) {
        return new ScalarField(ScalarType.INT32, value);
    }

    static ScalarField uint32(long value) {
        return new ScalarField(ScalarType.UINT32, value);
    }

    static ScalarField int64(long value) {
        return new ScalarField(ScalarType.INT64, value);
    }

    static ScalarField uint64(long value) {
        return new ScalarField(ScalarType.UINT64, value);
    }

    static ScalarField float32(float value) {
        return new ScalarField(ScalarType.FLOAT32, Float.floatToRawIntBits(value));
    }

    static ScalarField float64(double value) {
        return new ScalarField(ScalarType.FLOAT64, Double.doubleToRawLongBits(value));
    }

    // Arrays, which hold their elements as ScalarType does, and the arrays of longs as they are

    static ArrayField int8Array(byte[] values) {
        return new ArrayField(ScalarType.INT8, ScalarType.fromBytes(values));
    }

    static ArrayField uint8Array(short[] values) {
        return new ArrayField(ScalarType.UINT8, ScalarType.fromShorts(values));
    }

    static ArrayField int16Array(short[] values) {
        return new ArrayField(ScalarType.INT16, ScalarType.fromShorts(values));
    }

    static ArrayField uint16Array(int[] values) {
        return new ArrayField(ScalarType.UINT16, ScalarType.fromInts(values));
    }

    static ArrayField int32Array(int[] values) {
        return new ArrayField(ScalarType.INT32, ScalarType.fromInts(values));
    }

    static ArrayField uint32Array(long[] values) {
        return new ArrayField(ScalarType.UINT32, values);
    }

    static ArrayField int64Array(long[] values) {
        return new ArrayField(ScalarType.INT64, values);
    }

    static ArrayField uint64Array(long[] values) {
        return new ArrayField(ScalarType.UINT64, values);
    }

    static ArrayField float32Array(float[] values) {
        return new ArrayField(ScalarType.FLOAT32, ScalarType.fromFloats(values));
    }

    static ArrayField float64Array(double[] values) {
        return new ArrayField(ScalarType.FLOAT64, ScalarType.fromDoubles(values));
    }

    static BoolArrayField boolArray(boolean[] values) {
        return new BoolArrayField(values);
    }

    static StringField string(String value) {
        return new StringField(value);
    }

    /**
     * A struct of the scalars in the order of their offsets, as generated code sets it from its
     * record's bytes
     */
    static StructField struct(List<ScalarField> scalars) {
        return new StructField(scalars);
    }

    /**
     * An array of structs of scalars of the types, as generated code sets it from its records' bytes,
     * with each struct's scalars in the order of their offsets
     *
     * @throws IllegalArgumentException if a struct's scalars aren't of the types
     */
    static StructArrayField structArray(List<ScalarType> types, List<List<ScalarField>> items) {
        var values = new ArrayList<List<Long>>(items.size());
        for (var scalars : items) {
            var item = struct(scalars);
            if (!item.types().equals(types)) {
                throw new IllegalArgumentException(String.format("%s isn't a struct of %s", item, types));
            }

            values.add(item.values());
        }

        return new StructArrayField(types, values);
    }

    /** A table nested from its builder at its default alignment, as generated code nests it */
    static TableField table(FieldDescription... fields) {
        return new TableField(Source.BUILDER, tableOf(fields), OptionalInt.empty());
    }

    /** A table in a vector, from its builder, as generated code writes it */
    static NestedTable vectorItem(FieldDescription... fields) {
        return new NestedTable(Source.BUILDER, tableOf(fields));
    }

    static VectorField vector(List<NestedTable> items) {
        return new VectorField(items);
    }

    static StringVectorField stringVector(List<String> values) {
        return new StringVectorField(values);
    }

    private static TableDescription tableOf(FieldDescription... fields) {
        return new TableDescription(DEFAULT_LAYOUT, OptionalLong.empty(), List.of(fields));
    }

    /** A message's table of a layout without a version */
    static TableDescription message(LayoutDescription layout, FieldDescription... fields) {
        return new TableDescription(layout, OptionalLong.empty(), List.of(fields));
    }

    /** A message's table of a versioned layout */
    static TableDescription message(LayoutDescription layout, long version, FieldDescription... fields) {
        return new TableDescription(layout, OptionalLong.of(version), List.of(fields));
    }

    /**
     * {@return the bytes of a struct of the fields}, each at the offset after the one before,
     * without padding, and little-endian, as the JDK's own buffer writes them rather than tafla
     */
    static byte[] structBytes(List<ScalarField> fields) {
        var size = fields.stream().mapToInt(field -> field.type().size()).sum();

        var bytes = ByteBuffer.allocate(size).order(ByteOrder.LITTLE_ENDIAN);
        for (var field : fields) {
            field.type().put(bytes, field.value());
        }

        return bytes.array();
    }
}
