package io.github.snovader.tafla.benchmarks.codecs;

import io.github.snovader.tafla.benchmarks.*;
import org.msgpack.core.*;
import org.msgpack.core.buffer.*;

import java.io.*;
import java.util.Arrays;
import java.util.List;

/**
 * msgpack-core's packer and unpacker. MessagePack is schemaless: a table packs as a map with its
 * keys, which the reader matches in whatever order they come, as msgpackr's objects do. A column
 * packs as an array of float32s or uint32s, element by element, since MessagePack has no typed
 * arrays; msgpackr's extension that copies typed arrays' bytes is its own.
 *
 * <p>One packer and one unpacker serve every message of a codec, as msgpackr reuses its buffer
 * and state: the packer is cleared before each message, and {@code toByteArray()} copies the
 * message out of it, and the unpacker is reset to each message's bytes, keeping the string decoder
 * that it allocates for its first string.</p>
 */
public final class MessagePackCodec {
    private MessagePackCodec() {}

    public static Codec create() {
        var state = new State();
        return new Codec(
            "msgpack",
            new Records(state),
            new Documents(state),
            new Lists(state),
            new Structs(state),
            new Samples(state),
            new StringTables(state)
        );
    }

    /** The packer and unpacker that a codec's workloads share */
    private static final class State {
        private final MessageBufferPacker packer = MessagePack.newDefaultBufferPacker();
        private final ArrayBufferInput input = new ArrayBufferInput(new byte[0]);
        private final MessageUnpacker unpacker = MessagePack.newDefaultUnpacker(input);

        /** {@return the packer, cleared of the last message} */
        MessageBufferPacker packer() {
            packer.clear();
            return packer;
        }

        /** {@return the unpacker, reset to read {@code bytes}} */
        MessageUnpacker unpacker(byte[] bytes) throws IOException {
            input.reset(bytes);
            unpacker.reset(input);

            return unpacker;
        }
    }

    /** Packs and unpacks one shape, rethrowing the packer's and unpacker's exceptions unchecked */
    private abstract static class MessagePackWorkload<T> implements Workload<T> {
        final State state;

        MessagePackWorkload(State state) {
            this.state = state;
        }

        @Override
        public final byte[] serialize(T input) {
            var packer = state.packer();
            try {
                pack(packer, input);
            } catch (IOException ex) {
                throw new UncheckedIOException(ex);
            }

            return packer.toByteArray();
        }

        @Override
        public final T deserialize(byte[] bytes) {
            try {
                return unpack(state.unpacker(bytes));
            } catch (IOException ex) {
                throw new UncheckedIOException(ex);
            }
        }

        abstract void pack(MessagePacker packer, T input) throws IOException;

        abstract T unpack(MessageUnpacker unpacker) throws IOException;
    }

    private static final class Records extends MessagePackWorkload<RecordInput> {
        Records(State state) {
            super(state);
        }

        @Override
        void pack(MessagePacker packer, RecordInput input) throws IOException {
            packer.packMapHeader(4);
            packer.packString("x").packDouble(input.x());
            packer.packString("y").packDouble(input.y());
            packer.packString("id").packLong(input.id());
            packer.packString("name").packString(input.name());
        }

        @Override
        RecordInput unpack(MessageUnpacker unpacker) throws IOException {
            double x = 0;
            double y = 0;
            long id = 0;
            String name = "";

            for (int i = unpacker.unpackMapHeader(); i > 0; i--) {
                switch (unpacker.unpackString()) {
                    case "x" -> x = unpacker.unpackDouble();
                    case "y" -> y = unpacker.unpackDouble();
                    case "id" -> id = unpacker.unpackLong();
                    case "name" -> name = unpacker.unpackString();
                    default -> unpacker.skipValue();
                }
            }

            return new RecordInput(x, y, id, name);
        }
    }

    /** A document packs as one map, without a nested one for its columns, as msgpackr packs it */
    private static final class Documents extends MessagePackWorkload<DocumentInput> implements DocumentWorkload {
        private static final long[] NO_KEYS = {};
        private static final float[] NO_VALUES = {};

        Documents(State state) {
            super(state);
        }

