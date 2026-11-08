package io.github.snovader.tafla.test;

import io.github.snovader.tafla.*;

import java.nio.DoubleBuffer;
import java.nio.FloatBuffer;
import java.nio.IntBuffer;
import java.nio.LongBuffer;
import java.nio.ShortBuffer;
import java.util.*;

/**
 * The type of a scalar field, of an array's elements and of a struct's scalars, which descriptions
 * name as {@code cases.json} does, such as {@code "uint16"} and {@code "uint16[]"}.
 *
 * <p>Descriptions hold every value as a {@code long}: an integer as its value, a {@code uint64} as
 * its bits, as {@link ByteBuffer} reads it, and a float as its raw bits, a float32's as an
 * {@code int} widened to a {@code long}. Comparing values then compares floats by their bits,
 * telling -0.0 from 0.0, and NaN from any other value, NaNs included.</p>
 */
enum ScalarType {
    INT8(Byte.BYTES),
    UINT8(Byte.BYTES),
    INT16(Short.BYTES),
    UINT16(Short.BYTES),
    INT32(Integer.BYTES),
    UINT32(Integer.BYTES),
    INT64(Long.BYTES),
    UINT64(Long.BYTES),
    FLOAT32(Float.BYTES),
    FLOAT64(Double.BYTES);

    private final int size;

    ScalarType(int size) {
        this.size = size;
    }

    /** {@return the type's name in {@code cases.json}, such as {@code "uint16"}} */
    String jsonName() {
        return name().toLowerCase(Locale.ROOT);
    }

    /** {@return the byte length of a value} */
    int size() {
        return size;
    }

    void set(TableBuilder builder, long value) {
        switch (this) {
            case INT8 -> builder.setInt8((byte) value);
            case UINT8 -> builder.setUint8((short) value);
            case INT16 -> builder.setInt16((short) value);
            case UINT16 -> builder.setUint16((int) value);
            case INT32 -> builder.setInt32((int) value);
            case UINT32 -> builder.setUint32(value);
            case INT64 -> builder.setInt64(value);
            case UINT64 -> builder.setUint64(value);
            case FLOAT32 -> builder.setFloat32(Float.intBitsToFloat((int) value));
            case FLOAT64 -> builder.setFloat64(Double.longBitsToDouble(value));
        }
    }

    void setArray(TableBuilder builder, long[] values) {
        switch (this) {
            case INT8 -> builder.setInt8Array(toBytes(values));
            case UINT8 -> builder.setUint8Array(toShorts(values));
            case INT16 -> builder.setInt16Array(toShorts(values));
            case UINT16 -> builder.setUint16Array(toInts(values));
            case INT32 -> builder.setInt32Array(toInts(values));
            case UINT32 -> builder.setUint32Array(values);
            case INT64 -> builder.setInt64Array(values);
            case UINT64 -> builder.setUint64Array(values);
            case FLOAT32 -> builder.setFloat32Array(toFloats(values));
            case FLOAT64 -> builder.setFloat64Array(toDoubles(values));
        }
    }

    void add(TableSizer sizer) {
        switch (this) {
            case INT8 -> sizer.addInt8();
            case UINT8 -> sizer.addUint8();
            case INT16 -> sizer.addInt16();
            case UINT16 -> sizer.addUint16();
            case INT32 -> sizer.addInt32();
            case UINT32 -> sizer.addUint32();
            case INT64 -> sizer.addInt64();
            case UINT64 -> sizer.addUint64();
            case FLOAT32 -> sizer.addFloat32();
            case FLOAT64 -> sizer.addFloat64();
        }
    }

    void addArray(TableSizer sizer, int count) {
        switch (this) {
            case INT8 -> sizer.addInt8Array(count);
            case UINT8 -> sizer.addUint8Array(count);
            case INT16 -> sizer.addInt16Array(count);
            case UINT16 -> sizer.addUint16Array(count);
            case INT32 -> sizer.addInt32Array(count);
            case UINT32 -> sizer.addUint32Array(count);
            case INT64 -> sizer.addInt64Array(count);
            case UINT64 -> sizer.addUint64Array(count);
            case FLOAT32 -> sizer.addFloat32Array(count);
            case FLOAT64 -> sizer.addFloat64Array(count);
        }
    }

