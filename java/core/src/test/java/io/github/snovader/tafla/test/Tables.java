package io.github.snovader.tafla.test;

import io.github.snovader.tafla.*;
import io.github.snovader.tafla.test.FieldDescription.*;

import java.util.*;
import java.util.function.*;

/**
 * Builds tables as their descriptions set them out, as every implementation's conformance test does,
 * with {@link TableBuilder}, or with {@link TableSizer} and {@link TableWriter}
 */
final class Tables {
    private Tables() {}

    /** {@return a builder of the table with all its fields set} */
    static TableBuilder build(TableDescription table) {
        var builder = createBuilder(table);
        for (var field : table.fields()) {
            setField(builder, field);
        }

        return builder;
    }

    /** {@return a builder of the table's layout and version, with none of its fields set yet} */
    static TableBuilder createBuilder(TableDescription table) {
        var fields = table.fields().size();
        var layout = table.layout().tableLayout();
        var version = table.version();

        return version.isPresent()
            ? new TableBuilder(fields, layout, version.getAsLong())
            : new TableBuilder(fields, layout);
    }

    /** Sets the field on the builder with the setter of its type, building a nested table first */
    static void setField(TableBuilder builder, FieldDescription field) {
        switch (field) {
            case ScalarField scalar -> scalar.type().set(builder, scalar.value());
            case BoolField bool -> builder.setUint8(bool.uint8());
            case ArrayField array -> array.type().setArray(builder, array.values());
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

    private static void setTable(TableBuilder builder, TableField field) {
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

    /**
     * Sets a vector from its items' builders when they're all builders. {@link TableBuilder} takes a
     * vector of builders or one of bytes, so a vector with bytes is set from every item's bytes, which
     * a builder writes as its parent would.
     */
    private static void setVector(TableBuilder builder, List<NestedTable> items) {
        var builders = items.stream().map(item -> build(item.table())).toList();

        if (items.stream().allMatch(item -> item.from() == Source.BUILDER)) {
            builder.setVector(builders);
        } else {
            builder.setVector(builders.stream().map(TableBuilder::bytes).toArray(byte[][]::new));
        }
    }

    /**
     * {@return the bytes of the table, sized with {@link TableSizer} and written with
     * {@link TableWriter}}. They write tables nested in it in place, so those must have no header
     * and its offset type, as {@link #nestedInPlace} makes them.
     */
    static byte[] write(TableDescription table) {
        return write(size(table).writer(), table);
    }

    /** {@return a sizer of the table's layout and version, with all its fields added} */
    static TableSizer size(TableDescription table) {
        var fields = table.fields().size();
        var layout = table.layout().tableLayout();
        var version = table.version();

        var sizer = version.isPresent()
            ? new TableSizer(fields, layout, version.getAsLong())
            : new TableSizer(fields, layout);
        for (var field : table.fields()) {
            addField(sizer, field);
        }

        return sizer;
    }

    /** {@return the bytes that the writer writes from the table's fields, each with its index} */
    static byte[] write(TableWriter writer, TableDescription table) {
        return write(writer, table, IntUnaryOperator.identity());
    }

    /**
     * {@return the bytes that the writer writes from the table's fields}, with each field's index
     * and each item end's item passed through {@code indices}, which can change them
     */
    static byte[] write(TableWriter writer, TableDescription table, IntUnaryOperator indices) {
        writeFields(writer, table.fields(), indices);
        return writer.bytes();
    }

    /** Writes the table's fields with the writer, each with its index, which may be followed by more */
    static void writeFields(TableWriter writer, TableDescription table) {
        writeFields(writer, table.fields(), IntUnaryOperator.identity());
    }

    /**
     * {@return the description with every table nested in it made one that a {@link TableWriter}
     * writes in place}: from its builder, without a header, and with the root's offset type
     */
    static TableDescription nestedInPlace(TableDescription table) {
        var fields = fieldsInPlace(table.fields(), table.layout().offsetType());
        return new TableDescription(table.layout(), table.version(), fields);
    }

    private static List<FieldDescription> fieldsInPlace(List<FieldDescription> fields, OffsetType offsetType) {
        return fields.stream().map(field -> fieldInPlace(field, offsetType)).toList();
    }

    private static FieldDescription fieldInPlace(FieldDescription field, OffsetType offsetType) {
        return switch (field) {
            case TableField table -> new TableField(
                Source.BUILDER,
                tableInPlace(table.table(), offsetType),
                table.alignment()
            );
            case VectorField vector -> new VectorField(
                vector.items().stream().map(item -> itemInPlace(item, offsetType)).toList()
            );
            default -> field;
        };
    }

    private static NestedTable itemInPlace(NestedTable item, OffsetType offsetType) {
        return new NestedTable(Source.BUILDER, tableInPlace(item.table(), offsetType));
    }

    private static TableDescription tableInPlace(TableDescription nested, OffsetType offsetType) {
        var layout = LayoutDescription.headerless(offsetType);
        return new TableDescription(layout, OptionalLong.empty(), fieldsInPlace(nested.fields(), offsetType));
    }

    /** Adds the field to the sizer with the method of its type, and then a nested table's fields */
    private static void addField(TableSizer sizer, FieldDescription field) {
        switch (field) {
            case ScalarField scalar -> scalar.type().add(sizer);
            case BoolField ignored -> sizer.addUint8();
            case ArrayField array -> array.type().addArray(sizer, array.values().length);
            case StringField string -> sizer.addString(string.value());
            case StructField struct -> sizer.addStruct(StructBytes.size(struct.types()));
            case StructArrayField array -> sizer.addStructArray(
                array.items().size(),
                StructBytes.size(array.fields()),
                StructBytes.alignment(array.fields())
            );
            case TableField table -> {
                sizer.addTable(alignmentOf(table));
                addNested(sizer, table.table());
            }
            case VectorField vector -> {
                sizer.addVector(vector.items().size());
                for (var item : vector.items()) {
                    addNested(sizer, item.table());
                }
            }
        }
    }

    private static void addNested(TableSizer sizer, TableDescription table) {
        var parent = sizer.startTable(table.fields().size());
        for (var field : table.fields()) {
            addField(sizer, field);
        }
        sizer.endTable(parent);
    }

    private static void writeFields(TableWriter writer, List<FieldDescription> fields, IntUnaryOperator indices) {
        for (int index = 0; index < fields.size(); index++) {
            writeField(writer, index, fields.get(index), indices);
        }
    }

    /** Writes field {@code index} with the setter of its type, and then a nested table's fields */
    private static void writeField(TableWriter writer, int fieldIndex, FieldDescription field, IntUnaryOperator indices) {
        var index = indices.applyAsInt(fieldIndex);
        switch (field) {
            case ScalarField scalar -> scalar.type().set(writer, index, scalar.value());
            case BoolField bool -> writer.setUint8(index, bool.uint8());
            case ArrayField array -> array.type().setArray(writer, index, array.values());
            case StringField string -> writer.setString(index, string.value());
            case StructField struct -> writer.setStruct(index, StructBytes.pack(struct.types(), List.of(struct.values())));
            case StructArrayField array -> writer.setStructArray(
                index,
                StructBytes.pack(array.fields(), array.items()),
                StructBytes.size(array.fields()),
                StructBytes.alignment(array.fields())
            );
            case TableField table -> {
                writer.setTable(index, alignmentOf(table));
                writeNested(writer, table.table(), indices);
            }
            case VectorField vector -> writeVector(writer, index, vector.items(), indices);
        }
    }

    private static void writeVector(
        TableWriter writer,
        int index,
        List<NestedTable> items,
        IntUnaryOperator indices
    ) {
        var vector = writer.setVector(index, items.size());
        for (int item = 0; item < items.size(); item++) {
            writeNested(writer, items.get(item).table(), indices);
            writer.setItemEnd(vector, indices.applyAsInt(item));
        }
    }

    private static void writeNested(TableWriter writer, TableDescription table, IntUnaryOperator indices) {
        var parent = writer.startTable(table.fields().size());
        writeFields(writer, table.fields(), indices);
        writer.endTable(parent);
    }

    /**
     * {@return the alignment that {@code SPEC.md} gives a nested table}: the one passed to
     * {@code setTable()}, or by default 4 for bytes, and for a builder the largest alignment of its
     * arrays, arrays of structs and own nested tables, or 1 if it has none of them
     */
    static int alignmentOf(TableField field) {
        if (field.alignment().isPresent()) {
            return field.alignment().getAsInt();
        }

        if (field.from() == Source.BYTES) {
            return Integer.BYTES;
        }

        var alignment = 1;
        for (var nested : field.table().fields()) {
            var fieldAlignment = nested instanceof TableField table
                ? alignmentOf(table)
                : arrayAlignment(nested).orElse(1);
            alignment = Math.max(alignment, fieldAlignment);
        }

        return alignment;
    }

    /**
     * {@return the alignment of an array's elements from the start of its table}: the size of a
     * scalar array's elements, or of the largest scalar of an array's structs, and nothing for a
     * field of any other type
     */
    static OptionalInt arrayAlignment(FieldDescription field) {
        return switch (field) {
            case ArrayField array -> OptionalInt.of(array.type().size());
            case StructArrayField array -> OptionalInt.of(StructBytes.alignment(array.fields()));
            default -> OptionalInt.empty();
        };
    }
}
