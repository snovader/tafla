package io.github.snovader.tafla.benchmarks.codecs;

/**
 * A workload that can also write into bytes that the caller owns and reuses for each message, rather
 * than into new bytes
 */
public interface ReusedBytesWorkload<T> extends Workload<T> {
    /**
     * Writes the input into the start of {@code bytes}, which are at least as long as its encoding
     * and needn't be zero, as {@link #serialize} writes it into new bytes
     *
     * @return the number of bytes that hold it, from the start of {@code bytes}
     */
    int serializeInto(T input, byte[] bytes);
}
