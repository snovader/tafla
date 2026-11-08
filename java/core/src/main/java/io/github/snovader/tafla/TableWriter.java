package io.github.snovader.tafla;

import java.util.*;

/**
 * Writes a table forward into bytes of exactly its length, which a {@link TableSizer} counted,
 * without recording its fields first. Its writer comes from {@link TableSizer#writer()}, and takes
 * the fields that the sizer added, in the same order, each with its index in its table.
 * {@link TableBuilder} records the fields, and then writes them with a writer of its own.
 *
 * <p>Each field's offset is written as the writer reaches the field. A nested table follows its
 * parent's {@link #setTable}, between {@link #startTable} and {@link #endTable}; the items of a
 * vector follow its {@link #setVector} the same way, each followed by {@link #setItemEnd}, which
 * writes where it ends. Arrays and nested tables are padded as the sizer and {@code TableBuilder}
 * count their padding, so that each field lands where they counted it.</p>
 *
 * <p>Arrays of the elements' own Java type are copied in bulk, and values convert to their
 * field's type as the package documentation's Conversions table describes, as for
 * {@code TableBuilder}.</p>
 *
 * <p>It's the target of the code that tafla-gen generates, whose calls follow from the schema, so
 * it doesn't check them again: fields are set in index order, each once, as the sizer added them,
 * and each vector item is followed by its {@link #setItemEnd}. A field index outside its table, or
 * an item outside its vector, writes an offset or end over another slot or field, so the table's
 * bytes are undefined, though never outside them: a slot past them throws an
 * {@link IndexOutOfBoundsException}. Hand-written tables use {@code TableBuilder}, which checks
 * every field.</p>
 *
 * <p>Fields that differ from those that the sizer added, such as values that changed in between,
 * never write outside the table's bytes either. A field that would end past them throws an
 * {@link IndexOutOfBoundsException}, after which the writer is unusable, and {@link #bytes()}
 * throws an {@link IllegalStateException} for fields that didn't fill them.</p>
 */
public final class TableWriter {
    private final byte[] bytes;
    /** The offset size, the width in bytes of offsets, lengths and header slots */
    private final int offsetSize;
    private final int headerSize;
    /** Where the next field goes */
    private int position;
    /** The start of the table being written, which its offsets count from */
    private int table = 0;
    /** The position of that table's first offset, after the root's header */
    private int slots;
    /** The long strings that the sizer encoded, in order */
    private final List<TableSizer.EncodedString> encodedStrings;
    /** The index in them of the next long string */
    private int nextEncodedString = 0;

    /**
     * Writes the header of a table of {@code fields} fields into {@code bytes}, which are new, and
     * as long as the table, as {@link TableSizer#writer()} makes them after checking the header,
     * with the long strings that the sizer encoded
     */
    TableWriter(
        byte[] bytes,
        int fields,
        TableLayout layout,
        long version,
        List<TableSizer.EncodedString> encodedStrings
    ) {
        this.bytes = bytes;
        this.encodedStrings = encodedStrings;
        this.offsetSize = layout.offsetSize();
        this.headerSize = layout.headerSize();
        this.slots = headerSize;
        this.position = headerSize + fields * offsetSize;

        var slot = 0;
        if (layout.sized()) {
            setLength(slot, bytes.length);
            slot += offsetSize;
        }

        if (layout.versioned()) {
            setLength(slot, version);
        }
    }

    /**
     * Returns the table's bytes, which the writer wrote into rather than copies.
     *
     * @throws IllegalStateException if the fields didn't fill the bytes, as when they differ from
     *     those that the sizer counted
     */
    public byte[] bytes() {
        if (position != bytes.length) {
            throw new IllegalStateException(
                String.format("The fields wrote %d of the table's %d bytes", position, bytes.length)
            );
        }

        return bytes;
    }

