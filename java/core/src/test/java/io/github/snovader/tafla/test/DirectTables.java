package io.github.snovader.tafla.test;

import io.github.snovader.tafla.*;
import io.github.snovader.tafla.test.FieldDescription.*;

import java.util.*;
import java.util.function.*;

/**
 * Builds tables from their descriptions with {@link DirectTableBuilder}, making the calls that
 * {@link Tables#build} makes of a {@link TableBuilder}, or those of {@link InPlaceTables}, with the
 * same {@link FillFaults}. Every builder keeps runs of {@code segmentLength} bytes or more as
 * segments.
 */
final class DirectTables {
    private final int segmentLength;
    private final FillFaults faults;

    /** Sets a table in place, with the setter of a builder or of an item */
    @FunctionalInterface
    private interface TableSetter {
        void setTable(int fields, Consumer<DirectTableBuilder.Item> fill);
    }

    /** Sets a vector's items in place, with the setter of a builder or of an item */
    @FunctionalInterface
    private interface VectorSetter {
        void setVector(int count, int fields, ObjIntConsumer<DirectTableBuilder.Item> fill);
    }

    DirectTables(int segmentLength) {
        this(segmentLength, FillFaults.none());
    }

    DirectTables(int segmentLength, FillFaults faults) {
        this.segmentLength = segmentLength;
        this.faults = faults;
    }

    /** {@return a builder of the table with all its fields set, as {@link Tables#build} sets them} */
    DirectTableBuilder build(TableDescription table) {
        var builder = createBuilder(table, table.fields().size());
        for (var field : table.fields()) {
            setField(builder, field);
        }

        return builder;
    }

    /** {@return a builder of the table with all its fields set, as {@link InPlaceTables} sets them} */
    DirectTableBuilder buildInPlace(TableDescription table) {
        var builder = createBuilder(table, table.fields().size());
        for (var field : table.fields()) {
            setFieldInPlace(builder, field, table.layout().offsetType());
        }

        return builder;
    }

    /** {@return a builder of the table's layout and version, of {@code fields} fields, none set yet} */
    DirectTableBuilder createBuilder(TableDescription table, int fields) {
        var layout = table.layout().tableLayout();
        var version = table.version();

        var builder = version.isPresent()
            ? new DirectTableBuilder(fields, layout, version.getAsLong())
            : new DirectTableBuilder(fields, layout);
        return builder.segmentsFrom(segmentLength);
    }

    /** Sets the field as {@link Tables#setField} sets it, building a nested table first */
    void setField(DirectTableBuilder builder, FieldDescription field) {
        switch (field) {
            case ScalarField scalar -> setScalar(builder, scalar.type(), scalar.value());
            case BoolField bool -> builder.setUint8(bool.uint8());
            case ArrayField array -> setArray(builder, array.type(), array.values());
            case StringField string -> builder.setString(string.value());
            case StructField struct -> builder.setStruct(StructBytes.pack(struct.types(), List.of(struct.values())));
            case StructArrayField array -> builder.setStructArray(
                StructBytes.pack(array.fields(), array.items()),
                StructBytes.size(array.fields()),
                StructBytes.alignment(array.fields())
            );
            case TableField table -> setTable(builder, table);
            case VectorField vector -> setVector(builder, vector.items());
        }
    }

    /** Sets the field as {@link InPlaceTables#setField} sets it */
    void setFieldInPlace(DirectTableBuilder builder, FieldDescription field, OffsetType offsetType) {
        switch (field) {
            case TableField nested when InPlaceTables.recordsInPlace(nested, offsetType) ->
                setTable(builder::setTable, nested.table(), offsetType);
            case VectorField vector when InPlaceTables.recordsInPlace(vector, offsetType) ->
                setVector(builder::setVector, vector.items(), offsetType);
            default -> setField(builder, field);
        }
    }

    private void setTable(DirectTableBuilder builder, TableField field) {
        var nested = build(field.table());
        var alignment = field.alignment();

        switch (field.from()) {
            case BUILDER -> {
                if (alignment.isPresent()) {
                    builder.setTable(nested, alignment.getAsInt());
                } else {
                    builder.setTable(nested);
                }
            }
            case BYTES -> {
                if (alignment.isPresent()) {
                    builder.setTable(nested.bytes(), alignment.getAsInt());
                } else {
                    builder.setTable(nested.bytes());
                }
            }
        }
    }

    private void setVector(DirectTableBuilder builder, List<NestedTable> items) {
        var builders = items.stream().map(item -> build(item.table())).toList();

        if (items.stream().allMatch(item -> item.from() == Source.BUILDER)) {
            builder.setVector(builders);
        } else {
            builder.setVector(builders.stream().map(DirectTableBuilder::bytes).toArray(byte[][]::new));
        }
    }

    /** Sets the field on the item as {@link InPlaceTables} sets it */
    private void setItemField(DirectTableBuilder.Item item, FieldDescription field, OffsetType offsetType) {
        switch (field) {
            case ScalarField scalar -> setScalar(item, scalar.type(), scalar.value());
            case BoolField bool -> item.setUint8(bool.uint8());
            case ArrayField array -> setArray(item, array.type(), array.values());
            case StringField string -> item.setString(string.value());
            case StructField struct -> item.setStruct(StructBytes.pack(struct.types(), List.of(struct.values())));
            case StructArrayField array -> item.setStructArray(
                StructBytes.pack(array.fields(), array.items()),
                StructBytes.size(array.fields()),
                StructBytes.alignment(array.fields())
            );
            case TableField nested when InPlaceTables.recordsInPlace(nested, offsetType) ->
                setTable(item::setTable, nested.table(), offsetType);
            case TableField nested -> setItemTable(item, nested);
            case VectorField vector when InPlaceTables.recordsInPlace(vector, offsetType) ->
                setVector(item::setVector, vector.items(), offsetType);
            case VectorField vector -> setItemVector(item, vector.items());
        }
    }

