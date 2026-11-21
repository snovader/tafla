import { SIZEOF_32, SIZEOF_64 } from './constants.ts';

// Little-endian stores of scalars into a Uint8Array, whatever the host's byte
// order, for the write functions in `utils.ts` and `TableWriter`'s setters and
// slots. They don't check `offset`: each caller checks that the scalar lies
// within `bytes` first, and throws its own error if it doesn't.
//
// Each converts its value once, as `DataView`'s setters do, and throws their
// `TypeError` for a value of the wrong type, such as a number for an int64,
// before it writes any byte. Signed and unsigned types of the same width wrap
// a value to the same bytes, so a store serves both.
//
// They write the bytes one at a time rather than through a `DataView` of
// them: V8 keeps a Uint8Array of up to 64 bytes on its heap until a DataView
// asks for its ArrayBuffer. Integers of up to 32 bits are shifted out of the
// value. Int64s and floats are set in a scratch `DataView`, and read back as
// 32-bit words, which are shifted out as int32s are. A function per width
// keeps each small enough to inline.

const scratch = new DataView(new ArrayBuffer(SIZEOF_64));

/** Stores an int8 or uint8 at `offset` */
export function storeInt8(
    bytes: Uint8Array,
    offset: number,
    value: number
): void {
    // The Uint8Array wraps the value to a byte, as DataView's setters do
    bytes[offset] = value;
}

/** Stores an int16 or uint16 at `offset`, little-endian */
export function storeInt16(
    bytes: Uint8Array,
    offset: number,
    value: number
): void {
    // `| 0` converts and wraps to 32 bits, whose low 16 are DataView's
    const word = value | 0;
    bytes[offset] = word;
    bytes[offset + 1] = word >> 8;
}

/** Stores an int32 or uint32 at `offset`, little-endian */
export function storeInt32(
    bytes: Uint8Array,
    offset: number,
    value: number
): void {
    const word = value | 0;
    bytes[offset] = word;
    bytes[offset + 1] = word >> 8;
    bytes[offset + 2] = word >> 16;
    bytes[offset + 3] = word >> 24;
}

/** Stores an int64 or uint64 at `offset`, little-endian */
export function storeInt64(
    bytes: Uint8Array,
    offset: number,
    value: bigint
): void {
    scratch.setBigInt64(0, value, true);

    // Each 64-bit store copies the scratch's two 32-bit words itself. A copy
    // that both shared would be one more call, whose bytecode counts against
    // the inlining budget of the code that calls the store, and made the
    // 64-bit write functions 4% to 5% slower in Node 26. A loop of two takes
    // less bytecode than 8 copies, and Turboshaft unrolls a loop of fewer
    // than 4 iterations.
    for (let i = 0; i < SIZEOF_64; i += SIZEOF_32) {
        const word = scratch.getInt32(i, true);
        const position = offset + i;
        bytes[position] = word;
        bytes[position + 1] = word >> 8;
        bytes[position + 2] = word >> 16;
        bytes[position + 3] = word >> 24;
    }
}

/** Stores a float32 at `offset`, little-endian */
export function storeFloat32(
    bytes: Uint8Array,
    offset: number,
    value: number
): void {
    scratch.setFloat32(0, value, true);
    storeInt32(bytes, offset, scratch.getInt32(0, true));
}

/** Stores a float64 at `offset`, little-endian */
export function storeFloat64(
    bytes: Uint8Array,
    offset: number,
    value: number
): void {
    scratch.setFloat64(0, value, true);

    // The scratch's words, as storeInt64() copies them
    for (let i = 0; i < SIZEOF_64; i += SIZEOF_32) {
        const word = scratch.getInt32(i, true);
        const position = offset + i;
        bytes[position] = word;
        bytes[position + 1] = word >> 8;
        bytes[position + 2] = word >> 16;
        bytes[position + 3] = word >> 24;
    }
}
