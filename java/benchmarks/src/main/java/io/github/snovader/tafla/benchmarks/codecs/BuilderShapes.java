package io.github.snovader.tafla.benchmarks.codecs;

import io.github.snovader.tafla.*;
import io.github.snovader.tafla.benchmarks.*;

import java.util.*;
import java.util.function.*;

/**
 * Tables of shapes that the workloads don't have, written by {@link TableBuilder}, which compare
 * one version of the builder with another rather than with other libraries: tables of arrays
 * shorter and longer than the builder copies when they're set, and of more arrays than it keeps
 * in its fields, the document and the vector of nested tables nested from builders and from their
 * bytes rather than in place, and chains of nested tables. Each shape also writes its table in
 * another way, with {@link TableWriter} or in place, which the builder's bytes must equal.
 */
public final class BuilderShapes {
    /** The depth of the chains of nested tables */
    private static final int CHAIN_DEPTH = 100;
    /** The depth of the chain of tables whose padding moves the tables in them */
    private static final int PADDED_CHAIN_DEPTH = 1000;

    private static final TableLayout DOCUMENT_LAYOUT = TableLayout.builder().sized(true).versioned(true).build();
    private static final int DOCUMENT_VERSION = 1;

    private static final byte[] INT8_100 = int8Array(100);
    private static final byte[] INT8_255 = int8Array(255);
    private static final byte[] INT8_256 = int8Array(256);
    private static final double[] FLOAT64_16 = float64Array(16);
    private static final long[] INT64_4 = {1, -2, 3, -4};
    private static final String NAME = "tafla-benchmarks";

    /**
     * A table that the builder writes, and that {@code expected} writes in another way
     *
     * @param build writes the table with {@link TableBuilder}, as the benchmark times it
     * @param expected writes the same table with {@link TableWriter}, or in place
     */
    public record Shape(Supplier<byte[]> build, Supplier<byte[]> expected) {}

    private BuilderShapes() {}

    /** {@return a new shape of each name, in the order that the benchmark lists them} */
    public static SequencedMap<String, Shape> all() {
        var shapes = new LinkedHashMap<String, Shape>();
        shapes.put("1 int8[100]", arrays(1, INT8_100));
        shapes.put("4 int8[100]", arrays(4, INT8_100));
        shapes.put("8 int8[255]", arrays(8, INT8_255));
        shapes.put("8 int8[256]", arrays(8, INT8_256));
        shapes.put("1000 int8[255]", arrays(1000, INT8_255));
        shapes.put("10 float64[16]", new Shape(BuilderShapes::float64Arrays, BuilderShapes::writtenFloat64Arrays));
        shapes.put("mixed", new Shape(BuilderShapes::mixed, BuilderShapes::writtenMixed));
        shapes.put("nested builder", new Shape(BuilderShapes::nestedBuilder, BuilderShapes::nestedInPlace));
        shapes.put("chain of builders", new Shape(() -> builderChain(CHAIN_DEPTH), () -> chainInPlace(CHAIN_DEPTH)));
        shapes.put("padded chain in place", new Shape(BuilderShapes::paddedChainInPlace, BuilderShapes::paddedBuilderChain));
        for (var rows : Workloads.DOCUMENT_ROWS) {
            shapes.put("document of " + rows + " rows from builders", documents(rows, false));
            shapes.put("document of " + rows + " rows from bytes", documents(rows, true));
        }
        shapes.put("vector of 100 tables from builders", vectors(false));
        shapes.put("vector of 100 tables from bytes", vectors(true));

        return shapes;
    }

    /**
     * {@return a new shape by its name}
     *
     * @throws IllegalArgumentException if no shape has that name
     */
    public static Shape named(String name) {
        var shape = all().get(name);
        if (shape == null) {
            throw new IllegalArgumentException("No shape is named " + name);
        }

        return shape;
    }

    /**
     * {@return the builder's bytes of a shape}, checked
     *
     * @throws IllegalStateException if they aren't the bytes that the shape writes in another way
     */
    public static byte[] buildChecked(String name, Shape shape) {
        var bytes = shape.build().get();
        if (!Arrays.equals(bytes, shape.expected().get())) {
            throw new IllegalStateException("The builder wrote other bytes for " + name);
        }

        return bytes;
    }

