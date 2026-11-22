import { SIZEOF_8, SIZEOF_16, SIZEOF_32, SIZEOF_64 } from './constants.ts';
import {
    setFloat32Elements,
    setFloat64Elements,
    setInt8Elements,
    setInt16Elements,
    setInt32Elements,
    setInt64Elements,
    setUint8Elements,
    setUint16Elements,
    setUint32Elements,
    setUint64Elements
} from './elements.ts';
import { encodeInto, utf8Length } from './encoding.ts';
import {
    assertAlignment,
    assertCount,
    headerLength,
    maxLength,
    nestedHeaderLength,
    padding,
    TableLayout
} from './layout.ts';
import { TableReader } from './table-reader.ts';
import { setSlot, structCount } from './table-writer.ts';
import type { ElementSize, OffsetSize, OffsetType } from './types.ts';
/** The shared buffer's length at first */
const FIRST_SHARED = 4096;
/**
 * The bytes that a builder takes at first for each field, besides its
 * header, so that a table of short fields seldom grows. A setter that
 * grows in most of its calls, as a record's one string did with 40 bytes
 * for the whole table, has the growth inlined into it by V8, and is then
 * too large to be inlined into a loop that calls it: 100 short strings took
 * 1.3 times as long after such a record was built.
 */
const FIELD_ROOM = 16;
/**
 * The longest shared buffer, which is kept between builders once a table
 * needed it. A builder that needs more writes in a buffer of its own, which
 * isn't kept. With 64 KiB, tables past it grew a buffer of their own in
 * every build: 130 columns of 500 bytes took 1.35–1.45 times as long as
 * from the builder that recorded its fields, and 0.94–0.95 with 1 MiB.
 */
const SHARED_BYTES = 1024 * 1024;
/**
 * The length that a shared buffer is doubled to when it's replaced because
 * it's full, rather than for a table that needs more. Nested builders keep
 * their bytes, so a program that nests many fills it again and again.
 * Replacing it with one of 4 KiB each time took 1.07–1.46 times as long for
 * vectors and chains of nested builders.
 */
const DOUBLED_BYTES = 64 * 1024;
/**
 * The shortest run of bytes, of an array, an array of structs, a struct or a
 * table nested as bytes, that's kept as a segment, which `bytes()` writes
 * once, into the table, rather than copied into the buffer when it's set.
 * Copying every run into the buffer made a document of 100,000 rows take
 * four times as long.
 */
const SEGMENT_BYTES = 1024;
/**
 * The shortest table of a complete nested builder that's kept as a segment,
 * and written once, by `bytes()`, rather than copied into its parent. It's
 * no longer than {@link SEGMENT_BYTES}, so a table that kept segments of its
 * own is kept in turn, and tables nested in tables are each written once,
 * rather than copied into each parent. Keeping them from 128 bytes, rather
 * than 256, took 0.92 of the time for 32 builders of a uint32, each nested
 * in the next, and from 64 bytes the same.
 */
const KEPT_TABLE_BYTES = 128;

/**
 * The buffer that builders write their tables in, with its view: each takes
 * the bytes after the last one's, and `bytes()` copies its table out of
 * them, so no table holds the buffer. Builders write every byte of their
 * tables, padding included, so what the buffer held before isn't read.
 *
 * It's kept between builders. A `Uint8Array` of more than 64 bytes is
 * allocated outside V8's heap, in about 150 ns, so a buffer for each builder
 * made a document of 100 rows take 1.2 times as long as recording its fields
 * did. One buffer that only one builder at a time could hold, as this
 * builder first had, made every other builder that was being written
 * allocate its own: a record took 1.2 times as long from two builders
 * written at once as it did when its fields were recorded, and 32 builders,
 * each nested in the next, 3 times as long.
 *
 * A builder's bytes are its own until it's written: it gives them back
 * then, and when it moves to more. A nested builder keeps its bytes, since
 * it can still be written or nested again, so the buffer is replaced when
 * it's full while builders hold bytes of it, and the old one lives as long
 * as they do.
 */
let shared: Uint8Array<ArrayBuffer> = new Uint8Array(0);
let sharedView = new DataView(shared.buffer);
/** Where the shared buffer's bytes that no builder holds start */
let top = 0;
/** The builders that hold bytes of the shared buffer, and aren't written */
let live = 0;

/** Replaces the shared buffer with one of at least `length` bytes */
function replaceShared(length: number): void {
    const doubled = Math.min(2 * shared.length, DOUBLED_BYTES);

    shared = new Uint8Array(Math.max(length, doubled, FIRST_SHARED));
    sharedView = new DataView(shared.buffer);
    top = 0;
    live = 0;
}

// The kinds of segment, which bytes() switches on to write each
const INT8_ARRAY = 0;
const UINT8_ARRAY = 1;
const INT16_ARRAY = 2;
const UINT16_ARRAY = 3;
const INT32_ARRAY = 4;
const UINT32_ARRAY = 5;
const INT64_ARRAY = 6;
const UINT64_ARRAY = 7;
const FLOAT32_ARRAY = 8;
const FLOAT64_ARRAY = 9;
/** Bytes copied as they are: a struct, an array of structs or a table */
const BYTES = 10;
/** A nested builder's table, a {@link NestedTable} */
const TABLE = 11;

/**
 * The slots of a segment in `#segments`: the position in the buffer that its
 * bytes go before, their length, its kind, its values and their count
 */
const SEGMENT_SLOTS = 5;

/**
 * A complete builder's table, kept as a segment of its parent: the bytes
 * that its buffer holds, and its own segments. A complete builder doesn't
 * write its buffer again, nor leave it for another builder once it's kept.
 */
interface NestedTable {
    readonly bytes: Uint8Array;
    readonly base: number;
    readonly end: number;
    readonly segments: unknown[] | undefined;
}

/**
 * The fields of a table that a {@link TableBuilder} writes in place, which
 * its `setTableInPlace()` and `setVectorInPlace()` give to `fill`: the
 * builder itself, without `bytes()` and `finalize()`, whose setters set the
 * nested table's fields while `fill` runs.
 */
export type TableItem = Omit<TableBuilder, 'bytes' | 'finalize'>;

/**
 * Copies `length` bytes from `from` in one view to `at` in another, four at
 * a time, for a short nested table: a vector of 100 tables of three float32s
 * from their builders took 0.83 of the time that it did a byte at a time
 */
