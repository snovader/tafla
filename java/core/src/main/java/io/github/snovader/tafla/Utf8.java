package io.github.snovader.tafla;

import java.nio.charset.*;

/** Encodes and decodes strings as the TypeScript and Python implementations do */
final class Utf8 {
    private static final char REPLACEMENT_CHARACTER = '\uFFFD';

    private Utf8() {}

    /**
     * Strings of fewer {@code char}s than this are sized and written a {@code char} at a time, in
     * place, without allocating. Longer ones are encoded once, with {@link String#getBytes}, which
     * the JDK vectorizes, and copied. In the benchmarks' strings suite on JDK 25, a table of 16
     * strings of 32 {@code char}s took 0.41 µs to write a {@code char} at a time and 0.23 µs
     * encoded once if they were ASCII, and 0.94 and 0.87 µs if each {@code char} took 3 bytes.
     * Shorter strings with characters of 2 to 4 bytes are faster a {@code char} at a time: 8
     * {@code char}s with one of 2 bytes took 0.24 and 0.38 µs, and 16 with a surrogate pair 0.48
     * and 0.53 µs, though 16 of ASCII took 0.27 and 0.19 µs.
     */
    static final int SHORT_LENGTH = 32;

    /**
     * Encodes {@code string}, with lone surrogates replaced by U+FFFD, as TypeScript's
     * {@code TextEncoder} replaces them. {@link String#getBytes} would write each as {@code '?'}.
     */
    static byte[] encode(String string) {
        // Only a lone surrogate needs replacing; getBytes encodes a pair as its code point. In a
        // string of Latin-1, whose chars can't be surrogates, the JIT drops this loop.
        //
        // The loop calls nothing but charAt(). Generated code inlines this method once for each
        // string field, and C2 stops inlining once a method's graph passes 18,000 nodes, so in a
        // large method one copy may keep its calls, made for every char. On JDK 27, 16 fields of
        // 1,024 chars took 5.3 us rather than 1.5 in most JVMs when the loop also called length()
        // and isSurrogate(); with charAt() alone, such a copy is rarer and costs 3.6 us.
        var length = string.length();
        for (int i = 0; i < length; i++) {
            var unit = string.charAt(i);
            if (unit >= Character.MIN_SURROGATE && unit <= Character.MAX_SURROGATE) {
                return encodeFromSurrogate(string, i);
            }
        }

        return string.getBytes(StandardCharsets.UTF_8);
    }

    /** {@link #encode}s a string whose first surrogate is at {@code start} */
    private static byte[] encodeFromSurrogate(String string, int start) {
        for (int i = start; i < string.length(); i++) {
            var unit = string.charAt(i);
            if (!Character.isSurrogate(unit)) {
                continue;
            }

            if (Character.isHighSurrogate(unit) && isLowSurrogateAt(string, i + 1)) {
                i++;
            } else {
                return encodeReplacingLoneSurrogates(string);
            }
        }

        return string.getBytes(StandardCharsets.UTF_8);
    }

    /**
     * {@return the byte length of {@code string} in UTF-8, as {@link #encode} writes it}: a lone
     * surrogate takes the 3 bytes of U+FFFD, and a surrogate pair 4. It's a {@code long}, since a
     * string of {@code char}s of 3 bytes each may be longer than an {@code int} counts.
     */
    static long length(String string) {
        long length = string.length();

        for (int i = 0; i < string.length(); i++) {
            var unit = string.charAt(i);
            if (unit < 0x80) {
                continue;
            }

            if (unit < 0x800) {
                length += 1;
            } else if (Character.isHighSurrogate(unit) && isLowSurrogateAt(string, i + 1)) {
                // The pair's two units take 4 bytes
                length += 2;
                i++;
            } else {
                length += 2;
            }
        }

        return length;
    }

    /**
     * Writes {@code string} as UTF-8 into {@code bytes} from {@code offset}, with lone surrogates
     * replaced by U+FFFD, as {@link #encode} writes it, without allocating.
     *
     * @return the number of bytes written
     * @throws IndexOutOfBoundsException if the bytes end before the string does, which leaves the
     *     bytes before their end written
     */
    static int encodeInto(String string, byte[] bytes, int offset) {
        var position = offset;

        for (int i = 0; i < string.length(); i++) {
            var unit = string.charAt(i);

            if (unit < 0x80) {
                bytes[position++] = (byte) unit;
            } else if (unit < 0x800) {
                bytes[position++] = (byte) (0xc0 | (unit >> 6));
                bytes[position++] = continuation(unit);
            } else if (Character.isHighSurrogate(unit) && isLowSurrogateAt(string, i + 1)) {
                var codePoint = Character.toCodePoint(unit, string.charAt(++i));
                bytes[position++] = (byte) (0xf0 | (codePoint >> 18));
                bytes[position++] = continuation(codePoint >> 12);
                bytes[position++] = continuation(codePoint >> 6);
                bytes[position++] = continuation(codePoint);
            } else {
                var character = Character.isSurrogate(unit) ? REPLACEMENT_CHARACTER : unit;
                bytes[position++] = (byte) (0xe0 | (character >> 12));
                bytes[position++] = continuation(character >> 6);
                bytes[position++] = continuation(character);
            }
        }

        return position - offset;
    }

    private static boolean isLowSurrogateAt(String string, int index) {
        return index < string.length() && Character.isLowSurrogate(string.charAt(index));
    }

    /** {@return the continuation byte of a sequence that holds the low 6 of {@code bits}} */
    private static byte continuation(int bits) {
        return (byte) (0x80 | (bits & 0x3f));
    }

    /**
     * Decodes {@code length} bytes from {@code offset}, keeping a leading U+FEFF, which is part of
     * the string rather than a byte order mark.
     *
     * @throws CharacterCodingException if the bytes aren't valid UTF-8
     */
    static String decode(byte[] bytes, int offset, int length) throws CharacterCodingException {
        // The String constructor replaces each invalid sequence with U+FFFD, and decodes faster than
        // a CharsetDecoder, so a string without U+FFFD was valid. A string with it is decoded again
        // strictly, to tell a U+FFFD in the bytes from a replaced sequence.
        var string = new String(bytes, offset, length, StandardCharsets.UTF_8);
        if (string.indexOf(REPLACEMENT_CHARACTER) < 0) {
            return string;
        }

        var decoder = StandardCharsets.UTF_8.newDecoder();
        return decoder.decode(java.nio.ByteBuffer.wrap(bytes, offset, length)).toString();
    }

    /**
     * Encodes a string with a lone surrogate a {@code char} at a time, into bytes of its length
     *
     * @throws OutOfMemoryError if it's longer than a Java array can be, as getBytes throws it
     */
    private static byte[] encodeReplacingLoneSurrogates(String string) {
        var length = length(string);
        if (length > Integer.MAX_VALUE) {
            throw new OutOfMemoryError("A string of " + length + " bytes is longer than an array can be");
        }

        var bytes = new byte[(int) length];
        encodeInto(string, bytes, 0);

        return bytes;
    }
}