    /** A table of {@code fields} int8 arrays, each of {@code values} */
    private static Shape arrays(int fields, byte[] values) {
        Supplier<byte[]> build = () -> {
            var builder = new TableBuilder(fields);
            for (int i = 0; i < fields; i++) {
                builder.setInt8Array(values);
            }

            return builder.bytes();
        };
        Supplier<byte[]> expected = () -> {
            var sizer = new TableSizer(fields);
            for (int i = 0; i < fields; i++) {
                sizer.addInt8Array(values.length);
            }

            var writer = sizer.writer();
            for (int i = 0; i < fields; i++) {
                writer.setInt8Array(i, values);
            }

            return writer.bytes();
        };

        return new Shape(build, expected);
    }

    private static byte[] float64Arrays() {
        var builder = new TableBuilder(10);
        for (int i = 0; i < 10; i++) {
            builder.setFloat64Array(FLOAT64_16);
        }

        return builder.bytes();
    }

    private static byte[] writtenFloat64Arrays() {
        var sizer = new TableSizer(10);
        for (int i = 0; i < 10; i++) {
            sizer.addFloat64Array(FLOAT64_16.length);
        }

        var writer = sizer.writer();
        for (int i = 0; i < 10; i++) {
            writer.setFloat64Array(i, FLOAT64_16);
        }

        return writer.bytes();
    }

    /** A table of an int32, a string, a float64 array, an int8 array and a float64 */
    private static byte[] mixed() {
        var builder = new TableBuilder(5);
        builder.setInt32(-7);
        builder.setString(NAME);
        builder.setFloat64Array(FLOAT64_16);
        builder.setInt8Array(INT8_100);
        builder.setFloat64(0.5);

        return builder.bytes();
    }

    private static byte[] writtenMixed() {
        var sizer = new TableSizer(5);
        sizer.addInt32();
        sizer.addString(NAME);
        sizer.addFloat64Array(FLOAT64_16.length);
        sizer.addInt8Array(INT8_100.length);
        sizer.addFloat64();

        var writer = sizer.writer();
        writer.setInt32(0, -7);
        writer.setString(1, NAME);
        writer.setFloat64Array(2, FLOAT64_16);
        writer.setInt8Array(3, INT8_100);
        writer.setFloat64(4, 0.5);

        return writer.bytes();
    }

    /** A table of an int32 and a table of three int8 arrays, nested from its builder */
    private static byte[] nestedBuilder() {
        var nested = new TableBuilder(3);
        for (int i = 0; i < 3; i++) {
            nested.setInt8Array(INT8_100);
        }

        var builder = new TableBuilder(2);
        builder.setInt32(-7);
        builder.setTable(nested);

        return builder.bytes();
    }

    private static byte[] nestedInPlace() {
        var builder = new TableBuilder(2);
        builder.setInt32(-7);
        builder.setTable(3, nested -> {
            for (int i = 0; i < 3; i++) {
                nested.setInt8Array(INT8_100);
            }
        });

        return builder.bytes();
    }

    /**
     * A chain of {@code depth} tables each nested in the next from its builder, each of an int8
     * array and, but for the innermost, the table before it
     */
    private static byte[] builderChain(int depth) {
        var table = new TableBuilder(1);
        table.setInt8Array(INT8_100);
        for (int i = 0; i < depth; i++) {
            var parent = new TableBuilder(2);
            parent.setInt8Array(INT8_100);
            parent.setTable(table);
            table = parent;
        }

        return table.bytes();
    }

    private static byte[] chainInPlace(int depth) {
        var builder = new TableBuilder(2);
        builder.setInt8Array(INT8_100);
        builder.setTable(depth == 1 ? 1 : 2, table -> chainInPlace(table, depth - 1));

        return builder.bytes();
    }

    private static void chainInPlace(TableBuilder.Item table, int depth) {
        table.setInt8Array(INT8_100);
        if (depth > 0) {
            table.setTable(depth == 1 ? 1 : 2, nested -> chainInPlace(nested, depth - 1));
        }
    }

