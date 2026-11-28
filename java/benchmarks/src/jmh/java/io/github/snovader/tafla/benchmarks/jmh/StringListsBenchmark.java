package io.github.snovader.tafla.benchmarks.jmh;

import io.github.snovader.tafla.benchmarks.*;
import io.github.snovader.tafla.benchmarks.codecs.*;
import org.openjdk.jmh.annotations.*;

import java.util.*;

/**
 * The lists suite: a table of one list of 10, 100 or 10,000 strings of one kind, from
 * {@link ListShapes}, as a vector of strings and as a vector of tables of one string each, with
 * the code that tafla-gen generates, which {@link StringListCodecs} names
 */
@State(Scope.Benchmark)
public class StringListsBenchmark {
    @Param({StringListCodecs.STRING_VECTOR, StringListCodecs.NAME_VECTOR, StringListCodecs.STRING_VECTOR_AA})
    public String codec;

    @Param({"10", "100", "10000"})
    public int count;

    @Param({"16-ascii", "256-ascii", "8-3-byte"})
    public String kind;

    // Fields rather than constants, so that the JIT can't fold the input into the code
    private List<String> strings;
    private StringListWorkload workload;
    private byte[] bytes;

    @Setup
    public void setUp() {
        strings = ListShapes.of(count, kind).strings();
        workload = StringListCodecs.named(codec);
        bytes = StringListCodecs.serializeChecked(workload, strings);
    }

    /** Writes the table */
    @Benchmark
    public byte[] serialize() {
        return workload.serialize(strings);
    }

    /** Reads every string */
    @Benchmark
    public List<String> deserialize() {
        return workload.deserialize(bytes);
    }

    /** Reads the last string */
    @Benchmark
    public String readLast() {
        return workload.readLast(bytes);
    }
}
