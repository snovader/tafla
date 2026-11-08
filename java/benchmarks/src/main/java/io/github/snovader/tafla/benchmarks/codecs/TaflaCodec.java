package io.github.snovader.tafla.benchmarks.codecs;

import io.github.snovader.tafla.*;
import io.github.snovader.tafla.benchmarks.*;

import java.lang.invoke.*;
import java.nio.ByteOrder;
import java.util.*;

/**
 * tafla's tables, nested from their builders. The runtime writes tables, and leaves records at
 * fixed offsets to the standard library, as its package documentation says. The fixed-size record
 * is written through little-endian {@link VarHandle}s and read with {@link ByteBuffer}'s getters
 * at fixed positions. An array of structs is written through them too, and read from
 * {@link ByteBuffer#getStructArray}'s view, as tafla-gen's code does.
 */
public final class TaflaCodec {
    private static final TableLayout DOCUMENT_LAYOUT = TableLayout.builder().sized(true).versioned(true).build();
    private static final int DOCUMENT_VERSION = 1;

    /** The fixed-size record: a uint32 and two float32s */
    private static final int SAMPLE_SIZE = Integer.BYTES + 2 * Float.BYTES;

    /** An item as a struct: its three float32s, aligned to 4 in an array */
    private static final int ITEM_SIZE = 3 * Float.BYTES;
    private static final int ITEM_ALIGNMENT = Float.BYTES;

    // Views of byte arrays as arrays of wider values, which write at any index, little-endian
    private static final VarHandle INT32 =
        MethodHandles.byteArrayViewVarHandle(int[].class, ByteOrder.LITTLE_ENDIAN);
    private static final VarHandle FLOAT32 =
        MethodHandles.byteArrayViewVarHandle(float[].class, ByteOrder.LITTLE_ENDIAN);

    private TaflaCodec() {}

    public static Codec create() {
        return new Codec("tafla", new Records(), new Documents(), new Lists(), new Structs(), new Samples());
    }

    private static final class Records implements Workload<RecordInput> {
        @Override
        public byte[] serialize(RecordInput input) {
            var builder = new TableBuilder(4);
            builder.setFloat64(input.x());
            builder.setFloat64(input.y());
            builder.setInt64(input.id());
            builder.setString(input.name());

            return builder.bytes();
        }

        @Override
        public RecordInput deserialize(byte[] bytes) {
            var buffer = new ByteBuffer(bytes);

            return new RecordInput(
                buffer.getFloat64(buffer.rootOffset(0)),
                buffer.getFloat64(buffer.rootOffset(1)),
                buffer.getInt64(buffer.rootOffset(2)),
                buffer.getString(buffer.rootOffset(3))
            );
        }
    }

    private static final class Documents implements DocumentWorkload {
        @Override
        public byte[] serialize(DocumentInput input) {
            var columns = new TableBuilder(4);
            columns.setInt64(input.base());
            columns.setUint32Array(input.keys());
            columns.setFloat32Array(input.a());
            columns.setFloat32Array(input.b());

            var document = new TableBuilder(4, DOCUMENT_LAYOUT, DOCUMENT_VERSION);
            document.setFloat64(input.x());
            document.setFloat64(input.y());
            document.setString(input.name());
            document.setTable(columns);

            return document.bytes();
        }

        @Override
        public DocumentInput deserialize(byte[] bytes) {
            var buffer = openDocument(bytes);
            var columns = buffer.rootOffset(3);

            return new DocumentInput(
                buffer.getFloat64(buffer.rootOffset(0)),
                buffer.getFloat64(buffer.rootOffset(1)),
                buffer.getString(buffer.rootOffset(2)),
                buffer.getInt64(buffer.fieldOffset(columns, 0)),
                buffer.getUint32Array(buffer.fieldOffset(columns, 1)),
                buffer.getFloat32Array(buffer.fieldOffset(columns, 2)),
                buffer.getFloat32Array(buffer.fieldOffset(columns, 3))
            );
        }