    /**
     * Starts the fields of a nested table or vector item of {@code fields} fields, after its
     * parent's {@link #setTable} or {@link #setVector}.
     *
     * @return the parent's start, which {@link #endTable} takes back
     * @throws IllegalArgumentException if {@code fields} is negative
     * @throws IndexOutOfBoundsException if the table's offsets end past the bytes
     */
    public int startTable(int fields) {
        var parent = table;
        var start = position;
        skip((long) TableSizer.checkCount(fields) * offsetSize);

        table = start;
        slots = start;

        return parent;
    }

    /** Ends the fields of the table that {@link #startTable} started */
    public void endTable(int parent) {
        table = parent;
        // Only the root starts at 0, and only its offsets follow a header
        slots = parent == 0 ? headerSize : parent;
    }

    public void setInt8(int index, byte value) {
        setOffset(index);
        bytes[position] = value;
        position += Byte.BYTES;
    }

    public void setUint8(int index, short value) {
        setInt8(index, (byte) value);
    }

    public void setInt16(int index, short value) {
        setOffset(index);
        Elements.INT16.set(bytes, position, value);
        position += Short.BYTES;
    }

    public void setUint16(int index, int value) {
        setInt16(index, (short) value);
    }

    public void setInt32(int index, int value) {
        setOffset(index);
        Elements.INT32.set(bytes, position, value);
        position += Integer.BYTES;
    }

    public void setUint32(int index, long value) {
        setInt32(index, (int) value);
    }

    public void setInt64(int index, long value) {
        setOffset(index);
        Elements.INT64.set(bytes, position, value);
        position += Long.BYTES;
    }

    /** Writes a {@code uint64} from a {@code long} with its bits */
    public void setUint64(int index, long value) {
        setInt64(index, value);
    }

    public void setFloat32(int index, float value) {
        setOffset(index);
        Elements.FLOAT32.set(bytes, position, value);
        position += Float.BYTES;
    }

    public void setFloat64(int index, double value) {
        setOffset(index);
        Elements.FLOAT64.set(bytes, position, value);
        position += Double.BYTES;
    }

    // The array setters write the count and then the elements, padded so that the elements start
    // at a multiple of their size from the start of the table

    public void setInt8Array(int index, byte[] values) {
        startArray(index, values.length, Byte.BYTES);
        Elements.setInt8(bytes, position, values);
        position += values.length * Byte.BYTES;
    }

    public void setUint8Array(int index, short[] values) {
        startArray(index, values.length, Byte.BYTES);
        Elements.setUint8(bytes, position, values);
        position += values.length * Byte.BYTES;
    }

    public void setInt16Array(int index, short[] values) {
        startArray(index, values.length, Short.BYTES);
        Elements.setInt16(bytes, position, values);
        position += values.length * Short.BYTES;
    }

    public void setUint16Array(int index, int[] values) {
        startArray(index, values.length, Short.BYTES);
        Elements.setUint16(bytes, position, values);
        position += values.length * Short.BYTES;
    }

    public void setInt32Array(int index, int[] values) {
        startArray(index, values.length, Integer.BYTES);
        Elements.setInt32(bytes, position, values);
        position += values.length * Integer.BYTES;
    }

    public void setUint32Array(int index, long[] values) {
        startArray(index, values.length, Integer.BYTES);
        Elements.setUint32(bytes, position, values);
        position += values.length * Integer.BYTES;
    }

    public void setInt64Array(int index, long[] values) {
        startArray(index, values.length, Long.BYTES);
        Elements.setInt64(bytes, position, values);
        position += values.length * Long.BYTES;
    }

    /** Writes {@code uint64}s from {@code long}s with their bits */
    public void setUint64Array(int index, long[] values) {
        setInt64Array(index, values);
    }

    public void setFloat32Array(int index, float[] values) {
        startArray(index, values.length, Float.BYTES);
        Elements.setFloat32(bytes, position, values);
        position += values.length * Float.BYTES;
    }

