package io.github.snovader.tafla;

import java.nio.*;
import java.nio.charset.*;
import java.util.*;

/**
 * Reads and writes the supplied bytes in the table layout it was created with, storing multi-byte
 * values little-endian. Array reads copy the elements into a new Java array, and array view reads
 * return a read-only view of them in place, as struct reads return the bytes of structs.
 *
 * <p>Every read checks that it stays within the bytes, and throws a {@link DecodeException}
 * otherwise. Writes past the end throw an {@link IndexOutOfBoundsException}, and lengths, offsets
 * and versions too large for the layout's offset type an {@link IllegalArgumentException}; either
 * writes nothing.</p>
 *
 * <p>Unsigned values are read into the next wider type, such as a {@code uint32} into a
 * {@code long}, except for {@code uint64}, which Java has no wider type for: it's read into a
 * {@code long} with the same bits, for {@link Long#toUnsignedString} and the like. Unsigned
 * setters write the low bits of their argument, as a cast would.</p>
 */
public final class ByteBuffer {
    private final byte[] bytes;
    private final java.nio.ByteBuffer view;

    private final boolean sized;
    private final boolean versioned;
    /** The type of offsets, lengths and header slots */
    private final OffsetType offsetType;
    /** The offset size, the width in bytes of offsets, lengths and header slots */
    private final int offsetSize;
    /** The largest offset, length or version that the offset type holds */
    private final long maxOffset;
    /** The byte length of the header slots before the root's field offsets */
    private final int headerSize;

    private int position = 0;

    public ByteBuffer(byte[] bytes) {
        this(bytes, TableLayout.DEFAULT);
    }

    public ByteBuffer(byte[] bytes, TableLayout layout) {
        this.bytes = bytes;
        this.view = java.nio.ByteBuffer.wrap(bytes).order(ByteOrder.LITTLE_ENDIAN);

        this.sized = layout.sized();
        this.versioned = layout.versioned();
        this.offsetType = layout.offsetType();
        this.offsetSize = layout.offsetSize();
        this.maxOffset = offsetType.maxValue();
        this.headerSize = layout.headerSize();
    }

    /** {@return a buffer of {@code size} zeros, to write into} */
    public static ByteBuffer allocate(int size) {
        return new ByteBuffer(new byte[size]);
    }

    public byte[] bytes() {
        return bytes;
    }

    /**
     * {@return the table's byte length: the header's when the layout is sized, and otherwise that
     * of the bytes}
     */
    public long getSize() {
        return sized ? getUnsigned(0) : bytes.length;
    }

    /**
     * Returns the version that the table's header carries after its length. It's as wide as the
     * header's other slots, and read unsigned.
     *
     * @throws IllegalStateException if the buffer's layout isn't versioned
     */
    public long getVersion() {
        if (!versioned) {
            throw new IllegalStateException("The table has no version");
        }

        return getUnsigned(sized ? offsetSize : 0);
    }

    /**
     * Returns the position of field {@code index} of the root table.
     *
     * @throws DecodeException if the offset isn't within the bytes, or points past the largest
     *     {@code int}. An offset to elsewhere outside them throws from the read at its position.
     */
    public int rootOffset(int index) {
        return toPosition(getUnsigned(headerSize + (long) index * offsetSize));
    }

    /**
     * Returns the position of field {@code index} of a nested table, which has no header and stores
     * its field offsets from its own start.
     *
     * @param tablePosition the nested table's position, which is the offset of the field that
     *     holds it
     * @throws DecodeException if the offset isn't within the bytes, or points past the largest
     *     {@code int}. An offset to elsewhere outside them throws from the read at its position.
     */
    public int fieldOffset(int tablePosition, int index) {
        var offset = getUnsigned(tablePosition + (long) index * offsetSize);
        return toPosition(tablePosition + offset);
    }

    public void setOffset(int offset) {
        setUnsigned(offset);
    }

    // The scalar getters let the JDK's buffer check the range, which is free, and rethrow its error,
    // as the TypeScript getters rethrow DataView's

    public byte getInt8(int position) {
        try {
            return view.get(position);
        } catch (IndexOutOfBoundsException ex) {
            throw outOfRange(position, Byte.BYTES, ex);
        }
    }

    public short getUint8(int position) {
        return (short) Byte.toUnsignedInt(getInt8(position));
    }

