package io.github.snovader.tafla.benchmarks;

import java.util.*;
import java.util.function.*;

/**
 * The lists of strings that the lists suite writes and reads: for each shape, 10, 100 and 10,000
 * distinct strings of the same length and mix of UTF-8 widths.
 *
 * <p>They're the strings of three of the eight kinds of the TypeScript suite in
 * {@code typescript/benchmarks/strings/lists/}, from the same seeded generator, seeds and
 * alphabet: 16 and 256 characters of ASCII, and 8 characters of 3 bytes. Each shape has a
 * generator of its own, whose seed is its kind's index among the TypeScript suite's kinds, after
 * 8 for each count before its own.</p>
 */
public final class ListShapes {
    /** The counts of strings of each kind */
    public static final List<Integer> COUNTS = List.of(10, 100, 10_000);

    private static final String ASCII = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    /** The first of the CJK ideographs, of three bytes in UTF-8 */
    private static final int THREE_BYTES = 0x4e00;

    /** The number of kinds of the TypeScript suite, of which the seeds count a kind's index */
    private static final int TYPESCRIPT_KINDS = 8;

    /**
     * A kind of strings, with the name that the benchmark's {@code kind} parameter takes, the
     * TypeScript suite's name of it, the strings' length in code points, the kind's index among the
     * TypeScript suite's kinds, and the picker of each character
     */
    private record Kind(String name, String description, int length, int index, ToIntFunction<Xorshift> pick) {}

    private static final List<Kind> KINDS = List.of(
        new Kind("16-ascii", "16 chars of ASCII", 16, 1, ListShapes::ascii),
        new Kind("256-ascii", "256 chars of ASCII", 256, 2, ListShapes::ascii),
        new Kind("8-3-byte", "8 chars of 3 bytes", 8, 6, ListShapes::threeBytes)
    );

    /** Every shape, in the TypeScript suite's order: by count, and then by kind */
    public static final List<ListShape> ALL = shapes();

    private ListShapes() {}

    /**
     * {@return the shape of {@code count} strings of the kind of that name}
     *
     * @throws IllegalArgumentException if no shape has that count and kind
     */
    public static ListShape of(int count, String kind) {
        return ALL.stream()
            .filter(shape -> shape.strings().size() == count && shape.kind().equals(kind))
            .findFirst()
            .orElseThrow(() -> new IllegalArgumentException("No shape is " + count + " strings of " + kind));
    }

    private static List<ListShape> shapes() {
        var shapes = new ArrayList<ListShape>();
        for (int countIndex = 0; countIndex < COUNTS.size(); countIndex++) {
            for (var kind : KINDS) {
                shapes.add(shape(COUNTS.get(countIndex), kind, 1 + kind.index() + TYPESCRIPT_KINDS * countIndex));
            }
        }

        return List.copyOf(shapes);
    }

    private static int ascii(Xorshift random) {
        return ASCII.charAt(random.nextIndex(ASCII.length()));
    }

    private static int threeBytes(Xorshift random) {
        return THREE_BYTES + random.nextIndex(0x5000);
    }

    /**
     * {@code count} distinct strings of the kind, made in order, each a code point at a time
     *
     * @throws IllegalStateException if the strings aren't distinct
     */
    private static ListShape shape(int count, Kind kind, int seed) {
        var name = String.format(Locale.ROOT, "%,d × %s", count, kind.description());
        var random = new Xorshift(seed);
        var codePoints = new int[kind.length()];

        var strings = new ArrayList<String>(count);
        for (int s = 0; s < count; s++) {
            for (int i = 0; i < codePoints.length; i++) {
                codePoints[i] = kind.pick().applyAsInt(random);
            }
            strings.add(new String(codePoints, 0, codePoints.length));
        }

        if (new HashSet<>(strings).size() != count) {
            throw new IllegalStateException("The strings of " + name + " aren't distinct");
        }

        return new ListShape(name, kind.name(), List.copyOf(strings));
    }
}