    public void setFloat64Array(int index, double[] values) {
        startArray(index, values.length, Double.BYTES);
        Elements.setFloat64(bytes, position, values);
        position += values.length * Double.BYTES;
    }

    /**
     * Writes {@code string} as UTF-8, with lone surrogates replaced by U+FFFD. A long string is
     * copied from the bytes that the sizer encoded, if it's the string that the sizer added.
     */
    public void setString(int index, String string) {
        if (string.length() >= Utf8.SHORT_LENGTH) {
            setEncodedString(index, encodingOf(string));
            return;
        }

        setOffset(index);

        var start = position + offsetSize;
        var length = Utf8.encodeInto(string, bytes, start);
        setLength(position, length);
        position = start + length;
    }

    /**
     * Writes a string from its UTF-8 bytes, as {@link #setString} writes a long string from the
     * bytes that the sizer encoded. {@link TableBuilder} writes the long strings that it encoded
     * to count them with it.
     */
    void setEncodedString(int index, byte[] utf8) {
        setOffset(index);

        System.arraycopy(utf8, 0, bytes, position + offsetSize, utf8.length);
        setLength(position, utf8.length);
        position += offsetSize + utf8.length;
    }

    /**
     * Writes a struct field from its bytes: its scalars, little-endian, one after another, without
     * padding
     */
    public void setStruct(int index, byte[] bytes) {
        setOffset(index);

        System.arraycopy(bytes, 0, this.bytes, position, bytes.length);
        position += bytes.length;
    }

    /**
     * Writes an array of structs of {@code size} bytes each, from their bytes one after another,
     * after their count, padded so that they start at a multiple of {@code alignment} from the
     * start of the table
     *
     * @param alignment what the structs' start is padded to a multiple of: 1, 2, 4 or 8
     * @throws IllegalArgumentException if {@code size} is less than 1, {@code bytes} aren't a whole
     *     number of structs, or the alignment isn't 1, 2, 4 or 8
     */
    public void setStructArray(int index, byte[] bytes, int size, int alignment) {
        startArray(index, structCount(bytes.length, size), alignment);

        System.arraycopy(bytes, 0, this.bytes, position, bytes.length);
        position += bytes.length;
    }

    /**
     * {@return the number of {@code size}-byte structs in {@code byteLength} bytes}, for this
     * writer's and {@link TableBuilder}'s arrays of structs
     *
     * @throws IllegalArgumentException if {@code size} is less than 1, or the bytes aren't a whole
     *     number of structs
     */
    static int structCount(int byteLength, int size) {
        ByteBuffer.assertStructSize(size);
        if (byteLength % size != 0) {
            throw new IllegalArgumentException(
                String.format("%d bytes aren't a whole number of %d-byte structs", byteLength, size)
            );
        }

        return byteLength / size;
    }

    /**
     * Writes an array of structs of {@code size} bytes each straight into the table, each with
     * {@code write}, after their count, padded so that they start at a multiple of
     * {@code alignment} from the start of the table. It writes the bytes that
     * {@link #setStructArray} writes from the structs' bytes one after another, without collecting
     * them first. The generated code passes each struct's own {@code write(byte[], int)}:
     *
     * {@snippet :
     * writer.setStructs(0, fields.points(), Point.SIZE, Point.ALIGNMENT, Point::write);
     * }
     *
     * @param alignment what the structs' start is padded to a multiple of: 1, 2, 4 or 8
     * @param write writes a struct's {@code size} bytes at an offset in the table's bytes
     * @throws IllegalArgumentException if the alignment isn't 1, 2, 4 or 8
     * @throws IndexOutOfBoundsException from {@code write}, if a struct ends past the bytes
     */
    public <T> void setStructs(int index, T[] structs, int size, int alignment, StructWrite<? super T> write) {
        startArray(index, structs.length, alignment);

        // The sizer counted the structs' room, and the array's bounds keep every write within it
        for (int i = 0; i < structs.length; i++) {
            write.write(structs[i], bytes, position + i * size);
        }
        position += structs.length * size;
    }