function copyWords(
    source: DataView,
    from: number,
    target: DataView,
    at: number,
    length: number
): void {
    let i = 0;
    for (; i + SIZEOF_32 <= length; i += SIZEOF_32) {
        target.setUint32(at + i, source.getUint32(from + i, true), true);
    }
    for (; i < length; i++) {
        target.setUint8(at + i, source.getUint8(from + i));
    }
}

const EMPTY = new Uint8Array(0);
const EMPTY_VIEW = new DataView(EMPTY.buffer);

/**
 * Builds a table: a header with one offset per field, after the table's total
 * length and version when its layout has them, followed by the fields in the
 * order they're set. Arrays, strings and vectors start with their count, and a
 * vector then holds the end of each item. Arrays of scalars and of structs are
 * padded so their elements are aligned from the start of the table. Fields
 * are read back by index with `TableReader.rootOffset()`, or
 * `TableReader.fieldOffset()` for a table nested in another.
 *
 * Setting a field writes it, and its offset, into the builder's bytes of a
 * buffer that every builder shares, each with bytes of its own.
 * `finalize()` and `bytes()` then copy the table out of them, into bytes of
 * exactly its length, and give the builder's back, so that building tables
 * allocates little more than their bytes. The buffer is kept between
 * builders, at up to 1 MiB once a table needed that much; a longer table is
 * written in a buffer of its own.
 * Runs of 1 KiB or more, of arrays, arrays of structs, structs and tables
 * given as bytes, are kept until then instead, and written once, straight
 * into the table's bytes, so they must not change until it's written: an
 * array whose length has changed by then makes `finalize()` and `bytes()`
 * throw a `RangeError`, and leaves the builder unfinalized. Shorter values
 * are copied when they're set.
 *
 * A table nested with `setTable()` or `setVector()` is copied in when it's
 * set, from its bytes or its builder's buffer. A builder whose table is 128
 * bytes or longer is kept instead, and its table written once, by
 * `bytes()`, rather than copied into each table that nests it in turn: a
 * builder doesn't change once its fields are set. `setTableInPlace()` and
 * `setVectorInPlace()` write a nested table, or each item of a vector,
 * among this builder's own fields instead, without a builder of its own, and
 * write the same bytes as nested builders without copying them.
 *
 * A setter throws a `RangeError`, and leaves the builder as it was, when its
 * field would make the table longer than the layout's `offsetType`
 * addresses: 255 bytes, 64 KiB or 4 GiB. Typed arrays of a field's element
 * type are copied as bytes; any other array-like, such as a `number[]` or a
 * typed array of another type, is converted as the typed array's constructor
 * would.
 */
export class TableBuilder {
    /**
     * The table once written: its bytes, and then the reader of them that
     * `finalize()` returns
     */
    #written: Uint8Array | TableReader | undefined;

    readonly #layout: TableLayout;
    /** The offset size, the width of offsets, lengths and header slots */
    readonly #size: OffsetSize;
    /** The longest table that the layout's offsets and lengths address */
    readonly #maxLength: number;

    /** The buffer that holds the builder's bytes: the shared one, or its own */
    #bytes: Uint8Array<ArrayBuffer> = EMPTY;
    #view: DataView = EMPTY_VIEW;
    /**
     * Where the builder's bytes start in the buffer, which every position
     * below counts from, so that they stay valid when the bytes move
     */
    #base = 0;
    /** The builder's bytes in the buffer, written or not */
    #capacity = 0;
    /** The bytes of the buffer written so far */
    #end = 0;
    /**
     * The bytes of the segments so far, by which the table's positions after
     * them are ahead of the buffer's
     */
    #shift = 0;
    /** The segments, {@link SEGMENT_SLOTS} slots each, once there are any */
    #segments: unknown[] | undefined;
    /** Whether a parent keeps the builder's bytes as a segment */
    #kept = false;

    // What the builder counts of the table whose fields it's writing: its
    // own, or, while a `fill` runs, those of the table that it writes in
    // place, until it restores its own
    #fields: number;
    /** The number of fields set so far */
    #count = 0;
    /** The table's start, in the table, which its offsets count from */
    #table = 0;
    /** The position of the table's first offset in the buffer */
    #slots: number;
    /**
     * The largest alignment that a field needs from the table's start, which
     * a parent pads this table to when it nests it
     */
    #alignment: ElementSize = SIZEOF_8;
    /** The number of tables being written in place, one in another */
    #nesting = 0;

    /**
     * @param fields - The number of fields that the table will hold.
     * @param layout - The table's header and offset type, which its readers
     * must use too, with the version to write to the header when it has one.
     * @throws {RangeError} If `fields` isn't a non-negative integer, or the
     * header alone is longer than the layout addresses.
     */
    constructor(fields: number, layout: TableLayout = TableLayout.DEFAULT) {
        const size = layout.offsetSize();
        const header = headerLength(fields, layout);

        this.#layout = layout;
        this.#size = size;
        this.#maxLength = maxLength(size);
        this.#fields = fields;

        this.#take(header + fields * FIELD_ROOM);

        // The length, when the layout is sized, is written by bytes(), and
        // the offsets as the fields are set
        const slots = layout.headerSize();
        const version = layout.version();
        if (version !== undefined) {
            this.#setUnsigned(slots - size, version);
        }

        this.#slots = slots;
        this.#end = header;
    }

