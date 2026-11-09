package io.github.snovader.tafla.test;

import java.util.Arrays;
import java.util.function.ToIntFunction;

import static org.junit.jupiter.api.Assertions.*;

/**
 * Assertions about writing a table into bytes that the caller gives, rather than into new bytes,
 * which may hold anything
 */
final class GivenBytesAssertions {
    private GivenBytesAssertions() {}

    /**
     * Writes a table into bytes {@code extra} bytes longer than it, which hold {@code fill}, and
     * asserts that the write returns the table's length, that their start holds {@code expected},
     * the table's bytes as written into new bytes, and that the bytes after it still hold
     * {@code fill}
     */
    static void assertWrittenInto(byte[] expected, ToIntFunction<byte[]> writeInto, int extra, byte fill) {
        var bytes = filled(expected.length + extra, fill);

        assertEquals(expected.length, writeInto.applyAsInt(bytes), "The table's length");
        assertArrayEquals(expected, Arrays.copyOf(bytes, expected.length), "The table");
        assertArrayEquals(filled(extra, fill), Arrays.copyOfRange(bytes, expected.length, bytes.length), "After it");
    }

    /**
     * Asserts that writing a table of {@code length} bytes into bytes {@code shorter} bytes shorter
     * throws an {@code IllegalArgumentException} naming both lengths, having written none of them
     */
    static void assertTooShort(int length, ToIntFunction<byte[]> writeInto, int shorter, byte fill) {
        var bytes = filled(length - shorter, fill);

        var exception = assertThrows(IllegalArgumentException.class, () -> writeInto.applyAsInt(bytes));
        assertEquals(
            String.format("The table's %d bytes don't fit in %d bytes", length, bytes.length),
            exception.getMessage()
        );
        assertArrayEquals(filled(bytes.length, fill), bytes, "The bytes given");
    }

    private static byte[] filled(int length, byte fill) {
        var bytes = new byte[length];
        Arrays.fill(bytes, fill);

        return bytes;
    }
}