    public short getInt16(int position) {
        try {
            return view.getShort(position);
        } catch (IndexOutOfBoundsException ex) {
            throw outOfRange(position, Short.BYTES, ex);
        }
    }

    public int getUint16(int position) {
        return Short.toUnsignedInt(getInt16(position));
    }

    public int getInt32(int position) {
        try {
            return view.getInt(position);
        } catch (IndexOutOfBoundsException ex) {
            throw outOfRange(position, Integer.BYTES, ex);
        }
    }

    public long getUint32(int position) {
        return Integer.toUnsignedLong(getInt32(position));
    }

    public long getInt64(int position) {
        try {
            return view.getLong(position);
        } catch (IndexOutOfBoundsException ex) {
            throw outOfRange(position, Long.BYTES, ex);
        }
    }

    /** {@return the {@code uint64} at {@code position}, as a {@code long} with the same bits} */
    public long getUint64(int position) {
        return getInt64(position);
    }

    public float getFloat32(int position) {
        try {
            return view.getFloat(position);
        } catch (IndexOutOfBoundsException ex) {
            throw outOfRange(position, Float.BYTES, ex);
        }
    }

    public double getFloat64(int position) {
        try {
            return view.getDouble(position);
        } catch (IndexOutOfBoundsException ex) {
            throw outOfRange(position, Double.BYTES, ex);
        }
    }

    // The array getters check the length prefix against the bytes before allocating for it. Arrays
    // of the elements' own Java type are copied in bulk, and unsigned ones are widened one by one.

    public byte[] getInt8Array(int position) {
        var values = new byte[arrayLength(position, Byte.BYTES)];
        System.arraycopy(bytes, position + offsetSize, values, 0, values.length);

        return values;
    }

    public short[] getUint8Array(int position) {
        var values = new short[arrayLength(position, Byte.BYTES)];
        for (int i = 0, j = position + offsetSize; i < values.length; i++, j++) {
            values[i] = (short) Byte.toUnsignedInt(bytes[j]);
        }

        return values;
    }

    public short[] getInt16Array(int position) {
        var values = new short[arrayLength(position, Short.BYTES)];
        elementsAt(position, values.length * Short.BYTES).asShortBuffer().get(values);

        return values;
    }

    public int[] getUint16Array(int position) {
        var values = new int[arrayLength(position, Short.BYTES)];
        for (int i = 0, j = position + offsetSize; i < values.length; i++, j += Short.BYTES) {
            values[i] = Short.toUnsignedInt(view.getShort(j));
        }

        return values;
    }

    public int[] getInt32Array(int position) {
        var values = new int[arrayLength(position, Integer.BYTES)];
        elementsAt(position, values.length * Integer.BYTES).asIntBuffer().get(values);

        return values;
    }

    public long[] getUint32Array(int position) {
        var values = new long[arrayLength(position, Integer.BYTES)];
        for (int i = 0, j = position + offsetSize; i < values.length; i++, j += Integer.BYTES) {
            values[i] = Integer.toUnsignedLong(view.getInt(j));
        }

        return values;
    }

    public long[] getInt64Array(int position) {
        var values = new long[arrayLength(position, Long.BYTES)];
        elementsAt(position, values.length * Long.BYTES).asLongBuffer().get(values);

        return values;
    }

    /** {@return the {@code uint64} array at {@code position}, as {@code long}s with the same bits} */
    public long[] getUint64Array(int position) {
        return getInt64Array(position);
    }

    public float[] getFloat32Array(int position) {
        var values = new float[arrayLength(position, Float.BYTES)];
        elementsAt(position, values.length * Float.BYTES).asFloatBuffer().get(values);

        return values;
    }

    public double[] getFloat64Array(int position) {
        var values = new double[arrayLength(position, Double.BYTES)];
        elementsAt(position, values.length * Double.BYTES).asDoubleBuffer().get(values);

        return values;
    }

    // The array view getters return the elements in place, without copying them: a read-only,
    // little-endian view of the bytes, at any alignment. A view keeps the bytes alive and shows
    // writes made to them later. Unsigned elements are viewed in the signed type of their size,
    // with the same bits, as getUint64Array() reads them, for Integer.toUnsignedLong and the like.
    // Viewing three columns of 100,000 elements took 0.04 µs and 448 bytes, against 47 µs and
    // 1.6 MB to copy them into arrays.