    /**
     * Writes the table, once, and returns a `TableReader` that reads it; later
     * calls return the same reader.
     *
     * @throws {RangeError} If fewer fields were set than the table holds, or
     * an array kept until now has changed its length.
     */
    finalize(): TableReader {
        const written = this.#written ?? this.#write();
        if (written instanceof TableReader) {
            return written;
        }

        const reader = new TableReader(written, this.#layout);
        this.#written = reader;

        return reader;
    }

    /**
     * Writes the table, once, as `finalize()` does, and returns its bytes,
     * which `finalize()` then returns a reader of
     *
     * @throws {RangeError} If fewer fields were set than the table holds, or
     * an array kept until now has changed its length.
     */
    bytes(): Uint8Array {
        const written = this.#written ?? this.#write();
        return written instanceof TableReader ? written.bytes() : written;
    }

    // Each scalar is checked and given room before it's written, and its
    // offset written once it is, so that a value that throws, such as a
    // number for an int64, leaves the builder as it was

    setInt8(value: number): void {
        const index = this.#check(0, SIZEOF_8);
        const at = this.#reserve(SIZEOF_8);

        this.#view.setInt8(this.#base + at, value);

        this.#endScalar(index, at, SIZEOF_8);
    }

    setUint8(value: number): void {
        const index = this.#check(0, SIZEOF_8);
        const at = this.#reserve(SIZEOF_8);

        this.#view.setUint8(this.#base + at, value);

        this.#endScalar(index, at, SIZEOF_8);
    }

    setInt16(value: number): void {
        const index = this.#check(0, SIZEOF_16);
        const at = this.#reserve(SIZEOF_16);

        this.#view.setInt16(this.#base + at, value, true);

        this.#endScalar(index, at, SIZEOF_16);
    }

    setUint16(value: number): void {
        const index = this.#check(0, SIZEOF_16);
        const at = this.#reserve(SIZEOF_16);

        this.#view.setUint16(this.#base + at, value, true);

        this.#endScalar(index, at, SIZEOF_16);
    }

    setInt32(value: number): void {
        const index = this.#check(0, SIZEOF_32);
        const at = this.#reserve(SIZEOF_32);

        this.#view.setInt32(this.#base + at, value, true);

        this.#endScalar(index, at, SIZEOF_32);
    }

    setUint32(value: number): void {
        const index = this.#check(0, SIZEOF_32);
        const at = this.#reserve(SIZEOF_32);

        this.#view.setUint32(this.#base + at, value, true);

        this.#endScalar(index, at, SIZEOF_32);
    }

    setInt64(value: bigint): void {
        const index = this.#check(0, SIZEOF_64);
        const at = this.#reserve(SIZEOF_64);

        this.#view.setBigInt64(this.#base + at, value, true);

        this.#endScalar(index, at, SIZEOF_64);
    }

    setUint64(value: bigint): void {
        const index = this.#check(0, SIZEOF_64);
        const at = this.#reserve(SIZEOF_64);

        this.#view.setBigUint64(this.#base + at, value, true);

        this.#endScalar(index, at, SIZEOF_64);
    }

    setFloat32(value: number): void {
        const index = this.#check(0, SIZEOF_32);
        const at = this.#reserve(SIZEOF_32);

        this.#view.setFloat32(this.#base + at, value, true);

        this.#endScalar(index, at, SIZEOF_32);
    }

    setFloat64(value: number): void {
        const index = this.#check(0, SIZEOF_64);
        const at = this.#reserve(SIZEOF_64);

        this.#view.setFloat64(this.#base + at, value, true);

        this.#endScalar(index, at, SIZEOF_64);
    }

    // Each array setter writes its elements with its own type's function,
    // so that each conversion sees one type, as TableWriter's do. Converting
    // 27,028 floats from a number[] took 15-20 µs, against 3.5-5 µs to copy a
    // Float32Array, in Node and d8.

    /**
     * Sets an array field. An `Int8Array` is copied as bytes; any other
     * array-like, such as a `number[]` or another typed array, is converted as
     * `new Int8Array(values)` would, which takes several times as long.
     */
    setInt8Array(values: ArrayLike<number> | Int8Array): void {
        const count = values.length;
        if (count * SIZEOF_8 >= SEGMENT_BYTES) {
            this.#addSegment(
                INT8_ARRAY,
                values,
                count,
                count * SIZEOF_8,
                SIZEOF_8
            );
            return;
        }

        const elements = this.#startArray(count, count * SIZEOF_8, SIZEOF_8);
        setInt8Elements(this.#bytes, this.#base + elements, values, count);
        this.#endArray(count, elements, count * SIZEOF_8, SIZEOF_8);
    }

    /**
     * Sets an array field. A `Uint8Array` is copied as bytes; any other
     * array-like, such as a `number[]` or another typed array, is converted as
     * `new Uint8Array(values)` would, which takes several times as long.
     */
    setUint8Array(values: ArrayLike<number> | Uint8Array): void {
        const count = values.length;
        if (count * SIZEOF_8 >= SEGMENT_BYTES) {
            this.#addSegment(
                UINT8_ARRAY,
                values,
                count,
                count * SIZEOF_8,
                SIZEOF_8
            );
            return;
        }

        const elements = this.#startArray(count, count * SIZEOF_8, SIZEOF_8);
        setUint8Elements(this.#bytes, this.#base + elements, values, count);
        this.#endArray(count, elements, count * SIZEOF_8, SIZEOF_8);
    }

    /**
     * Sets an array field. An `Int16Array` is copied as bytes; any other
     * array-like, such as a `number[]` or another typed array, is converted as
     * `new Int16Array(values)` would, which takes several times as long.
     */
    setInt16Array(values: ArrayLike<number> | Int16Array): void {
        const count = values.length;
        if (count * SIZEOF_16 >= SEGMENT_BYTES) {
            this.#addSegment(
                INT16_ARRAY,
                values,
                count,
                count * SIZEOF_16,
                SIZEOF_16
            );
            return;
        }

        const elements = this.#startArray(count, count * SIZEOF_16, SIZEOF_16);
        setInt16Elements(this.#bytes, this.#base + elements, values, count);
        this.#endArray(count, elements, count * SIZEOF_16, SIZEOF_16);
    }

    /**
     * Sets an array field. A `Uint16Array` is copied as bytes; any other
     * array-like, such as a `number[]` or another typed array, is converted as
     * `new Uint16Array(values)` would, which takes several times as long.
     */
    setUint16Array(values: ArrayLike<number> | Uint16Array): void {
        const count = values.length;
        if (count * SIZEOF_16 >= SEGMENT_BYTES) {
            this.#addSegment(
                UINT16_ARRAY,
                values,
                count,
                count * SIZEOF_16,
                SIZEOF_16
            );
            return;
        }

        const elements = this.#startArray(count, count * SIZEOF_16, SIZEOF_16);
        setUint16Elements(this.#bytes, this.#base + elements, values, count);
        this.#endArray(count, elements, count * SIZEOF_16, SIZEOF_16);
    }

    /**
     * Sets an array field. An `Int32Array` is copied as bytes; any other
     * array-like, such as a `number[]` or another typed array, is converted as
     * `new Int32Array(values)` would, which takes several times as long.
     */
    setInt32Array(values: ArrayLike<number> | Int32Array): void {
        const count = values.length;
        if (count * SIZEOF_32 >= SEGMENT_BYTES) {
            this.#addSegment(
                INT32_ARRAY,
                values,
                count,
                count * SIZEOF_32,
                SIZEOF_32
            );
            return;
        }

        const elements = this.#startArray(count, count * SIZEOF_32, SIZEOF_32);
        setInt32Elements(this.#bytes, this.#base + elements, values, count);
        this.#endArray(count, elements, count * SIZEOF_32, SIZEOF_32);
    }

    /**
     * Sets an array field. A `Uint32Array` is copied as bytes; any other
     * array-like, such as a `number[]` or another typed array, is converted as
     * `new Uint32Array(values)` would, which takes several times as long.
     */
    setUint32Array(values: ArrayLike<number> | Uint32Array): void {
        const count = values.length;
        if (count * SIZEOF_32 >= SEGMENT_BYTES) {
            this.#addSegment(
                UINT32_ARRAY,
                values,
                count,
                count * SIZEOF_32,
                SIZEOF_32
            );
            return;
        }

        const elements = this.#startArray(count, count * SIZEOF_32, SIZEOF_32);
        setUint32Elements(this.#bytes, this.#base + elements, values, count);
        this.#endArray(count, elements, count * SIZEOF_32, SIZEOF_32);
    }

    /**
     * Sets an array field. A `BigInt64Array` is copied as bytes; any other
     * array-like, such as a `bigint[]` or another typed array, is converted as
     * `new BigInt64Array(values)` would, which takes several times as long.
     */
    setInt64Array(values: ArrayLike<bigint> | BigInt64Array): void {
        const count = values.length;
        if (count * SIZEOF_64 >= SEGMENT_BYTES) {
            this.#addSegment(
                INT64_ARRAY,
                values,
                count,
                count * SIZEOF_64,
                SIZEOF_64
            );
            return;
        }

        const elements = this.#startArray(count, count * SIZEOF_64, SIZEOF_64);
        setInt64Elements(this.#bytes, this.#base + elements, values, count);
        this.#endArray(count, elements, count * SIZEOF_64, SIZEOF_64);
    }

    /**
     * Sets an array field. A `BigUint64Array` is copied as bytes; any other
     * array-like, such as a `bigint[]` or another typed array, is converted as
     * `new BigUint64Array(values)` would, which takes several times as long.
     */
    setUint64Array(values: ArrayLike<bigint> | BigUint64Array): void {
        const count = values.length;
        if (count * SIZEOF_64 >= SEGMENT_BYTES) {
            this.#addSegment(
                UINT64_ARRAY,
                values,
                count,
                count * SIZEOF_64,
                SIZEOF_64
            );
            return;
        }

        const elements = this.#startArray(count, count * SIZEOF_64, SIZEOF_64);
        setUint64Elements(this.#bytes, this.#base + elements, values, count);
        this.#endArray(count, elements, count * SIZEOF_64, SIZEOF_64);
    }

    /**
     * Sets an array field. A `Float32Array` is copied as bytes; any other
     * array-like, such as a `number[]` or another typed array, is converted as
     * `new Float32Array(values)` would, which takes several times as long.
     */
    setFloat32Array(values: ArrayLike<number> | Float32Array): void {
        const count = values.length;
        if (count * SIZEOF_32 >= SEGMENT_BYTES) {
            this.#addSegment(
                FLOAT32_ARRAY,
                values,
                count,
                count * SIZEOF_32,
                SIZEOF_32
            );
            return;
        }

        const elements = this.#startArray(count, count * SIZEOF_32, SIZEOF_32);
        setFloat32Elements(this.#bytes, this.#base + elements, values, count);
        this.#endArray(count, elements, count * SIZEOF_32, SIZEOF_32);
    }

    /**
     * Sets an array field. A `Float64Array` is copied as bytes; any other
     * array-like, such as a `number[]` or another typed array, is converted as
     * `new Float64Array(values)` would, which takes several times as long.
     */
    setFloat64Array(values: ArrayLike<number> | Float64Array): void {
        const count = values.length;
        if (count * SIZEOF_64 >= SEGMENT_BYTES) {
            this.#addSegment(
                FLOAT64_ARRAY,
                values,
                count,
                count * SIZEOF_64,
                SIZEOF_64
            );
            return;
        }

        const elements = this.#startArray(count, count * SIZEOF_64, SIZEOF_64);
        setFloat64Elements(this.#bytes, this.#base + elements, values, count);
        this.#endArray(count, elements, count * SIZEOF_64, SIZEOF_64);
    }

    /**
     * Writes `string` as UTF-8, with lone surrogates replaced by U+FFFD. The
     * builder's buffer makes room for its longest UTF-8, 3 bytes for each
     * UTF-16 code unit, and grows to it for a long string; the table holds
     * only the bytes written.
     */
    setString(string: string): void {
        const index = this.#checkRoom();
        const units = string.length;
        const at = this.#end;
        const start = at + this.#size;
        const before = at + this.#shift - this.#table + this.#size;

        // With room for its longest UTF-8, 3 bytes for each UTF-16 code
        // unit, a string needs no count, which took 10-14 times as long as
        // encoding a long one. Only a string whose longest UTF-8 would be too
        // long for the table is counted, so that it fails only if it is.
        let room = 3 * units;
        if (!(before + room <= this.#maxLength)) {
            room = utf8Length(string);
            this.#checkLength(index, before + room);
        }

        if (start + room > this.#capacity) {
            this.#grow(start + room);
        }

        const length = encodeInto(string, this.#bytes, this.#base + start);
        this.#setOffset(index, at);
        this.#setUnsigned(at, length);
        this.#end = start + length;
        this.#count = index + 1;
    }

    /**
     * Sets a struct field from its bytes: its scalars, little-endian, one
     * after another, which are written as they are, without padding.
     */
    setStruct(bytes: Uint8Array): void {
        const byteLength = bytes.length;
        const index = this.#check(0, byteLength);
        const at = this.#end;

        this.#setOffset(index, at);
        this.#appendBytes(bytes, byteLength);
        this.#count = index + 1;
    }

    /**
     * Sets an array of structs of `size` bytes each, from their bytes one
     * after another. It's padded so that the structs start at a multiple of
     * `alignment` from the start of the table, which is the size of the
     * structs' largest scalar.
     *
     * @throws {RangeError} If `size` isn't a positive integer, `bytes` aren't
     * a whole number of structs, or `alignment` isn't 1, 2, 4 or 8.
     */
    setStructArray(
        bytes: Uint8Array,
        size: number,
        alignment: ElementSize
    ): void {
        const count = structCount(bytes.length, size);
        assertAlignment(alignment);

        const byteLength = bytes.length;
        if (byteLength >= SEGMENT_BYTES) {
            this.#addSegment(BYTES, bytes, count, byteLength, alignment);
            return;
        }

        const elements = this.#startArray(count, byteLength, alignment);
        copyBytes(bytes, 0, byteLength, this.#bytes, this.#base + elements);
        this.#endArray(count, elements, byteLength, alignment);
    }

    /**
     * Embeds a table, from its builder or its bytes, without a length prefix.
     * Readers find its fields with `TableReader.fieldOffset()` from the field's
     * offset, so its layout must be {@link TableLayout.DEFAULT}.
     *
     * A builder must have all its fields set. Its table is written as
     * `bytes()` would write it, header included, so its layout may have a
     * header or another offset type, but without bytes of its own: copied
     * in from its buffer, or, if it's 128 bytes or longer, kept until this
     * table is written. It can still be finalized or nested elsewhere too,
     * since a complete builder doesn't change.
     *
     * The nested table's typed arrays are aligned from its own start, so it's
     * padded to start at a multiple of `alignment`, which they need to be read
     * as views rather than copies. For a builder, the default is the largest
     * alignment of its arrays and nested tables, or 1 if it has neither. For
     * bytes, the default of 4 suits tables of 8-, 16- and 32-bit elements;
     * pass 8 for a table with 64-bit arrays.
     *
     * @throws {RangeError} If the builder has fields left to set, or
     * `alignment` isn't 1, 2, 4 or 8.
     */
    setTable(
        table: TableBuilder | Uint8Array,
        alignment: ElementSize = table instanceof TableBuilder
            ? table.#alignment
            : SIZEOF_32
    ): void {
        assertAlignment(alignment);

        const byteLength = TableBuilder.#lengthOf(table);
        const end = this.#end;
        const pad = padding(end + this.#shift - this.#table, alignment);
        const index = this.#check(pad, byteLength);

        const at = end + pad;
        this.#pad(at);
        this.#setOffset(index, at);
        this.#append(table);
        this.#count = index + 1;
        this.#alignTo(alignment);
    }

    /**
     * Writes a vector of tables, from their builders or their bytes, after
     * the end of each, which lets readers find any item without reading the
     * others. Builders must have all their fields set, and are written as
     * `setTable()` writes them. Items aren't padded, so their typed arrays
     * are read as copies when they don't happen to be aligned.
     *
     * @throws {RangeError} If a builder has fields left to set.
     */
    setVector(vector: ReadonlyArray<TableBuilder | Uint8Array>): void {
        const size = this.#size;
        const count = vector.length;

        let byteLength = size + count * size;
        for (let i = 0; i < count; i++) {
            byteLength += TableBuilder.#lengthOf(vector[i]);
        }

        const index = this.#check(0, byteLength);
        const at = this.#reserve(size + count * size);
        const position = at + this.#shift;

        this.#setOffset(index, at);
        this.#setUnsigned(at, count);
        this.#end = at + size + count * size;

        for (let i = 0; i < count; i++) {
            this.#append(vector[i]);
            this.#setUnsigned(
                at + (i + 1) * size,
                this.#end + this.#shift - position
            );
        }

        this.#count = index + 1;
    }

    /**
     * Writes a table of `fields` fields nested in this one, in place,
     * without a builder of its own: `fill` sets its fields, in order, on the
     * {@link TableItem} that it's given, and each is written straight into
     * this table. It's written as a nested builder of this table's offset
     * type and no header is: padded to start at a multiple of its largest
     * alignment, as `setTable()` pads a builder by default, and into the same
     * bytes, which a nested builder writes into a buffer of its own first.
     *
     * The item is this builder, whose setters set the nested table's fields
     * until `fill` returns. The table can't be finalized or nested meanwhile.
     *
     * @throws {RangeError} If `fields` isn't a non-negative integer, `fill`
     * leaves fields of the table unset, or as the other setters throw.
     * @throws What `fill` throws. Either way, the builder is left as it was.
     */
    setTableInPlace(fields: number, fill: (table: TableItem) => void): void {
        this.#checkRoom();
        const header = nestedHeaderLength(fields, this.#layout);

        // Where the table starts, unpadded, and what to restore after it
        const parentFields = this.#fields;
        const index = this.#count;
        const parentTable = this.#table;
        const slots = this.#slots;
        const alignment = this.#alignment;
        const nesting = this.#nesting;
        const at = this.#end;
        const shift = this.#shift;
        const segments = this.#segments?.length ?? 0;
        const start = at + shift;

        this.#startTable(fields, header);
        try {
            fill(this);
            this.#assertSet();
        } catch (error) {
            this.#restore(parentFields, index, parentTable, slots, alignment);
            this.#nesting = nesting;
            this.#truncate(at, shift, segments);
            throw error;
        }

        const length = this.#end + this.#shift - start;
        const tableAlignment = this.#alignment;
        this.#restore(parentFields, index, parentTable, slots, alignment);
        this.#nesting = nesting;

        // Its alignment is known once its fields are set, so it's padded
        // from the parent's start then, as a nested builder is
        const pad = padding(start - parentTable, tableAlignment);
        const total = start - parentTable + pad + length;
        if (!(total <= this.#maxLength)) {
            this.#truncate(at, shift, segments);
            throw tooLong(index, total, this.#layout.offsetType());
        }

        if (pad !== 0) {
            this.#moveTable(at, segments, pad);
        }

        this.#setUnsigned(
            slots + index * this.#size,
            start + pad - parentTable
        );
        this.#count = index + 1;
        this.#alignTo(tableAlignment);
    }

    /**
     * Writes a vector of `count` tables of `fields` fields each, in place,
     * as `setTableInPlace()` writes one: `fill(item, index)` sets the fields
     * of item `index`, for each in order. Items aren't padded, as
     * `setVector()` doesn't pad them.
     *
     * @throws {RangeError} If `count` or `fields` isn't a non-negative
     * integer, the vector's count and item ends and its items' offsets make
     * the table too long, before any item is written, `fill` leaves fields
     * of an item unset, or as the other setters throw.
     * @throws What `fill` throws. Either way, the builder is left as it was.
     */
    setVectorInPlace(
        count: number,
        fields: number,
        fill: (item: TableItem, index: number) => void
    ): void {
        assertCount(count);
        this.#checkRoom();

        // The count, the items' ends and their offsets, which fail before
        // any item is written, rather than after a `fill` for each
        const size = this.#size;
        const header = nestedHeaderLength(fields, this.#layout);
        const index = this.#count;
        const at = this.#end;
        const shift = this.#shift;
        const parentTable = this.#table;
        const position = at + shift;
        const headers =
            position - parentTable + size + count * size + count * header;
        if (!(headers <= this.#maxLength)) {
            throw tooLong(index, headers, this.#layout.offsetType());
        }

        const parentFields = this.#fields;
        const slots = this.#slots;
        const alignment = this.#alignment;
        const nesting = this.#nesting;
        const segments = this.#segments?.length ?? 0;

        const ends = at + size;
        this.#reserve(size + count * size);
        this.#setUnsigned(at, count);
        this.#end = ends + count * size;

        try {
            for (let item = 0; item < count; item++) {
                this.#startTable(fields, header);
                fill(this, item);
                this.#assertSet();
                this.#nesting = nesting;

                this.#setUnsigned(
                    ends + item * size,
                    this.#end + this.#shift - position
                );
            }
        } catch (error) {
            this.#restore(parentFields, index, parentTable, slots, alignment);
            this.#nesting = nesting;
            this.#truncate(at, shift, segments);
            throw error;
        }

        this.#restore(parentFields, index, parentTable, slots, alignment);

        const length = this.#end + this.#shift - parentTable;
        if (!(length <= this.#maxLength)) {
            this.#truncate(at, shift, segments);
            throw tooLong(index, length, this.#layout.offsetType());
        }

        this.#setUnsigned(slots + index * size, position - parentTable);
        this.#count = index + 1;
    }

    /**
     * Starts writing a table of `fields` fields in place at the end of the
     * buffer, whose header of `header` bytes holds only their offsets, and
     * counts its fields rather than its parent's
     */
    #startTable(fields: number, header: number): void {
        const at = this.#reserve(header);

        this.#fields = fields;
        this.#count = 0;
        this.#table = at + this.#shift;
        this.#slots = at;
        this.#alignment = SIZEOF_8;
        this.#end = at + header;
        this.#nesting++;
    }

    /**
     * Counts the parent's fields again after a table written in place: those
     * set before it, its start, its offsets' position and its alignment. The
     * caller restores the nesting.
     */
    #restore(
        fields: number,
        count: number,
        table: number,
        slots: number,
        alignment: ElementSize
    ): void {
        this.#fields = fields;
        this.#count = count;
        this.#table = table;
        this.#slots = slots;
        this.#alignment = alignment;
    }

    /**
     * Removes what was written from `end` in the buffer, with `shift` bytes
     * of segments before it, and the segments from slot `segments` on,
     * which releases their values
     */
    #truncate(end: number, shift: number, segments: number): void {
        this.#end = end;
        this.#shift = shift;

        if (this.#segments !== undefined) {
            this.#segments.length = segments;
        }
    }

    /**
     * Moves the table written in place from `at` forward by `pad` bytes of
     * padding, with its segments from slot `segments` on. Its offsets count
     * from its own start, so none changes.
     */
    #moveTable(at: number, segments: number, pad: number): void {
        const end = this.#reserve(pad);
        const bytes = this.#bytes;
        const base = this.#base;

        bytes.copyWithin(base + at + pad, base + at, base + end);
        bytes.fill(0, base + at, base + at + pad);
        this.#end = end + pad;

        const list = this.#segments;
        if (list !== undefined) {
            for (
                let slot = segments;
                slot < list.length;
                slot += SEGMENT_SLOTS
            ) {
                (list[slot] as number) += pad;
            }
        }
    }

    /**
     * Where an array's elements go in the buffer, after the padding that
     * aligns them and its count, once the array fits the table and the
     * buffer has room for it. It writes nothing, so that elements that throw
     * leave the builder as it was.
     *
     * @throws {RangeError} If `count`, an array-like's length, isn't a
     * non-negative integer, as `TableSizer` checks it, or as `#check()`
     * throws.
     */
    #startArray(
        count: number,
        byteLength: number,
        alignment: ElementSize
    ): number {
        assertCount(count);
        const start = this.#end + this.#size;
        const pad = padding(start + this.#shift - this.#table, alignment);
        this.#check(pad, this.#size + byteLength);

        const elements = start + pad;
        if (elements + byteLength > this.#capacity) {
            this.#grow(elements + byteLength);
        }

        return elements;
    }

    /**
     * Writes the padding, offset and count of an array whose elements were
     * written at `elements`, aligned to `alignment`, and moves past them
     */
    #endArray(
        count: number,
        elements: number,
        byteLength: number,
        alignment: ElementSize
    ): void {
        const at = elements - this.#size;

        this.#pad(at);
        this.#setOffset(this.#count, at);
        this.#setUnsigned(at, count);
        this.#end = elements + byteLength;
        this.#count++;
        this.#alignTo(alignment);
    }

    /**
     * Writes an array's padding, offset and count, and keeps its elements as
     * a segment that `bytes()` writes after the count
     */
    #addSegment(
        kind: number,
        values: unknown,
        count: number,
        byteLength: number,
        alignment: ElementSize
    ): void {
        assertCount(count);
        const end = this.#end;
        const size = this.#size;
        const pad = padding(end + size + this.#shift - this.#table, alignment);
        const index = this.#check(pad, size + byteLength);

        const at = end + pad;
        this.#reserve(pad + size);
        this.#pad(at);
        this.#setOffset(index, at);
        this.#setUnsigned(at, count);
        this.#end = at + size;
        this.#pushSegment(kind, values, count, byteLength);
        this.#count = index + 1;
        this.#alignTo(alignment);
    }

    /** Keeps a segment of `byteLength` bytes at the end of the buffer */
    #pushSegment(
        kind: number,
        values: unknown,
        count: number,
        byteLength: number
    ): void {
        this.#segments ??= [];

        this.#segments.push(this.#end, byteLength, kind, values, count);
        this.#shift += byteLength;
    }

    /**
     * Appends bytes, as a segment when they're long, or else copied into the
     * buffer
     */
    #appendBytes(bytes: Uint8Array, byteLength: number): void {
        if (byteLength >= SEGMENT_BYTES) {
            this.#pushSegment(BYTES, bytes, byteLength, byteLength);
            return;
        }

        const at = this.#reserve(byteLength);
        copyBytes(bytes, 0, byteLength, this.#bytes, this.#base + at);
        this.#end = at + byteLength;
    }

    /**
     * Appends a table: its bytes, or a complete builder's table, with its
     * length written into a sized header. The builder's buffer is copied in,
     * unless its table is {@link KEPT_TABLE_BYTES} or longer: then it's kept
     * as a segment, so that a table nested in tables nested in others is
     * written once, by `bytes()`, rather than copied into each parent in
     * turn. A shorter table has no segments, since none is shorter.
     */
    #append(table: TableBuilder | Uint8Array): void {
        if (!(table instanceof TableBuilder)) {
            this.#appendBytes(table, table.length);
            return;
        }

        if (table.#written !== undefined) {
            const bytes = table.bytes();
            this.#appendBytes(bytes, bytes.length);
            return;
        }

        const length = table.#end;
        const tableLength = length + table.#shift;
        if (table.#layout.sized()) {
            setSlot(table.#bytes, table.#base, table.#size, tableLength);
        }

        if (tableLength >= KEPT_TABLE_BYTES) {
            const nested: NestedTable = {
                bytes: table.#bytes,
                base: table.#base,
                end: length,
                segments: table.#segments
            };

            table.#kept = true;
            this.#pushSegment(TABLE, nested, 0, tableLength);
            return;
        }

        const at = this.#reserve(length);
        if (length <= SHORT_COPY) {
            copyWords(
                table.#view,
                table.#base,
                this.#view,
                this.#base + at,
                length
            );
        } else {
            copyBytes(
                table.#bytes,
                table.#base,
                table.#base + length,
                this.#bytes,
                this.#base + at
            );
        }
        this.#end = at + length;
    }

    /** Writes zero padding from the end of the buffer to `at` */
    #pad(at: number): void {
        const end = this.#end;
        if (at === end) {
            return;
        }

        if (at > this.#capacity) {
            this.#grow(at);
        }

        // The buffer's bytes may have been written before, by another
        // builder, a table that failed, or as room for a string
        const bytes = this.#bytes;
        const base = this.#base;
        for (let i = end; i < at; i++) {
            bytes[base + i] = 0;
        }
        this.#end = at;
    }

    #endScalar(index: number, at: number, byteLength: number): void {
        this.#setOffset(index, at);
        this.#end = at + byteLength;
        this.#count = index + 1;
    }

    /** Writes the offset of field `index`, which starts at `at` in the buffer */
    #setOffset(index: number, at: number): void {
        this.#setUnsigned(
            this.#slots + index * this.#size,
            at + this.#shift - this.#table
        );
    }