    private void setItemTable(DirectTableBuilder.Item item, TableField field) {
        var nested = build(field.table());
        var alignment = field.alignment();

        switch (field.from()) {
            case BUILDER -> {
                if (alignment.isPresent()) {
                    item.setTable(nested, alignment.getAsInt());
                } else {
                    item.setTable(nested);
                }
            }
            case BYTES -> {
                if (alignment.isPresent()) {
                    item.setTable(nested.bytes(), alignment.getAsInt());
                } else {
                    item.setTable(nested.bytes());
                }
            }
        }
    }

    private void setItemVector(DirectTableBuilder.Item item, List<NestedTable> items) {
        var builders = items.stream().map(nested -> build(nested.table())).toList();

        if (items.stream().allMatch(nested -> nested.from() == Source.BUILDER)) {
            item.setVector(builders);
        } else {
            item.setVector(builders.stream().map(DirectTableBuilder::bytes).toArray(byte[][]::new));
        }
    }

    private Consumer<DirectTableBuilder.Item> fill(TableDescription table, OffsetType offsetType) {
        return item -> {
            for (var field : table.fields()) {
                setItemField(item, field, offsetType);
            }
        };
    }

    private void setTable(TableSetter setter, TableDescription table, OffsetType offsetType) {
        var fault = faults.next();
        var fill = fill(table, offsetType);

        setter.setTable(FillFaults.declared(fault, table.fields().size()), item -> {
            fill.accept(item);
            FillFaults.afterFill(fault, true);
        });
    }

    private void setVector(VectorSetter setter, List<NestedTable> items, OffsetType offsetType) {
        var fault = faults.next();
        var fields = items.isEmpty() ? 0 : items.getFirst().table().fields().size();

        setter.setVector(items.size(), FillFaults.declared(fault, fields), (item, index) -> {
            fill(items.get(index).table(), offsetType).accept(item);
            FillFaults.afterFill(fault, index == items.size() - 1);
        });
    }

    private static void setScalar(DirectTableBuilder builder, ScalarType type, long value) {
        switch (type) {
            case INT8 -> builder.setInt8((byte) value);
            case UINT8 -> builder.setUint8((short) value);
            case INT16 -> builder.setInt16((short) value);
            case UINT16 -> builder.setUint16((int) value);
            case INT32 -> builder.setInt32((int) value);
            case UINT32 -> builder.setUint32(value);
            case INT64 -> builder.setInt64(value);
            case UINT64 -> builder.setUint64(value);
            case FLOAT32 -> builder.setFloat32(Float.intBitsToFloat((int) value));
            case FLOAT64 -> builder.setFloat64(Double.longBitsToDouble(value));
        }
    }

    private static void setArray(DirectTableBuilder builder, ScalarType type, long[] values) {
        switch (type) {
            case INT8 -> builder.setInt8Array(ScalarType.toBytes(values));
            case UINT8 -> builder.setUint8Array(ScalarType.toShorts(values));
            case INT16 -> builder.setInt16Array(ScalarType.toShorts(values));
            case UINT16 -> builder.setUint16Array(ScalarType.toInts(values));
            case INT32 -> builder.setInt32Array(ScalarType.toInts(values));
            case UINT32 -> builder.setUint32Array(values);
            case INT64 -> builder.setInt64Array(values);
            case UINT64 -> builder.setUint64Array(values);
            case FLOAT32 -> builder.setFloat32Array(ScalarType.toFloats(values));
            case FLOAT64 -> builder.setFloat64Array(ScalarType.toDoubles(values));
        }
    }

    private static void setScalar(DirectTableBuilder.Item item, ScalarType type, long value) {
        switch (type) {
            case INT8 -> item.setInt8((byte) value);
            case UINT8 -> item.setUint8((short) value);
            case INT16 -> item.setInt16((short) value);
            case UINT16 -> item.setUint16((int) value);
            case INT32 -> item.setInt32((int) value);
            case UINT32 -> item.setUint32(value);
            case INT64 -> item.setInt64(value);
            case UINT64 -> item.setUint64(value);
            case FLOAT32 -> item.setFloat32(Float.intBitsToFloat((int) value));
            case FLOAT64 -> item.setFloat64(Double.longBitsToDouble(value));
        }
    }

    private static void setArray(DirectTableBuilder.Item item, ScalarType type, long[] values) {
        switch (type) {
            case INT8 -> item.setInt8Array(ScalarType.toBytes(values));
            case UINT8 -> item.setUint8Array(ScalarType.toShorts(values));
            case INT16 -> item.setInt16Array(ScalarType.toShorts(values));
            case UINT16 -> item.setUint16Array(ScalarType.toInts(values));
            case INT32 -> item.setInt32Array(ScalarType.toInts(values));
            case UINT32 -> item.setUint32Array(values);
            case INT64 -> item.setInt64Array(values);
            case UINT64 -> item.setUint64Array(values);
            case FLOAT32 -> item.setFloat32Array(ScalarType.toFloats(values));
            case FLOAT64 -> item.setFloat64Array(ScalarType.toDoubles(values));
        }
    }
}
