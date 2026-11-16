package io.github.snovader.tafla.benchmarks.jmh;

import io.github.snovader.tafla.benchmarks.codecs.*;
import org.openjdk.jmh.annotations.*;

import java.util.function.*;

/**
 * Tables of {@link BuilderShapes}, written by tafla's {@code TableBuilder} alone, to compare one
 * version of it with another
 */
@State(Scope.Benchmark)
public class BuilderBenchmark {
    /** The one codec, tafla's {@code TableBuilder}, which the task's table of results needs */
    @Param({"tafla"})
    public String codec;

    @Param({
        "1 int8[100]",
        "4 int8[100]",
        "8 int8[255]",
        "8 int8[256]",
        "1000 int8[255]",
        "10 float64[16]",
        "mixed",
        "nested builder",
        "chain of builders",
        "padded chain in place",
        "document of 100 rows from builders",
        "document of 100 rows from bytes",
        "document of 100000 rows from builders",
        "document of 100000 rows from bytes",
        "vector of 100 tables from builders",
        "vector of 100 tables from bytes"
    })
    public String shape;

    private Supplier<byte[]> build;

    @Setup
    public void setUp() {
        var shape = BuilderShapes.named(this.shape);
        BuilderShapes.buildChecked(this.shape, shape);
        build = shape.build();
    }

    @Benchmark
    public byte[] build() {
        return build.get();
    }
}
