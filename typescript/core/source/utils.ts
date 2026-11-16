import { encodeInto, utf8Length } from './encoding.ts';
import { outOfRange, writeOutOfRange } from './errors.ts';

// Little-endian reads straight from a Uint8Array, for small records at fixed
// offsets, such as structs and stream frames, that aren't worth a ByteBuffer.
// Each throws a DecodeError if the value doesn't lie within `bytes`.

const int32s = new Int32Array(1);
const float32s = new Float32Array(int32s.buffer);
// A float64's words are set little-endian, whatever the host's byte order
const float64s = new DataView(new ArrayBuffer(8));

export function readInt8(bytes: Uint8Array, offset: number): number {
    return (readUint8(bytes, offset) << 24) >> 24;
}

export function readUint8(bytes: Uint8Array, offset: number): number {
    assertReadable(bytes, offset, 1);
    return bytes[offset];
}

export function readInt16(bytes: Uint8Array, offset: number): number {
    return (readUint16(bytes, offset) << 16) >> 16;
}

export function readUint16(bytes: Uint8Array, offset: number): number {
    assertReadable(bytes, offset, 2);
    return bytes[offset] | (bytes[offset + 1] << 8);
}

export function readInt32(bytes: Uint8Array, offset: number): number {
    assertReadable(bytes, offset, 4);
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
    return BigInt.asIntN(64, readUint64(bytes, offset));
}

export function readUint64(bytes: Uint8Array, offset: number): bigint {
    assertReadable(bytes, offset, 8);
    return (
        BigInt(readUint32(bytes, offset)) +
        (BigInt(readUint32(bytes, offset + 4)) << 32n)
    );
}

export function readFloat32(bytes: Uint8Array, offset: number): number {
    int32s[0] = readInt32(bytes, offset);
    return float32s[0];
}

export function readFloat64(bytes: Uint8Array, offset: number): number {
    assertReadable(bytes, offset, 8);
    float64s.setInt32(0, readInt32(bytes, offset), true);
    float64s.setInt32(4, readInt32(bytes, offset + 4), true);
    return float64s.getFloat64(0, true);
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
// as the code that tafla-gen generates writes structs. Each converts its
// value as DataView's setter does, so a number for an int64 throws a
// TypeError, and then throws a RangeError, and writes nothing, if the value
// doesn't lie within `bytes`.
//
// The value is set little-endian in the scratch, whatever the host's byte
// order, and its bytes copied, as TableWriter copies them: V8 keeps a
// Uint8Array of up to 64 bytes on its heap until a DataView asks for its
// ArrayBuffer. A function per width keeps each small enough to inline, and
// checks inline, building its error out of line.
//
// TurboFan inlines up to 920 bytes of bytecode into a function, and the
// serializers that tafla-gen generates spend them on these writes, their
// table's checks and their string. So each copy checks its offset itself,
// rather than through within(), which V8 would inline into each write as
// well, and the copy of 8 bytes is a loop over its two 32-bit words, 25%
// less bytecode than 8 copies, which V8 unrolls, as it unrolls any loop of
// fewer than 4 iterations. 100 writeFloat64()s took 376 ns rather than
// 394 in Node 26, and 338 ns rather than 349 in Node 27, where a loop over
// the 8 bytes took 28% to 30% longer than 8 copies.

const scratch = new DataView(new ArrayBuffer(8));
const scratchBytes = new Uint8Array(scratch.buffer);

export function writeInt8(
    bytes: Uint8Array,
    offset: number,
    value: number
): void {
    scratch.setInt8(0, value);
    copy1(bytes, offset);
}

export function writeUint8(
    bytes: Uint8Array,
    offset: number,
    value: number
): void {
    scratch.setUint8(0, value);
    copy1(bytes, offset);
}

export function writeInt16(
    bytes: Uint8Array,
    offset: number,
    value: number
): void {
    scratch.setInt16(0, value, true);
    copy2(bytes, offset);
}

export function writeUint16(
    bytes: Uint8Array,
    offset: number,
    value: number
): void {
    scratch.setUint16(0, value, true);
    copy2(bytes, offset);
}

export function writeInt32(
    bytes: Uint8Array,
    offset: number,
    value: number
): void {
    scratch.setInt32(0, value, true);
    copy4(bytes, offset);
}

export function writeUint32(
    bytes: Uint8Array,
    offset: number,
    value: number
): void {
    scratch.setUint32(0, value, true);
    copy4(bytes, offset);
}

export function writeInt64(
    bytes: Uint8Array,
    offset: number,
    value: bigint
): void {
    scratch.setBigInt64(0, value, true);
    copy8(bytes, offset);
}

export function writeUint64(
    bytes: Uint8Array,
    offset: number,
    value: bigint
): void {
    scratch.setBigUint64(0, value, true);
    copy8(bytes, offset);
}

export function writeFloat32(
    bytes: Uint8Array,
    offset: number,
    value: number
): void {
    scratch.setFloat32(0, value, true);
    copy4(bytes, offset);
}

export function writeFloat64(
    bytes: Uint8Array,
    offset: number,
    value: number
): void {
    scratch.setFloat64(0, value, true);
    copy8(bytes, offset);
}

function copy1(bytes: Uint8Array, offset: number): void {
    if (!(isOffset(offset) && offset <= bytes.length - 1)) {
        throw writeOutOfRange(offset, 1, bytes.length);
    }

    bytes[offset] = scratchBytes[0];
}

function copy2(bytes: Uint8Array, offset: number): void {
    if (!(isOffset(offset) && offset <= bytes.length - 2)) {
        throw writeOutOfRange(offset, 2, bytes.length);
    }

    bytes[offset] = scratchBytes[0];
    bytes[offset + 1] = scratchBytes[1];
}

function copy4(bytes: Uint8Array, offset: number): void {
    if (!(isOffset(offset) && offset <= bytes.length - 4)) {
        throw writeOutOfRange(offset, 4, bytes.length);
    }

    bytes[offset] = scratchBytes[0];
    bytes[offset + 1] = scratchBytes[1];
    bytes[offset + 2] = scratchBytes[2];
    bytes[offset + 3] = scratchBytes[3];
}

function copy8(bytes: Uint8Array, offset: number): void {
    if (!(isOffset(offset) && offset <= bytes.length - 8)) {
        throw copy8Error(bytes, offset);
    }

    for (let i = 0; i < 8; i += 4) {
        const word = scratch.getInt32(i, true);
        const position = offset + i;
        bytes[position] = word;
        bytes[position + 1] = word >> 8;
        bytes[position + 2] = word >> 16;
        bytes[position + 3] = word >> 24;
    }
}

/** copy8()'s error, built with two arguments, which take less bytecode */
function copy8Error(bytes: Uint8Array, offset: number): RangeError {
    return writeOutOfRange(offset, 8, bytes.length);
}

/**
 * Writes `string` as UTF-8 at `offset` in `bytes`, with lone surrogates
 * replaced by U+FFFD, as a table's string holds it after its length, and
 * returns the number of bytes written, which `utf8Length()` counts first.
 * It's for the code that tafla-gen generates, which writes the string after
 * a message's fields of fixed size itself.
 *
 * @throws {RangeError} If the string doesn't lie within `bytes`, having
 * written what fits of it when it isn't a short ASCII string.
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
