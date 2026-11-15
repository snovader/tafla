import {
    IS_LITTLE_ENDIAN,
    SIZEOF_8,
    SIZEOF_16,
    SIZEOF_32,
    SIZEOF_64
} from './constants.ts';
import { decode, decodeShortAscii } from './encoding.ts';
import { byteOrderSwapped } from './endianness.ts';
import { DecodeError, outOfRange } from './errors.ts';
import { TableLayout } from './layout.ts';
import type { OffsetSize } from './types.ts';

/**
 * Reads a table within the supplied byte slice, in the table layout it was
 * created with, whose multi-byte values are little-endian. Array reads may
 * share the slice's storage when aligned; unaligned reads return a copy. On a
 * big-endian host, arrays of multi-byte elements are always copies, converted
 * to the host's byte order.
 *
 * Every read checks that it stays within the slice, even when the backing
 * allocation has more bytes, and throws a {@link DecodeError} otherwise.
 * Once the slice's buffer is detached, every read throws one, and so does
 * every read of a slice of fixed length once its resizable buffer has shrunk
 * below it. A slice made over a resizable buffer without a length tracks the
 * buffer's length: once the buffer is resized, a read returns what the bytes
 * then hold or throws a `DecodeError`, and never reads outside them.
 *
 * Tables are written with `TableBuilder`, whose `finalize()` returns a reader
 * of its table, or with `TableSizer` and `TableWriter`.
 */
export class ByteBuffer {
    readonly #bytes: Uint8Array;
    readonly #view: DataView;

    readonly #sized: boolean;
    readonly #versioned: boolean;
    /** The offset size, the width of offsets, lengths and header slots */
    readonly #size: OffsetSize;
    /** The byte length of the header slots before the root's field offsets */
    readonly #headerSize: number;

    constructor(bytes: Uint8Array, layout: TableLayout = TableLayout.DEFAULT) {
        this.#bytes = bytes;
        this.#view = new DataView(
            bytes.buffer,
            bytes.byteOffset,
            bytes.byteLength
        );

