package io.github.snovader.tafla.benchmarks.jmh;

import io.github.snovader.tafla.benchmarks.*;
import io.github.snovader.tafla.benchmarks.codecs.*;
import org.openjdk.jmh.annotations.*;

import java.util.*;

/**
 * The strings suite: a table of 16 distinct strings of one shape and nothing else, so that a call
 * spends its time on their UTF-8, from {@link StringShapes}, whose names the shapes are
 */
@State(Scope.Benchmark)
public class StringsBenchmark {
    @Param({"tafla", "tafla-gen", "flatbuffers", "protobuf", "msgpack"})
    public String codec;

    @Param({
        "empty",
        "8-ascii",
        "8-one-2-byte",
        "8-3-byte",
        "16-two-2-byte",
        "15-one-4-byte",
        "16-ascii",
        "32-ascii",
        "32-3-byte",
        "42-ascii",
        "43-ascii",
        "63-ascii",
        "64-ascii",
        "63-last-2-byte",
        "64-last-2-byte",
        "63-last-3-byte",
        "64-last-3-byte",
        "127-ascii",
        "128-ascii",
        "256-ascii",
        "256-3-byte",
        "511-ascii",
        "512-ascii",
        "1024-ascii"
    })
    public String shape;

    // Fields rather than constants, so that the JIT can't fold the input into the code
    private List<String> strings;
    private Workload<List<String>> workload;
    private byte[] bytes;

    @Setup
    public void setUp() {
        strings = StringShapes.named(shape).strings();
        workload = Codecs.named(codec).strings();
        bytes = Codecs.serializeChecked(workload, strings);
    }

    @Benchmark
    public byte[] serialize() {
        return workload.serialize(strings);
    }

    @Benchmark
    public List<String> deserialize() {
        return workload.deserialize(bytes);
    }
}
