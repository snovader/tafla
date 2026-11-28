package io.github.snovader.tafla.benchmarks.codecs;

import java.util.List;

/** One encoding of a list of strings, which the lists suite writes, reads, and reads the last of */
public interface StringListWorkload extends Workload<List<String>> {
    /** {@return the last string}, read without reading the others */
    String readLast(byte[] bytes);
}
