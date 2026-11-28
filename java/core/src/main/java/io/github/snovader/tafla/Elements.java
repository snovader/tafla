package io.github.snovader.tafla;

import java.lang.invoke.*;
import java.nio.*;

/**
 * Writes values little-endian into bytes, for {@link TableWriter}: the elements of arrays, from
 * {@code position}, and scalars. Arrays of the elements' own Java type are
 * copied in bulk, unsigned ones are narrowed element by element, as a cast would, and bools are
 * written as 1 and 0. Callers check that the values fit; a write past the end throws an
 * {@link IndexOutOfBoundsException}.
 */
final class Elements {
    // Views of byte arrays as arrays of wider values, which read and write at any index,
    // little-endian, and throw an IndexOutOfBoundsException past the end. TableReader reads through
    // them too.
    static final VarHandle INT16 = MethodHandles.byteArrayViewVarHandle(short[].class, ByteOrder.LITTLE_ENDIAN);
    static final VarHandle INT32 = MethodHandles.byteArrayViewVarHandle(int[].class, ByteOrder.LITTLE_ENDIAN);
    static final VarHandle INT64 = MethodHandles.byteArrayViewVarHandle(long[].class, ByteOrder.LITTLE_ENDIAN);
    static final VarHandle FLOAT32 = MethodHandles.byteArrayViewVarHandle(float[].class, ByteOrder.LITTLE_ENDIAN);
    static final VarHandle FLOAT64 = MethodHandles.byteArrayViewVarHandle(double[].class, ByteOrder.LITTLE_ENDIAN);

    static {
        loadCopySignature();
    }

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
     * Writes bools as bytes of 1 and 0, in a loop, since the JDK has no bulk conversion from a
     * {@code boolean[]}: {@code System.arraycopy()} and {@code MemorySegment.copy()} reject one. It
     * wrote 1,000 bools in 0.16 µs, as fast as the loop of {@link #setUint8} wrote 1,000
     * {@code uint8}s, where {@link #setInt8} copied 1,000 bytes in 0.015 µs, in JDK 25.
     */
    static void setBool(byte[] bytes, int position, boolean[] values) {
        for (int i = 0; i < values.length; i++) {
            bytes[position + i] = (byte) (values[i] ? 1 : 0);
        }
    }

    /**
     * {@return a little-endian view of the {@code byteLength} bytes from {@code position}}, whose
     * typed views copy arrays in bulk
     */
    private static ByteBuffer view(byte[] bytes, int position, int byteLength) {
        // A typed view starts at the buffer's position, which wrap() sets to the offset
        return ByteBuffer.wrap(bytes, position, byteLength).order(ByteOrder.LITTLE_ENDIAN);
    }

    /**
     * Loads {@code jdk.internal.foreign.MemorySessionImpl}, a class in the signature of the JDK's
     * {@code ScopedMemoryAccess.copyMemory()}, in which the bulk copies of NIO's typed views end,
     * those of {@link TableReader}'s array getters too. C2 inlines that copy into a caller only if
     * the class is loaded when it compiles the caller. HotSpot loads it only when it queues the
     * copy itself for compilation, which grows hot as early as its callers do. A caller compiled
     * before then calls the copy, and allocates the two views, for every array, as
     * {@link TableBuilder}'s writes of arrays did in every run of the benchmarks' document of 100
     * rows. Loading the class first makes every bulk copy inline, in whatever order they compile.
     * A JDK without the class leaves the copies as they are.
     */
    private static void loadCopySignature() {
        try {
            Class.forName("jdk.internal.foreign.MemorySessionImpl", false, null);
        } catch (ClassNotFoundException exception) {
            // A JDK whose copy has another signature
        }
    }
}
