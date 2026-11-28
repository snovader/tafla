package io.github.snovader.tafla.benchmarks;

import java.util.*;

/**
 * 16 distinct strings of the same length in code points and mix of UTF-8 widths, which a table of
 * the strings suite holds, and nothing else
 *
 * @param name the shape's name, such as {@code 64-ascii}, which the benchmark's parameter takes
 */
public record StringShape(String name, List<String> strings) {
    /** {@return the shape's name}, which names it in test reports rather than its strings */
    @Override
    public String toString() {
        return name;
    }
}
