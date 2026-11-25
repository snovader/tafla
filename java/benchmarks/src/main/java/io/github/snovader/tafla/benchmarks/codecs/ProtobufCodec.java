package io.github.snovader.tafla.benchmarks.codecs;

import com.google.protobuf.InvalidProtocolBufferException;
import com.google.protobuf.Parser;
import io.github.snovader.tafla.benchmarks.*;
import io.github.snovader.tafla.benchmarks.generated.protobuf.Messages;

import java.io.*;
import java.util.Arrays;
import java.util.List;

/**
 * protoc's generated messages, built with their builders and parsed with their parsers. Repeated
 * scalars are added and read one element at a time, which keeps them unboxed, and are copied into
 * arrays when parsed. uint32s are Java {@code int}s with the same bits. Repeated strings are added
 * together, and read as the parsed message's list.
 *
 * <p>protobuf has no structs, so an array of them is a repeated message of the three floats, whose
 * fields protobuf writes as fixed32s. Packed repeated floats would be a column for each field,
 * which the documents time already.</p>
 */
public final class ProtobufCodec {
    private ProtobufCodec() {}

    public static Codec create() {
        return new Codec(
            "protobuf",
            new Records(),
            new Documents(),
            new Lists(),
            new Structs(),
            new Samples(),
            new StringTables()
        );
    }

    /**
     * Parses a message, rethrowing an invalid message's checked exception unchecked
     *
     * @throws UncheckedIOException if the bytes aren't a valid message
     */
    private static <T> T parse(Parser<T> parser, byte[] bytes) {
        try {
            return parser.parseFrom(bytes);
        } catch (InvalidProtocolBufferException ex) {
            throw new UncheckedIOException(ex);
        }
    }

    private static final class Records implements Workload<RecordInput> {
        @Override
        public byte[] serialize(RecordInput input) {
            return Messages.Record.newBuilder()
                .setX(input.x())
                .setY(input.y())
                .setId(input.id())
                .setName(input.name())
                .build()
                .toByteArray();
        }

        @Override
        public RecordInput deserialize(byte[] bytes) {
            var record = parse(Messages.Record.parser(), bytes);

            return new RecordInput(record.getX(), record.getY(), record.getId(), record.getName());
        }
    }

    private static final class Documents implements DocumentWorkload {
        @Override
        public byte[] serialize(DocumentInput input) {
            var columns = Messages.Columns.newBuilder().setBase(input.base());
            for (var key : input.keys()) {
                columns.addKeys((int) key);
            }
            for (var value : input.a()) {
                columns.addA(value);
            }
            for (var value : input.b()) {
                columns.addB(value);
            }

            return Messages.Document.newBuilder()
                .setX(input.x())
                .setY(input.y())
                .setName(input.name())
                .setColumns(columns)
                .build()
                .toByteArray();
        }

        @Override
        public DocumentInput deserialize(byte[] bytes) {
            var document = parse(Messages.Document.parser(), bytes);
            var columns = document.getColumns();

            var keys = new long[columns.getKeysCount()];
            for (int i = 0; i < keys.length; i++) {
                keys[i] = Integer.toUnsignedLong(columns.getKeys(i));
            }

            var a = new float[columns.getACount()];
            for (int i = 0; i < a.length; i++) {
                a[i] = columns.getA(i);
            }

            var b = new float[columns.getBCount()];
            for (int i = 0; i < b.length; i++) {
                b[i] = columns.getB(i);
            }

            return new DocumentInput(
                document.getX(),
                document.getY(),
                document.getName(),
                columns.getBase(),
                keys,
                a,
                b
            );
        }

        @Override
        public double sumA(byte[] bytes) {
            var columns = parse(Messages.Document.parser(), bytes).getColumns();

            double sum = 0;
            for (int i = 0, length = columns.getACount(); i < length; i++) {
                sum += columns.getA(i);
            }

            return sum;
        }
    }

    private static final class Lists implements Workload<ListInput> {
        @Override
        public byte[] serialize(ListInput input) {
            var list = Messages.List.newBuilder();
            for (var item : input.items()) {
                list.addItems(itemMessage(item));
            }

            return list.build().toByteArray();
        }

        @Override
        public ListInput deserialize(byte[] bytes) {
            var list = parse(Messages.List.parser(), bytes);

            var items = new ItemInput[list.getItemsCount()];
            for (int i = 0; i < items.length; i++) {
                var item = list.getItems(i);
                var pair = item.getPair();

                items[i] = new ItemInput(item.getValue(), pair.getFirst(), pair.getSecond());
            }

            return new ListInput(Arrays.asList(items));
        }

        private static Messages.Item.Builder itemMessage(ItemInput input) {
            var pair = Messages.Pair.newBuilder().setFirst(input.first()).setSecond(input.second());
            return Messages.Item.newBuilder().setValue(input.value()).setPair(pair);
        }
    }

    private static final class Structs implements Workload<ListInput> {
        @Override
        public byte[] serialize(ListInput input) {
            var triples = Messages.Triples.newBuilder();
            for (var item : input.items()) {
                triples.addItems(
                    Messages.Triple.newBuilder()
                        .setValue(item.value())
                        .setFirst(item.first())
                        .setSecond(item.second())
                );
            }

            return triples.build().toByteArray();
        }

        @Override
        public ListInput deserialize(byte[] bytes) {
            var triples = parse(Messages.Triples.parser(), bytes);

            var items = new ItemInput[triples.getItemsCount()];
            for (int i = 0; i < items.length; i++) {
                var triple = triples.getItems(i);
                items[i] = new ItemInput(triple.getValue(), triple.getFirst(), triple.getSecond());
            }

            return new ListInput(Arrays.asList(items));
        }
    }

    private static final class Samples implements Workload<SampleInput> {
        @Override
        public byte[] serialize(SampleInput input) {
            return Messages.Sample.newBuilder()
                .setKey((int) input.key())
                .setA(input.a())
                .setB(input.b())
                .build()
                .toByteArray();
        }

        @Override
        public SampleInput deserialize(byte[] bytes) {
            var sample = parse(Messages.Sample.parser(), bytes);

            return new SampleInput(Integer.toUnsignedLong(sample.getKey()), sample.getA(), sample.getB());
        }
    }

    /** A repeated string, whose strings the parser decodes as it parses them */
    private static final class StringTables implements Workload<List<String>> {
        @Override
        public byte[] serialize(List<String> strings) {
            return Messages.Strings.newBuilder().addAllItems(strings).build().toByteArray();
        }

        @Override
        public List<String> deserialize(byte[] bytes) {
            return parse(Messages.Strings.parser(), bytes).getItemsList();
        }
    }
}
