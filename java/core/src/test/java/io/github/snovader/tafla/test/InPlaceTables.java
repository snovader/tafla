package io.github.snovader.tafla.test;

import io.github.snovader.tafla.*;
import io.github.snovader.tafla.test.FieldDescription.*;

import java.util.*;
import java.util.function.*;

/**
 * Builds tables as {@link Tables#build} does, but writes each table nested from a builder that a
 * {@link TableWriter} writes in place, at its own alignment, with
 * {@link TableBuilder#setTable(int, Consumer)}, and each vector of them whose items hold as many
 * fields with {@link TableBuilder#setVector(int, int, ObjIntConsumer)}, rather than from their
 * builders. It remembers whether a vector failed before its fill set any item, and injects its
 * {@link FillFaults} into the tables and vectors that it writes in place.
 */
final class InPlaceTables {
    private final FillFaults faults;
    private boolean failedBeforeItems = false;

    /** Sets a table in place, with the setter of a builder or of an item */
    @FunctionalInterface
    private interface TableSetter {
        void setTable(int fields, Consumer<TableBuilder.Item> fill);
    }

    /** Sets a vector's items in place, with the setter of a builder or of an item */
    @FunctionalInterface
    private interface VectorSetter {
        void setVector(int count, int fields, ObjIntConsumer<TableBuilder.Item> fill);
    }

    InPlaceTables() {
        this(FillFaults.none());
    }

    InPlaceTables(FillFaults faults) {
        this.faults = faults;
    }

    /** {@return a builder of the table with all its fields set} */
    TableBuilder build(TableDescription table) {
        var builder = Tables.createBuilder(table);
        var offsetType = table.layout().offsetType();

        for (var field : table.fields()) {
            setField(builder, field, offsetType);
        }

        return builder;
    }

    /**
     * Sets a field of a table of {@code offsetType} offsets on its builder, writing a table or
     * vector in place when it can
     */
    void setField(TableBuilder builder, FieldDescription field, OffsetType offsetType) {
        switch (field) {
            case TableField nested when writesInPlace(nested, offsetType) ->
                setTable(builder::setTable, nested.table(), offsetType);
            case VectorField vector when writesInPlace(vector, offsetType) ->
                setVector(builder::setVector, vector.items(), offsetType);
            default -> Tables.setField(builder, field);
        }
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
            case TableField nested when writesInPlace(nested, offsetType) ->
                setTable(item::setTable, nested.table(), offsetType);
            case TableField nested -> setTable(item, nested);
            case VectorField vector when writesInPlace(vector, offsetType) ->
                setVector(item::setVector, vector.items(), offsetType);
            case VectorField vector -> setVector(item, vector.items());
        }
    }

    /** Sets a table that isn't written in place on the item, as {@link Tables#setField} does */
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

    /** Sets a vector that isn't written in place on the item, as {@link Tables#setField} does */
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
        var filled = new boolean[1];

        try {
            setter.setVector(items.size(), FillFaults.declared(fault, fields), (item, index) -> {
                filled[0] = true;
                fill(items.get(index).table(), offsetType).accept(item);
                FillFaults.afterFill(fault, index == items.size() - 1);
            });
        } catch (IllegalArgumentException e) {
            failedBeforeItems |= !filled[0];
            throw e;
        }
    }

    /**
     * {@return whether the field's table is written in place in a table of {@code offsetType}
     * offsets}: from a builder without a header and of those offsets, at its own alignment
     */
    static boolean writesInPlace(TableField field, OffsetType offsetType) {
        return field.alignment().isEmpty() && writesInPlace(field.from(), field.table(), offsetType);
    }

    /** {@return whether the vector's items are each written in place, and hold as many fields} */
    static boolean writesInPlace(VectorField vector, OffsetType offsetType) {
        var items = vector.items();
        return items.stream().allMatch(item ->
            writesInPlace(item.from(), item.table(), offsetType)
                && item.table().fields().size() == items.getFirst().table().fields().size()
        );
    }

    private static boolean writesInPlace(Source from, TableDescription table, OffsetType offsetType) {
        return from == Source.BUILDER
            && table.layout().equals(LayoutDescription.headerless(offsetType))
            && table.version().isEmpty();
    }
}