        @Override
        void pack(MessagePacker packer, DocumentInput input) throws IOException {
            packer.packMapHeader(7);
            packer.packString("x").packDouble(input.x());
            packer.packString("y").packDouble(input.y());
            packer.packString("name").packString(input.name());
            packer.packString("base").packLong(input.base());

            packer.packString("keys").packArrayHeader(input.keys().length);
            for (var key : input.keys()) {
                packer.packLong(key);
            }

            packFloats(packer.packString("a"), input.a());
            packFloats(packer.packString("b"), input.b());
        }

        @Override
        DocumentInput unpack(MessageUnpacker unpacker) throws IOException {
            double x = 0;
            double y = 0;
            String name = "";
            long base = 0;
            long[] keys = NO_KEYS;
            float[] a = NO_VALUES;
            float[] b = NO_VALUES;

            for (int i = unpacker.unpackMapHeader(); i > 0; i--) {
                switch (unpacker.unpackString()) {
                    case "x" -> x = unpacker.unpackDouble();
                    case "y" -> y = unpacker.unpackDouble();
                    case "name" -> name = unpacker.unpackString();
                    case "base" -> base = unpacker.unpackLong();
                    case "keys" -> keys = unpackKeys(unpacker);
                    case "a" -> a = unpackFloats(unpacker);
                    case "b" -> b = unpackFloats(unpacker);
                    default -> unpacker.skipValue();
                }
            }

            return new DocumentInput(x, y, name, base, keys, a, b);
        }

        /** Skips to column {@code a} and sums it as it unpacks, without unpacking the fields after it */
        @Override
        public double sumA(byte[] bytes) {
            try {
                var unpacker = state.unpacker(bytes);
                for (int i = unpacker.unpackMapHeader(); i > 0; i--) {
                    if (unpacker.unpackString().equals("a")) {
                        return sumFloats(unpacker);
                    }

                    unpacker.skipValue();
                }

                return 0;
            } catch (IOException ex) {
                throw new UncheckedIOException(ex);
            }
        }

        private static void packFloats(MessagePacker packer, float[] values) throws IOException {
            packer.packArrayHeader(values.length);
            for (var value : values) {
                packer.packFloat(value);
            }
        }

        private static long[] unpackKeys(MessageUnpacker unpacker) throws IOException {
            var keys = new long[unpacker.unpackArrayHeader()];
            for (int i = 0; i < keys.length; i++) {
                keys[i] = unpacker.unpackLong();
            }

            return keys;
        }

        private static float[] unpackFloats(MessageUnpacker unpacker) throws IOException {
            var values = new float[unpacker.unpackArrayHeader()];
            for (int i = 0; i < values.length; i++) {
                values[i] = unpacker.unpackFloat();
            }

            return values;
        }

        private static double sumFloats(MessageUnpacker unpacker) throws IOException {
            double sum = 0;
            for (int i = unpacker.unpackArrayHeader(); i > 0; i--) {
                sum += unpacker.unpackFloat();
            }

            return sum;
        }
    }

    /** An item packs as one map of its three floats, as msgpackr packs it */
    private static final class Lists extends MessagePackWorkload<ListInput> {
        Lists(State state) {
            super(state);
        }

        @Override
        void pack(MessagePacker packer, ListInput input) throws IOException {
            var items = input.items();

            packer.packMapHeader(1);
            packer.packString("items").packArrayHeader(items.size());
            for (var item : items) {
                packer.packMapHeader(3);
                packer.packString("value").packFloat(item.value());
                packer.packString("first").packFloat(item.first());
                packer.packString("second").packFloat(item.second());
            }
        }

        @Override
        ListInput unpack(MessageUnpacker unpacker) throws IOException {
            var items = new ItemInput[0];

            for (int i = unpacker.unpackMapHeader(); i > 0; i--) {
                if (unpacker.unpackString().equals("items")) {
                    items = unpackItems(unpacker);
                } else {
                    unpacker.skipValue();
                }
            }

            return new ListInput(Arrays.asList(items));
        }

