import {
    IS_LITTLE_ENDIAN,
    SIZEOF_8,
    SIZEOF_16,
    SIZEOF_32,
    SIZEOF_64
} from './constants.ts';
import { decode } from './encoding.ts';
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
        return (
            tablePosition + this.#getSize(tablePosition + index * this.#size)
        );
    }

    // The scalar getters let the DataView check the range, which is free,
    // and rethrow its error. A check of their own made reading 27,028 floats
    // one at a time 2.7 times slower in d8 and Node.

    getInt8(position: number): number {
        try {
            return this.#view.getInt8(position);
        } catch {
            throw outOfRange(position, SIZEOF_8, this.#byteLength());
        }
    }

    getUint8(position: number): number {
        try {
            return this.#view.getUint8(position);
        } catch {
            throw outOfRange(position, SIZEOF_8, this.#byteLength());
        }
    }

    getInt16(position: number): number {
        try {
            return this.#view.getInt16(position, true);
        } catch {
            throw outOfRange(position, SIZEOF_16, this.#byteLength());
        }
    }

    getUint16(position: number): number {
        try {
            return this.#view.getUint16(position, true);
        } catch {
            throw outOfRange(position, SIZEOF_16, this.#byteLength());
        }
    }

    getInt32(position: number): number {
        try {
            return this.#view.getInt32(position, true);
        } catch {
            throw outOfRange(position, SIZEOF_32, this.#byteLength());
        }
    }

    getUint32(position: number): number {
        try {
            return this.#view.getUint32(position, true);
        } catch {
            throw outOfRange(position, SIZEOF_32, this.#byteLength());
        }
    }

    getInt64(position: number): bigint {
        try {
            return this.#view.getBigInt64(position, true);
        } catch {
            throw outOfRange(position, SIZEOF_64, this.#byteLength());
        }
    }

    getUint64(position: number): bigint {
        try {
            return this.#view.getBigUint64(position, true);
        } catch {
            throw outOfRange(position, SIZEOF_64, this.#byteLength());
        }
    }

    getFloat32(position: number): number {
        try {
            return this.#view.getFloat32(position, true);
        } catch {
            throw outOfRange(position, SIZEOF_32, this.#byteLength());
        }
    }

    getFloat64(position: number): number {
        try {
            return this.#view.getFloat64(position, true);
        } catch {
            throw outOfRange(position, SIZEOF_64, this.#byteLength());
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
        const bytes = new Uint8Array(
            this.#view.buffer,
            this.#arrayOffset(position, length),
            length
        );

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
        const count = this.#getSize(position);
        this.#assertRange(position + this.#size, count * this.#size);

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
        const count = this.getVectorLength(position);
        if (!(Number.isInteger(index) && index >= 0 && index < count)) {
            throw new RangeError(
                `Item ${index} is outside the vector of ${count} at ${position}`
            );
        }

        const start =
            index === 0
                ? (count + 1) * this.#size
                : this.#getSize(position + index * this.#size);
        const end = this.#getSize(position + (index + 1) * this.#size);

        return this.#itemOffset(position, start, end);
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
     * position, starts no later than it ends and lies within the bytes.
     *
     * @returns The item's position.
     */
    #itemOffset(position: number, start: number, end: number): number {
        if (start > end) {
            throw new DecodeError(
                `The vector item at ${position + start} ends before it starts, at ${position + end}`
            );
        }

        this.#assertRange(position + start, end - start);
        return position + start;
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
     * The number of bytes that reads may use: the view's, or none once its
     * buffer is detached or has shrunk below it, when DataView throws a
     * `TypeError` for its length as for every read
     */
    #byteLength(): number {
        try {
            return this.#view.byteLength;
        } catch {
            return 0;
        }
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

    #getSize(position: number): number {
        switch (this.#size) {
            case SIZEOF_8:
                return this.getUint8(position);

            case SIZEOF_16:
                return this.getUint16(position);

            case SIZEOF_32:
                return this.getUint32(position);
        }
    }
}

export function assertStructSize(size: number): void {
    if (!(Number.isInteger(size) && size > 0)) {
        throw new RangeError(
            `A struct's size must be a positive integer, not ${size}`
        );
    }
}