    void set(TableWriter writer, int index, long value) {
        switch (this) {
            case INT8 -> writer.setInt8(index, (byte) value);
            case UINT8 -> writer.setUint8(index, (short) value);
            case INT16 -> writer.setInt16(index, (short) value);
            case UINT16 -> writer.setUint16(index, (int) value);
            case INT32 -> writer.setInt32(index, (int) value);
            case UINT32 -> writer.setUint32(index, value);
            case INT64 -> writer.setInt64(index, value);
            case UINT64 -> writer.setUint64(index, value);
            case FLOAT32 -> writer.setFloat32(index, Float.intBitsToFloat((int) value));
            case FLOAT64 -> writer.setFloat64(index, Double.longBitsToDouble(value));
        }
    }

    void setArray(TableWriter writer, int index, long[] values) {
        switch (this) {
            case INT8 -> writer.setInt8Array(index, toBytes(values));
            case UINT8 -> writer.setUint8Array(index, toShorts(values));
            case INT16 -> writer.setInt16Array(index, toShorts(values));
            case UINT16 -> writer.setUint16Array(index, toInts(values));
            case INT32 -> writer.setInt32Array(index, toInts(values));
            case UINT32 -> writer.setUint32Array(index, values);
            case INT64 -> writer.setInt64Array(index, values);
            case UINT64 -> writer.setUint64Array(index, values);
            case FLOAT32 -> writer.setFloat32Array(index, toFloats(values));
            case FLOAT64 -> writer.setFloat64Array(index, toDoubles(values));
        }
    }

    long read(ByteBuffer buffer, int position) {
        return switch (this) {
            case INT8 -> buffer.getInt8(position);
            case UINT8 -> buffer.getUint8(position);
            case INT16 -> buffer.getInt16(position);
            case UINT16 -> buffer.getUint16(position);
            case INT32 -> buffer.getInt32(position);
            case UINT32 -> buffer.getUint32(position);
            case INT64 -> buffer.getInt64(position);
            case UINT64 -> buffer.getUint64(position);
            case FLOAT32 -> Float.floatToRawIntBits(buffer.getFloat32(position));
            case FLOAT64 -> Double.doubleToRawLongBits(buffer.getFloat64(position));
        };
    }

    long[] readArray(ByteBuffer buffer, int position) {
        return switch (this) {
            case INT8 -> fromBytes(buffer.getInt8Array(position));
            case UINT8 -> fromShorts(buffer.getUint8Array(position));
            case INT16 -> fromShorts(buffer.getInt16Array(position));
            case UINT16 -> fromInts(buffer.getUint16Array(position));
            case INT32 -> fromInts(buffer.getInt32Array(position));
            case UINT32 -> buffer.getUint32Array(position);
            case INT64 -> buffer.getInt64Array(position);
            case UINT64 -> buffer.getUint64Array(position);
            case FLOAT32 -> fromFloats(buffer.getFloat32Array(position));
            case FLOAT64 -> fromDoubles(buffer.getFloat64Array(position));
        };
    }

    /** Reads an array through its view getter, with unsigned elements widened as the array getters do */
    long[] readArrayView(ByteBuffer buffer, int position) {
        return switch (this) {
            case INT8 -> fromBytes(toArray(buffer.getInt8ArrayView(position)));
            case UINT8 -> lowBits(fromBytes(toArray(buffer.getUint8ArrayView(position))));
            case INT16 -> fromShorts(toArray(buffer.getInt16ArrayView(position)));
            case UINT16 -> lowBits(fromShorts(toArray(buffer.getUint16ArrayView(position))));
            case INT32 -> fromInts(toArray(buffer.getInt32ArrayView(position)));
            case UINT32 -> lowBits(fromInts(toArray(buffer.getUint32ArrayView(position))));
            case INT64 -> toArray(buffer.getInt64ArrayView(position));
            case UINT64 -> toArray(buffer.getUint64ArrayView(position));
            case FLOAT32 -> fromFloats(toArray(buffer.getFloat32ArrayView(position)));
            case FLOAT64 -> fromDoubles(toArray(buffer.getFloat64ArrayView(position)));
        };
    }

    /** {@return the values with only their low {@link #size()} bytes, as an unsigned getter reads them} */
    private long[] lowBits(long[] values) {
        var mask = (1L << (size() * Byte.SIZE)) - 1;
        for (int i = 0; i < values.length; i++) {
            values[i] &= mask;
        }

        return values;
    }

