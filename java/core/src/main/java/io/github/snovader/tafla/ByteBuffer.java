package io.github.snovader.tafla;

import java.nio.*;
import java.nio.charset.*;
import java.util.*;

/**
 * Reads a table from the supplied bytes in the table layout it was created with, whose multi-byte
 * values are little-endian. Array reads copy the elements into a new Java array, and array view
 * reads return a read-only view of them in place, as struct reads return the bytes of structs.
 * {@link TableBuilder}, or a {@link TableSizer}'s {@link TableWriter}, writes the tables.
 *
 * <p>Every read checks that it stays within the bytes, and throws a {@link DecodeException}
 * otherwise.</p>
 *
 * <p>Unsigned values are read into the next wider type, such as a {@code uint32} into a
 * {@code long}, except for {@code uint64}, which Java has no wider type for: it's read into a
 * {@code long} with the same bits, for {@link Long#toUnsignedString} and the like.</p>
 */
public final class ByteBuffer {
    private final byte[] bytes;
    /** A little-endian view of the bytes, which the array, struct and element views slice */
    private final java.nio.ByteBuffer view;

    private final boolean sized;
    private final boolean versioned;
    /** The offset size, the width in bytes of offsets, lengths and header slots */
    private final int offsetSize;
    /** The byte length of the header slots before the root's field offsets */
    private final int headerSize;

    public ByteBuffer(byte[] bytes) {
        this(bytes, TableLayout.DEFAULT);
    }

    public ByteBuffer(byte[] bytes, TableLayout layout) {
        this.bytes = bytes;
        this.view = java.nio.ByteBuffer.wrap(bytes).order(ByteOrder.LITTLE_ENDIAN);

        this.sized = layout.sized();
        this.versioned = layout.versioned();
        this.offsetSize = layout.offsetSize();
        this.headerSize = layout.headerSize();
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

    // The scalar getters read the bytes through Elements' VarHandles, which check the range, as the
    // array itself does for a byte, and rethrow their error, as the TypeScript getters rethrow
    // DataView's. Reading through the wrapped view instead, whose limit and byte order are fields
    // that a read loads, made reading a vector of 100 nested tables into records 11% slower.

    public byte getInt8(int position) {
        try {
            return bytes[position];
        } catch (IndexOutOfBoundsException ex) {
            throw outOfRange(position, Byte.BYTES, ex);
        }
    }

    public short getUint8(int position) {
        return (short) Byte.toUnsignedInt(getInt8(position));
    }

    public short getInt16(int position) {
        try {
            return (short) Elements.INT16.get(bytes, position);
        } catch (IndexOutOfBoundsException ex) {
            throw outOfRange(position, Short.BYTES, ex);
        }
    }

    public int getUint16(int position) {
        return Short.toUnsignedInt(getInt16(position));
    }

    public int getInt32(int position) {
        try {
            return (int) Elements.INT32.get(bytes, position);
        } catch (IndexOutOfBoundsException ex) {
            throw outOfRange(position, Integer.BYTES, ex);
        }
    }

    public long getUint32(int position) {
        return Integer.toUnsignedLong(getInt32(position));
    }

    public long getInt64(int position) {
        try {
            return (long) Elements.INT64.get(bytes, position);
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
            return (float) Elements.FLOAT32.get(bytes, position);
        } catch (IndexOutOfBoundsException ex) {
            throw outOfRange(position, Float.BYTES, ex);
        }
    }

    public double getFloat64(int position) {
        try {
            return (double) Elements.FLOAT64.get(bytes, position);
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
            values[i] = Short.toUnsignedInt((short) Elements.INT16.get(bytes, j));
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
            values[i] = Integer.toUnsignedLong((int) Elements.INT32.get(bytes, j));
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
        return getVectorItemOffset(position, getVectorLength(position), index);
    }

    /**
     * Returns the position of item {@code index} of the vector at {@code position}, as
     * {@code getVectorOffset()} does, from the {@code count} that {@code getVectorLength()} returned
     * for it, so that a reader of several items reads and checks the count once.
     *
     * <p>Another count, such as one read before the bytes changed, reads other bytes as the item's
     * ends. Those reads still stay within the bytes, and return a position or throw a
     * {@code DecodeException}.</p>
     *
     * @throws DecodeException if the item's ends aren't within the bytes, or the item ends before it
     *     starts or past the bytes
     * @throws IndexOutOfBoundsException if {@code index} is negative or not below {@code count}
     */
    public int getVectorItemOffset(int position, int count, int index) {
        if (index < 0 || index >= count) {
            throw new IndexOutOfBoundsException(
                String.format("Item %d is outside the vector of %d at %d", index, count, position)
            );
        }

        var start = index == 0 ? firstItemStart(count) : itemEnd(position, index - 1);
        return itemOffset(position, start, itemEnd(position, index));
    }

    /**
     * Returns where the items of the vector at {@code position} start, which is where item 0
     * starts: after the count and the items' ends. It reads nothing, and a reader of the items in
     * order passes it to {@code getVectorItemEnd()} as item 0's start, which checks it against the
     * item's end.
     *
     * <p>{@code count} is the one that {@code getVectorLength(position)} returned, which checks
     * that the ends lie within the bytes, and so does the position returned.</p>
     */
    public int getVectorItemsStart(int position, int count) {
        return (int) (position + firstItemStart(count));
    }

    /**
     * Returns where item {@code index} of the vector at {@code position} ends, as a position in the
     * bytes, after checking that the item ends no earlier than {@code start}, where it starts, and
     * within the bytes. A reader of the items in order reads one end for each item, as
     * {@code getVectorOffsets()} does, where {@code getVectorItemOffset()} reads two: item 0 starts
     * at {@code getVectorItemsStart(position, count)}, and each item after it at the end that this
     * returned for the item before it.
     *
     * <p>It leaves {@code index} to the reader, which reads items below the count that
     * {@code getVectorLength()} returned. Another index reads other bytes as the item's end, which
     * still stay within the bytes, and return a position or throw a {@code DecodeException}.</p>
     *
     * @throws DecodeException if the item's end isn't within the bytes, or the item ends before
     *     {@code start} or past the bytes
     */
    public int getVectorItemEnd(int position, int index, int start) {
        var end = itemEnd(position, index);
        itemOffset(position, (long) start - position, end);

        return (int) (position + end);
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

        // The read checks the rest of the range, as it does for the scalar getters. A check of its
        // own as well made reading a vector of 100 nested tables 10% slower.
        try {
            return switch (offsetSize) {
                case Byte.BYTES -> Byte.toUnsignedLong(bytes[index]);

                case Short.BYTES -> Short.toUnsignedLong((short) Elements.INT16.get(bytes, index));

                default -> Integer.toUnsignedLong((int) Elements.INT32.get(bytes, index));
            };
        } catch (IndexOutOfBoundsException ex) {
            throw outOfRange(index, offsetSize, ex);
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