    /**
     * Writes an offset, length, count, item end or header slot at `at` in
     * the buffer, little-endian. A value too large for the width is wrapped,
     * as DataView's setters wrap it; the setters reject a table long enough
     * to need one.
     */
    #setUnsigned(at: number, value: number): void {
        const view = this.#view;
        const position = this.#base + at;
        if (this.#size === SIZEOF_32) {
            view.setUint32(position, value, true);
        } else if (this.#size === SIZEOF_16) {
            view.setUint16(position, value, true);
        } else {
            view.setUint8(position, value);
        }
    }

    /**
     * Makes room for `byteLength` bytes at the end of the buffer, and
     * returns where they go
     */
    #reserve(byteLength: number): number {
        const end = this.#end;
        if (end + byteLength > this.#capacity) {
            this.#grow(end + byteLength);
        }

        return end;
    }

    /**
     * Gives the builder room for at least `length` bytes, at least doubling
     * it, up to {@link SHARED_BYTES} unless `length` is longer: more of the
     * shared buffer where its bytes are, when no builder holds bytes after
     * them, or else bytes that it moves to.
     */
    #grow(length: number): void {
        const doubled = Math.max(length, 2 * this.#capacity);
        const capacity =
            length <= SHARED_BYTES ? Math.min(doubled, SHARED_BYTES) : doubled;
        const bytes = this.#bytes;
        const base = this.#base;

        if (bytes === shared) {
            if (
                (base + this.#capacity === top || live === 1) &&
                base + capacity <= bytes.length
            ) {
                // To a multiple of 8, as the shared buffer's length is, so
                // that the next builder's bytes are aligned
                top = (base + capacity + 7) & ~7;
                this.#capacity = top - base;
                return;
            }

            live--;
        }

        this.#take(capacity);
        copyBytes(bytes, base, base + this.#end, this.#bytes, this.#base);
    }

    /**
     * Takes `length` bytes for the builder: the shared buffer's next, in a
     * new one when it hasn't that many, or a buffer of the builder's own
     * when they're more than {@link SHARED_BYTES}
     */
    #take(length: number): void {
        if (length > SHARED_BYTES) {
            const bytes = new Uint8Array(length);

            this.#bytes = bytes;
            this.#view = new DataView(bytes.buffer);
            this.#base = 0;
            this.#capacity = length;
            return;
        }

        const room = (length + 7) & ~7;
        if (top + room > shared.length) {
            replaceShared(room);
        }

        this.#bytes = shared;
        this.#view = sharedView;
        this.#base = top;
        this.#capacity = room;
        top += room;
        live++;
    }

    #alignTo(alignment: ElementSize): void {
        if (alignment > this.#alignment) {
            this.#alignment = alignment;
        }
    }

    /**
     * The byte length of a table to nest, which a builder knows once all its
     * fields are set
     *
     * @throws {RangeError} If the builder has fields left to set.
     */
    static #lengthOf(table: TableBuilder | Uint8Array): number {
        if (!(table instanceof TableBuilder)) {
            return table.length;
        }

        table.#assertComplete();
        const written = table.#written;
        if (written === undefined) {
            return table.#end + table.#shift;
        }

        return written instanceof TableReader
            ? written.bytes().length
            : written.length;
    }

    /**
     * @throws {TypeError} If a table is being written in place, whose fields
     * the builder is counting rather than its own.
     * @throws {RangeError} If the table has fields left to set.
     */
    #assertComplete(): void {
        if (this.#nesting !== 0) {
            throw new TypeError(
                "A table can't be finalized or nested while a table is recorded in place in it"
            );
        }

        this.#assertSet();
    }

    /** @throws {RangeError} If the table being counted has fields left to set. */
    #assertSet(): void {
        if (this.#count !== this.#fields) {
            throw new RangeError(
                `Only ${this.#count} of the table's ${this.#fields} fields were set`
            );
        }
    }

    /**
     * Checks that a field can be added, and returns its index
     *
     * @throws {TypeError} If the table is already written.
     * @throws {RangeError} If the table already holds all its fields.
     */
    #checkRoom(): number {
        if (this.#written !== undefined) {
            throw new TypeError('The table was already finalized');
        }

        const index = this.#count;
        if (index === this.#fields) {
            throw tooManyFields(this.#fields);
        }

        return index;
    }

    /**
     * Checks that field `index` leaves the table being written `length`
     * bytes long, which the layout addresses
     *
     * @throws {RangeError} If it doesn't.
     */
    #checkLength(index: number, length: number): void {
        if (!(length <= this.#maxLength)) {
            throw tooLong(index, length, this.#layout.offsetType());
        }
    }

    /**
     * Checks that a field of `byteLength` bytes after `padding` bytes can be
     * added, before any of it is written, and returns its index. Every
     * offset and length is shorter than the table, so the table's length is
     * the one value to check.
     *
     * @throws {TypeError} If the table is already written.
     * @throws {RangeError} If the table already holds all its fields, or the
     * field would make it longer than the layout addresses.
     */
    #check(padding: number, byteLength: number): number {
        const index = this.#checkRoom();
        this.#checkLength(
            index,
            this.#end + this.#shift - this.#table + padding + byteLength
        );

        return index;
    }

    /**
     * Writes the table into bytes of its exact length: the builder's bytes,
     * with the segments between them, and the length in a sized header. It
     * then gives the builder's bytes back, unless a parent keeps them. A
     * buffer of the builder's own, which is longer than
     * {@link SHARED_BYTES}, becomes the table instead, when it holds no
     * segments and no parent keeps it: cut to the table's length by
     * `transfer()`, which took a quarter to a half of the time that copying
     * a table of 64 KiB to 1 MiB out of it did.
     *
     * @throws {RangeError} If fewer fields were set than the table holds, or
     * an array kept as a segment has changed its length.
     */
    #write(): Uint8Array {
        this.#assertComplete();

        const length = this.#end + this.#shift;
        const segments = this.#segments;
        let table: Uint8Array;
        const bytes = this.#bytes;
        const base = this.#base;
        if (segments !== undefined && segments.length > 0) {
            table = new Uint8Array(length);
            writeRuns(table, 0, bytes, base, this.#end, segments);
        } else if (bytes.length > SHARED_BYTES && !this.#kept) {
            table = new Uint8Array(bytes.buffer.transfer(length));
        } else {
            table = bytes.slice(base, base + length);
        }

        if (this.#layout.sized()) {
            setSlot(table, 0, this.#size, length);
        }

        // Release the builder's bytes, unless a parent keeps them, and the
        // values, which may be large arrays
        if (bytes === shared && !this.#kept) {
            live--;
            if (live === 0) {
                top = 0;
            } else if (base + this.#capacity === top) {
                top = base;
            }
        }
        this.#bytes = EMPTY;
        this.#view = EMPTY_VIEW;
        this.#segments = undefined;
        this.#written = table;

        return table;
    }
}

