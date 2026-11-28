package io.github.snovader.tafla.benchmarks.test;

import io.github.snovader.tafla.benchmarks.*;
import net.jqwik.api.*;

import java.nio.charset.StandardCharsets;
import java.util.*;

import static org.junit.jupiter.api.Assertions.*;

/**
 * Checks that the strings suite's shapes are the strings of TypeScript's generator, in
 * {@code typescript/benchmarks/strings/shapes.ts}, which printed these: of its own shapes for the
 * shapes that the TypeScript suite has, and of its generator with the same parameters for the
 * others. So the languages' suites time the same strings.
 */
public class StringShapesTest {
    /**
     * A shape's name, its strings' length in code points and in UTF-8, and their
     * {@link List#hashCode()}, which the TypeScript script computed from their UTF-16 code units
     */
    private record Summary(String name, int codePoints, int utf8Bytes, int hash) {}

    @Example
    void should_generate_the_typescript_strings() {
        var expected = List.of(
            new Summary("empty", 0, 0, 1_353_309_697),
            new Summary("8-ascii", 8, 128, -2_105_790_493),
            new Summary("8-one-2-byte", 8, 144, 962_002_782),
            new Summary("8-3-byte", 8, 384, -1_854_973_701),
            new Summary("16-two-2-byte", 16, 288, -767_231_347),
            new Summary("15-one-4-byte", 15, 288, 1_858_880_051),
            new Summary("16-ascii", 16, 256, 1_650_563_284),
            new Summary("32-ascii", 32, 512, 1_218_869_111),
            new Summary("32-3-byte", 32, 1536, 1_624_687_424),
            new Summary("42-ascii", 42, 672, -1_570_417_687),
            new Summary("43-ascii", 43, 688, -919_957_805),
            new Summary("63-ascii", 63, 1008, -1_205_092_645),
            new Summary("64-ascii", 64, 1024, -418_033_798),
            new Summary("63-last-2-byte", 63, 1024, 1_906_407_459),
            new Summary("64-last-2-byte", 64, 1040, -1_517_547_817),
            new Summary("63-last-3-byte", 63, 1040, -452_395_319),
            new Summary("64-last-3-byte", 64, 1056, 1_210_571_433),
            new Summary("127-ascii", 127, 2032, 1_041_688_063),
            new Summary("128-ascii", 128, 2048, 1_277_646_856),
            new Summary("256-ascii", 256, 4096, -1_911_627_822),
            new Summary("256-3-byte", 256, 12_288, 256_577_790),
            new Summary("511-ascii", 511, 8176, 129_696_868),
            new Summary("512-ascii", 512, 8192, -689_583_364),
            new Summary("1024-ascii", 1024, 16_384, 1_352_982_789)
        );

        assertEquals(expected, StringShapes.ALL.stream().map(StringShapesTest::summarize).toList());
    }

    @Example
    void should_generate_the_typescript_first_strings() {
        assertEquals("2I8eEZQc", StringShapes.named("8-ascii").strings().getFirst());
        assertEquals("2FsêeXxp", StringShapes.named("8-one-2-byte").strings().getFirst());
        assertEquals("2LæWnMrC7PiãrPpC", StringShapes.named("16-two-2-byte").strings().getFirst());
        assertEquals("2MPZTRA😢FaUr9Oq", StringShapes.named("15-one-4-byte").strings().getFirst());
        assertEquals("2Yla1TvXXAM51Mvn", StringShapes.named("16-ascii").strings().getFirst());
    }

    @Example
    void should_reject_an_unknown_shape() {
        assertThrows(IllegalArgumentException.class, () -> StringShapes.named("65-ascii"));
    }

    /** @throws AssertionError if the shape's strings differ in length */
    private static Summary summarize(StringShape shape) {
        var strings = shape.strings();
        var first = strings.getFirst();
        var codePoints = first.codePointCount(0, first.length());

        int utf8Bytes = 0;
        for (var string : strings) {
            assertEquals(codePoints, string.codePointCount(0, string.length()), shape.name());
            utf8Bytes += string.getBytes(StandardCharsets.UTF_8).length;
        }

        return new Summary(shape.name(), codePoints, utf8Bytes, strings.hashCode());
    }
}
