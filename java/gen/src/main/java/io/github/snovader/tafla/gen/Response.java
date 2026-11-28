package io.github.snovader.tafla.gen;

import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.List;

/**
 * A response of the generator's module, copied from its memory and read in order from after
 * its length: its status, and then the counts and strings that the status says.
 */
final class Response {
    private final ByteBuffer bytes;

    /**
     * @param response the whole response, which starts with its own length
     */
    Response(byte[] response) {
        bytes = ByteBuffer.wrap(response).order(ByteOrder.LITTLE_ENDIAN);
        bytes.position(Integer.BYTES);
    }

    int count() {
        return bytes.getInt();
    }

    /** A string's bytes, such as a file's contents, which are written as they are */
    byte[] bytes() {
        byte[] string = new byte[count()];
        bytes.get(string);
        return string;
    }

    String string() {
        return new String(bytes(), StandardCharsets.UTF_8);
    }

    /** A count and that many strings */
    List<String> strings() {
        int count = count();
        List<String> strings = new ArrayList<>(count);
        for (int index = 0; index < count; index++) {
            strings.add(string());
        }
        return strings;
    }
}