        private static ItemInput[] unpackItems(MessageUnpacker unpacker) throws IOException {
            var items = new ItemInput[unpacker.unpackArrayHeader()];
            for (int i = 0; i < items.length; i++) {
                items[i] = unpackItem(unpacker);
            }

            return items;
        }

        private static ItemInput unpackItem(MessageUnpacker unpacker) throws IOException {
            float value = 0;
            float first = 0;
            float second = 0;

            for (int i = unpacker.unpackMapHeader(); i > 0; i--) {
                switch (unpacker.unpackString()) {
                    case "value" -> value = unpacker.unpackFloat();
                    case "first" -> first = unpacker.unpackFloat();
                    case "second" -> second = unpacker.unpackFloat();
                    default -> unpacker.skipValue();
                }
            }

            return new ItemInput(value, first, second);
        }
    }

    /**
     * Structs have a fixed shape, so each packs as an array of its fields in order, without their
     * keys, and so does the table that holds them, as rmp-serde's {@code to_vec()} packs both. The
     * reader checks the arrays' lengths, since it reads the fields by their position.
     */
    private static final class Structs extends MessagePackWorkload<ListInput> {
        private static final int TABLE_FIELDS = 1;
        private static final int ITEM_FIELDS = 3;

        Structs(State state) {
            super(state);
        }

        @Override
        void pack(MessagePacker packer, ListInput input) throws IOException {
            var items = input.items();

            packer.packArrayHeader(TABLE_FIELDS).packArrayHeader(items.size());
            for (var item : items) {
                packer.packArrayHeader(ITEM_FIELDS);
                packer.packFloat(item.value()).packFloat(item.first()).packFloat(item.second());
            }
        }

        @Override
        ListInput unpack(MessageUnpacker unpacker) throws IOException {
            unpackFields(unpacker, TABLE_FIELDS);

            var items = new ItemInput[unpacker.unpackArrayHeader()];
            for (int i = 0; i < items.length; i++) {
                unpackFields(unpacker, ITEM_FIELDS);
                items[i] = new ItemInput(unpacker.unpackFloat(), unpacker.unpackFloat(), unpacker.unpackFloat());
            }

            return new ListInput(Arrays.asList(items));
        }

        /** @throws MessageTypeException if the next value isn't an array of {@code fields} values */
        private static void unpackFields(MessageUnpacker unpacker, int fields) throws IOException {
            var length = unpacker.unpackArrayHeader();
            if (length != fields) {
                throw new MessageTypeException(String.format("Expected %d fields, not %d", fields, length));
            }
        }
    }

    private static final class Samples extends MessagePackWorkload<SampleInput> {
        Samples(State state) {
            super(state);
        }

        @Override
        void pack(MessagePacker packer, SampleInput input) throws IOException {
            packer.packMapHeader(3);
            packer.packString("key").packLong(input.key());
            packer.packString("a").packFloat(input.a());
            packer.packString("b").packFloat(input.b());
        }

        @Override
        SampleInput unpack(MessageUnpacker unpacker) throws IOException {
            long key = 0;
            float a = 0;
            float b = 0;

            for (int i = unpacker.unpackMapHeader(); i > 0; i--) {
                switch (unpacker.unpackString()) {
                    case "key" -> key = unpacker.unpackLong();
                    case "a" -> a = unpacker.unpackFloat();
                    case "b" -> b = unpacker.unpackFloat();
                    default -> unpacker.skipValue();
                }
            }

            return new SampleInput(key, a, b);
        }
    }

    /** An array of strings, which the unpacker decodes as it unpacks them */
    private static final class StringTables extends MessagePackWorkload<List<String>> {
        StringTables(State state) {
            super(state);
        }

        @Override
        void pack(MessagePacker packer, List<String> strings) throws IOException {
            packer.packArrayHeader(strings.size());
            for (var string : strings) {
                packer.packString(string);
            }
        }

        @Override
        List<String> unpack(MessageUnpacker unpacker) throws IOException {
            var strings = new String[unpacker.unpackArrayHeader()];
            for (int i = 0; i < strings.length; i++) {
                strings[i] = unpacker.unpackString();
            }

            return Arrays.asList(strings);
        }
    }
}