        this.#sized = layout.sized();
        this.#versioned = layout.versioned();
        this.#size = layout.offsetSize();
        this.#headerSize = layout.headerSize();
    }

    bytes(): Uint8Array {
        return this.#bytes;
    }

    getSize(): number {
        return this.#sized ? this.#getSize(0) : this.#bytes.length;
    }

    /**
     * Returns the version that the table's header carries after its length.
     * It's as wide as the header's other slots.
     *
     * @throws {TypeError} If the buffer's layout isn't versioned.
     */
    getVersion(): number {
        if (!this.#versioned) {
            throw new TypeError('The table has no version');
        }

        return this.#getSize(this.#sized ? this.#size : 0);
    }

    /** Returns the position of field `index` of the root table */
    rootOffset(index: number): number {
        return this.#getSize(this.#headerSize + index * this.#size);
    }

    /**
     * Returns the position of field `index` of a nested table, which has no
     * header and stores its field offsets from its own start.
     *
     * @param tablePosition - The nested table's position, which is the
     * offset of the field that holds it.
     */
    fieldOffset(tablePosition: number, index: number): number {
        // Reads the offset as #getSize() does, without a call of its own,
        // which leaves more of V8's inlining budget to the getters that call
        // it: see getVectorItemOffset()
        const size = this.#size;
        const view = this.#view;
        const position = tablePosition + index * size;
        try {
            return (
                tablePosition +
                (size === 4
                    ? view.getUint32(position, true)
                    : readNarrowSize(view, position, size))
            );
        } catch {
            throw readError(position, size, this.#bytes);
        }
    }

    // The scalar getters let the DataView check the range, which is free,
    // and rethrow its error. A check of their own made reading 27,028 floats
    // one at a time 2.7 times slower in d8 and Node. They build the error in
    // readError(), out of their bytecode, which V8 inlines while it's small:
    // see getVectorItemOffset().

    getInt8(position: number): number {
        try {
            return this.#view.getInt8(position);
        } catch {
            throw readError(position, SIZEOF_8, this.#bytes);
        }
    }

    getUint8(position: number): number {
        try {
            return this.#view.getUint8(position);
        } catch {
            throw readError(position, SIZEOF_8, this.#bytes);
        }
    }

    getInt16(position: number): number {
        try {
            return this.#view.getInt16(position, true);
        } catch {
            throw readError(position, SIZEOF_16, this.#bytes);
        }
    }

    getUint16(position: number): number {
        try {
            return this.#view.getUint16(position, true);
        } catch {
            throw readError(position, SIZEOF_16, this.#bytes);
        }
    }

    getInt32(position: number): number {
        try {
            return this.#view.getInt32(position, true);
        } catch {
            throw readError(position, SIZEOF_32, this.#bytes);
        }
    }

    getUint32(position: number): number {
        try {
            return this.#view.getUint32(position, true);
        } catch {
            throw readError(position, SIZEOF_32, this.#bytes);
        }
    }

    getInt64(position: number): bigint {
        try {
            return this.#view.getBigInt64(position, true);
        } catch {
            throw readError(position, SIZEOF_64, this.#bytes);
        }
    }

    getUint64(position: number): bigint {
        try {
            return this.#view.getBigUint64(position, true);
        } catch {
            throw readError(position, SIZEOF_64, this.#bytes);
        }
    }

    getFloat32(position: number): number {
        try {
            return this.#view.getFloat32(position, true);
        } catch {
            throw readError(position, SIZEOF_32, this.#bytes);
        }
    }

    getFloat64(position: number): number {
        try {
            return this.#view.getFloat64(position, true);
        } catch {
            throw readError(position, SIZEOF_64, this.#bytes);
        }
    }

    // The array getters are written out per element type, as TableWriter's
    // setters are. The elements are stored little-endian, so a big-endian
    // host gets a copy in its own byte order; single bytes have no order to
    // swap.

    /**
     * Reads the array at `position`, as a view of the buffer.
     */
    getInt8Array(position: number): Int8Array {
        const length = this.#getSize(position);
        const byteOffset = this.#arrayOffset(position, length);

        return new Int8Array(this.#view.buffer, byteOffset, length);
    }

    /**
     * Reads the array at `position`, as a view of the buffer.
     */
    getUint8Array(position: number): Uint8Array {
        const length = this.#getSize(position);
        const byteOffset = this.#arrayOffset(position, length);

        return new Uint8Array(this.#view.buffer, byteOffset, length);
    }

    /**
     * Reads the array at `position`: a view of the buffer when its elements
     * are aligned, as TableBuilder aligns them, and otherwise a copy, as on
     * a big-endian host.
     */
    getInt16Array(position: number): Int16Array {
        const length = this.#getSize(position);
        const byteLength = length * SIZEOF_16;
        const byteOffset = this.#arrayOffset(position, byteLength);

        if (!IS_LITTLE_ENDIAN) {
            return byteOrderSwapped(
                Int16Array,
                this.#view.buffer,
                byteOffset,
                length
            );
        }

        return byteOffset % SIZEOF_16 === 0
            ? new Int16Array(this.#view.buffer, byteOffset, length)
            : new Int16Array(this.#copy(byteOffset, byteLength));
    }

    /**
     * Reads the array at `position`: a view of the buffer when its elements
     * are aligned, as TableBuilder aligns them, and otherwise a copy, as on
     * a big-endian host.
     */
    getUint16Array(position: number): Uint16Array {
        const length = this.#getSize(position);
        const byteLength = length * SIZEOF_16;
        const byteOffset = this.#arrayOffset(position, byteLength);

        if (!IS_LITTLE_ENDIAN) {
            return byteOrderSwapped(
                Uint16Array,
                this.#view.buffer,
                byteOffset,
                length
            );
        }

        return byteOffset % SIZEOF_16 === 0
            ? new Uint16Array(this.#view.buffer, byteOffset, length)
            : new Uint16Array(this.#copy(byteOffset, byteLength));
    }

    /**
     * Reads the array at `position`: a view of the buffer when its elements
     * are aligned, as TableBuilder aligns them, and otherwise a copy, as on
     * a big-endian host.
     */
    getInt32Array(position: number): Int32Array {
        const length = this.#getSize(position);
        const byteLength = length * SIZEOF_32;
        const byteOffset = this.#arrayOffset(position, byteLength);

        if (!IS_LITTLE_ENDIAN) {
            return byteOrderSwapped(
                Int32Array,
                this.#view.buffer,
                byteOffset,
                length
            );
        }

        return byteOffset % SIZEOF_32 === 0
            ? new Int32Array(this.#view.buffer, byteOffset, length)
            : new Int32Array(this.#copy(byteOffset, byteLength));
    }

    /**
     * Reads the array at `position`: a view of the buffer when its elements
     * are aligned, as TableBuilder aligns them, and otherwise a copy, as on
     * a big-endian host.
     */
    getUint32Array(position: number): Uint32Array {
        const length = this.#getSize(position);
        const byteLength = length * SIZEOF_32;
        const byteOffset = this.#arrayOffset(position, byteLength);

        if (!IS_LITTLE_ENDIAN) {
            return byteOrderSwapped(
                Uint32Array,
                this.#view.buffer,
                byteOffset,
                length
            );
        }

        return byteOffset % SIZEOF_32 === 0
            ? new Uint32Array(this.#view.buffer, byteOffset, length)
            : new Uint32Array(this.#copy(byteOffset, byteLength));
    }

    /**
     * Reads the array at `position`: a view of the buffer when its elements
     * are aligned, as TableBuilder aligns them, and otherwise a copy, as on
     * a big-endian host.
     */
    getInt64Array(position: number): BigInt64Array {
        const length = this.#getSize(position);
        const byteLength = length * SIZEOF_64;
        const byteOffset = this.#arrayOffset(position, byteLength);

        if (!IS_LITTLE_ENDIAN) {
            return byteOrderSwapped(
                BigInt64Array,
                this.#view.buffer,
                byteOffset,
                length
            );
        }

        return byteOffset % SIZEOF_64 === 0
            ? new BigInt64Array(this.#view.buffer, byteOffset, length)
            : new BigInt64Array(this.#copy(byteOffset, byteLength));
    }

    /**
     * Reads the array at `position`: a view of the buffer when its elements
     * are aligned, as TableBuilder aligns them, and otherwise a copy, as on
     * a big-endian host.
     */
    getUint64Array(position: number): BigUint64Array {
        const length = this.#getSize(position);
        const byteLength = length * SIZEOF_64;
        const byteOffset = this.#arrayOffset(position, byteLength);

        if (!IS_LITTLE_ENDIAN) {
            return byteOrderSwapped(
                BigUint64Array,
                this.#view.buffer,
                byteOffset,
                length
            );
        }

        return byteOffset % SIZEOF_64 === 0
            ? new BigUint64Array(this.#view.buffer, byteOffset, length)
            : new BigUint64Array(this.#copy(byteOffset, byteLength));
    }

    /**
     * Reads the array at `position`: a view of the buffer when its elements
     * are aligned, as TableBuilder aligns them, and otherwise a copy, as on
     * a big-endian host.
     */
    getFloat32Array(position: number): Float32Array {
        const length = this.#getSize(position);
        const byteLength = length * SIZEOF_32;
        const byteOffset = this.#arrayOffset(position, byteLength);

        if (!IS_LITTLE_ENDIAN) {
            return byteOrderSwapped(
                Float32Array,
                this.#view.buffer,
                byteOffset,
                length
            );
        }

        return byteOffset % SIZEOF_32 === 0
            ? new Float32Array(this.#view.buffer, byteOffset, length)
            : new Float32Array(this.#copy(byteOffset, byteLength));
    }

    /**
     * Reads the array at `position`: a view of the buffer when its elements
     * are aligned, as TableBuilder aligns them, and otherwise a copy, as on
     * a big-endian host.
     */
    getFloat64Array(position: number): Float64Array {
        const length = this.#getSize(position);
        const byteLength = length * SIZEOF_64;
        const byteOffset = this.#arrayOffset(position, byteLength);

        if (!IS_LITTLE_ENDIAN) {
            return byteOrderSwapped(
                Float64Array,
                this.#view.buffer,
                byteOffset,
                length
            );
        }

        return byteOffset % SIZEOF_64 === 0
            ? new Float64Array(this.#view.buffer, byteOffset, length)
            : new Float64Array(this.#copy(byteOffset, byteLength));
    }

    /**
     * Checks that `byteLength` bytes of elements follow the length prefix at
     * `position`.
     *
     * @returns The byte offset of the elements in the backing buffer.
     */
    #arrayOffset(position: number, byteLength: number): number {
        this.#assertRange(position + this.#size, byteLength);
        return this.#view.byteOffset + position + this.#size;
    }

    /** A typed array can't start at an unaligned offset, so copy its bytes */
    #copy(byteOffset: number, byteLength: number): ArrayBufferLike {
        return this.#view.buffer.slice(byteOffset, byteOffset + byteLength);
    }

    /**
     * Returns a view of the `size` bytes of the struct at `position`, whose
     * scalars lie at fixed offsets from its start
     */
    getStruct(position: number, size: number): Uint8Array {
        this.#assertRange(position, size);
        // A struct of no bytes passes the range check of a buffer that has
        // lost the slice, which would then give an empty view or a TypeError
        if (size === 0 && this.#lost()) {
            throw outOfRange(position, size, 0);
        }

        return this.#bytes.subarray(position, position + size);
    }

    /**
     * Returns a view of the structs of the array at `position`, each of
     * `size` bytes, one after another after the count. Their number is the
     * view's length over `size`.
     *
     * @throws {RangeError} If `size` isn't a positive integer.
     */
    getStructArray(position: number, size: number): Uint8Array {
        assertStructSize(size);

        const count = this.#getSize(position);
        const start = position + this.#size;
        this.#assertRange(start, count * size);

        return this.#bytes.subarray(start, start + count * size);
    }

    /** @throws {DecodeError} If the string isn't valid UTF-8. */
    getString(position: number): string {
        const length = this.#getSize(position);
        const byteOffset = this.#arrayOffset(position, length);

        // A short ASCII string is read a byte at a time, within the range
        // that #arrayOffset() checked, and any other by the fatal decoder.
        // The loop is a function of its own, so that this one takes 173
        // bytes of bytecode rather than 250. In Node 26 and 27, V8 inlines
        // neither into a reader of other fields too, such as the comparison
        // suite's record, which still inlines its other reads. This one then
        // inlines #arrayOffset() and the loop, where the record used to
        // inline this, at 135 bytes, and call #arrayOffset().
        const ascii = decodeShortAscii(
            this.#bytes,
            position + this.#size,
            length
        );
        if (ascii !== undefined) {
            return ascii;
        }

        const bytes = new Uint8Array(this.#view.buffer, byteOffset, length);
        try {
            return decode(bytes);
        } catch (error) {
            throw new DecodeError(
                `The string at ${position} isn't valid UTF-8`,
                { cause: error }
            );
        }
    }

    /**
     * Returns the number of items of the vector at `position`. Every item
     * has an end, so a count that the bytes can't hold fails before anything
     * is allocated for it.
     *
     * @throws {DecodeError} If the count or the ends after it don't lie within
     * the bytes.
     */
    getVectorLength(position: number): number {
        // Reads the count as #getSize() does, and checks its ends with one
        // comparison rather than #assertRange(), so that V8 inlines it with
        // the item reads: see getVectorItemOffset()
        const size = this.#size;
        const view = this.#view;
        let count: number;
        try {
            count =
                size === 4
                    ? view.getUint32(position, true)
                    : readNarrowSize(view, position, size);
        } catch {
            throw readError(position, size, this.#bytes);
        }

        if (!(position + (count + 1) * size <= this.#bytes.length)) {
            throw readError(position + size, count * size, this.#bytes);
        }

        return count;
    }

    /**
     * Returns the position of item `index` of the vector at `position`. It's
     * read from the ends of that item and the one before it, without reading
     * any other item.
     *
     * @throws {DecodeError} If the count or the ends after it don't lie within
     * the bytes, which is checked before `index`, or the item doesn't.
     * @throws {RangeError} If the vector has no item `index`.
     */
    getVectorOffset(position: number, index: number): number {
        return this.getVectorItemOffset(
            position,
            this.getVectorLength(position),
            index
        );
    }

    /**
     * Returns the position of item `index` of the vector at `position`, as
     * `getVectorOffset()` does, from the vector's `count` that the caller has
     * already read with `getVectorLength()`, so that reading many items, as
     * a view of the vector does, reads and checks the count once.
     *
     * The count must be the vector's: another count reads other bytes as the
     * item's ends, which, as corrupted ends do, give a position within the
     * bytes or throw a `DecodeError`.
     *
     * @throws {RangeError} If `index` isn't an integer in [0, `count`).
     * @throws {DecodeError} If the item's ends or the item don't lie within
     * the bytes, or the item ends before it starts.
     */
    getVectorItemOffset(
        position: number,
        count: number,
        index: number
    ): number {
        // V8 inlines calls into an optimized function until the bytecode
        // inlined into it reaches a budget, 920 bytes in V8 15, and counts a
        // call as its own bytecode and what its optimized code inlined. A
        // view's get(i) and the getters of the item it returns share that
        // budget, so this body keeps every failure out of line: it reads the
        // item's ends in one try, and #itemError() finds which check failed.
        // It takes 219 bytes and, in a process of uint32 offsets, inlines
        // nothing more, where reading the ends through #getSize() and
        // #itemOffset() took 189 and inlined 452. A loop that sums the items
        // of a vector through get(i) then inlines every read, and the
        // comparison suite's view summed 100 nested tables in 0.91 µs rather
        // than 1.85 in Node 27.
        //
        // An index that equals its unsigned 32-bit conversion is an integer
        // in [0, 2^32), which holds every count. A vector's position, as
        // rootOffset() and fieldOffset() find it, its count and the ends are
        // integers, so the item lies within the bytes when it ends there.
        if (index >>> 0 === index && index < count) {
            const size = this.#size;
            const view = this.#view;
            const startPosition = position + index * size;
            const endPosition = startPosition + size;
            try {
                const start =
                    index === 0
                        ? (count + 1) * size
                        : size === 4
                          ? view.getUint32(startPosition, true)
                          : readNarrowSize(view, startPosition, size);
                const end =
                    size === 4
                        ? view.getUint32(endPosition, true)
                        : readNarrowSize(view, endPosition, size);

                if (start <= end && position + end <= this.#bytes.length) {
                    return position + start;
                }
            } catch {
                // #itemError() reads the ends again, and throws their error
            }
        }

        throw this.#itemError(position, count, index);
    }

    /**
     * Returns the position of every item of the vector at `position`, as
     * `getVectorOffset()` finds each.
     *
     * The positions are numbers, as `getVectorOffset()` returns, rather than
     * a `Uint32Array`: a vector in a larger buffer, such as one of many
     * tables stored one after another, can lie past 4 GiB, although its
     * items' offsets from it fit in 32 bits.
     *
     * @throws {DecodeError} If the count, the ends or the items don't lie
     * within the bytes, or an item ends before it starts.
     */
    getVectorOffsets(position: number): number[] {
        // Checking the ends bounds the count before allocating for it
        const count = this.getVectorLength(position);
        // Filling an array of the count's length reads as fast as a
        // Uint32Array, where push() and a Float64Array measured slower
        const offsets = new Array<number>(count);

        let start = (count + 1) * this.#size;
        for (let i = 0; i < count; i++) {
            const end = this.#getSize(position + (i + 1) * this.#size);
            offsets[i] = this.#itemOffset(position, start, end);
            start = end;
        }

        return offsets;
    }

    getVector(position: number): Uint8Array[] {
        const count = this.getVectorLength(position);
        const vector = new Array<Uint8Array>(count);

        let start = (count + 1) * this.#size;
        for (let i = 0; i < count; i++) {
            const end = this.#getSize(position + (i + 1) * this.#size);
            const offset = this.#itemOffset(position, start, end);
            vector[i] = this.#bytes.subarray(offset, offset + end - start);
            start = end;
        }

        return vector;
    }

    /**
     * Checks that a vector item, from `start` to `end` from the vector's
     * position, starts no later than it ends and lies within the bytes. The
     * vector's position and the ends are integers, as in
     * getVectorItemOffset(), so the item lies within the bytes when it ends
     * there.
     *
     * @returns The item's position.
     */
    #itemOffset(position: number, start: number, end: number): number {
        if (start > end || position + end > this.#bytes.length) {
            throw itemRangeError(position, start, end, this.#bytes.length);
        }

        return position + start;
    }

    /**
     * Finds why `getVectorItemOffset()` couldn't read item `index`, by its
     * checks one at a time, and returns the error to throw. An end that can't
     * be read throws its `DecodeError` from here.
     */
    #itemError(position: number, count: number, index: number): Error {
        if (!(index >>> 0 === index && index < count)) {
            return new RangeError(
                `Item ${index} is outside the vector of ${count} at ${position}`
            );
        }

        const start =
            index === 0
                ? (count + 1) * this.#size
                : this.#getSize(position + index * this.#size);
        const end = this.#getSize(position + (index + 1) * this.#size);

        return itemRangeError(position, start, end, this.#bytes.length);
    }

    #assertRange(position: number, byteLength: number): void {
        const available = this.#byteLength();
        if (
            !Number.isSafeInteger(position) ||
            position < 0 ||
            !Number.isSafeInteger(byteLength) ||
            byteLength < 0 ||
            position > available - byteLength
        ) {
            throw outOfRange(position, byteLength, available);
        }
    }

    /**
     * The number of bytes that reads may use: the slice's length, which is
     * 0 once its buffer is detached or has shrunk below it. A view that
     * tracks a resizable buffer's length follows the buffer, where the
     * DataView keeps the slice's length when the reader was made.
     *
     * The DataView's `byteLength` getter, which throws in those cases, took
     * 3-4 ns a read against 1 ns for the Uint8Array's length. Reading the
     * vector of 100 nested tables of the comparison suite took 1.33 µs
     * rather than 1.64 µs in Node 27, and 1.32 µs rather than 1.63 µs in
     * Node 26. d8 15.7 took 1.31-1.33 µs with the DataView's, and 2-4% longer
     * with the Uint8Array's.
     */
    #byteLength(): number {
        return this.#bytes.length;
    }

    /**
     * Whether the buffer is detached or has shrunk below the slice, when
     * DataView throws a `TypeError` for its length
     */
    #lost(): boolean {
        try {
            void this.#view.byteLength;
            return false;
        } catch {
            return true;
        }
    }

    /**
     * Reads the offset, count, length or item end at `position`. A uint32 is
     * read inline, and narrower ones by readNarrowSize(), which a process
     * that reads only uint32 offsets never compiles into its reads.
     *
     * The sizes are literals rather than SIZEOF_32 and SIZEOF_16, which V8
     * loads from their module and checks for initialization at every read:
     * the loop that sums the vector of 100 nested tables compiled to 877
     * instructions rather than 969.
     *
     * fieldOffset(), getVectorLength() and getVectorItemOffset() read their
     * offsets as this does, for V8's inlining budget. A switch that called
     * getUint8(), getUint16() or getUint32() took 133 bytes of bytecode at
     * every read. A reader function of each offset size, which a buffer held,
     * took 69, but V8 inlines none of three functions called from one place:
     * a process that had read all three offset types summed the vector of
     * 100 nested tables in 2.90 µs, rather than 1.79 with the switch and 0.72
     * as this reads.
     */
    #getSize(position: number): number {
        const size = this.#size;
        const view = this.#view;
        try {
            return size === 4
                ? view.getUint32(position, true)
                : readNarrowSize(view, position, size);
        } catch {
            throw readError(position, size, this.#bytes);
        }
    }
}

/** Reads an offset of a layout whose offsets are narrower than uint32's */
function readNarrowSize(
    view: DataView,
    position: number,
    size: OffsetSize
): number {
    return size === 2
        ? view.getUint16(position, true)
        : view.getUint8(position);
}

/**
 * Builds the error of a read outside `bytes`. Its own function keeps it out
 * of the getters' bytecode, which V8 inlines while it's small.
 */
function readError(
    position: number,
    byteLength: number,
    bytes: Uint8Array
): DecodeError {
    return outOfRange(position, byteLength, bytes.length);
}

/**
 * Builds the error of a vector item, from `start` to `end` from the vector's
 * position, that ends before it starts or past the bytes
 */
function itemRangeError(
    position: number,
    start: number,
    end: number,
    available: number
): DecodeError {
    return start > end
        ? new DecodeError(
              `The vector item at ${position + start} ends before it starts, at ${position + end}`
          )
        : outOfRange(position + start, end - start, available);
}

export function assertStructSize(size: number): void {
    if (!(Number.isInteger(size) && size > 0)) {
        throw new RangeError(
            `A struct's size must be a positive integer, not ${size}`
        );
    }
}
