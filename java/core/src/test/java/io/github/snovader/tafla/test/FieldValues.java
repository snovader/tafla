package io.github.snovader.tafla.test;

import net.jqwik.api.*;

import java.util.*;

/**
 * Arbitrary values of fields, in the Java types that the code that tafla-gen generates takes them
 * as: unsigned integers widened to the next wider type, a {@code uint64} as a {@code long} of its
 * bits, and arrays of the same types. They're the values of {@link Descriptions}, over each type's
 * whole range and often at its limits, such as -0.0, NaN, the infinities and the smallest and
 * largest integers, with strings of code points of every UTF-8 length and empty arrays, arrays of
 * structs and vectors.
 */
final class FieldValues {
    private FieldValues() {}

    static Arbitrary<Byte> int8s() {
        return Descriptions.values(ScalarType.INT8).map(Long::byteValue);
    }

    static Arbitrary<Short> uint8s() {
        return Descriptions.values(ScalarType.UINT8).map(Long::shortValue);
    }

    static Arbitrary<Short> int16s() {
        return Descriptions.values(ScalarType.INT16).map(Long::shortValue);
    }

    static Arbitrary<Integer> uint16s() {
        return Descriptions.values(ScalarType.UINT16).map(Long::intValue);
    }

    static Arbitrary<Integer> int32s() {
        return Descriptions.values(ScalarType.INT32).map(Long::intValue);
    }

    static Arbitrary<Long> uint32s() {
        return Descriptions.values(ScalarType.UINT32);
    }

    static Arbitrary<Long> int64s() {
        return Descriptions.values(ScalarType.INT64);
    }

    static Arbitrary<Long> uint64s() {
        return Descriptions.values(ScalarType.UINT64);
    }

    static Arbitrary<Float> float32s() {
        return Descriptions.values(ScalarType.FLOAT32).map(bits -> Float.intBitsToFloat(bits.intValue()));
    }

    static Arbitrary<Double> float64s() {
        return Descriptions.values(ScalarType.FLOAT64).map(Double::longBitsToDouble);
    }

    static Arbitrary<String> strings() {
        return Descriptions.strings();
    }

    static Arbitrary<byte[]> int8Arrays() {
        return Descriptions.arrays(ScalarType.INT8).map(ScalarType::toBytes);
    }

    static Arbitrary<short[]> uint8Arrays() {
        return Descriptions.arrays(ScalarType.UINT8).map(ScalarType::toShorts);
    }

    static Arbitrary<short[]> int16Arrays() {
        return Descriptions.arrays(ScalarType.INT16).map(ScalarType::toShorts);
    }

    static Arbitrary<int[]> uint16Arrays() {
        return Descriptions.arrays(ScalarType.UINT16).map(ScalarType::toInts);
    }

    static Arbitrary<int[]> int32Arrays() {
        return Descriptions.arrays(ScalarType.INT32).map(ScalarType::toInts);
    }

    static Arbitrary<long[]> uint32Arrays() {
        return Descriptions.arrays(ScalarType.UINT32);
    }

    static Arbitrary<long[]> int64Arrays() {
        return Descriptions.arrays(ScalarType.INT64);
    }

    static Arbitrary<long[]> uint64Arrays() {
        return Descriptions.arrays(ScalarType.UINT64);
    }

    static Arbitrary<float[]> float32Arrays() {
        return Descriptions.arrays(ScalarType.FLOAT32).map(ScalarType::toFloats);
    }

    static Arbitrary<double[]> float64Arrays() {
        return Descriptions.arrays(ScalarType.FLOAT64).map(ScalarType::toDoubles);
    }

    static Arbitrary<boolean[]> boolArrays() {
        return Descriptions.boolArrays();
    }

    /** {@return the fields of the tables of vectors, empty too} */
    static <T> Arbitrary<List<T>> vectorsOf(Arbitrary<T> items) {
        return items.list().ofMaxSize(Descriptions.MAX_ITEMS);
    }

    /** {@return the records of arrays of structs, empty too} */
    static <T, A> Arbitrary<A> structArraysOf(Arbitrary<T> structs, Class<A> arrayClass) {
        return structs.array(arrayClass).ofMaxSize(Descriptions.MAX_STRUCTS);
    }
}
