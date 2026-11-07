package io.github.snovader.tafla.test;

import io.github.snovader.tafla.*;
import io.github.snovader.tafla.test.FieldDescription.*;

import java.util.*;

/**
 * Writes and reads the bytes of structs of scalars, as {@code SPEC.md} lays them out: each scalar
 * of a struct after the one before it, little-endian and without padding, and each struct of an
 * array after the one before it.
 */
final class StructBytes {
    private StructBytes() {}

    /** {@return the byte length of a struct of scalars of the types, the sum of their sizes} */
    static int size(List<ScalarType> types) {
        return types.stream().mapToInt(ScalarType::size).sum();
    }

    /** {@return the alignment of an array of structs of scalars of the types: the largest size} */
    static int alignment(List<ScalarType> types) {
        return types.stream().mapToInt(ScalarType::size).max().orElse(Byte.BYTES);
    }

    /**
     * {@return the bytes of structs of scalars of the types, one after another}, written with
     * {@link ByteBuffer}'s setters
     */
    static byte[] pack(List<ScalarType> types, List<List<Long>> items) {
        var buffer = ByteBuffer.allocate(items.size() * size(types));
        for (var values : items) {
            for (int i = 0; i < types.size(); i++) {
                types.get(i).write(buffer, values.get(i));
            }
        }

        return buffer.bytes();
    }

    /**
     * Writes a struct of scalars of the types at {@code offset} in {@code bytes}, as a generated
     * struct's {@code write()} does
     */
    static void write(List<ScalarType> types, List<Long> values, byte[] bytes, int offset) {
        var struct = pack(types, List.of(values));
        System.arraycopy(struct, 0, bytes, offset, struct.length);
    }

    /**
     * {@return the struct of scalars of the types at {@code position}, read from the view of its
     * bytes}
     */
    static StructField read(ByteBuffer buffer, int position, List<ScalarType> types) {
        var values = valuesAt(buffer.getStruct(position, size(types)), types, 0);

        var scalars = new ArrayList<ScalarField>(types.size());
        for (int i = 0; i < types.size(); i++) {
            scalars.add(new ScalarField(types.get(i), values.get(i)));
        }

        return new StructField(scalars);
    }

    /**
     * {@return the array of structs of scalars of the types at {@code position}, each read from the
     * view of their bytes at its offset in it}
     */
    static StructArrayField readArray(ByteBuffer buffer, int position, List<ScalarType> types) {
        var size = size(types);
        var structs = buffer.getStructArray(position, size);
        var count = structs.remaining() / size;

        var items = new ArrayList<List<Long>>(count);
        for (int i = 0; i < count; i++) {
            items.add(valuesAt(structs, types, i * size));
        }

        return new StructArrayField(types, items);
    }

    /**
     * {@return the values of the scalars of the types from index {@code start} of a view}, each at
     * its offset and in the view's byte order, as a reader of a struct's view reads them
     */
    private static List<Long> valuesAt(java.nio.ByteBuffer view, List<ScalarType> types, int start) {
        var values = new ArrayList<Long>(types.size());

        var offset = start;
        for (var type : types) {
            values.add(type.get(view, offset));
            offset += type.size();
        }

        return values;
    }
}
