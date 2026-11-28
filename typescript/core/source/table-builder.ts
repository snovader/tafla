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
    nestedHeaderLength,
    padding,
    TableLayout
} from './layout.ts';
import { memoryOf, type ArenaMemory, type TableArena } from './table-arena.ts';
import { TableReader } from './table-reader.ts';
import { setSlot, structCount } from './table-writer.ts';
import type { ElementSize, OffsetSize, OffsetType } from './types.ts';
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
 * The fields of a table that a {@link TableBuilder} writes in place, which
 * its `setTableInPlace()` and `setVectorInPlace()` give to `fill`: the
 * builder itself, without `bytes()`, `finalize()` and `finish()`, whose
 * setters set the
 * nested table's fields while `fill` runs.
 */
export type TableItem = Omit<TableBuilder, 'bytes' | 'finalize' | 'finish'>;

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
 * Setting a field copies it, and writes its offset, into the builder's
 * bytes of its arena's buffer, which the builders of an arena share, each
 * with bytes of its own: no value is read after its setter returns, so a
 * typed array can change afterwards without changing the table. The table
 * then comes out of those bytes in one of two ways:
 *
 * - `bytes()` copies it into bytes of exactly its length, which are the
 *   caller's, and `finalize()` reads them;
 * - `finish()` lends it, as a view of the buffer, without copying it, until
 *   the arena's next table.
 *
 * Either gives the builder's bytes back, so that building tables allocates
 * little more than their bytes, or nothing with `finish()`. The arena keeps
 * the buffer between builders, at up to its `maxRetainedBytes` once a table
 * needed that much; a longer table is written in a buffer of its own.
 *
 * A table nested with `setTable()` or `setVector()` is copied in when it's
 * set, from its bytes or its builder's buffer, and the builder can still be
 * written, or nested again. `setTableInPlace()` and `setVectorInPlace()`
 * write a nested table, or each item of a vector, among this builder's own
 * fields instead, without a builder of its own, and write the same bytes as
 * nested builders without copying them.
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
     * `finalize()` returns, or `null` once `finish()` has lent it
     */
    declare private _written: Uint8Array | TableReader | null | undefined;

    declare private readonly _layout: TableLayout;
    /** The offset size, the width of offsets, lengths and header slots */
    declare private readonly _size: OffsetSize;
    /** The longest table that the layout's offsets and lengths address */
    declare private readonly _maxLength: number;

    /** The arena whose buffer the builder takes its bytes of */
    declare private readonly _arena: ArenaMemory;
    /** The buffer that holds the builder's bytes: the arena's, or its own */
    private _bytes: Uint8Array<ArrayBuffer> = EMPTY;
    private _view: DataView = EMPTY_VIEW;
    /**
     * Where the builder's bytes start in the buffer, which every position
     * below counts from, so that they stay valid when the bytes move
     */
    private _base = 0;
    /** The builder's bytes in the buffer, written or not */
    private _capacity = 0;
    /** The bytes of the buffer written so far */
    private _end = 0;

    // What the builder counts of the table whose fields it's writing: its
    // own, or, while a `fill` runs, those of the table that it writes in
    // place, until it restores its own
    declare private _fields: number;
    /** The number of fields set so far */
    private _count = 0;
    /** The table's start, in the table, which its offsets count from */
    private _table = 0;
    /** The position of the table's first offset in the buffer */
    declare private _slots: number;
    /**
     * The largest alignment that a field needs from the table's start, which
     * a parent pads this table to when it nests it
     */
    private _alignment: ElementSize = SIZEOF_8;
    /** The number of tables being written in place, one in another */
    private _nesting = 0;

    /**
     * @param arena - The arena whose buffer the table is written in:
     * `TableArena.DEFAULT`, or one of the caller's own. Builders of
     * different arenas can be nested in each other.
     * @param fields - The number of fields that the table will hold.
     * @param layout - The table's header and offset type, which its readers
     * must use too, with the version to write to the header when it has one.
     * @throws {RangeError} If `fields` isn't a non-negative integer, or the
     * header alone is longer than the layout addresses.
     * @throws {TypeError} If `arena` isn't a `TableArena`.
     */
    constructor(
        arena: TableArena,
        fields: number,
        layout: TableLayout = TableLayout.DEFAULT
    ) {
        const size = layout.offsetSize();
        const header = headerLength(fields, layout);

        this._layout = layout;
        this._size = size;
        // The layout's `maxLength()` and `headerSize()`, which
        // `headerLength()` has called too, are worked out here instead:
        // V8 counts the bytecode of each call that it inlines, and with
        // them repeated a row of many short builders took 1.15–1.2 times
        // as long in Node 26
        this._maxLength = 0xffff_ffff >>> (32 - 8 * size);
        this._fields = fields;
        this._arena = memoryOf(arena);

        this.#take(header + fields * FIELD_ROOM);

        // The length, when the layout is sized, is written by bytes(), and
        // the offsets as the fields are set
        const slots = header - fields * size;
        const version = layout.version();
        if (version !== undefined) {
            this.#setUnsigned(slots - size, version);
        }

        this._slots = slots;
        this._end = header;
    }

    /**
     * Writes the table, once, and returns a `TableReader` that reads it; later
     * calls return the same reader.
     *
     * @throws {RangeError} If fewer fields were set than the table holds.
     * @throws {TypeError} If `finish()` lent the table.
     */
    finalize(): TableReader {
        const written = this._written ?? this.#write();
        if (written instanceof TableReader) {
            return written;
        }

        const reader = new TableReader(written, this._layout);
        this._written = reader;

        return reader;
    }

    /**
     * Writes the table, once, as `finalize()` does, and returns its bytes,
     * which `finalize()` then returns a reader of. They're a copy of the
     * builder's, of the table's length, and the caller's to keep.
     *
     * @throws {RangeError} If fewer fields were set than the table holds.
     * @throws {TypeError} If `finish()` lent the table.
     */
    bytes(): Uint8Array {
        const written = this._written ?? this.#write();
        return written instanceof TableReader ? written.bytes() : written;
    }

    /**
     * Writes the table and lends it, as a view of the arena's buffer, without
     * copying it. The view holds the table until a builder of the same arena
     * next writes in its buffer: one that's made afterwards, or any that
     * sets a field. So use it, or copy it, before building the next table,
     * as a loop that writes each table out as it's built does. A table
     * longer than the arena keeps is lent from a buffer of its own, which
     * nothing else writes, with that buffer's spare room after it.
     *
     * The builder's table is then lent, and can't be written again, nor
     * nested. After `bytes()` or `finalize()`, `finish()` returns the bytes
     * that they copied instead.
     *
     * @throws {RangeError} If fewer fields were set than the table holds.
     * @throws {TypeError} If `finish()` lent the table already.
     */
    finish(): Uint8Array {
        if (this._written !== undefined) {
            return this.bytes();
        }

        this.#assertComplete();
        const length = this._end;
        const bytes = this._bytes;
        const base = this._base;
        if (this._layout.sized()) {
            setSlot(bytes, base, this._size, length);
        }

        const table = bytes.subarray(base, base + length);
        this.#release();
        this._written = null;

        return table;
    }

    // Each scalar is checked and given room before it's written, and its
    // offset written once it is, so that a value that throws, such as a
    // number for an int64, leaves the builder as it was

    setInt8(value: number): void {
        const index = this.#check(0, SIZEOF_8);
        const at = this.#reserve(SIZEOF_8);

        this._view.setInt8(this._base + at, value);

        this.#endScalar(index, at, SIZEOF_8);
    }

    setUint8(value: number): void {
        const index = this.#check(0, SIZEOF_8);
        const at = this.#reserve(SIZEOF_8);

        this._view.setUint8(this._base + at, value);

        this.#endScalar(index, at, SIZEOF_8);
    }

    setInt16(value: number): void {
        const index = this.#check(0, SIZEOF_16);
        const at = this.#reserve(SIZEOF_16);

        this._view.setInt16(this._base + at, value, true);

        this.#endScalar(index, at, SIZEOF_16);
    }

    setUint16(value: number): void {
        const index = this.#check(0, SIZEOF_16);
        const at = this.#reserve(SIZEOF_16);

        this._view.setUint16(this._base + at, value, true);

        this.#endScalar(index, at, SIZEOF_16);
    }

    setInt32(value: number): void {
        const index = this.#check(0, SIZEOF_32);
        const at = this.#reserve(SIZEOF_32);

        this._view.setInt32(this._base + at, value, true);

        this.#endScalar(index, at, SIZEOF_32);
    }

    setUint32(value: number): void {
        const index = this.#check(0, SIZEOF_32);
        const at = this.#reserve(SIZEOF_32);

        this._view.setUint32(this._base + at, value, true);

        this.#endScalar(index, at, SIZEOF_32);
    }

    setInt64(value: bigint): void {
        const index = this.#check(0, SIZEOF_64);
        const at = this.#reserve(SIZEOF_64);

        this._view.setBigInt64(this._base + at, value, true);

        this.#endScalar(index, at, SIZEOF_64);
    }

    setUint64(value: bigint): void {
        const index = this.#check(0, SIZEOF_64);
        const at = this.#reserve(SIZEOF_64);

        this._view.setBigUint64(this._base + at, value, true);

        this.#endScalar(index, at, SIZEOF_64);
    }

    setFloat32(value: number): void {
        const index = this.#check(0, SIZEOF_32);
        const at = this.#reserve(SIZEOF_32);

        this._view.setFloat32(this._base + at, value, true);

        this.#endScalar(index, at, SIZEOF_32);
    }

    setFloat64(value: number): void {
        const index = this.#check(0, SIZEOF_64);
        const at = this.#reserve(SIZEOF_64);

        this._view.setFloat64(this._base + at, value, true);

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

        const elements = this.#startArray(count, count * SIZEOF_8, SIZEOF_8);
        setInt8Elements(this._bytes, this._base + elements, values, count);
        this.#endArray(count, elements, count * SIZEOF_8, SIZEOF_8);
    }

    /**
     * Sets an array field. A `Uint8Array` is copied as bytes; any other
     * array-like, such as a `number[]` or another typed array, is converted as
     * `new Uint8Array(values)` would, which takes several times as long.
     */
    setUint8Array(values: ArrayLike<number> | Uint8Array): void {
        const count = values.length;

        const elements = this.#startArray(count, count * SIZEOF_8, SIZEOF_8);
        setUint8Elements(this._bytes, this._base + elements, values, count);
        this.#endArray(count, elements, count * SIZEOF_8, SIZEOF_8);
    }

    /**
     * Sets an array field. An `Int16Array` is copied as bytes; any other
     * array-like, such as a `number[]` or another typed array, is converted as
     * `new Int16Array(values)` would, which takes several times as long.
     */
    setInt16Array(values: ArrayLike<number> | Int16Array): void {
        const count = values.length;

        const elements = this.#startArray(count, count * SIZEOF_16, SIZEOF_16);
        setInt16Elements(this._bytes, this._base + elements, values, count);
        this.#endArray(count, elements, count * SIZEOF_16, SIZEOF_16);
    }

    /**
     * Sets an array field. A `Uint16Array` is copied as bytes; any other
     * array-like, such as a `number[]` or another typed array, is converted as
     * `new Uint16Array(values)` would, which takes several times as long.
     */
    setUint16Array(values: ArrayLike<number> | Uint16Array): void {
        const count = values.length;

        const elements = this.#startArray(count, count * SIZEOF_16, SIZEOF_16);
        setUint16Elements(this._bytes, this._base + elements, values, count);
        this.#endArray(count, elements, count * SIZEOF_16, SIZEOF_16);
    }

    /**
     * Sets an array field. An `Int32Array` is copied as bytes; any other
     * array-like, such as a `number[]` or another typed array, is converted as
     * `new Int32Array(values)` would, which takes several times as long.
     */
    setInt32Array(values: ArrayLike<number> | Int32Array): void {
        const count = values.length;

        const elements = this.#startArray(count, count * SIZEOF_32, SIZEOF_32);
        setInt32Elements(this._bytes, this._base + elements, values, count);
        this.#endArray(count, elements, count * SIZEOF_32, SIZEOF_32);
    }

    /**
     * Sets an array field. A `Uint32Array` is copied as bytes; any other
     * array-like, such as a `number[]` or another typed array, is converted as
     * `new Uint32Array(values)` would, which takes several times as long.
     */
    setUint32Array(values: ArrayLike<number> | Uint32Array): void {
        const count = values.length;

        const elements = this.#startArray(count, count * SIZEOF_32, SIZEOF_32);
        setUint32Elements(this._bytes, this._base + elements, values, count);
        this.#endArray(count, elements, count * SIZEOF_32, SIZEOF_32);
    }

    /**
     * Sets an array field. A `BigInt64Array` is copied as bytes; any other
     * array-like, such as a `bigint[]` or another typed array, is converted as
     * `new BigInt64Array(values)` would, which takes several times as long.
     */
    setInt64Array(values: ArrayLike<bigint> | BigInt64Array): void {
        const count = values.length;

        const elements = this.#startArray(count, count * SIZEOF_64, SIZEOF_64);
        setInt64Elements(this._bytes, this._base + elements, values, count);
        this.#endArray(count, elements, count * SIZEOF_64, SIZEOF_64);
    }

    /**
     * Sets an array field. A `BigUint64Array` is copied as bytes; any other
     * array-like, such as a `bigint[]` or another typed array, is converted as
     * `new BigUint64Array(values)` would, which takes several times as long.
     */
    setUint64Array(values: ArrayLike<bigint> | BigUint64Array): void {
        const count = values.length;

        const elements = this.#startArray(count, count * SIZEOF_64, SIZEOF_64);
        setUint64Elements(this._bytes, this._base + elements, values, count);
        this.#endArray(count, elements, count * SIZEOF_64, SIZEOF_64);
    }

    /**
     * Sets an array field. A `Float32Array` is copied as bytes; any other
     * array-like, such as a `number[]` or another typed array, is converted as
     * `new Float32Array(values)` would, which takes several times as long.
     */
    setFloat32Array(values: ArrayLike<number> | Float32Array): void {
        const count = values.length;

        const elements = this.#startArray(count, count * SIZEOF_32, SIZEOF_32);
        setFloat32Elements(this._bytes, this._base + elements, values, count);
        this.#endArray(count, elements, count * SIZEOF_32, SIZEOF_32);
    }

    /**
     * Sets an array field. A `Float64Array` is copied as bytes; any other
     * array-like, such as a `number[]` or another typed array, is converted as
     * `new Float64Array(values)` would, which takes several times as long.
     */
    setFloat64Array(values: ArrayLike<number> | Float64Array): void {
        const count = values.length;

        const elements = this.#startArray(count, count * SIZEOF_64, SIZEOF_64);
        setFloat64Elements(this._bytes, this._base + elements, values, count);
        this.#endArray(count, elements, count * SIZEOF_64, SIZEOF_64);
    }

    /**
     * Sets an array of bools, which is written as a `uint8` array, 1 for
     * each true value and 0 for each false one. It's written when it's set,
     * however long, since each value is converted as it's written.
     */
    setBoolArray(values: ArrayLike<boolean>): void {
        const count = values.length;
        const elements = this.#startArray(count, count, SIZEOF_8);

        const bytes = this._bytes;
        const start = this._base + elements;
        for (let i = 0; i < count; i++) {
            bytes[start + i] = values[i] ? 1 : 0;
        }
        this.#endArray(count, elements, count, SIZEOF_8);
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
        const at = this._end;
        const start = at + this._size;
        const before = at - this._table + this._size;

        // With room for its longest UTF-8, 3 bytes for each UTF-16 code
        // unit, a string needs no count, which took 10-14 times as long as
        // encoding a long one. Only a string whose longest UTF-8 would be too
        // long for the table is counted, so that it fails only if it is.
        let room = 3 * units;
        if (!(before + room <= this._maxLength)) {
            room = utf8Length(string);
            this.#checkLength(index, before + room);
        }

        if (start + room > this._capacity) {
            this.#grow(start + room);
        }

        const length = encodeInto(string, this._bytes, this._base + start);
        this.#setOffset(index, at);
        this.#setUnsigned(at, length);
        this._end = start + length;
        this._count = index + 1;
    }

    /**
     * Writes a vector of strings, each as UTF-8 with lone surrogates
     * replaced by U+FFFD, after the end of each, which lets readers find any
     * string without reading the others. It takes no more bytes for each
     * string than the end of it, where a vector of tables of one string
     * takes each table's offset and the string's count too.
     *
     * It takes an array, rather than any array-like, since a string is an
     * array-like of strings too, of one character each.
     */
    setStringVector(strings: readonly string[]): void {
        const index = this.#checkRoom();
        const count = strings.length;
        const size = this._size;
        const at = this._end;
        const before = at - this._table;
        const ends = (count + 1) * size;

        // With room for each string's longest UTF-8, 3 bytes for each UTF-16
        // code unit, as setString() makes room for one, the strings need no
        // count. They're counted only when that room would make the table
        // too long, so that the vector fails only if it is.
        let units = 0;
        for (let i = 0; i < count; i++) {
            units += strings[i].length;
        }

        let room = ends + 3 * units;
        if (!(before + room <= this._maxLength)) {
            room = ends;
            for (let i = 0; i < count; i++) {
                room += utf8Length(strings[i]);
            }
            this.#checkLength(index, before + room);
        }

        if (at + room > this._capacity) {
            this.#grow(at + room);
        }

        const bytes = this._bytes;
        const base = this._base;
        let position = at + ends;
        for (let i = 0; i < count; i++) {
            position += encodeInto(strings[i], bytes, base + position);
            this.#setUnsigned(at + (i + 1) * size, position - at);
        }

        this.#setOffset(index, at);
        this.#setUnsigned(at, count);
        this._end = position;
        this._count = index + 1;
    }

    /**
     * Sets a struct field from its bytes: its scalars, little-endian, one
     * after another, which are written as they are, without padding.
     */
    setStruct(bytes: Uint8Array): void {
        const byteLength = bytes.length;
        const index = this.#check(0, byteLength);
        const at = this._end;

        this.#setOffset(index, at);
        this.#appendBytes(bytes, byteLength);
        this._count = index + 1;
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
        const elements = this.#startArray(count, byteLength, alignment);
        copyBytes(bytes, 0, byteLength, this._bytes, this._base + elements);
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
     * in from its buffer. It can still be written or nested elsewhere too,
     * since a complete builder doesn't change, but not once `finish()` has
     * lent its table.
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
     * @throws {TypeError} If `finish()` lent the builder's table.
     */
    setTable(
        table: TableBuilder | Uint8Array,
        alignment: ElementSize = table instanceof TableBuilder
            ? // oxlint-disable-next-line eslint/no-underscore-dangle -- another builder's own field, which the rule allows only after `this`
              table._alignment
            : SIZEOF_32
    ): void {
        assertAlignment(alignment);

        const byteLength = TableBuilder.#lengthOf(table);
        const end = this._end;
        const pad = padding(end - this._table, alignment);
        const index = this.#check(pad, byteLength);

        const at = end + pad;
        this.#pad(at);
        this.#setOffset(index, at);
        this.#append(table);
        this._count = index + 1;
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
        const size = this._size;
        const count = vector.length;

        let byteLength = size + count * size;
        for (let i = 0; i < count; i++) {
            byteLength += TableBuilder.#lengthOf(vector[i]);
        }

        const index = this.#check(0, byteLength);
        const at = this.#reserve(size + count * size);

        this.#setOffset(index, at);
        this.#setUnsigned(at, count);
        this._end = at + size + count * size;

        for (let i = 0; i < count; i++) {
            this.#append(vector[i]);
            this.#setUnsigned(at + (i + 1) * size, this._end - at);
        }

        this._count = index + 1;
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
     * The table's alignment is known once its fields are set, so a table
     * that needs padding is moved by it then, with the tables nested in it,
     * which are moved again with each table padded around them. A caller
     * that knows the alignment, which the table's arrays and nested tables
     * fix, can give it as `alignment`: the table is then padded for it
     * before `fill` runs, and not moved. If its fields need more, it's moved
     * as without it; if they need less, it stays padded for `alignment`, as
     * `setTable()` pads a builder for an `alignment` given to it.
     *
     * The item is this builder, whose setters set the nested table's fields
     * until `fill` returns. The table can't be written or nested meanwhile.
     *
     * @throws {RangeError} If `fields` isn't a non-negative integer,
     * `alignment` isn't 1, 2, 4 or 8, `fill` leaves fields of the table
     * unset, or as the other setters throw.
     * @throws What `fill` throws. Either way, the builder is left as it was.
     */
    setTableInPlace(
        fields: number,
        fill: (table: TableItem) => void,
        alignment: ElementSize = SIZEOF_8
    ): void {
        this.#checkRoom();
        assertAlignment(alignment);
        const header = nestedHeaderLength(fields, this._layout);

        // Where the table starts, padded for the alignment given, and what
        // to restore after it
        const parentFields = this._fields;
        const index = this._count;
        const parentTable = this._table;
        const slots = this._slots;
        const parentAlignment = this._alignment;
        const nesting = this._nesting;
        const at = this._end;
        const start = at + padding(at - parentTable, alignment);

        this.#pad(start);
        this.#startTable(fields, header);
        try {
            fill(this);
            this.#assertSet();
        } catch (error) {
            this.#restore(
                parentFields,
                index,
                parentTable,
                slots,
                parentAlignment
            );
            this._nesting = nesting;
            this._end = at;
            throw error;
        }

        const length = this._end - start;
        const needed = this._alignment;
        const tableAlignment = needed > alignment ? needed : alignment;
        this.#restore(parentFields, index, parentTable, slots, parentAlignment);
        this._nesting = nesting;

        // Padded from the parent's start once its alignment is known, as a
        // nested builder is, unless the alignment given was enough
        const pad = padding(start - parentTable, tableAlignment);
        const total = start - parentTable + pad + length;
        if (!(total <= this._maxLength)) {
            this._end = at;
            throw tooLong(index, total, this._layout.offsetType());
        }

        if (pad !== 0) {
            this.#moveTable(start, pad);
        }

        this.#setUnsigned(
            slots + index * this._size,
            start + pad - parentTable
        );
        this._count = index + 1;
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
        const size = this._size;
        const header = nestedHeaderLength(fields, this._layout);
        const index = this._count;
        const at = this._end;
        const parentTable = this._table;
        const headers = at - parentTable + size + count * size + count * header;
        if (!(headers <= this._maxLength)) {
            throw tooLong(index, headers, this._layout.offsetType());
        }

        const parentFields = this._fields;
        const slots = this._slots;
        const alignment = this._alignment;
        const nesting = this._nesting;

        const ends = at + size;
        this.#reserve(size + count * size);
        this.#setUnsigned(at, count);
        this._end = ends + count * size;

        try {
            for (let item = 0; item < count; item++) {
                this.#startTable(fields, header);
                fill(this, item);
                this.#assertSet();
                this._nesting = nesting;

                this.#setUnsigned(ends + item * size, this._end - at);
            }
        } catch (error) {
            this.#restore(parentFields, index, parentTable, slots, alignment);
            this._nesting = nesting;
            this._end = at;
            throw error;
        }

        this.#restore(parentFields, index, parentTable, slots, alignment);

        const length = this._end - parentTable;
        if (!(length <= this._maxLength)) {
            this._end = at;
            throw tooLong(index, length, this._layout.offsetType());
        }

        this.#setUnsigned(slots + index * size, at - parentTable);
        this._count = index + 1;
    }

    /**
     * Starts writing a table of `fields` fields in place at the end of the
     * buffer, whose header of `header` bytes holds only their offsets, and
     * counts its fields rather than its parent's
     */
    #startTable(fields: number, header: number): void {
        const at = this.#reserve(header);

        this._fields = fields;
        this._count = 0;
        this._table = at;
        this._slots = at;
        this._alignment = SIZEOF_8;
        this._end = at + header;
        this._nesting++;
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
        this._fields = fields;
        this._count = count;
        this._table = table;
        this._slots = slots;
        this._alignment = alignment;
    }

    /**
     * Moves the table written in place from `at` forward by `pad` bytes of
     * padding. Its offsets count from its own start, so none changes.
     */
    #moveTable(at: number, pad: number): void {
        const end = this.#reserve(pad);
        const bytes = this._bytes;
        const base = this._base;

        bytes.copyWithin(base + at + pad, base + at, base + end);
        bytes.fill(0, base + at, base + at + pad);
        this._end = end + pad;
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
        const start = this._end + this._size;
        const pad = padding(start - this._table, alignment);
        this.#check(pad, this._size + byteLength);

        const elements = start + pad;
        if (elements + byteLength > this._capacity) {
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
        const at = elements - this._size;

        this.#pad(at);
        this.#setOffset(this._count, at);
        this.#setUnsigned(at, count);
        this._end = elements + byteLength;
        this._count++;
        this.#alignTo(alignment);
    }

    /** Copies bytes to the end of the buffer */
    #appendBytes(bytes: Uint8Array, byteLength: number): void {
        const at = this.#reserve(byteLength);
        copyBytes(bytes, 0, byteLength, this._bytes, this._base + at);
        this._end = at + byteLength;
    }

    /**
     * Appends a table: its bytes, or a complete builder's table, copied
     * from its buffer, with its length written into a sized header
     */
    #append(table: TableBuilder | Uint8Array): void {
        if (!(table instanceof TableBuilder)) {
            this.#appendBytes(table, table.length);
            return;
        }

        /* oxlint-disable eslint/no-underscore-dangle -- another builder's own fields, which the rule allows only after `this` */
        if (table._written !== undefined) {
            const bytes = table.bytes();
            this.#appendBytes(bytes, bytes.length);
            return;
        }

        const length = table._end;
        if (table._layout.sized()) {
            setSlot(table._bytes, table._base, table._size, length);
        }

        const at = this.#reserve(length);
        if (length <= SHORT_COPY) {
            copyWords(
                table._view,
                table._base,
                this._view,
                this._base + at,
                length
            );
        } else {
            copyBytes(
                table._bytes,
                table._base,
                table._base + length,
                this._bytes,
                this._base + at
            );
        }
        /* oxlint-enable eslint/no-underscore-dangle */
        this._end = at + length;
    }

    /** Writes zero padding from the end of the buffer to `at` */
    #pad(at: number): void {
        const end = this._end;
        if (at === end) {
            return;
        }

        if (at > this._capacity) {
            this.#grow(at);
        }

        // The buffer's bytes may have been written before, by another
        // builder, a table that failed, or as room for a string
        const bytes = this._bytes;
        const base = this._base;
        for (let i = end; i < at; i++) {
            bytes[base + i] = 0;
        }
        this._end = at;
    }

    #endScalar(index: number, at: number, byteLength: number): void {
        this.#setOffset(index, at);
        this._end = at + byteLength;
        this._count = index + 1;
    }

    /** Writes the offset of field `index`, which starts at `at` in the buffer */
    #setOffset(index: number, at: number): void {
        this.#setUnsigned(this._slots + index * this._size, at - this._table);
    }

    /**
     * Writes an offset, length, count, item end or header slot at `at` in
     * the buffer, little-endian. A value too large for the width is wrapped,
     * as DataView's setters wrap it; the setters reject a table long enough
     * to need one.
     */
    #setUnsigned(at: number, value: number): void {
        const view = this._view;
        const position = this._base + at;
        if (this._size === SIZEOF_32) {
            view.setUint32(position, value, true);
        } else if (this._size === SIZEOF_16) {
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
        const end = this._end;
        if (end + byteLength > this._capacity) {
            this.#grow(end + byteLength);
        }

        return end;
    }

    /**
     * Gives the builder room for at least `length` bytes, at least doubling
     * it, up to what the arena keeps unless `length` is longer: more of the
     * arena's buffer where its bytes are, when no builder holds bytes after
     * them, or else bytes that it moves to.
     */
    #grow(length: number): void {
        const arena = this._arena;
        const max = arena.max;
        const doubled = Math.max(length, 2 * this._capacity);
        const capacity = length <= max ? Math.min(doubled, max) : doubled;
        const bytes = this._bytes;
        const base = this._base;

        if (bytes === arena.bytes) {
            if (
                (base + this._capacity === arena.top || arena.live === 1) &&
                base + capacity <= bytes.length
            ) {
                // To a multiple of 8, as the arena's buffer's length is, so
                // that the next builder's bytes are aligned
                const top = (base + capacity + 7) & ~7;
                arena.top = top;
                this._capacity = top - base;
                return;
            }

            arena.live--;
        }

        this.#take(capacity);
        copyBytes(bytes, base, base + this._end, this._bytes, this._base);
    }

    /**
     * Takes `length` bytes for the builder: the next of its arena's buffer,
     * in a new one when it hasn't that many, or a buffer of the builder's
     * own when they're more than the arena keeps
     */
    #take(length: number): void {
        const arena = this._arena;
        if (length > arena.max) {
            this.#takeOwn(length);
            return;
        }

        const room = (length + 7) & ~7;
        if (arena.top + room > arena.bytes.length) {
            arena.replace(room);
        }

        const top = arena.top;
        this._bytes = arena.bytes;
        this._view = arena.view;
        this._base = top;
        this._capacity = room;
        arena.top = top + room;
        arena.live++;
    }

    /**
     * Takes a buffer of the builder's own, of `length` bytes. It's apart
     * from `#take()`, which the constructor calls, to keep the bytecode
     * that V8 inlines with the constructor small.
     */
    #takeOwn(length: number): void {
        const bytes = new Uint8Array(length);

        this._bytes = bytes;
        this._view = new DataView(bytes.buffer);
        this._base = 0;
        this._capacity = length;
    }

    #alignTo(alignment: ElementSize): void {
        if (alignment > this._alignment) {
            this._alignment = alignment;
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
        /* oxlint-disable eslint/no-underscore-dangle -- another builder's own fields, which the rule allows only after `this` */
        const written = table._written;
        // A lent table, null, failed #assertComplete()
        if (written == null) {
            return table._end;
        }
        /* oxlint-enable eslint/no-underscore-dangle */

        return written instanceof TableReader
            ? written.bytes().length
            : written.length;
    }

    /**
     * @throws {TypeError} If `finish()` lent the table, or a table is being
     * written in place, whose fields the builder is counting rather than its
     * own.
     * @throws {RangeError} If the table has fields left to set.
     */
    #assertComplete(): void {
        if (this._written === null) {
            throw new TypeError(
                "The table was lent by finish(), and can't be written again or nested"
            );
        }

        if (this._nesting !== 0) {
            throw new TypeError(
                "A table can't be finalized or nested while a table is recorded in place in it"
            );
        }

        this.#assertSet();
    }

    /** @throws {RangeError} If the table being counted has fields left to set. */
    #assertSet(): void {
        if (this._count !== this._fields) {
            throw new RangeError(
                `Only ${this._count} of the table's ${this._fields} fields were set`
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
        if (this._written !== undefined) {
            throw new TypeError('The table was already finalized');
        }

        const index = this._count;
        if (index === this._fields) {
            throw tooManyFields(this._fields);
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
        if (!(length <= this._maxLength)) {
            throw tooLong(index, length, this._layout.offsetType());
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
            this._end - this._table + padding + byteLength
        );

        return index;
    }

    /**
     * Writes the table into bytes of its exact length, a copy of the
     * builder's, with the length in a sized header, and gives the builder's
     * bytes back.
     *
     * A table is copied out of a buffer of the builder's own, longer than
     * its arena keeps, too, rather than cut from it by `transfer()`.
     * Detaching the buffer makes V8 check every typed array's buffer for the
     * rest of the process: after one table of 1.2 MB, reading 1,000
     * `float64` fields took 1.18–1.20 times as long in Node 26 and 27, and
     * 2.2 times in Node 24, and 100 short strings 1.12–1.14. Copying it took
     * as long as cutting it.
     *
     * @throws {RangeError} If fewer fields were set than the table holds.
     * @throws {TypeError} If `finish()` lent the table.
     */
    #write(): Uint8Array {
        this.#assertComplete();

        const length = this._end;
        const base = this._base;
        const table = this._bytes.slice(base, base + length);
        if (this._layout.sized()) {
            setSlot(table, 0, this._size, length);
        }

        this.#release();
        this._written = table;

        return table;
    }

    /**
     * Gives the builder's bytes back to its arena, which the next builder
     * takes, or lets the buffer of its own go
     */
    #release(): void {
        const arena = this._arena;
        const base = this._base;
        if (this._bytes === arena.bytes) {
            const live = arena.live - 1;
            arena.live = live;
            if (live === 0) {
                arena.top = 0;
            } else if (base + this._capacity === arena.top) {
                arena.top = base;
            }
        }

        this._bytes = EMPTY;
        this._view = EMPTY_VIEW;
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