    /**
     * Writes a struct of {@link #setStructs}, such as a generated struct's
     * {@code write(byte[], int)}
     */
    @FunctionalInterface
    public interface StructWrite<T> {
        /** Writes {@code struct}'s scalars at {@code offset} in {@code bytes}, little-endian */
        void write(T struct, byte[] bytes, int offset);
    }

    /**
     * Writes the offset of a nested table, padded to start at a multiple of {@code alignment},
     * whose fields then follow between {@link #startTable} and {@link #endTable}
     *
     * @param alignment what the table's start is padded to a multiple of: 1, 2, 4 or 8
     * @throws IllegalArgumentException if the alignment isn't 1, 2, 4 or 8
     */
    public void setTable(int index, int alignment) {
        position += TableLayout.padding(position - table, alignment);
        setOffset(index);
    }

    /**
     * Writes the offset and count of a vector of {@code count} tables, whose items then follow,
     * each between {@link #startTable} and {@link #endTable} and then {@link #setItemEnd}.
     *
     * @return the vector's position, which {@link #setItemEnd} takes
     * @throws IllegalArgumentException if {@code count} is negative
     * @throws IndexOutOfBoundsException if the vector's count and ends end past the bytes
     */
    public int setVector(int index, int count) {
        TableSizer.checkCount(count);
        setOffset(index);

        var vector = position;
        setLength(vector, count);
        skip((count + 1L) * offsetSize);

        return vector;
    }

    /** Writes the end of item {@code item} of the vector, which was just written */
    public void setItemEnd(int vector, int item) {
        setLength(vector + (item + 1) * offsetSize, position - vector);
    }

    /**
     * Writes a finished table's bytes as they are, in place of {@link #startTable} and its fields:
     * after its parent's {@link #setTable}, or as a vector item before its {@link #setItemEnd}.
     * {@link TableBuilder} copies the tables that it can't write in place with it, such as tables
     * nested as bytes.
     *
     * @throws IndexOutOfBoundsException if they'd end past the bytes, having written none of them
     */
    void copyTable(byte[] table) {
        System.arraycopy(table, 0, bytes, position, table.length);
        position += table.length;
    }

    /**
     * {@return the UTF-8 bytes of the next long string}, which the sizer encoded if it added the
     * same string
     */
    private byte[] encodingOf(String string) {
        var index = nextEncodedString++;
        if (index < encodedStrings.size()) {
            var encoded = encodedStrings.get(index);
            if (encoded.string().equals(string)) {
                return encoded.utf8();
            }
        }

        return Utf8.encode(string);
    }

    /** Pads the array so that its elements, after the count, are aligned, and writes the count */
    private void startArray(int index, int count, int alignment) {
        position += TableLayout.padding(position + offsetSize - table, alignment);
        setOffset(index);
        setLength(position, count);
        position += offsetSize;
    }

    /**
     * Moves past {@code byteLength} bytes that later writes fill in, such as a table's offsets
     *
     * @throws IndexOutOfBoundsException if they end past the bytes
     */
    private void skip(long byteLength) {
        var end = position + byteLength;
        if (end > bytes.length) {
            throw new IndexOutOfBoundsException(
                String.format("Write exceeds the table: %d bytes at %d of %d", byteLength, position, bytes.length)
            );
        }

        position = (int) end;
    }

    private void setOffset(int index) {
        setLength(slots + index * offsetSize, position - table);
    }

    /** Writes an offset, length, count or header slot at {@code position} */
    private void setLength(int position, long value) {
        switch (offsetSize) {
            case Byte.BYTES -> bytes[position] = (byte) value;

            case Short.BYTES -> Elements.INT16.set(bytes, position, (short) value);

            default -> Elements.INT32.set(bytes, position, (int) value);
        }
    }
}