/**
 * Writes a buffer's first `end` bytes, with the segments between them, from
 * `start` in the table, and the bytes and segments of a nested builder's
 * table in turn
 *
 * @throws {RangeError} If an array has changed its length.
 */
function writeRuns(
    table: Uint8Array,
    start: number,
    bytes: Uint8Array,
    base: number,
    end: number,
    segments: unknown[] | undefined
): void {
    let from = 0;
    let position = start;
    if (segments !== undefined) {
        for (let slot = 0; slot < segments.length; slot += SEGMENT_SLOTS) {
            const at = segments[slot] as number;
            const byteLength = segments[slot + 1] as number;
            const kind = segments[slot + 2] as number;
            const values = segments[slot + 3];

            copyBytes(bytes, base + from, base + at, table, position);
            position += at - from;
            from = at;

            if (kind === TABLE) {
                const nested = values as NestedTable;
                writeRuns(
                    table,
                    position,
                    nested.bytes,
                    nested.base,
                    nested.end,
                    nested.segments
                );
            } else {
                writeSegment(
                    table,
                    position,
                    kind,
                    values,
                    segments[slot + 4] as number,
                    byteLength
                );
            }
            position += byteLength;
        }
    }

    copyBytes(bytes, base + from, base + end, table, position);
}

/**
 * Writes a segment's values, `byteLength` bytes of them, at `position` in the
 * table
 *
 * @throws {RangeError} If an array no longer holds `count` elements, or bytes
 * `byteLength` bytes, as when their buffer was transferred.
 */
