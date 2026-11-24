import { SIZEOF_8, SIZEOF_16, SIZEOF_32, SIZEOF_64 } from './constants.ts';
import { encodeInto, utf8Length } from './encoding.ts';
import { outOfRange, writeOutOfRange } from './errors.ts';
import {
    storeFloat32,
    storeFloat64,
    storeInt8,
    storeInt16,
    storeInt32,
    storeInt64
} from './scalars.ts';

// Little-endian reads straight from a Uint8Array, for small records at fixed
// offsets, such as structs and stream frames, that aren't worth a TableReader.
// Each throws a DecodeError if the value doesn't lie within `bytes`.

const int32s = new Int32Array(1);
const float32s = new Float32Array(int32s.buffer);
// A 64-bit value's words are set little-endian, whatever the host's byte
// order, and the value read from them. Adding an int64's words as bigints
// allocates each of them, and took 26 ns a read rather than 5 to 10 in
// Node 26. Each read sets the words itself: through a function that they
// shared, readFloat64() took 6% longer.
const words64 = new DataView(new ArrayBuffer(SIZEOF_64));

export function readInt8(bytes: Uint8Array, offset: number): number {
    return (readUint8(bytes, offset) << 24) >> 24;
}

export function readUint8(bytes: Uint8Array, offset: number): number {
    assertReadable(bytes, offset, SIZEOF_8);
    return bytes[offset];
}

export function readInt16(bytes: Uint8Array, offset: number): number {
    return (readUint16(bytes, offset) << 16) >> 16;
}

export function readUint16(bytes: Uint8Array, offset: number): number {
    assertReadable(bytes, offset, SIZEOF_16);
    return bytes[offset] | (bytes[offset + 1] << 8);
}

export function readInt32(bytes: Uint8Array, offset: number): number {
    assertReadable(bytes, offset, SIZEOF_32);
    return (
        bytes[offset] |
        (bytes[offset + 1] << 8) |
        (bytes[offset + 2] << 16) |
        (bytes[offset + 3] << 24)
    );
}

export function readUint32(bytes: Uint8Array, offset: number): number {
    return readInt32(bytes, offset) >>> 0;
}

export function readInt64(bytes: Uint8Array, offset: number): bigint {
    assertReadable(bytes, offset, SIZEOF_64);
    words64.setInt32(0, readInt32(bytes, offset), true);
    words64.setInt32(SIZEOF_32, readInt32(bytes, offset + SIZEOF_32), true);
    return words64.getBigInt64(0, true);
}

export function readUint64(bytes: Uint8Array, offset: number): bigint {
    assertReadable(bytes, offset, SIZEOF_64);
    words64.setInt32(0, readInt32(bytes, offset), true);
    words64.setInt32(SIZEOF_32, readInt32(bytes, offset + SIZEOF_32), true);
    return words64.getBigUint64(0, true);
}

export function readFloat32(bytes: Uint8Array, offset: number): number {
    int32s[0] = readInt32(bytes, offset);
    return float32s[0];
}

export function readFloat64(bytes: Uint8Array, offset: number): number {
    assertReadable(bytes, offset, SIZEOF_64);
    words64.setInt32(0, readInt32(bytes, offset), true);
    words64.setInt32(SIZEOF_32, readInt32(bytes, offset + SIZEOF_32), true);
    return words64.getFloat64(0, true);
}

function assertReadable(
    bytes: Uint8Array,
    offset: number,
    byteLength: number
): void {
    if (!within(bytes, offset, byteLength)) {
        throw outOfRange(offset, byteLength, bytes.length);
    }
}

// Little-endian writes straight into a Uint8Array, the reads' counterparts,
// as the code that tafla-gen generates writes structs. Each throws a
// RangeError if the value wouldn't lie within `bytes`, and then converts the
// value as DataView's setter does, so a number for an int64 throws a
// TypeError; either way, it writes nothing. Unlike DataView, which converts
// the value before it checks the range, a write checks first: an offset
// outside the bytes throws the RangeError whatever the value, without
// converting it. The stores in `scalars.ts` convert and write the values, as
// they do for TableWriter's setters.
//
// TurboFan inlines up to 920 bytes of bytecode into a function, and the
// serializers that tafla-gen generates spend them on these writes, their
// table's checks and their string. So each write checks its offset itself,
// rather than through within(), which V8 would inline into each write as
// well. A 64-bit write checks through assertWritable64(), which builds its
// error out of line from two arguments, since they take less bytecode than
// three. The write is then 27 bytes, which TurboFan inlines without
// counting them, as it does any function of up to 30, so that a serializer
// pays for the check and the store but not for the write around them.

export function writeInt8(
    bytes: Uint8Array,
    offset: number,
    value: number
): void {
    if (!(isOffset(offset) && offset <= bytes.length - SIZEOF_8)) {
        throw writeOutOfRange(offset, SIZEOF_8, bytes.length);
    }

    storeInt8(bytes, offset, value);
}

export function writeUint8(
    bytes: Uint8Array,
    offset: number,
    value: number
): void {
    if (!(isOffset(offset) && offset <= bytes.length - SIZEOF_8)) {
        throw writeOutOfRange(offset, SIZEOF_8, bytes.length);
    }

    storeInt8(bytes, offset, value);
}

