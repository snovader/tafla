package io.github.snovader.tafla.benchmarks.codecs;

import io.github.snovader.tafla.*;
import io.github.snovader.tafla.benchmarks.*;

import java.lang.invoke.*;
import java.nio.ByteOrder;

/**
 * tafla's tables written by the prototype {@link DirectTableBuilder}, with the calls that
 * {@link TaflaCodec} makes of its {@link TableBuilder}s, so that they write the same bytes, which
 * are read as {@code TaflaCodec} reads them. Its fixed-size record is {@code TaflaCodec}'s, which
 * no builder writes.
 */
public final class TaflaDirectCodec {
    private static final TableLayout DOCUMENT_LAYOUT = TableLayout.builder().sized(true).versioned(true).build();
    private static final int DOCUMENT_VERSION = 1;

    /** An item as a struct: its three float32s, aligned to 4 in an array */
    private static final int ITEM_SIZE = 3 * Float.BYTES;
    private static final int ITEM_ALIGNMENT = Float.BYTES;

    private static final VarHandle FLOAT32 =
        MethodHandles.byteArrayViewVarHandle(float[].class, ByteOrder.LITTLE_ENDIAN);

    private TaflaDirectCodec() {}

    /** {@return the codec of the proposed builder, which keeps runs of 256 bytes or more as segments} */
    public static Codec create() {
        return create("tafla-direct", 256);
    }

    /** {@return the codec of a builder that copies every value into its bytes when it's set} */
    public static Codec copying() {
        return create("tafla-direct-copying", Integer.MAX_VALUE);
    }

    private static Codec create(String name, int segmentLength) {
        var tafla = TaflaCodec.create();

        return new Codec(
            name,
            new Records(tafla.record(), segmentLength),
            new Documents(tafla.document(), segmentLength),
            new Lists(tafla.list(), segmentLength),
            new Structs(tafla.structs(), segmentLength),
            tafla.sample()
        );
    }

    private record Records(Workload<RecordInput> reader, int segmentLength) implements Workload<RecordInput> {
        @Override
        public byte[] serialize(RecordInput input) {
            var builder = new DirectTableBuilder(4).segmentsFrom(segmentLength);
            builder.setFloat64(input.x());
            builder.setFloat64(input.y());
            builder.setInt64(input.id());
            builder.setString(input.name());

            return builder.bytes();
        }

        @Override
        public RecordInput deserialize(byte[] bytes) {
            return reader.deserialize(bytes);
        }
    }

    private record Documents(DocumentWorkload reader, int segmentLength) implements DocumentWorkload {
        @Override
        public byte[] serialize(DocumentInput input) {
            var document = new DirectTableBuilder(4, DOCUMENT_LAYOUT, DOCUMENT_VERSION).segmentsFrom(segmentLength);
            document.setFloat64(input.x());
            document.setFloat64(input.y());
            document.setString(input.name());
            document.setTable(4, columns -> {
                columns.setInt64(input.base());
                columns.setUint32Array(input.keys());
                columns.setFloat32Array(input.a());
                columns.setFloat32Array(input.b());
            });

            return document.bytes();
        }

        @Override
        public DocumentInput deserialize(byte[] bytes) {
            return reader.deserialize(bytes);
        }

        @Override
        public double sumA(byte[] bytes) {
            return reader.sumA(bytes);
        }
    }

    private record Lists(Workload<ListInput> reader, int segmentLength) implements Workload<ListInput> {
        @Override
        public byte[] serialize(ListInput input) {
            var items = input.items();

            var builder = new DirectTableBuilder(1).segmentsFrom(segmentLength);
            builder.setVector(items.size(), 2, (item, index) -> setItem(item, items.get(index)));

            return builder.bytes();
        }

        @Override
        public ListInput deserialize(byte[] bytes) {
            return reader.deserialize(bytes);
        }

        private static void setItem(DirectTableBuilder.Item item, ItemInput input) {
            item.setFloat32(input.value());
            item.setTable(2, pair -> {
                pair.setFloat32(input.first());
                pair.setFloat32(input.second());
            });
        }
    }

    private record Structs(Workload<ListInput> reader, int segmentLength) implements Workload<ListInput> {
        @Override
        public byte[] serialize(ListInput input) {
            var items = input.items();
            var structs = new byte[items.size() * ITEM_SIZE];
            for (int i = 0; i < items.size(); i++) {
                writeItem(items.get(i), structs, i * ITEM_SIZE);
            }

            var builder = new DirectTableBuilder(1).segmentsFrom(segmentLength);
            builder.setStructArray(structs, ITEM_SIZE, ITEM_ALIGNMENT);

            return builder.bytes();
        }

        @Override
        public ListInput deserialize(byte[] bytes) {
            return reader.deserialize(bytes);
        }

        private static void writeItem(ItemInput item, byte[] bytes, int offset) {
            FLOAT32.set(bytes, offset, item.value());
            FLOAT32.set(bytes, offset + Float.BYTES, item.first());
            FLOAT32.set(bytes, offset + 2 * Float.BYTES, item.second());
        }
    }
}