function writeSegment(
    table: Uint8Array,
    position: number,
    kind: number,
    values: unknown,
    count: number,
    byteLength: number
): void {
    const expected = kind === BYTES ? byteLength : count;
    const length = (values as ArrayLike<unknown>).length;
    if (length !== expected) {
        throw new RangeError(
            `An array of ${expected} elements held ${length} when the table was written`
        );
    }

    switch (kind) {
        case INT8_ARRAY:
            setInt8Elements(table, position, values as Int8Array, count);
            break;

        case UINT8_ARRAY:
            setUint8Elements(table, position, values as Uint8Array, count);
            break;

        case INT16_ARRAY:
            setInt16Elements(table, position, values as Int16Array, count);
            break;

        case UINT16_ARRAY:
            setUint16Elements(table, position, values as Uint16Array, count);
            break;

        case INT32_ARRAY:
            setInt32Elements(table, position, values as Int32Array, count);
            break;

        case UINT32_ARRAY:
            setUint32Elements(table, position, values as Uint32Array, count);
            break;

        case INT64_ARRAY:
            setInt64Elements(table, position, values as BigInt64Array, count);
            break;

        case UINT64_ARRAY:
            setUint64Elements(table, position, values as BigUint64Array, count);
            break;

        case FLOAT32_ARRAY:
            setFloat32Elements(table, position, values as Float32Array, count);
            break;

        case FLOAT64_ARRAY:
            setFloat64Elements(table, position, values as Float64Array, count);
            break;

        default:
            table.set(values as Uint8Array, position);
    }
}