    /**
     * Gets the value at {@code index} of the JDK's own buffer, in its byte order, as descriptions
     * hold it, such as a scalar of a struct's view
     */
    long get(java.nio.ByteBuffer source, int index) {
        return switch (this) {
            case INT8 -> source.get(index);
            case UINT8 -> Byte.toUnsignedLong(source.get(index));
            case INT16 -> source.getShort(index);
            case UINT16 -> Short.toUnsignedLong(source.getShort(index));
            case INT32 -> source.getInt(index);
            case UINT32 -> Integer.toUnsignedLong(source.getInt(index));
            case INT64, UINT64 -> source.getLong(index);
            case FLOAT32 -> Float.floatToRawIntBits(source.getFloat(index));
            case FLOAT64 -> Double.doubleToRawLongBits(source.getDouble(index));
        };
    }

    /**
     * Puts a value with the JDK's own buffer, in its byte order, writing an integer's low bits as a
     * cast would: bytes for tafla to read, or to compare its bytes with
     */
    void put(java.nio.ByteBuffer target, long value) {
        switch (this) {
            case INT8, UINT8 -> target.put((byte) value);
            case INT16, UINT16 -> target.putShort((short) value);
            case INT32, UINT32 -> target.putInt((int) value);
            case INT64, UINT64 -> target.putLong(value);
            case FLOAT32 -> target.putFloat(Float.intBitsToFloat((int) value));
            case FLOAT64 -> target.putDouble(Double.longBitsToDouble(value));
        }
    }

    // Arrays of values as descriptions hold them, converted to and from the Java arrays that
    // tafla's setters and getters take, with floats as their raw bits

    static byte[] toBytes(long[] values) {
        var array = new byte[values.length];
        for (int i = 0; i < values.length; i++) {
            array[i] = (byte) values[i];
        }

        return array;
    }

    static short[] toShorts(long[] values) {
        var array = new short[values.length];
        for (int i = 0; i < values.length; i++) {
            array[i] = (short) values[i];
        }

        return array;
    }

    static int[] toInts(long[] values) {
        var array = new int[values.length];
        for (int i = 0; i < values.length; i++) {
            array[i] = (int) values[i];
        }

        return array;
    }

    static float[] toFloats(long[] values) {
        var array = new float[values.length];
        for (int i = 0; i < values.length; i++) {
            array[i] = Float.intBitsToFloat((int) values[i]);
        }

        return array;
    }

    static double[] toDoubles(long[] values) {
        var array = new double[values.length];
        for (int i = 0; i < values.length; i++) {
            array[i] = Double.longBitsToDouble(values[i]);
        }

        return array;
    }

    static long[] fromBytes(byte[] array) {
        var values = new long[array.length];
        for (int i = 0; i < array.length; i++) {
            values[i] = array[i];
        }

        return values;
    }

    static long[] fromShorts(short[] array) {
        var values = new long[array.length];
        for (int i = 0; i < array.length; i++) {
            values[i] = array[i];
        }

        return values;
    }

    static long[] fromInts(int[] array) {
        var values = new long[array.length];
        for (int i = 0; i < array.length; i++) {
            values[i] = array[i];
        }

        return values;
    }

    static long[] fromFloats(float[] array) {
        var values = new long[array.length];
        for (int i = 0; i < array.length; i++) {
            values[i] = Float.floatToRawIntBits(array[i]);
        }

        return values;
    }

    static long[] fromDoubles(double[] array) {
        var values = new long[array.length];
        for (int i = 0; i < array.length; i++) {
            values[i] = Double.doubleToRawLongBits(array[i]);
        }

        return values;
    }

    // The elements of the JDK's views, copied into arrays

    private static byte[] toArray(java.nio.ByteBuffer view) {
        var array = new byte[view.remaining()];
        view.get(array);

        return array;
    }

    private static short[] toArray(ShortBuffer view) {
        var array = new short[view.remaining()];
        view.get(array);

        return array;
    }

    private static int[] toArray(IntBuffer view) {
        var array = new int[view.remaining()];
        view.get(array);

        return array;
    }

    private static long[] toArray(LongBuffer view) {
        var array = new long[view.remaining()];
        view.get(array);

        return array;
    }

    private static float[] toArray(FloatBuffer view) {
        var array = new float[view.remaining()];
        view.get(array);

        return array;
    }

    private static double[] toArray(DoubleBuffer view) {
        var array = new double[view.remaining()];
        view.get(array);

        return array;
    }
}