    public java.nio.ByteBuffer getInt8ArrayView(int position) {
        return arrayView(position, Byte.BYTES);
    }

    /** {@return a view of the {@code uint8} array at {@code position}, as bytes with the same bits} */
    public java.nio.ByteBuffer getUint8ArrayView(int position) {
        return arrayView(position, Byte.BYTES);
    }

    public ShortBuffer getInt16ArrayView(int position) {
        return arrayView(position, Short.BYTES).asShortBuffer();
    }

    /** {@return a view of the {@code uint16} array at {@code position}, as shorts with the same bits} */
    public ShortBuffer getUint16ArrayView(int position) {
        return getInt16ArrayView(position);
    }

    public IntBuffer getInt32ArrayView(int position) {
        return arrayView(position, Integer.BYTES).asIntBuffer();
    }

    /** {@return a view of the {@code uint32} array at {@code position}, as ints with the same bits} */
    public IntBuffer getUint32ArrayView(int position) {
        return getInt32ArrayView(position);
    }

    public LongBuffer getInt64ArrayView(int position) {
        return arrayView(position, Long.BYTES).asLongBuffer();
    }

    /** {@return a view of the {@code uint64} array at {@code position}, as longs with the same bits} */
    public LongBuffer getUint64ArrayView(int position) {
        return getInt64ArrayView(position);
    }

    public FloatBuffer getFloat32ArrayView(int position) {
        return arrayView(position, Float.BYTES).asFloatBuffer();
    }

    public DoubleBuffer getFloat64ArrayView(int position) {
        return arrayView(position, Double.BYTES).asDoubleBuffer();
    }

    /** {@return a read-only, little-endian view of the elements of the array at {@code position}} */
    private java.nio.ByteBuffer arrayView(int position, int elementSize) {
        var byteLength = arrayLength(position, elementSize) * elementSize;
        return readOnlyView(position + offsetSize, byteLength);
    }

    /**
     * Returns a read-only, little-endian view of the {@code size} bytes of the struct at
     * {@code position}, whose index 0 is the struct's start. Its scalars lie at fixed offsets from
     * there, which the view's absolute getters read, such as {@code getFloat(4)}. Like an array
     * view, it keeps the bytes alive and shows writes made to them later.
     *
     * @throws DecodeException if the struct isn't within the bytes
     * @throws IllegalArgumentException if {@code size} is negative
     */
    public java.nio.ByteBuffer getStruct(int position, int size) {
        if (size < 0) {
            throw new IllegalArgumentException(String.format("A struct's size can't be negative: %d", size));
        }

        if (position < 0 || (long) position + size > bytes.length) {
            throw DecodeException.outOfRange(position, size, bytes.length);
        }

        return readOnlyView(position, size);
    }

    /**
     * Returns a read-only, little-endian view of the structs of the array at {@code position}, each
     * of {@code size} bytes, one after another after the count. Struct {@code i} starts at index
     * {@code i * size}, and their number is the view's {@code remaining() / size}.
     *
     * @throws DecodeException if the count or the structs aren't within the bytes
     * @throws IllegalArgumentException if {@code size} is less than 1
     */
    public java.nio.ByteBuffer getStructArray(int position, int size) {
        assertStructSize(size);

        var byteLength = arrayLength(position, size) * size;
        return readOnlyView(position + offsetSize, byteLength);
    }

    /**
     * {@return a read-only, little-endian view of the {@code byteLength} bytes from
     * {@code position}}, which a caller has checked lie within the bytes
     */
    private java.nio.ByteBuffer readOnlyView(int position, int byteLength) {
        // A slice and a read-only buffer are big-endian, whatever their source's order
        return view.slice(position, byteLength).asReadOnlyBuffer().order(ByteOrder.LITTLE_ENDIAN);
    }

    /**
     * Reads the element count at {@code position}, after checking that the elements after it lie
     * within the bytes, so that nothing is allocated for a count the bytes can't hold.
     */
    private int arrayLength(int position, int elementSize) {
        var length = getUnsigned(position);
        var start = (long) position + offsetSize;
        var byteLength = length * elementSize;

        if (start + byteLength > bytes.length) {
            throw DecodeException.outOfRange(start, byteLength, bytes.length);
        }

        return (int) length;
    }

    /**
     * {@return a little-endian view of the {@code byteLength} bytes after the length at
     * {@code position}}
     */
    private java.nio.ByteBuffer elementsAt(int position, int byteLength) {
        return view.slice(position + offsetSize, byteLength).order(ByteOrder.LITTLE_ENDIAN);
    }

