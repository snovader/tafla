package io.github.snovader.tafla;

import java.lang.invoke.*;
import java.nio.*;

/**
 * Writes values little-endian into bytes, for {@link TableWriter}: the elements of arrays, from
 * {@code position}, and scalars. Arrays of the elements' own Java type are
 * copied in bulk, and unsigned ones are narrowed element by element, as a cast would. Callers
 * check that the values fit; a write past the end throws an {@link IndexOutOfBoundsException}.
 */
final class Elements {
    // Views of byte arrays as arrays of wider values, which write at any index, little-endian
    static final VarHandle INT16 = MethodHandles.byteArrayViewVarHandle(short[].class, ByteOrder.LITTLE_ENDIAN);
    static final VarHandle INT32 = MethodHandles.byteArrayViewVarHandle(int[].class, ByteOrder.LITTLE_ENDIAN);
    static final VarHandle INT64 = MethodHandles.byteArrayViewVarHandle(long[].class, ByteOrder.LITTLE_ENDIAN);
    static final VarHandle FLOAT32 = MethodHandles.byteArrayViewVarHandle(float[].class, ByteOrder.LITTLE_ENDIAN);
    static final VarHandle FLOAT64 = MethodHandles.byteArrayViewVarHandle(double[].class, ByteOrder.LITTLE_ENDIAN);

    private Elements() {}

    static void setInt8(byte[] bytes, int position, byte[] values) {
        System.arraycopy(values, 0, bytes, position, values.length);
    }

    static void setUint8(byte[] bytes, int position, short[] values) {
        for (int i = 0; i < values.length; i++) {
            bytes[position + i] = (byte) values[i];
        }
    }

    static void setInt16(byte[] bytes, int position, short[] values) {
        view(bytes, position, values.length * Short.BYTES).asShortBuffer().put(values);
    }

    static void setUint16(byte[] bytes, int position, int[] values) {
        for (int i = 0; i < values.length; i++) {
            INT16.set(bytes, position + i * Short.BYTES, (short) values[i]);
        }
    }

    static void setInt32(byte[] bytes, int position, int[] values) {
        view(bytes, position, values.length * Integer.BYTES).asIntBuffer().put(values);
    }

    static void setUint32(byte[] bytes, int position, long[] values) {
        for (int i = 0; i < values.length; i++) {
            INT32.set(bytes, position + i * Integer.BYTES, (int) values[i]);
        }
    }

    static void setInt64(byte[] bytes, int position, long[] values) {
        view(bytes, position, values.length * Long.BYTES).asLongBuffer().put(values);
    }

    static void setFloat32(byte[] bytes, int position, float[] values) {
        view(bytes, position, values.length * Float.BYTES).asFloatBuffer().put(values);
    }

    static void setFloat64(byte[] bytes, int position, double[] values) {
        view(bytes, position, values.length * Double.BYTES).asDoubleBuffer().put(values);
    }

    /**
     * {@return a little-endian view of the {@code byteLength} bytes from {@code position}}, whose
     * typed views copy arrays in bulk
     */
    private static java.nio.ByteBuffer view(byte[] bytes, int position, int byteLength) {
        // A typed view starts at the buffer's position, which wrap() sets to the offset
        return java.nio.ByteBuffer.wrap(bytes, position, byteLength).order(ByteOrder.LITTLE_ENDIAN);
    }
}