/**
 * The longest run of bytes that copyBytes() copies a byte at a time. Up to
 * 32, rather than 64, made four builders of 24 bytes each, each nested in
 * the next, take 1.1 times as long.
 */
const SHORT_COPY = 64;

/**
 * Copies `source`'s bytes from `from` to `to` to `at` in `target`. Short runs
 * are copied a byte at a time, since `subarray()` allocates a view, and
 * gives a `Uint8Array` that V8 keeps on its heap an `ArrayBuffer`.
 */
function copyBytes(
    source: Uint8Array,
    from: number,
    to: number,
    target: Uint8Array,
    at: number
): void {
    const length = to - from;
    if (length <= SHORT_COPY) {
        for (let i = 0; i < length; i++) {
            target[at + i] = source[from + i];
        }
    } else if (source === target) {
        target.copyWithin(at, from, to);
    } else {
        target.set(source.subarray(from, to), at);
    }
}

// The errors are built outside the setters, which stay small enough to
// inline

function tooManyFields(fields: number): RangeError {
    return new RangeError(
        `Can't add field ${fields + 1} to a table of ${fields}`
    );
}

function tooLong(
    index: number,
    length: number,
    offsetType: OffsetType
): RangeError {
    return new RangeError(
        `Field ${index + 1} makes the table ${length} bytes, too long for ${offsetType} offsets`
    );
}
