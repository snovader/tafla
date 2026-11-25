package io.github.snovader.tafla.benchmarks;

import java.util.*;

/**
 * Generic table shapes, sized to bracket how tafla is used: small tables of scalars and a string,
 * documents whose header nests a table of numeric columns, vectors of small nested tables, arrays of
 * structs, and fixed-size records. The values are the TypeScript comparison's, from the same seeded
 * generator, so that the languages' numbers compare.
 */
public final class Workloads {
    /** Rows per document: a small one and a large one */
    public static final List<Integer> DOCUMENT_ROWS = List.of(100, 100_000);

    public static final int LIST_ITEMS = 100;

    public static final RecordInput RECORD = new RecordInput(59.3293, 18.0686, -631_152_000L, "tafla-benchmarks");

    public static final SampleInput SAMPLE = new SampleInput(2_743_826_400L, 21.75f, 3.25f);

    private Workloads() {}

    public static DocumentInput documentInput(int rows) {
        var random = new Xorshift(rows);

        // Keys below 2^24 take 4 bytes in protobuf's varints, like the floats
        var keys = new long[rows];
        var a = new float[rows];
        var b = new float[rows];
        for (int i = 0; i < rows; i++) {
            keys[i] = (long) Math.floor(random.next() * 0x1p24);
            a[i] = (float) ((random.next() - 0.5) * 100);
            b[i] = (float) ((random.next() - 0.5) * 100);
        }

        return new DocumentInput(RECORD.x(), RECORD.y(), RECORD.name(), RECORD.id(), keys, a, b);
    }

    public static ListInput listInput(int items) {
        var random = new Xorshift(items);

        // Float32 values, so that every format reads them back exactly
        var list = new ArrayList<ItemInput>(items);
        for (int i = 0; i < items; i++) {
            var value = (float) (random.next() * 30);
            var first = (float) (random.next() * 4 - 2);
            var second = (float) (random.next() * 50);

            list.add(new ItemInput(value, first, second));
        }

        return new ListInput(List.copyOf(list));
    }
}