    public void setInt8(byte value) {
        view.put(position, value);
        position += Byte.BYTES;
    }

    public void setUint8(short value) {
        setInt8((byte) value);
    }

    public void setInt16(short value) {
        view.putShort(position, value);
        position += Short.BYTES;
    }

    public void setUint16(int value) {
        setInt16((short) value);
    }

    public void setInt32(int value) {
        view.putInt(position, value);
        position += Integer.BYTES;
    }

    public void setUint32(long value) {
        setInt32((int) value);
    }

    public void setInt64(long value) {
        view.putLong(position, value);
        position += Long.BYTES;
    }

    /** Writes a {@code uint64} from a {@code long} with its bits */
    public void setUint64(long value) {
        setInt64(value);
    }

    public void setFloat32(float value) {
        view.putFloat(position, value);
        position += Float.BYTES;
    }

    public void setFloat64(double value) {
        view.putDouble(position, value);
        position += Double.BYTES;
    }

    // The array setters write the length and then the elements, after checking that both fit

    public void setInt8Array(byte[] values) {
        startArray(values.length, Byte.BYTES);
        Elements.setInt8(bytes, position, values);
        position += values.length * Byte.BYTES;
    }

    public void setUint8Array(short[] values) {
        startArray(values.length, Byte.BYTES);
        Elements.setUint8(bytes, position, values);
        position += values.length * Byte.BYTES;
    }

    public void setInt16Array(short[] values) {
        startArray(values.length, Short.BYTES);
        Elements.setInt16(bytes, position, values);
        position += values.length * Short.BYTES;
    }

    public void setUint16Array(int[] values) {
        startArray(values.length, Short.BYTES);
        Elements.setUint16(bytes, position, values);
        position += values.length * Short.BYTES;
    }

    public void setInt32Array(int[] values) {
        startArray(values.length, Integer.BYTES);
        Elements.setInt32(bytes, position, values);
        position += values.length * Integer.BYTES;
    }

    public void setUint32Array(long[] values) {
        startArray(values.length, Integer.BYTES);
        Elements.setUint32(bytes, position, values);
        position += values.length * Integer.BYTES;
    }

    public void setInt64Array(long[] values) {
        startArray(values.length, Long.BYTES);
        Elements.setInt64(bytes, position, values);
        position += values.length * Long.BYTES;
    }

    /** Writes {@code uint64}s from {@code long}s with their bits */
    public void setUint64Array(long[] values) {
        setInt64Array(values);
    }

    public void setFloat32Array(float[] values) {
        startArray(values.length, Float.BYTES);
        Elements.setFloat32(bytes, position, values);
        position += values.length * Float.BYTES;
    }

    public void setFloat64Array(double[] values) {
        startArray(values.length, Double.BYTES);
        Elements.setFloat64(bytes, position, values);
        position += values.length * Double.BYTES;
    }

    /**
     * Checks that {@code length} elements of {@code elementSize} bytes fit after their length
     * prefix, and writes the prefix.
     */
    private void startArray(int length, int elementSize) {
        assertWritable(offsetSize + (long) length * elementSize);
        setCount(length);
    }

    /**
     * Writes the count of the {@code size}-byte structs in {@code bytes}, and then the bytes as
     * they are. Padding that aligns the structs goes before the count.
     *
     * @throws IllegalArgumentException if {@code size} is less than 1, or {@code bytes} aren't a
     *     whole number of structs
     */
    public void setStructArray(byte[] bytes, int size) {
        var count = structCount(bytes.length, size);

        startArray(count, size);
        System.arraycopy(bytes, 0, this.bytes, position, bytes.length);
        position += bytes.length;
    }

    /**
     * {@return the number of {@code size}-byte structs in {@code byteLength} bytes}
     *
     * @throws IllegalArgumentException if {@code size} is less than 1, or the bytes aren't a whole
     *     number of structs
     */
    static int structCount(int byteLength, int size) {
        assertStructSize(size);
        if (byteLength % size != 0) {
            throw new IllegalArgumentException(
                String.format("%d bytes aren't a whole number of %d-byte structs", byteLength, size)
            );
        }

        return byteLength / size;
    }

