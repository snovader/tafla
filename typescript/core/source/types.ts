import type { SIZEOF_8, SIZEOF_16, SIZEOF_32, SIZEOF_64 } from './constants.ts';

/**
 * A table's offset type: the unsigned integer type of its header slots, field
 * offsets, counts, lengths and item ends, named as `SPEC.md` and schemas name
 * it, which limits how long the table can be
 */
export type OffsetType = 'uint8' | 'uint16' | 'uint32';

/** The offset size, the width in bytes of an offset type */
export type OffsetSize = 1 | 2 | 4;

export type ElementSize =
    | typeof SIZEOF_8
    | typeof SIZEOF_16
    | typeof SIZEOF_32
    | typeof SIZEOF_64;

export type TypedArray =
    | BigInt64Array
    | BigUint64Array
    | Float32Array
    | Float64Array
    | Int8Array
    | Int16Array
    | Int32Array
    | Uint8Array
    | Uint16Array
    | Uint32Array;

export interface TypedArrayConstructor {
    BYTES_PER_ELEMENT: number;

    // oxlint-disable-next-line typescript/no-explicit-any -- Necessary for inference
    new (length: number): any;
    // oxlint-disable-next-line typescript/no-explicit-any -- Necessary for inference
    new (buffer: ArrayBufferLike, byteOffset: number, length: number): any;
}
