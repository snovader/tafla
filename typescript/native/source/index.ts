import { addon } from './addon.ts';

/** The typed array of each element type, which a read fills */
interface ElementArrays {
    Int8: Int8Array;
    Uint8: Uint8Array;
    Int16: Int16Array;
    Uint16: Uint16Array;
    Int32: Int32Array;
    Uint32: Uint32Array;
    Int64: BigInt64Array;
    Uint64: BigUint64Array;
    Float32: Float32Array;
    Float64: Float64Array;
}

/**
 * Reads the first `out.length` fields of the root table of `bytes`, which are
 * all of `out`'s element type, into `out`. The table has the default layout.
 *
 * Throws an `Error` with the code `TAFLA_DECODE` if a field doesn't lie
 * within the bytes, and a `TypeError` if `out` shares the bytes.
 */
type FieldsGetter<Out> = (bytes: Uint8Array, out: Out) => void;

/**
 * Reads a field of every item of the vector of tables at `vectorPosition`
 * into each `out`, in one pass over the items, and returns the vector's
 * length. Each `path` holds its field's index in the item, or the indexes of
 * the nested tables that lead to it and then its own. A call reads up to 16
 * columns, each a path and then its array. The table has the default layout.
 *
 * Throws an `Error` with the code `TAFLA_DECODE` if the vector, an item or a
 * field doesn't lie within the bytes, a `RangeError` if an `out` is shorter
 * than the vector, and a `TypeError` if an `out` shares the bytes.
 */
type ColumnsGetter<Out> = (
    bytes: Uint8Array,
    vectorPosition: number,
    path: Uint32Array,
    out: Out,
    ...columns: Array<Out | Uint32Array>
) => number;

type Addon = {
    [Type in keyof ElementArrays as `get${Type}Fields`]: FieldsGetter<
        ElementArrays[Type]
    >;
} & {
    [Type in keyof ElementArrays as `get${Type}Columns`]: ColumnsGetter<
        ElementArrays[Type]
    >;
};

export const {
    getInt8Fields,
    getUint8Fields,
    getInt16Fields,
    getUint16Fields,
    getInt32Fields,
    getUint32Fields,
    getInt64Fields,
    getUint64Fields,
    getFloat32Fields,
    getFloat64Fields,
    getInt8Columns,
    getUint8Columns,
    getInt16Columns,
    getUint16Columns,
    getInt32Columns,
    getUint32Columns,
    getInt64Columns,
    getUint64Columns,
    getFloat32Columns,
    getFloat64Columns
} = addon as Addon;