    /**
     * A chain of tables of an int8 and the next table, whose innermost table holds an int64 array
     * instead, so that each table is padded to 8 bytes, written in place, where the padding moves
     * the tables written in it
     */
    private static byte[] paddedChainInPlace() {
        var builder = new TableBuilder(1);
        builder.setTable(2, table -> paddedChainInPlace(table, PADDED_CHAIN_DEPTH));

        return builder.bytes();
    }

    private static void paddedChainInPlace(TableBuilder.Item table, int depth) {
        table.setInt8((byte) depth);
        if (depth == 0) {
            table.setInt64Array(INT64_4);
        } else {
            table.setTable(2, nested -> paddedChainInPlace(nested, depth - 1));
        }
    }

    private static byte[] paddedBuilderChain() {
        var table = new TableBuilder(2);
        table.setInt8((byte) 0);
        table.setInt64Array(INT64_4);
        for (int depth = 1; depth <= PADDED_CHAIN_DEPTH; depth++) {
            var parent = new TableBuilder(2);
            parent.setInt8((byte) depth);
            parent.setTable(table);
            table = parent;
        }

        var builder = new TableBuilder(1);
        builder.setTable(table);

        return builder.bytes();
    }

    /**
     * The document of {@code rows} rows, with its columns nested from their builder or its bytes,
     * which writes {@link TaflaCodec}'s bytes, written in place
     */
    private static Shape documents(int rows, boolean fromBytes) {
        var input = Workloads.documentInput(rows);
        var inPlace = TaflaCodec.create().document();
        Supplier<byte[]> build = () -> {
            var columns = new TableBuilder(4);
            columns.setInt64(input.base());
            columns.setUint32Array(input.keys());
            columns.setFloat32Array(input.a());
            columns.setFloat32Array(input.b());

            var document = new TableBuilder(4, DOCUMENT_LAYOUT, DOCUMENT_VERSION);
            document.setFloat64(input.x());
            document.setFloat64(input.y());
            document.setString(input.name());
            if (fromBytes) {
                document.setTable(columns.bytes(), Integer.BYTES);
            } else {
                document.setTable(columns);
            }

            return document.bytes();
        };

        return new Shape(build, () -> inPlace.serialize(input));
    }

    /**
     * The vector of 100 nested tables, with a builder for each item and the table in it, nested
     * from the builders or their bytes, which writes {@link TaflaCodec}'s bytes, written in place
     */
    private static Shape vectors(boolean fromBytes) {
        var input = Workloads.listInput(Workloads.LIST_ITEMS);
        var inPlace = TaflaCodec.create().list();
        Supplier<byte[]> build = () -> {
            var items = input.items();
            var builder = new TableBuilder(1);
            if (fromBytes) {
                var tables = new byte[items.size()][];
                for (int i = 0; i < tables.length; i++) {
                    tables[i] = itemOf(items.get(i), true).bytes();
                }
                builder.setVector(tables);
            } else {
                var tables = new ArrayList<TableBuilder>(items.size());
                for (var item : items) {
                    tables.add(itemOf(item, false));
                }
                builder.setVector(tables);
            }

            return builder.bytes();
        };

        return new Shape(build, () -> inPlace.serialize(input));
    }

    private static TableBuilder itemOf(ItemInput input, boolean fromBytes) {
        var pair = new TableBuilder(2);
        pair.setFloat32(input.first());
        pair.setFloat32(input.second());

        var item = new TableBuilder(2);
        item.setFloat32(input.value());
        if (fromBytes) {
            item.setTable(pair.bytes(), 1);
        } else {
            item.setTable(pair);
        }

        return item;
    }

    private static byte[] int8Array(int length) {
        var values = new byte[length];
        for (int i = 0; i < length; i++) {
            values[i] = (byte) (i * 31 + 7);
        }

        return values;
    }

    private static double[] float64Array(int length) {
        var values = new double[length];
        for (int i = 0; i < length; i++) {
            values[i] = i * 0.25 - 1;
        }

        return values;
    }
}
