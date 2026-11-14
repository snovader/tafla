package io.github.snovader.tafla.benchmarks.codecs;

import io.github.snovader.tafla.ByteBuffer;
import io.github.snovader.tafla.benchmarks.*;
import io.github.snovader.tafla.benchmarks.generated.tafla.ItemList;
import io.github.snovader.tafla.benchmarks.generated.tafla.Triples;

import java.util.LinkedHashMap;
import java.util.SequencedMap;

/**
 * The ways of reading tafla's items: {@link ByteBuffer}'s getters by hand, as the tafla codec reads
 * the list, and tafla-gen's getters, whose views read each item when it's asked for
 */
public final class ItemAccesses {
    /** An item as a struct: its three float32s */
    private static final int ITEM_SIZE = 3 * Float.BYTES;

    private ItemAccesses() {}

    /** {@return a new access of each way by name, with tafla's {@code ByteBuffer} first} */
    public static SequencedMap<String, ItemAccess> all() {
        var accesses = new LinkedHashMap<String, ItemAccess>();
        accesses.put("tafla", new ByteBufferAccess());
        accesses.put("tafla-gen", new GetterAccess());
        // The getters twice, an A/A pair that shows the harness's noise
        accesses.put("tafla-gen A/A", new GetterAccess());

        return accesses;
    }

    /**
     * {@return a new access by its name}
     *
     * @throws IllegalArgumentException if no access has that name
     */
    public static ItemAccess named(String name) {
        var access = all().get(name);
        if (access == null) {
            throw new IllegalArgumentException("No access is named " + name);
        }

        return access;
    }

    /** {@return the sum of an item's three float32s, in the order that every access adds them} */
    public static double sum(ItemInput item) {
        return (double) item.value() + item.first() + item.second();
    }

    private static final class ByteBufferAccess implements ItemAccess {
        @Override
        public double lastTable(byte[] bytes) {
            var buffer = new ByteBuffer(bytes);
            var vector = buffer.rootOffset(0);
            var item = buffer.getVectorOffset(vector, buffer.getVectorLength(vector) - 1);

            return sumTable(buffer, item);
        }

        @Override
        public double everyTable(byte[] bytes) {
            var buffer = new ByteBuffer(bytes);
            var offsets = buffer.getVectorOffsets(buffer.rootOffset(0));

            double sum = 0;
            for (var item : offsets) {
                sum += sumTable(buffer, item);
            }

            return sum;
        }

        @Override
        public double lastStruct(byte[] bytes) {
            var buffer = new ByteBuffer(bytes);
            var view = buffer.getStructArray(buffer.rootOffset(0), ITEM_SIZE);

            return sumStruct(view, view.remaining() - ITEM_SIZE);
        }

        @Override
        public double everyStruct(byte[] bytes) {
            var buffer = new ByteBuffer(bytes);
            var view = buffer.getStructArray(buffer.rootOffset(0), ITEM_SIZE);

            double sum = 0;
            for (int offset = 0; offset < view.remaining(); offset += ITEM_SIZE) {
                sum += sumStruct(view, offset);
            }

            return sum;
        }

        private static double sumTable(ByteBuffer buffer, int item) {
            var pair = buffer.fieldOffset(item, 1);

            return (double) buffer.getFloat32(buffer.fieldOffset(item, 0))
                + buffer.getFloat32(buffer.fieldOffset(pair, 0))
                + buffer.getFloat32(buffer.fieldOffset(pair, 1));
        }

        private static double sumStruct(java.nio.ByteBuffer view, int offset) {
            return (double) view.getFloat(offset)
                + view.getFloat(offset + Float.BYTES)
                + view.getFloat(offset + 2 * Float.BYTES);
        }
    }

    private static final class GetterAccess implements ItemAccess {
        @Override
        public double lastTable(byte[] bytes) {
            var items = new ItemList(bytes).items();
            var item = items.get(items.size() - 1);
            var pair = item.pair();

            return (double) item.value() + pair.first() + pair.second();
        }

        @Override
        public double everyTable(byte[] bytes) {
            // Iterated, which reads each item's end once, where get(i) reads two
            double sum = 0;
            for (var item : new ItemList(bytes).items()) {
                var pair = item.pair();
                sum += (double) item.value() + pair.first() + pair.second();
            }

            return sum;
        }

        @Override
        public double lastStruct(byte[] bytes) {
            var triples = new Triples(bytes).items();
            var triple = triples.get(triples.size() - 1);

            return (double) triple.value() + triple.first() + triple.second();
        }

        @Override
        public double everyStruct(byte[] bytes) {
            var triples = new Triples(bytes).items();

            double sum = 0;
            for (int i = 0; i < triples.size(); i++) {
                var triple = triples.get(i);
                sum += (double) triple.value() + triple.first() + triple.second();
            }

            return sum;
        }
    }
}
