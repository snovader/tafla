package io.github.snovader.tafla.benchmarks.codecs;

import com.google.flatbuffers.FlatBufferBuilder;
import com.google.flatbuffers.IntVector;
import io.github.snovader.tafla.benchmarks.*;
import io.github.snovader.tafla.benchmarks.generated.flatbuffers.Columns;
import io.github.snovader.tafla.benchmarks.generated.flatbuffers.Document;
import io.github.snovader.tafla.benchmarks.generated.flatbuffers.Item;
import io.github.snovader.tafla.benchmarks.generated.flatbuffers.List;
import io.github.snovader.tafla.benchmarks.generated.flatbuffers.Pair;
import io.github.snovader.tafla.benchmarks.generated.flatbuffers.Record;
import io.github.snovader.tafla.benchmarks.generated.flatbuffers.Sample;
import io.github.snovader.tafla.benchmarks.generated.flatbuffers.Strings;
import io.github.snovader.tafla.benchmarks.generated.flatbuffers.Triple;
import io.github.snovader.tafla.benchmarks.generated.flatbuffers.Triples;

import java.nio.ByteBuffer;
import java.util.Arrays;

/**
 * flatc's generated code, used as its documentation shows: a builder of the default size, vectors
 * created element by element, and float columns copied out of {@code xAsByteBuffer()} views. Keys
 * are widened to {@code long}s through the {@code IntVector} of their uint32s. A vector of structs
 * is started, filled with {@code createTriple()} from its last item to its first, and ended.
 */
public final class FlatBuffersCodec {
    private FlatBuffersCodec() {}

    public static Codec create() {
        return new Codec(
            "flatbuffers",
            new Records(),
            new Documents(),
            new Lists(),
            new Structs(),
            new Samples(),
            new StringTables()
        );
    }

    private static final class Records implements Workload<RecordInput> {
        @Override
        public byte[] serialize(RecordInput input) {
            var builder = new FlatBufferBuilder();
            var name = builder.createString(input.name());

            builder.finish(Record.createRecord(builder, input.x(), input.y(), input.id(), name));

            return builder.sizedByteArray();
        }

        @Override
        public RecordInput deserialize(byte[] bytes) {
            var record = Record.getRootAsRecord(ByteBuffer.wrap(bytes));

            return new RecordInput(record.x(), record.y(), record.id(), record.name());
        }
    }

    private static final class Documents implements DocumentWorkload {
        @Override
        public byte[] serialize(DocumentInput input) {
            var builder = new FlatBufferBuilder();
            var keys = createKeysVector(builder, input.keys());
            var a = Columns.createAVector(builder, input.a());
            var b = Columns.createBVector(builder, input.b());
            var columns = Columns.createColumns(builder, input.base(), keys, a, b);
            var name = builder.createString(input.name());

            builder.finish(Document.createDocument(builder, input.x(), input.y(), name, columns));

            return builder.sizedByteArray();
        }

        @Override
        public DocumentInput deserialize(byte[] bytes) {
            var document = Document.getRootAsDocument(ByteBuffer.wrap(bytes));
            var columns = document.columns();

            return new DocumentInput(
                document.x(),
                document.y(),
                document.name(),
                columns.base(),
                readKeys(columns.keysVector()),
                readFloats(columns.aAsByteBuffer()),
                readFloats(columns.bAsByteBuffer())
            );
        }

        @Override
        public double sumA(byte[] bytes) {
            var columns = Document.getRootAsDocument(ByteBuffer.wrap(bytes)).columns();

            double sum = 0;
            for (int i = 0, length = columns.aLength(); i < length; i++) {
                sum += columns.a(i);
            }

            return sum;
        }

        /** Writes uint32 keys from {@code long}s, as {@code createKeysVector()} does from {@code int}s */
        private static int createKeysVector(FlatBufferBuilder builder, long[] keys) {
            Columns.startKeysVector(builder, keys.length);
            for (int i = keys.length - 1; i >= 0; i--) {
                builder.addInt((int) keys[i]);
            }

            return builder.endVector();
        }