    /**
     * Checks a struct's size, which is positive
     *
     * @throws IllegalArgumentException if {@code size} is less than 1
     */
    static void assertStructSize(int size) {
        if (size < 1) {
            throw new IllegalArgumentException(String.format("A struct's size must be positive, not %d", size));
        }
    }

    /** Writes {@code bytes} as they are, without a length prefix, such as a struct */
    public void setBytes(byte[] bytes) {
        assertWritable(bytes.length);

        System.arraycopy(bytes, 0, this.bytes, position, bytes.length);
        position += bytes.length;
    }

    /** Writes {@code byteLength} zeros, such as to align the field after them */
    public void setPadding(int byteLength) {
        assertWritable(byteLength);

        Arrays.fill(bytes, position, position + byteLength, (byte) 0);
        position += byteLength;
    }

    /**
     * Reads the UTF-8 string at {@code position}, keeping a leading U+FEFF, which is part of the
     * string rather than a byte order mark.
     *
     * @throws DecodeException if the string isn't within the bytes or isn't valid UTF-8
     */
    public String getString(int position) {
        var length = arrayLength(position, Byte.BYTES);

        try {
            return Utf8.decode(bytes, position + offsetSize, length);
        } catch (CharacterCodingException ex) {
            throw new DecodeException(String.format("The string at %d isn't valid UTF-8", position), ex);
        }
    }

    /** Writes a string's UTF-8 bytes after their length */
    public void setString(byte[] utf8) {
        setInt8Array(utf8);
    }

    // A vector is its item count, the end of each item, counted from the vector's position, and
    // then the items. Item i starts where item i - 1 ends, and item 0 after the last end.

    /**
     * Returns the number of items of the vector at {@code position}.
     *
     * @throws DecodeException if the count or the ends after it aren't within the bytes
     */
    public int getVectorLength(int position) {
        return arrayLength(position, offsetSize);
    }

    /**
     * Returns the position of item {@code index} of the vector at {@code position}. It's read from
     * the ends of that item and the one before it, without reading any other item.
     *
     * @throws DecodeException if the count or the ends aren't within the bytes, or the item ends
     *     before it starts or past the bytes
     * @throws IndexOutOfBoundsException if the vector has no item {@code index}
     */
    public int getVectorOffset(int position, int index) {
        var count = getVectorLength(position);
        if (index < 0 || index >= count) {
            throw new IndexOutOfBoundsException(
                String.format("Item %d is outside the vector of %d at %d", index, count, position)
            );
        }

        var start = index == 0 ? firstItemStart(count) : itemEnd(position, index - 1);
        return itemOffset(position, start, itemEnd(position, index));
    }

    /**
     * Returns the position of each item of the vector at {@code position}.
     *
     * @throws DecodeException if the count or the ends aren't within the bytes, or an item ends
     *     before it starts or past the bytes
     */
    public int[] getVectorOffsets(int position) {
        // Checking the ends bounds the count before allocating for it
        var count = getVectorLength(position);
        var offsets = new int[count];

        var start = firstItemStart(count);
        for (int i = 0; i < count; i++) {
            var end = itemEnd(position, i);
            offsets[i] = itemOffset(position, start, end);
            start = end;
        }

        return offsets;
    }

    /**
     * Returns a copy of each item of the vector at {@code position}.
     *
     * @throws DecodeException if the count or the ends aren't within the bytes, or an item ends
     *     before it starts or past the bytes
     */
    public byte[][] getVector(int position) {
        var count = getVectorLength(position);
        var vector = new byte[count][];

        // Items lie one after another, so the copies take no more bytes than the vector
        var start = firstItemStart(count);
        for (int i = 0; i < count; i++) {
            var end = itemEnd(position, i);
            var offset = itemOffset(position, start, end);
            vector[i] = Arrays.copyOfRange(bytes, offset, offset + (int) (end - start));
            start = end;
        }

        return vector;
    }

    /**
     * Writes the item count, the end of each item, counted from the vector's position, and then the
     * items
     */
    public void setVector(byte[][] vector) {
        // The last end is the vector's byte length, which every other end and the count are shorter
        // than. Check that it fits, and that the vector does, before writing any of it.
        long byteLength = firstItemStart(vector.length);
        for (var item : vector) {
            byteLength += item.length;
        }
        assertCountFits(byteLength);
        assertWritable(byteLength);

        putUnsigned(vector.length);

        long end = firstItemStart(vector.length);
        for (var item : vector) {
            end += item.length;
            putUnsigned(end);
        }

        for (var item : vector) {
            setBytes(item);
        }
    }

