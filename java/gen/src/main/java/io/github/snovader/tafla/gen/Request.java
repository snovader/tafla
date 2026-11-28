package io.github.snovader.tafla.gen;

import java.io.ByteArrayOutputStream;
import java.nio.charset.StandardCharsets;

/**
 * A request to the generator's module, which the "WebAssembly" section of rust/gen/README.md
 * describes: little-endian {@code u32} counts, and strings, each its byte length as a count and
 * then its UTF-8 bytes.
 */
final class Request {
    private final ByteArrayOutputStream bytes = new ByteArrayOutputStream();

    Request count(int count) {
        bytes.write(count);
        bytes.write(count >>> 8);
        bytes.write(count >>> 16);
        bytes.write(count >>> 24);
        return this;
    }

    Request string(String string) {
        return bytes(string.getBytes(StandardCharsets.UTF_8));
    }

    /** A string that needn't be UTF-8, such as a schema as it was read */
    Request bytes(byte[] string) {
        count(string.length);
        bytes.writeBytes(string);
        return this;
    }

    byte[] toByteArray() {
        return bytes.toByteArray();
    }
}
