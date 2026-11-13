package io.github.snovader.tafla.test;

import io.github.snovader.tafla.*;
import io.github.snovader.tafla.test.FieldDescription.*;

import java.util.*;
import java.util.function.*;

/**
 * Builds tables as {@link Tables#build} does, but records each table nested from a builder that a
 * {@link TableWriter} writes in place, at its own alignment, with
 * {@link TableBuilder#setTable(int, Consumer)}, and each vector of them whose items hold as many
 * fields with {@link TableBuilder#setVector(int, int, ObjIntConsumer)}, rather than from their
 * builders. It remembers whether a vector failed before its fill set any item.
 */
final class InPlaceTables {
    private boolean failedBeforeItems = false;

    /** Sets a vector's items in place, with the setter of a builder or of an item */
    @FunctionalInterface
    private interface VectorSetter {
        void setVector(int count, int fields, ObjIntConsumer<TableBuilder.Item> fill);
    }

    /** {@return a builder of the table with all its fields set} */
    TableBuilder build(TableDescription table) {
        var builder = Tables.createBuilder(table);
        var offsetType = table.layout().offsetType();

        for (var field : table.fields()) {
            switch (field) {
                case TableField nested when recordsInPlace(nested, offsetType) ->
                    builder.setTable(nested.table().fields().size(), fill(nested.table(), offsetType));
                case VectorField vector when recordsInPlace(vector, offsetType) ->
                    setVector(builder::setVector, vector.items(), offsetType);
                default -> Tables.setField(builder, field);
            }
        }

        return builder;
    }

    /** {@return whether a vector failed before its fill set any of its items} */
    boolean failedBeforeItems() {
        return failedBeforeItems;
    }

    /** Sets the field on the item as {@link Tables#setField} sets it on a builder */
    private void setField(TableBuilder.Item item, FieldDescription field, OffsetType offsetType) {
        switch (field) {
            case ScalarField scalar -> scalar.type().set(item, scalar.value());
            case BoolField bool -> item.setUint8(bool.uint8());
            case ArrayField array -> array.type().setArray(item, array.values());
            case StringField string -> item.setString(string.value());
            case StructField struct -> item.setStruct(StructBytes.pack(struct.types(), List.of(struct.values())));
            case StructArrayField array -> item.setStructArray(
                StructBytes.pack(array.fields(), array.items()),
                StructBytes.size(array.fields()),
                StructBytes.alignment(array.fields())
            );
            case TableField nested when recordsInPlace(nested, offsetType) ->
                item.setTable(nested.table().fields().size(), fill(nested.table(), offsetType));
            case TableField nested -> setTable(item, nested);
            case VectorField vector when recordsInPlace(vector, offsetType) ->
                setVector(item::setVector, vector.items(), offsetType);
            case VectorField vector -> setVector(item, vector.items());
        }
    }

    /** Sets a table that isn't recorded in place on the item, as {@link Tables#setField} does */
    private static void setTable(TableBuilder.Item item, TableField field) {
        var nested = Tables.build(field.table());
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

    /** Sets a vector that isn't recorded in place on the item, as {@link Tables#setField} does */
    private static void setVector(TableBuilder.Item item, List<NestedTable> items) {
        var builders = items.stream().map(nested -> Tables.build(nested.table())).toList();

        if (items.stream().allMatch(nested -> nested.from() == Source.BUILDER)) {
            item.setVector(builders);
        } else {
            item.setVector(builders.stream().map(TableBuilder::bytes).toArray(byte[][]::new));
        }
    }

    /** {@return a fill that sets the table's fields on the item it's given} */
    private Consumer<TableBuilder.Item> fill(TableDescription table, OffsetType offsetType) {
        return item -> {
            for (var field : table.fields()) {
                setField(item, field, offsetType);
            }
        };
    }

    private void setVector(VectorSetter setter, List<NestedTable> items, OffsetType offsetType) {
        var fields = items.isEmpty() ? 0 : items.getFirst().table().fields().size();
        var filled = new boolean[1];

        try {
            setter.setVector(items.size(), fields, (item, index) -> {
                filled[0] = true;
                fill(items.get(index).table(), offsetType).accept(item);
            });
        } catch (IllegalArgumentException e) {
            failedBeforeItems |= !filled[0];
            throw e;
        }
    }

    /**
     * {@return whether the field's table is recorded in place in a table of {@code offsetType}
     * offsets}: from a builder without a header and of those offsets, at its own alignment
     */
    private static boolean recordsInPlace(TableField field, OffsetType offsetType) {
        return field.alignment().isEmpty() && recordsInPlace(field.from(), field.table(), offsetType);
    }

    /** {@return whether the vector's items are each recorded in place, and hold as many fields} */
    private static boolean recordsInPlace(VectorField vector, OffsetType offsetType) {
        var items = vector.items();
        return items.stream().allMatch(item ->
            recordsInPlace(item.from(), item.table(), offsetType)
                && item.table().fields().size() == items.getFirst().table().fields().size()
        );
    }

    private static boolean recordsInPlace(Source from, TableDescription table, OffsetType offsetType) {
        return from == Source.BUILDER
            && table.layout().equals(LayoutDescription.headerless(offsetType))
            && table.version().isEmpty();
    }
}
