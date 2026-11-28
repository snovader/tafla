package io.github.snovader.tafla.benchmarks;

import java.util.*;
import java.util.function.*;

/**
 * The strings that the strings suite writes and reads: for each shape, 16 distinct strings of the
 * same length and mix of UTF-8 widths, from ASCII to characters of 4 bytes, and 16 empty strings,
 * which time what a call takes without any.
 *
 * <p>The shapes that the TypeScript suite of {@code typescript/benchmarks/strings/} has are its
 * strings, from the same seeded generator, seeds and alphabets. In place of its shapes either side
 * of V8's lengths, the others sit either side of the lengths at which the Java libraries change how
 * they write a string: tafla's {@code Utf8.SHORT_LENGTH} of 64 {@code char}s, protobuf's length
 * prefix, whose size the number of {@code char}s doesn't tell from 43 to 127, so that protobuf
 * counts the string's bytes a second time, and msgpack-core's {@code CharsetEncoder}, which it
 * encodes with from 512.</p>
 *
 * <p>A shape's name gives its length in code points, and its mix: {@code ascii} for ASCII alone,
 * {@code 3-byte} for every character of 3 bytes, and {@code one-}, {@code two-} or
 * {@code last-} for some of 2, 3 or 4 bytes among ASCII.</p>
 */
public final class StringShapes {
    /** The strings of each table */
    public static final int COUNT = 16;

    private static final String ASCII = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    /** Latin-1 letters, of one byte in a compact Java string, as in V8's, and two in UTF-8 */
    private static final String TWO_BYTES = "àáâãäåæçèéêëìíîïñòóôõöøùúûüý";
    /** The first of the CJK ideographs, of three bytes in UTF-8 */
    private static final int THREE_BYTES = 0x4e00;
    /** The first of the emoji, of a surrogate pair and four bytes in UTF-8 */
    private static final int FOUR_BYTES = 0x1f600;

    /** Every shape, in the order of the suite's rows */
    public static final List<StringShape> ALL = List.of(
        new StringShape("empty", Collections.nCopies(COUNT, "")),
        shape("8-ascii", 8, 1),
        shape("8-one-2-byte", 8, 2, index -> index == 3, StringShapes::twoBytes),
        shape("8-3-byte", 8, 3, index -> true, StringShapes::threeBytes),
        shape("16-two-2-byte", 16, 4, index -> index == 2 || index == 11, StringShapes::twoBytes),
        shape("15-one-4-byte", 15, 5, index -> index == 7, StringShapes::fourBytes),
        shape("16-ascii", 16, 17),
        shape("32-ascii", 32, 19),
        shape("32-3-byte", 32, 6, index -> true, StringShapes::threeBytes),
        shape("42-ascii", 42, 20),
        shape("43-ascii", 43, 21),
        shape("63-ascii", 63, 22),
        shape("64-ascii", 64, 7),
        shape("63-last-2-byte", 63, 23, index -> index == 62, StringShapes::twoBytes),
        shape("64-last-2-byte", 64, 24, index -> index == 63, StringShapes::twoBytes),
        shape("63-last-3-byte", 63, 25, index -> index == 62, StringShapes::threeBytes),
        shape("64-last-3-byte", 64, 26, index -> index == 63, StringShapes::threeBytes),
        shape("127-ascii", 127, 27),
        shape("128-ascii", 128, 28),
        shape("256-ascii", 256, 8),
        shape("256-3-byte", 256, 9, index -> true, StringShapes::threeBytes),
        shape("511-ascii", 511, 29),
        shape("512-ascii", 512, 30),
        shape("1024-ascii", 1024, 10)
    );

    private StringShapes() {}

    /**
     * {@return the shape of that name}
     *
     * @throws IllegalArgumentException if no shape has that name
     */
    public static StringShape named(String name) {
        return ALL.stream()
            .filter(shape -> shape.name().equals(name))
            .findFirst()
            .orElseThrow(() -> new IllegalArgumentException("No shape is named " + name));
    }

    private static int twoBytes(Xorshift random) {
        return TWO_BYTES.charAt(random.nextIndex(TWO_BYTES.length()));
    }

    private static int threeBytes(Xorshift random) {
        return THREE_BYTES + random.nextIndex(0x5000);
    }

    private static int fourBytes(Xorshift random) {
        return FOUR_BYTES + random.nextIndex(0x50);
    }

    private static StringShape shape(String name, int length, int seed) {
        return shape(name, length, seed, index -> false, StringShapes::twoBytes);
    }

    /**
     * 16 distinct strings of {@code length} code points, of which those at the indexes that
     * {@code special} selects come from {@code pick}, and the others from ASCII
     *
     * @throws IllegalStateException if the strings aren't distinct
     */
    private static StringShape shape(
        String name,
        int length,
        int seed,
        IntPredicate special,
        ToIntFunction<Xorshift> pick
    ) {
        var random = new Xorshift(seed);
        var codePoints = new int[length];

        var strings = new ArrayList<String>(COUNT);
        for (int s = 0; s < COUNT; s++) {
            for (int i = 0; i < length; i++) {
                codePoints[i] = special.test(i) ? pick.applyAsInt(random) : ASCII.charAt(random.nextIndex(ASCII.length()));
            }
            strings.add(new String(codePoints, 0, length));
        }

        if (new HashSet<>(strings).size() != COUNT) {
            throw new IllegalStateException("The strings of " + name + " aren't distinct");
        }

        return new StringShape(name, List.copyOf(strings));
    }
}
