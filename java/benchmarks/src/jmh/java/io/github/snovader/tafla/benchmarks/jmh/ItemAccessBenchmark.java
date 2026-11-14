package io.github.snovader.tafla.benchmarks.jmh;

import io.github.snovader.tafla.benchmarks.*;
import io.github.snovader.tafla.benchmarks.codecs.*;
import org.openjdk.jmh.annotations.*;

/**
 * The last item of the vector of 100 nested tables and of the array of 100 structs, which a reader
 * can find without reading the others, and every item, read from tafla's bytes in each way of
 * {@link ItemAccesses}
 */
@State(Scope.Benchmark)
public class ItemAccessBenchmark {
    @Param({"tafla", "tafla-gen", "tafla-gen A/A"})
    public String codec;

    private ItemAccess access;
    private byte[] tables;
    private byte[] structs;

    @Setup
    public void setUp() {
        var input = Workloads.listInput(Workloads.LIST_ITEMS);
        var tafla = Codecs.named("tafla");
        access = ItemAccesses.named(codec);
        tables = Codecs.serializeChecked(tafla.list(), input);
        structs = Codecs.serializeChecked(tafla.structs(), input);

        var items = input.items();
        var last = ItemAccesses.sum(items.getLast());
        double every = 0;
        for (var item : items) {
            every += ItemAccesses.sum(item);
        }
        checkSum("the vector's last item", access.lastTable(tables), last);
        checkSum("every item of the vector", access.everyTable(tables), every);
        checkSum("the array's last struct", access.lastStruct(structs), last);
        checkSum("every struct of the array", access.everyStruct(structs), every);
    }

    @Benchmark
    public double vectorLastItem() {
        return access.lastTable(tables);
    }

    @Benchmark
    public double vectorEveryItem() {
        return access.everyTable(tables);
    }

    @Benchmark
    public double structsLastItem() {
        return access.lastStruct(structs);
    }

    @Benchmark
    public double structsEveryItem() {
        return access.everyStruct(structs);
    }

    /** Checks the sum by its bits, so that a broken access isn't timed */
    private void checkSum(String items, double actual, double expected) {
        if (Double.doubleToRawLongBits(actual) != Double.doubleToRawLongBits(expected)) {
            throw new IllegalStateException(
                String.format("%s summed %s as %s rather than %s", codec, items, actual, expected)
            );
        }
    }
}
