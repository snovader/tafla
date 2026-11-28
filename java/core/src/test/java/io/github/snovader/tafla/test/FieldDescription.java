package io.github.snovader.tafla.test;

import java.util.*;

/** The description of a field in the shape of {@code cases.json}, by its {@code type} */
sealed interface FieldDescription {
    /** An integer or float, held as {@link ScalarType} describes */
    record ScalarField(ScalarType type, long value) implements FieldDescription {
        @Override
        public String toString() {
            return DescriptionJson.write(this).toString();
        }
    }

    /**
     * A bool, which the runtimes have no setter or getter of: writers write it as the uint8 1 or 0,
     * and readers read it as true when its byte isn't 0
     */
    record BoolField(boolean value) implements FieldDescription {
        /** {@return the uint8 that writers write the bool as} */
        short uint8() {
            return (short) (value ? 1 : 0);
        }

        @Override
        public String toString() {
            return DescriptionJson.write(this).toString();
        }
    }

    /**
     * An array of integers or floats, each held as {@link ScalarType} describes. It compares its
     * elements, and nothing changes them: the array is passed to setters as it is, to be written
     * when the table is.
     */
    @SuppressWarnings("ArrayRecordComponent")
    record ArrayField(ScalarType type, long[] values) implements FieldDescription {
        @Override
        public boolean equals(Object other) {
            return other instanceof ArrayField array && type == array.type && Arrays.equals(values, array.values);
        }

        @Override
        public int hashCode() {
            return 31 * type.hashCode() + Arrays.hashCode(values);
        }

        @Override
        public String toString() {
            return DescriptionJson.write(this).toString();
        }
    }

    /**
     * An array of bools, which writers write as a uint8 array of 1 and 0, and readers read as true
     * where a byte isn't 0. It compares its elements, and nothing changes them, as for
     * {@link ArrayField}.
     */
    @SuppressWarnings("ArrayRecordComponent")
    record BoolArrayField(boolean[] values) implements FieldDescription {
        @Override
        public boolean equals(Object other) {
            return other instanceof BoolArrayField array && Arrays.equals(values, array.values);
        }

        @Override
        public int hashCode() {
            return Arrays.hashCode(values);
        }

        @Override
        public String toString() {
            return DescriptionJson.write(this).toString();
        }
    }

    record StringField(String value) implements FieldDescription {
        @Override
        public String toString() {
            return DescriptionJson.write(this).toString();
        }
    }

    /**
     * A struct, whose scalars lie one after another in the order of {@code fields}, so a struct
     * within a struct is described by its scalars
     */
    record StructField(List<ScalarField> fields) implements FieldDescription {
        public StructField {
            fields = List.copyOf(fields);
        }

        /** {@return the types of the struct's scalars, in order} */
        List<ScalarType> types() {
            return fields.stream().map(ScalarField::type).toList();
        }

        /** {@return the values of the struct's scalars, in order} */
        List<Long> values() {
            return fields.stream().map(ScalarField::value).toList();
        }

        @Override
        public String toString() {
            return DescriptionJson.write(this).toString();
        }
    }

    /**
     * An array of structs of scalars of the types in {@code fields}, with the values of each
     * struct's scalars in that order, held as {@link ScalarType} describes
     */
    record StructArrayField(List<ScalarType> fields, List<List<Long>> items) implements FieldDescription {
        public StructArrayField {
            fields = List.copyOf(fields);
            items = items.stream().map(List::copyOf).toList();
        }

        @Override
        public String toString() {
            return DescriptionJson.write(this).toString();
        }
    }

    /**
     * A table nested with {@code setTable()}
     *
     * @param alignment the alignment passed to {@code setTable()}, absent for its default
     */
    record TableField(Source from, TableDescription table, OptionalInt alignment) implements FieldDescription {
        @Override
        public String toString() {
            return DescriptionJson.write(this).toString();
        }
    }

    record VectorField(List<NestedTable> items) implements FieldDescription {
        public VectorField {
            items = List.copyOf(items);
        }

        @Override
        public String toString() {
            return DescriptionJson.write(this).toString();
        }
    }

    /**
     * A vector of strings: a count, the end of each string, and the strings' UTF-8, one after
     * another, with {@code setStringVector()}
     */
    record StringVectorField(List<String> values) implements FieldDescription {
        public StringVectorField {
            values = List.copyOf(values);
        }

        @Override
        public String toString() {
            return DescriptionJson.write(this).toString();
        }
    }

    /** A table nested from its builder or from the bytes that its builder writes */
    record NestedTable(Source from, TableDescription table) {
        @Override
        public String toString() {
            return DescriptionJson.write(this).toString();
        }
    }

    /** What a nested table is passed to its parent's builder as */
    enum Source {
        BUILDER,
        BYTES;

        String jsonName() {
            return name().toLowerCase(Locale.ROOT);
        }
    }
}
