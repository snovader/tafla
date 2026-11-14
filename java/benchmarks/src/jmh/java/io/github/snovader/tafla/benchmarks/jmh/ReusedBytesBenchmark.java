package io.github.snovader.tafla.benchmarks.jmh;

import io.github.snovader.tafla.benchmarks.*;
import io.github.snovader.tafla.benchmarks.codecs.*;
import org.openjdk.jmh.annotations.*;

/**
 * The document of 100,000 rows written into the start of the same bytes in every call, rather than
 * into new bytes, by the codecs that can: tafla-gen's {@code Document.serializeInto()}
 */
@State(Scope.Benchmark)
public class ReusedBytesBenchmark {
    @Param({"tafla-gen"})
    public String codec;

    @Param({"100000"})
    public int rows;

    private DocumentInput input;
    private ReusedBytesWorkload<DocumentInput> workload;
    /** Reused by every call, which keeps what it writes alive */
    private byte[] bytes;

    @Setup
    public void setUp() {
        input = Workloads.documentInput(rows);
        var workload = Codecs.reusedBytesDocuments().get(codec);
        if (workload == null) {
            throw new IllegalArgumentException(codec + " doesn't write into reused bytes");
        }

        this.workload = workload;
        bytes = Codecs.serializeIntoChecked(workload, input);
    }

    @Benchmark
    public int serializeDocument() {
        return workload.serializeInto(input, bytes);
    }
}