export function writeInt16(
    bytes: Uint8Array,
    offset: number,
    value: number
): void {
    if (!(isOffset(offset) && offset <= bytes.length - SIZEOF_16)) {
        throw writeOutOfRange(offset, SIZEOF_16, bytes.length);
    }

    storeInt16(bytes, offset, value);
}

export function writeUint16(
    bytes: Uint8Array,
    offset: number,
    value: number
): void {
    if (!(isOffset(offset) && offset <= bytes.length - SIZEOF_16)) {
        throw writeOutOfRange(offset, SIZEOF_16, bytes.length);
    }

    storeInt16(bytes, offset, value);
}

export function writeInt32(
    bytes: Uint8Array,
    offset: number,
    value: number
): void {
    if (!(isOffset(offset) && offset <= bytes.length - SIZEOF_32)) {
        throw writeOutOfRange(offset, SIZEOF_32, bytes.length);
    }

    storeInt32(bytes, offset, value);
}

export function writeUint32(
    bytes: Uint8Array,
    offset: number,
    value: number
): void {
    if (!(isOffset(offset) && offset <= bytes.length - SIZEOF_32)) {
        throw writeOutOfRange(offset, SIZEOF_32, bytes.length);
    }

    storeInt32(bytes, offset, value);
}

export function writeInt64(
    bytes: Uint8Array,
    offset: number,
    value: bigint
): void {
    assertWritable64(bytes, offset);
    storeInt64(bytes, offset, value);
}

export function writeUint64(
    bytes: Uint8Array,
    offset: number,
    value: bigint
): void {
    assertWritable64(bytes, offset);
    storeInt64(bytes, offset, value);
}

export function writeFloat32(
    bytes: Uint8Array,
    offset: number,
    value: number
): void {
    if (!(isOffset(offset) && offset <= bytes.length - SIZEOF_32)) {
        throw writeOutOfRange(offset, SIZEOF_32, bytes.length);
    }

    storeFloat32(bytes, offset, value);
}

export function writeFloat64(
    bytes: Uint8Array,
    offset: number,
    value: number
): void {
    assertWritable64(bytes, offset);
    storeFloat64(bytes, offset, value);
}

/** Checks that a 64-bit write at `offset` lies within `bytes` */
function assertWritable64(bytes: Uint8Array, offset: number): void {
    if (!(isOffset(offset) && offset <= bytes.length - SIZEOF_64)) {
        throw write64OutOfRange(bytes, offset);
    }
}

/** The `RangeError` of a 64-bit write, built from two arguments */
function write64OutOfRange(bytes: Uint8Array, offset: number): RangeError {
    return writeOutOfRange(offset, SIZEOF_64, bytes.length);
}

/**
 * Writes `string` as UTF-8 at `offset` in `bytes`, with lone surrogates
 * replaced by U+FFFD, as a table's string holds it after its length, and
 * returns the number of bytes written, which `utf8Length()` counts first.
 * It's for the code that tafla-gen generates, which writes the string after
 * a message's fields of fixed size itself.
 *
 * @throws {RangeError} If the string doesn't lie within `bytes`, having
 * written what fits of it, which may end with the first bytes of a
 * character.
 */
export function writeUtf8(
    bytes: Uint8Array,
    offset: number,
    string: string
): number {
    if (!(isOffset(offset) && offset <= bytes.length)) {
        throw stringOutOfRange(bytes, offset, string);
    }

    return encodeInto(string, bytes, offset);
}

function stringOutOfRange(
    bytes: Uint8Array,
    offset: number,
    string: string
): RangeError {
    return writeOutOfRange(offset, utf8Length(string), bytes.length);
}

/**
 * A `DataView` of `byteLength` bytes at `offset` in `bytes`, through which
 * the code that tafla-gen generates writes a long run of structs, or of
 * nested tables of fixed size. A `DataView` writes each scalar faster than
 * the write functions do, but takes longer to create, the longest over bytes
 * of up to 64 that V8 keeps on its heap until something asks for their
 * `ArrayBuffer`, so the generated code writes short runs with the write
 * functions.
 *
 * @throws {RangeError} If the bytes don't lie within `bytes`.
 */
export function viewOf(
    bytes: Uint8Array,
    offset: number,
    byteLength: number
): DataView {
    if (
        !(
            Number.isSafeInteger(byteLength) &&
            byteLength >= 0 &&
            within(bytes, offset, byteLength)
        )
    ) {
        throw writeOutOfRange(offset, byteLength, bytes.length);
    }

    return new DataView(bytes.buffer, bytes.byteOffset + offset, byteLength);
}

/** Whether `byteLength` bytes at `offset` lie within `bytes` */
function within(
    bytes: Uint8Array,
    offset: number,
    byteLength: number
): boolean {
    return (
        Number.isSafeInteger(offset) &&
        offset >= 0 &&
        offset <= bytes.length - byteLength
    );
}

/** Whether `offset` is a non-negative integer, as an offset into bytes is */
function isOffset(offset: number): boolean {
    return Number.isSafeInteger(offset) && offset >= 0;
}
