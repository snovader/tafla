package io.github.snovader.tafla.benchmarks.codecs;

import io.github.snovader.tafla.benchmarks.*;

import java.util.List;

/**
 * One library's encoding of the workloads
 *
 * @param strings the strings suite's table of 16 strings of a {@link StringShape}, or what the
 *     library writes for an array of strings
 */
public record Codec(
    String name,
    Workload<RecordInput> record,
    DocumentWorkload document,
    Workload<ListInput> list,
    Workload<ListInput> structs,
    Workload<SampleInput> sample,
    Workload<List<String>> strings
) {
    /** {@return the library's name}, which names its codec in test reports */
    @Override
    public String toString() {
        return name;
    }
}
