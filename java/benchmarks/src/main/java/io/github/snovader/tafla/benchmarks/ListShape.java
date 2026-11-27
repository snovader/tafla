package io.github.snovader.tafla.benchmarks;

import java.util.*;

/**
 * A list of distinct strings of the same length in code points and mix of UTF-8 widths, which the
 * lists suite writes and reads as a table of one vector
 *
 * @param name the shape's name, such as {@code 10,000 × 16 chars of ASCII}, as the TypeScript
 *     suite names it
 * @param kind the name of the strings' kind, such as {@code 16-ascii}, which the benchmark's
 *     {@code kind} parameter takes, with the strings' count as its {@code count}
 */
public record ListShape(String name, String kind, List<String> strings) {
    /** {@return the shape's name}, which names it in test reports rather than its strings */
    @Override
    public String toString() {
        return name;
    }
}