        @Override
        public double sumA(byte[] bytes) {
            var buffer = openDocument(bytes);
            var a = buffer.fieldOffset(buffer.rootOffset(3), 2);

            // The elements follow their count, which is of the layout's offset type, uint32. An
            // int count makes this a counted loop, which C2 optimizes; a long one made summing 100
            // elements 2.8 times slower.
            var length = Math.toIntExact(buffer.getUint32(a));
            var elements = a + Integer.BYTES;

            double sum = 0;
            for (int i = 0; i < length; i++) {
                sum += buffer.getFloat32(elements + i * Float.BYTES);
            }

            return sum;
        }

        /** @throws IllegalStateException if the document is in another version */
        private static ByteBuffer openDocument(byte[] bytes) {
            var buffer = new ByteBuffer(bytes, DOCUMENT_LAYOUT);
            if (buffer.getVersion() != DOCUMENT_VERSION) {
                throw new IllegalStateException("Unsupported document version " + buffer.getVersion());
            }

            return buffer;
        }
    }

    private static final class Lists implements Workload<ListInput> {
        @Override
        public byte[] serialize(ListInput input) {
            var items = input.items();
            var tables = new ArrayList<TableBuilder>(items.size());
            for (var item : items) {
                tables.add(itemTable(item));
            }

            var builder = new TableBuilder(1);
            builder.setVector(tables);

            return builder.bytes();
        }

        @Override
        public ListInput deserialize(byte[] bytes) {
            var buffer = new ByteBuffer(bytes);
            var offsets = buffer.getVectorOffsets(buffer.rootOffset(0));

            var items = new ItemInput[offsets.length];
            for (int i = 0; i < offsets.length; i++) {
                var item = offsets[i];
                var pair = buffer.fieldOffset(item, 1);

                items[i] = new ItemInput(
                    buffer.getFloat32(buffer.fieldOffset(item, 0)),
                    buffer.getFloat32(buffer.fieldOffset(pair, 0)),
                    buffer.getFloat32(buffer.fieldOffset(pair, 1))
                );
            }

            return new ListInput(Arrays.asList(items));
        }

        private static TableBuilder itemTable(ItemInput input) {
            var pair = new TableBuilder(2);
            pair.setFloat32(input.first());
            pair.setFloat32(input.second());

            var item = new TableBuilder(2);
            item.setFloat32(input.value());
            item.setTable(pair);

            return item;
        }
    }

    private static final class Structs implements Workload<ListInput> {
        @Override
        public byte[] serialize(ListInput input) {
            var items = input.items();
            var structs = new byte[items.size() * ITEM_SIZE];
            for (int i = 0; i < items.size(); i++) {
                writeItem(items.get(i), structs, i * ITEM_SIZE);
            }

            var builder = new TableBuilder(1);
            builder.setStructArray(structs, ITEM_SIZE, ITEM_ALIGNMENT);

            return builder.bytes();
        }

        @Override
        public ListInput deserialize(byte[] bytes) {
            var buffer = new ByteBuffer(bytes);
            var view = buffer.getStructArray(buffer.rootOffset(0), ITEM_SIZE);

            var items = new ItemInput[view.remaining() / ITEM_SIZE];
            for (int i = 0; i < items.length; i++) {
                var offset = i * ITEM_SIZE;
                items[i] = new ItemInput(
                    view.getFloat(offset),
                    view.getFloat(offset + Float.BYTES),
                    view.getFloat(offset + 2 * Float.BYTES)
                );
            }

            return new ListInput(Arrays.asList(items));
        }

        private static void writeItem(ItemInput item, byte[] bytes, int offset) {
            FLOAT32.set(bytes, offset, item.value());
            FLOAT32.set(bytes, offset + Float.BYTES, item.first());
            FLOAT32.set(bytes, offset + 2 * Float.BYTES, item.second());
        }
    }

    private static final class Samples implements Workload<SampleInput> {
        @Override
        public byte[] serialize(SampleInput input) {
            var bytes = new byte[SAMPLE_SIZE];
            INT32.set(bytes, 0, (int) input.key());
            FLOAT32.set(bytes, Integer.BYTES, input.a());
            FLOAT32.set(bytes, Integer.BYTES + Float.BYTES, input.b());

            return bytes;
        }

        @Override
        public SampleInput deserialize(byte[] bytes) {
            var buffer = new ByteBuffer(bytes);

            return new SampleInput(
                buffer.getUint32(0),
                buffer.getFloat32(Integer.BYTES),
                buffer.getFloat32(Integer.BYTES + Float.BYTES)
            );
        }
    }
}