        private static long[] readKeys(IntVector vector) {
            var keys = new long[vector.length()];
            for (int i = 0; i < keys.length; i++) {
                keys[i] = vector.getAsUnsigned(i);
            }

            return keys;
        }

        /** Copies the floats of a little-endian view, which {@code xAsByteBuffer()} returns */
        private static float[] readFloats(ByteBuffer bytes) {
            var floats = bytes.asFloatBuffer();
            var values = new float[floats.remaining()];
            floats.get(values);

            return values;
        }
    }

    private static final class Lists implements Workload<ListInput> {
        @Override
        public byte[] serialize(ListInput input) {
            var builder = new FlatBufferBuilder();
            var items = input.items();

            var offsets = new int[items.size()];
            for (int i = 0; i < offsets.length; i++) {
                offsets[i] = createItem(builder, items.get(i));
            }

            builder.finish(List.createList(builder, List.createItemsVector(builder, offsets)));

            return builder.sizedByteArray();
        }

        @Override
        public ListInput deserialize(byte[] bytes) {
            var list = List.getRootAsList(ByteBuffer.wrap(bytes));

            var items = new ItemInput[list.itemsLength()];
            for (int i = 0; i < items.length; i++) {
                var item = list.items(i);
                var pair = item.pair();

                items[i] = new ItemInput(item.value(), pair.first(), pair.second());
            }

            return new ListInput(Arrays.asList(items));
        }

        private static int createItem(FlatBufferBuilder builder, ItemInput input) {
            var pair = Pair.createPair(builder, input.first(), input.second());
            return Item.createItem(builder, input.value(), pair);
        }
    }

    private static final class Structs implements Workload<ListInput> {
        @Override
        public byte[] serialize(ListInput input) {
            var builder = new FlatBufferBuilder();
            var items = input.items();

            // Builders write back to front, so the last item is written first
            Triples.startItemsVector(builder, items.size());
            for (int i = items.size() - 1; i >= 0; i--) {
                var item = items.get(i);
                Triple.createTriple(builder, item.value(), item.first(), item.second());
            }
            var vector = builder.endVector();

            builder.finish(Triples.createTriples(builder, vector));

            return builder.sizedByteArray();
        }

        @Override
        public ListInput deserialize(byte[] bytes) {
            var triples = Triples.getRootAsTriples(ByteBuffer.wrap(bytes));

            var items = new ItemInput[triples.itemsLength()];
            for (int i = 0; i < items.length; i++) {
                var triple = triples.items(i);
                items[i] = new ItemInput(triple.value(), triple.first(), triple.second());
            }

            return new ListInput(Arrays.asList(items));
        }
    }

    private static final class Samples implements Workload<SampleInput> {
        @Override
        public byte[] serialize(SampleInput input) {
            var builder = new FlatBufferBuilder();
            builder.finish(Sample.createSample(builder, input.key(), input.a(), input.b()));

            return builder.sizedByteArray();
        }

        @Override
        public SampleInput deserialize(byte[] bytes) {
            var sample = Sample.getRootAsSample(ByteBuffer.wrap(bytes));

            return new SampleInput(sample.key(), sample.a(), sample.b());
        }
    }

    /** A vector of strings, created from the offsets of the strings that the builder wrote first */
    private static final class StringTables implements Workload<java.util.List<String>> {
        @Override
        public byte[] serialize(java.util.List<String> strings) {
            var builder = new FlatBufferBuilder();

            var offsets = new int[strings.size()];
            for (int i = 0; i < offsets.length; i++) {
                offsets[i] = builder.createString(strings.get(i));
            }

            var items = Strings.createItemsVector(builder, offsets);
            builder.finish(Strings.createStrings(builder, items));

            return builder.sizedByteArray();
        }

        @Override
        public java.util.List<String> deserialize(byte[] bytes) {
            var table = Strings.getRootAsStrings(ByteBuffer.wrap(bytes));

            var strings = new String[table.itemsLength()];
            for (int i = 0; i < strings.length; i++) {
                strings[i] = table.items(i);
            }

            return Arrays.asList(strings);
        }
    }
}
