package io.github.snovader.tafla.benchmarks.test;

import io.github.snovader.tafla.benchmarks.*;
import net.jqwik.api.*;

import java.nio.charset.StandardCharsets;
import java.util.*;

import static org.junit.jupiter.api.Assertions.*;

/**
 * Checks that the lists suite's shapes are the strings of TypeScript's generator, in
 * {@code typescript/benchmarks/strings/lists/shapes.ts}, which printed these of its own shapes of
 * the same names. So the languages' suites time the same strings.
 */
public class ListShapesTest {
    /**
     * A shape's name, its strings' count, their length in code points and in UTF-8, and their
     * {@link List#hashCode()}, which the TypeScript script computed from their UTF-16 code units
     */
    private record Summary(String name, int count, int codePoints, int utf8Bytes, int hash) {}

    @Example
    void should_generate_the_typescript_strings() {
        var expected = List.of(
            new Summary("10 × 16 chars of ASCII", 10, 16, 160, 1_698_323_918),
            new Summary("10 × 256 chars of ASCII", 10, 256, 2560, -2_017_488_324),
            new Summary("10 × 8 chars of 3 bytes", 10, 8, 240, 756_095_915),
            new Summary("100 × 16 chars of ASCII", 100, 16, 1600, 1_318_022_127),
            new Summary("100 × 256 chars of ASCII", 100, 256, 25_600, 1_768_416_528),
            new Summary("100 × 8 chars of 3 bytes", 100, 8, 2400, -1_593_309_681),
            new Summary("10,000 × 16 chars of ASCII", 10_000, 16, 160_000, 519_164_135),
            new Summary("10,000 × 256 chars of ASCII", 10_000, 256, 2_560_000, -1_933_331_963),
            new Summary("10,000 × 8 chars of 3 bytes", 10_000, 8, 240_000, 508_331_050)
        );

        assertEquals(expected, ListShapes.ALL.stream().map(ListShapesTest::summarize).toList());
    }

    @Example
    void should_generate_the_typescript_first_and_last_strings() {
        var ascii = ListShapes.of(10, "16-ascii").strings();
        var threeBytes = ListShapes.of(10_000, "8-3-byte").strings();

        assertEquals("2FsyeXxp1ewxJijA", ascii.getFirst());
        assertEquals("CcfRsv23GwWuZIqm", ascii.getLast());
        assertEquals("鑶鄷攴臚鷑靆畖鱯", threeBytes.getFirst());
        assertEquals("淟坓惗聳侞忝鞂姩", threeBytes.getLast());
        assertTrue(ListShapes.of(100, "256-ascii").strings().getFirst().startsWith("2OC9KSUHzW0k790o"));
    }

    @Example
    void should_reject_an_unknown_shape() {
        assertThrows(IllegalArgumentException.class, () -> ListShapes.of(1000, "16-ascii"));
        assertThrows(IllegalArgumentException.class, () -> ListShapes.of(10, "16-3-byte"));
    }

    /** @throws AssertionError if the shape's strings differ in length */
    private static Summary summarize(ListShape shape) {
        var strings = shape.strings();
        var first = strings.getFirst();
        var codePoints = first.codePointCount(0, first.length());

        int utf8Bytes = 0;
        for (var string : strings) {
            assertEquals(codePoints, string.codePointCount(0, string.length()), shape.name());
            utf8Bytes += string.getBytes(StandardCharsets.UTF_8).length;
        }

        return new Summary(shape.name(), strings.size(), codePoints, utf8Bytes, strings.hashCode());
    }
}