    /**
     * {@return where item 0 of a vector of {@code count} items starts, counted from the vector's
     * position}
     */
    private long firstItemStart(int count) {
        return (count + 1L) * offsetSize;
    }

    /**
     * Reads the end of item {@code index} of the vector at {@code position}, counted from that
     * position
     */
    private long itemEnd(int position, int index) {
        return getUnsigned(position + (index + 1L) * offsetSize);
    }

    /**
     * Checks that a vector item, from {@code start} to {@code end} from the vector's position,
     * starts no later than it ends and lies within the bytes.
     *
     * @return the item's position
     */
    private int itemOffset(int position, long start, long end) {
        if (start > end) {
            throw new DecodeException(
                String.format(
                    "The vector item at %d ends before it starts, at %d",
                    position + start,
                    position + end
                )
            );
        }

        if (position + end > bytes.length) {
            throw DecodeException.outOfRange(position + start, end - start, bytes.length);
        }

        return (int) (position + start);
    }

    /**
     * Reads an unsigned value as wide as the table's offsets, such as a length or an offset.
     *
     * @throws DecodeException if it isn't within the bytes
     */
    private long getUnsigned(long position) {
        var index = (int) position;
        if (index != position) {
            throw DecodeException.outOfRange(position, offsetSize, bytes.length);
        }

        // The JDK's buffer checks the rest of the range, as it does for the scalar getters. A check
        // of its own as well made reading a vector of 100 nested tables 10% slower.
        try {
            return switch (offsetSize) {
                case Byte.BYTES -> Byte.toUnsignedLong(view.get(index));

                case Short.BYTES -> Short.toUnsignedLong(view.getShort(index));

                default -> Integer.toUnsignedLong(view.getInt(index));
            };
        } catch (IndexOutOfBoundsException ex) {
            throw outOfRange(index, offsetSize, ex);
        }
    }

    /**
     * Writes an offset or a header slot, as wide as the table's offsets.
     *
     * @throws IllegalArgumentException if it doesn't fit in that width
     */
    private void setUnsigned(long value) {
        if (value < 0 || value > maxOffset) {
            throw new IllegalArgumentException(
                String.format("%d doesn't fit in %s offsets", value, offsetType)
            );
        }

        putUnsigned(value);
    }

    /**
     * Writes a count, length or item end, as wide as the table's offsets.
     *
     * @throws IllegalArgumentException if it doesn't fit in that width
     */
    private void setCount(long value) {
        assertCountFits(value);
        putUnsigned(value);
    }

    /** Writes an unsigned value as wide as the table's offsets, which it fits in */
    private void putUnsigned(long value) {
        switch (offsetSize) {
            case Byte.BYTES -> view.put(position, (byte) value);

            case Short.BYTES -> view.putShort(position, (short) value);

            default -> view.putInt(position, (int) value);
        }

        position += offsetSize;
    }

    /** Checks that a count, length or item end fits in a slot, before anything is written */
    private void assertCountFits(long value) {
        if (value < 0 || value > maxOffset) {
            throw new IllegalArgumentException(
                String.format(
                    "Can't write %d at %d: a %s count, length or item end holds at most %d",
                    value,
                    position,
                    offsetType,
                    maxOffset
                )
            );
        }
    }

    private void assertWritable(long byteLength) {
        if (position + byteLength > bytes.length) {
            throw new IndexOutOfBoundsException(
                String.format(
                    "Write exceeds the supplied buffer: %d bytes at %d of %d",
                    byteLength,
                    position,
                    bytes.length
                )
            );
        }
    }

    /**
     * Converts an offset to a position, which the read at it checks against the bytes, as in
     * TypeScript. Checking it against them here as well made resolving the field offsets of 100
     * nested tables 15% slower.
     */
    private int toPosition(long offset) {
        if (offset > Integer.MAX_VALUE) {
            throw new DecodeException(
                String.format(
                    "Offset %d points outside the supplied buffer of %d bytes",
                    offset,
                    bytes.length
                )
            );
        }

        return (int) offset;
    }

    private DecodeException outOfRange(int position, int byteLength, IndexOutOfBoundsException cause) {
        var exception = DecodeException.outOfRange(position, byteLength, bytes.length);
        exception.initCause(cause);

        return exception;
    }
}
