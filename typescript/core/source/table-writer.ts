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
    padding,
    TableLayout
} from './layout.ts';
import {
    storeFloat32,
    storeFloat64,
    storeInt8,
    storeInt16,
    storeInt32,
    storeInt64
} from './scalars.ts';
import { assertStructSize } from './table-reader.ts';
import type { ElementSize, OffsetSize } from './types.ts';

/**
 * Counts the bytes of a table, so that a {@link TableWriter} can write it
 * into bytes of exactly that length. It's for code that knows a table's
 * fields before it writes them, such as the code that tafla-gen generates,
 * which writes tables without recording their fields as `TableBuilder` does.
 *
 * The fields are added in order, each with its type's method, which takes
 * what decides its length: nothing for a scalar, the count of an array, the
 * string itself. A nested table is added with `addTable()`, and a vector
 * with `addVector()`, and the fields of each such table then follow between
 * `startTable()` and `endTable()`. The sizer pads arrays and nested tables
 * as the writer pads them, so a writer that takes the same fields in the same
 * order writes each where the sizer counted it.
 */
export class TableSizer {
    readonly #layout: TableLayout;
    readonly #version: number | undefined;
    readonly #fields: number;
    /** The offset size, the width of offsets, lengths and header slots */
    readonly #size: OffsetSize;
    /** The table's length so far, which is where the next field goes */
    #position: number;
    /** The start of the table being sized, which arrays are aligned from */
    #table = 0;

    /**
     * @param fields - The number of fields that the table holds.
     * @param layout - The table's header and offset type, which its readers
     * must use too.
     * @param version - The version that the writer writes to the header,
     * required when the layout is versioned.
     * @throws {TypeError} If `version` is given for a layout without a version
     * slot, or missing for one with it.
     * @throws {RangeError} If `fields` isn't a non-negative integer, `version`
     * isn't an integer that fits in the layout's offset type, or the header
     * alone is longer than the layout addresses.
     */
    constructor(fields: number, layout?: TableLayout<false>);
    constructor(fields: number, layout: TableLayout<true>, version: number);
    constructor(
        fields: number,
        layout: TableLayout = TableLayout.DEFAULT,
        version?: number
    ) {
        this.#layout = layout;
        this.#version = version;
        this.#fields = fields;
        this.#size = layout.offsetSize();
        this.#position = headerLength(fields, layout, version);
    }

    /**
     * The byte length of the table, with every field added
     *
     * @throws {RangeError} If the table is too long for its offset type:
     * longer than 255 bytes, 64 KiB or 4 GiB.
     */
    byteLength(): number {
        const length = this.#position;
        assertAddressable(length, this.#layout);

        return length;
    }

    /**
     * Returns a writer of the table, which takes its fields in the order
     * that they were added. It writes into new bytes of the table's length,
     * or into the start of `bytes`, and its `bytes()` then returns the view
     * of them that holds the table.
     *
     * Bytes that are written again, as by the next table written into them,
     * change that view and what's read from it. The caller decides when, so
     * the bytes are theirs: the writer keeps no reference to them once it's
     * done. Reusing them saves allocating and zeroing new bytes: in Node 27,
     * about 0.2 µs for a table of 1-3 KB, and 12 µs for one of 1.2 MB, whose
     * new pages fault on their first write, but nothing for a table of up to
     * 64 bytes, which V8 allocates on its heap.
     *
     * @param bytes - Bytes to write the table into, from their start, which
     * needn't be zero.
     * @throws {RangeError} If the table is too long for its offset type, or
     * longer than `bytes`, before anything is written.
     */
    writer(bytes?: Uint8Array): TableWriter {
        const length = this.byteLength();

        return newTableWriter(
            bytes === undefined
                ? new Uint8Array(length)
                : tableBytes(bytes, length),
            this.#fields,
            this.#layout,
            this.#version
        );
    }

    /**
     * Starts the fields of a nested table or vector item of `fields` fields,
     * after its parent's `addTable()` or `addVector()`.
     *
     * @returns The parent's start, which `endTable()` takes back.
     * @throws {RangeError} If `fields` isn't a non-negative integer.
     */
    startTable(fields: number): number {
        assertCount(fields);

        const parent = this.#table;
        this.#table = this.#position;
        this.#position += fields * this.#size;

        return parent;
    }

    /** Ends the fields of the table that `startTable()` started */
    endTable(parent: number): void {
        this.#table = parent;
    }

    addInt8(): void {
        this.#position += SIZEOF_8;
    }

    addUint8(): void {
        this.#position += SIZEOF_8;
    }

    addInt16(): void {
        this.#position += SIZEOF_16;
    }

    addUint16(): void {
        this.#position += SIZEOF_16;
    }

    addInt32(): void {
        this.#position += SIZEOF_32;
    }

    addUint32(): void {
        this.#position += SIZEOF_32;
    }

    addInt64(): void {
        this.#position += SIZEOF_64;
    }

    addUint64(): void {
        this.#position += SIZEOF_64;
    }

    addFloat32(): void {
        this.#position += SIZEOF_32;
    }

    addFloat64(): void {
        this.#position += SIZEOF_64;
    }

    // The array adders take the count of the elements, and throw a RangeError
    // for one that isn't a non-negative integer

    addInt8Array(count: number): void {
        this.#addArray(count, SIZEOF_8, SIZEOF_8);
    }

    addUint8Array(count: number): void {
        this.#addArray(count, SIZEOF_8, SIZEOF_8);
    }

    addInt16Array(count: number): void {
        this.#addArray(count, SIZEOF_16, SIZEOF_16);
    }

    addUint16Array(count: number): void {
        this.#addArray(count, SIZEOF_16, SIZEOF_16);
    }

    addInt32Array(count: number): void {
        this.#addArray(count, SIZEOF_32, SIZEOF_32);
    }

    addUint32Array(count: number): void {
        this.#addArray(count, SIZEOF_32, SIZEOF_32);
    }

    addInt64Array(count: number): void {
        this.#addArray(count, SIZEOF_64, SIZEOF_64);
    }

    addUint64Array(count: number): void {
        this.#addArray(count, SIZEOF_64, SIZEOF_64);
    }

    addFloat32Array(count: number): void {
        this.#addArray(count, SIZEOF_32, SIZEOF_32);
    }

    addFloat64Array(count: number): void {
        this.#addArray(count, SIZEOF_64, SIZEOF_64);
    }

    /** Adds a string, as UTF-8 with lone surrogates replaced by U+FFFD */
    addString(string: string): void {
        this.#position += this.#size + utf8Length(string);
    }

    /**
     * Adds a struct of `size` bytes, which may be 0
     *
     * @throws {RangeError} If `size` isn't a non-negative integer.
     */
    addStruct(size: number): void {
        if (!(Number.isSafeInteger(size) && size >= 0)) {
            throw new RangeError(
                `A struct's size must be a non-negative integer, not ${size}`
            );
        }

        this.#position += size;
    }

    /**
     * Adds `count` structs of `size` bytes, padded so that they start at a
     * multiple of `alignment` from the start of the table
     *
     * @throws {RangeError} If `count` isn't a non-negative integer, `size` a
     * positive one, or `alignment` 1, 2, 4 or 8.
     */
    addStructArray(count: number, size: number, alignment: ElementSize): void {
        assertStructSize(size);
        assertAlignment(alignment);
        this.#addArray(count, size, alignment);
    }

    /**
     * Adds a nested table, padded to start at a multiple of `alignment`,
     * whose fields then follow between `startTable()` and `endTable()`
     *
     * @throws {RangeError} If `alignment` isn't 1, 2, 4 or 8.
     */
    addTable(alignment: ElementSize): void {
        assertAlignment(alignment);
        this.#position += padding(this.#position - this.#table, alignment);
    }

    /**
     * Adds a vector of `count` tables, whose fields then follow, each
     * between `startTable()` and `endTable()`
     *
     * @throws {RangeError} If `count` isn't a non-negative integer.
     */
    addVector(count: number): void {
        assertCount(count);
        this.#position += (count + 1) * this.#size;
    }

    /**
     * Adds `count` nested tables of `byteLength` bytes each, whose fields'
     * sizes the schema fixes, which the writer's caller writes whole: a
     * field's table, after `addTable()`, or a vector's items, after
     * `addVector()`. It takes the place of each table's `startTable()`,
     * adders and `endTable()`.
     *
     * It's for the code that tafla-gen generates, which knows each such
     * table's length, and may change with it.
     */
    addTableBytes(count: number, byteLength: number): void {
        this.#position += count * byteLength;
    }

    /**
     * Adds an array of `count` elements of `size` bytes, padded so that they
     * are aligned after the count
     */
    #addArray(count: number, size: number, alignment: ElementSize): void {
        assertCount(count);

        const start = this.#position + this.#size;
        this.#position =
            start + padding(start - this.#table, alignment) + count * size;
    }
}

/**
 * Writes a table that was written already, from its bytes as they are, in
 * place of the fields that `startTable()` and `endTable()` would enclose:
 * after its parent's `setTable()`, or as a vector item before its
 * `setItemEnd()`. `TableBuilder` copies the tables that it can't write in
 * place with it: tables nested as bytes, builders that were written already,
 * and builders whose layout has a header or another offset type. Generated
 * code writes every nested table in place, so it's internal to the package,
 * and `TableWriter` assigns it in its static block, which can reach its
 * private fields.
 *
 * @throws {RangeError} If the bytes don't fit, without writing any of them.
 */
// oxlint-disable-next-line import/no-mutable-exports -- Assigned once, in TableWriter's static block
export let copyTable: (writer: TableWriter, table: Uint8Array) => void;

// The scalar types, which are also the kinds of `TableBuilder`'s scalar fields
export const INT8 = 0;
export const UINT8 = 1;
export const INT16 = 2;
export const UINT16 = 3;
export const INT32 = 4;
export const UINT32 = 5;
export const INT64 = 6;
export const UINT64 = 7;
export const FLOAT32 = 8;
export const FLOAT64 = 9;

/**
 * Reserves field `index`, a scalar of `size` bytes, as a setter does before
 * it stores its value: it throws a `RangeError` if the scalar doesn't fit,
 * and otherwise writes its offset, moves past it, and returns its position
 * in `target()`, where `TableBuilder` then stores it with the setter's
 * store. Each is a function that inlines little, so that TurboFan inlines
 * the builder's scalars whole. Internal to the package, as
 * {@link copyTable} is.
 */
// oxlint-disable-next-line import/no-mutable-exports -- Assigned once, in TableWriter's static block
export let reserveScalar: (
    writer: TableWriter,
    index: number,
    size: ElementSize
) => number;

/**
 * Passed to `TableWriter`'s constructor by {@link newTableWriter}, whose
 * callers checked what the constructor would. Only the package can pass it,
 * so the constructor checks every other caller's arguments.
 */
const CHECKED = Symbol('checked');

/** `TableWriter`'s constructor, as `newTableWriter()` calls it */
type CheckedTableWriter = new (
    bytes: Uint8Array,
    fields: number,
    layout: TableLayout,
    version: number | undefined,
    checked: typeof CHECKED
) => TableWriter;

/**
 * Writes a table forward into bytes of exactly its length, which a
 * {@link TableSizer} counted. Its writer comes from `TableSizer.writer()`, and
 * takes the fields that the sizer added, in the same order, each with its
 * index in its table.
 *
 * Each field's offset is written as the writer reaches the field. A nested
 * table follows its parent's `setTable()`, between `startTable()` and
 * `endTable()`; the items of a vector follow its `setVector()` the same way,
 * each followed by `setItemEnd()`, which writes where it ends. `TableBuilder`
 * writes through a writer too, once it has recorded its fields and counted
 * their length, so the two write the same bytes for the same fields.
 *
 * It's the target of the code that tafla-gen generates, whose calls follow
 * from the schema, so it doesn't check them again: fields are set in index
 * order, each once, as the sizer added them, and each vector item is followed
 * by its `setItemEnd()`. A field index outside its table, or an item outside
 * its vector, writes an offset or end over another slot or field, so the
 * table's bytes are undefined, though never outside them: a slot past them
 * isn't written. Hand-written tables use `TableBuilder`, which checks every
 * field. Counts that aren't non-negative integers, and alignments other than
 * 1, 2, 4 or 8, throw a `RangeError` before the writer moves.
 *
 * Every write stays within the bytes, even when they're part of a larger
 * buffer. A field that doesn't fit in them, as when it's longer than the one
 * that the sizer counted, throws a `RangeError` and leaves the writer where
 * it was, without writing any of the field: its offset, its count or length,
 * or its values. Only a string, whose UTF-8 length is known once it's
 * written, has then written what fits of it after the position, which may
 * end with the first bytes of a character. An array is written as long as
 * its `length` was when the setter read it, and one that has grown by the
 * time its elements are written throws the same way.
 *
 * Typed arrays of a field's element type are copied as bytes; any other
 * array-like, such as a `number[]` or a typed array of another type, is
 * converted as the typed array's constructor would. Values that don't fit
 * their type are written as `DataView`'s setters write them, and a `number`
 * given for an `int64` throws a `TypeError`, as they do. A scalar is checked
 * before its value is converted, so one that doesn't fit throws the
 * `RangeError` whatever its value.
 */
export class TableWriter {
    // Scalars, offsets and lengths are written as bytes rather than through
    // a DataView. V8 keeps a Uint8Array of up to 64 bytes on its heap until
    // something asks for its ArrayBuffer, as a DataView does: in Node 26 that
    // took 140 ns of the 290 ns that writing a Point of 60 bytes took, and
    // writing it as bytes took 140 ns in all. A vector of 100 tables of
    // scalars took 5-15% longer, which arrays of structs write faster anyway.
    readonly #bytes: Uint8Array;
    /** The offset size, the width of offsets, lengths and header slots */
    readonly #size: OffsetSize;
    readonly #headerSize: number;
    /** Where the next field goes */
    #position: number;
    /** The start of the table being written, which its offsets count from */
    #table = 0;
    /** The position of that table's first offset, after the root's header */
    #slots: number;

    static {
        copyTable = (writer, table) => writer.#copyTable(table);
        reserveScalar = TableWriter.#reserveScalar;
    }

    /**
     * Returns bytes for a table of `fields` fields in `layout`, which has no
     * version slot, `length` bytes long, with its header written: new bytes,
     * which are zero, or the start of `bytes`, which needn't be. The caller
     * writes every other byte, but for the zero bytes of new ones: each
     * field's offset, and the field.
     *
     * It's for the code that tafla-gen generates, which writes a message of
     * fields whose sizes the schema fixes, and a string after them, at
     * positions that it works out, without a sizer and a writer. It may
     * change with the generator.
     *
     * @throws {TypeError} If the layout has a version slot.
     * @throws {RangeError} As `TableSizer`'s constructor and its `writer()`
     * throw: if `fields` isn't a non-negative integer, the header alone is
     * longer than the layout addresses, `length` is longer than it addresses,
     * or longer than `bytes`, before writing any of them.
     */
    static tableBytes(
        fields: number,
        layout: TableLayout<false>,
        length: number,
        bytes?: Uint8Array
    ): Uint8Array {
        return newTableBytes(fields, layout, undefined, length, bytes);
    }

    /**
     * Returns bytes for a table of `fields` fields in `layout`, `length` bytes
     * long, with its header written, `version` included, as `tableBytes()`
     * does for a layout without a version slot.
     *
     * @throws {TypeError} If the layout has no version slot.
     * @throws {RangeError} As `tableBytes()` throws, and if `version` isn't an
     * integer that fits in the layout's offset type.
     */
    static versionedTableBytes(
        fields: number,
        layout: TableLayout<true>,
        version: number,
        length: number,
        bytes?: Uint8Array
    ): Uint8Array {
        return newTableBytes(fields, layout, version, length, bytes);
    }

    /**
     * Writes the header of a table of `fields` fields into `bytes`, which
     * must be as long as the table, as `TableSizer.writer()` makes them. They
     * needn't be zero: the writer writes every byte of the table, padding
     * included.
     *
     * @throws {TypeError} If `version` is given for a layout without a version
     * slot, or missing for one with it.
     * @throws {RangeError} If `fields` isn't a non-negative integer, `version`
     * doesn't fit in the layout's offset type, or `bytes` are longer than the
     * layout addresses.
     */
    constructor(bytes: Uint8Array, fields: number, layout?: TableLayout<false>);
    constructor(
        bytes: Uint8Array,
        fields: number,
        layout: TableLayout<true>,
        version: number
    );
    constructor(
        bytes: Uint8Array,
        fields: number,
        layout: TableLayout = TableLayout.DEFAULT,
        version?: number,
        checked?: typeof CHECKED
    ) {
        if (checked !== CHECKED) {
            assertWritable(bytes, fields, layout, version);
        }

        const size = layout.offsetSize();
        const headerSize = layout.headerSize();
        this.#bytes = bytes;
        this.#size = size;
        this.#headerSize = headerSize;
        this.#slots = headerSize;
        this.#position = headerSize + fields * size;

        writeHeader(bytes, layout, version);
    }

    /**
     * Returns the table's bytes.
     *
     * @throws {RangeError} If the fields didn't fill the bytes, as when they
     * differ from those that the sizer counted.
     */
    bytes(): Uint8Array {
        if (this.#position !== this.#bytes.length) {
            throw new RangeError(
                `The fields wrote ${this.#position} of the table's ${this.#bytes.length} bytes`
            );
        }

        return this.#bytes;
    }

    /**
     * Starts the fields of a nested table or vector item of `fields` fields,
     * after its parent's `setTable()` or `setVector()`.
     *
     * @returns The parent's start, which `endTable()` takes back.
     * @throws {RangeError} If `fields` isn't a non-negative integer.
     */
    startTable(fields: number): number {
        assertCount(fields);

        const parent = this.#table;
        this.#table = this.#position;
        this.#slots = this.#position;
        this.#position += fields * this.#size;

        return parent;
    }

    /** Ends the fields of the table that `startTable()` started */
    endTable(parent: number): void {
        this.#table = parent;
        // Only the root starts at 0, and only its offsets follow a header
        this.#slots = parent === 0 ? this.#headerSize : parent;
    }

    // A scalar's room is checked, and its value converted and stored, before
    // its offset is written, so that one that doesn't fit, or a number for an
    // int64, writes nothing. One that doesn't fit throws before its value is
    // converted.

    setInt8(index: number, value: number): void {
        assertFits(this.#bytes, this.#position + SIZEOF_8);
        storeInt8(this.#bytes, this.#position, value);
        this.#setOffset(index);
        this.#position += SIZEOF_8;
    }

    setUint8(index: number, value: number): void {
        assertFits(this.#bytes, this.#position + SIZEOF_8);
        storeInt8(this.#bytes, this.#position, value);
        this.#setOffset(index);
        this.#position += SIZEOF_8;
    }

    setInt16(index: number, value: number): void {
        assertFits(this.#bytes, this.#position + SIZEOF_16);
        storeInt16(this.#bytes, this.#position, value);
        this.#setOffset(index);
        this.#position += SIZEOF_16;
    }

    setUint16(index: number, value: number): void {
        assertFits(this.#bytes, this.#position + SIZEOF_16);
        storeInt16(this.#bytes, this.#position, value);
        this.#setOffset(index);
        this.#position += SIZEOF_16;
    }

    setInt32(index: number, value: number): void {
        assertFits(this.#bytes, this.#position + SIZEOF_32);
        storeInt32(this.#bytes, this.#position, value);
        this.#setOffset(index);
        this.#position += SIZEOF_32;
    }

    setUint32(index: number, value: number): void {
        assertFits(this.#bytes, this.#position + SIZEOF_32);
        storeInt32(this.#bytes, this.#position, value);
        this.#setOffset(index);
        this.#position += SIZEOF_32;
    }

    setInt64(index: number, value: bigint): void {
        assertFits(this.#bytes, this.#position + SIZEOF_64);
        storeInt64(this.#bytes, this.#position, value);
        this.#setOffset(index);
        this.#position += SIZEOF_64;
    }

    setUint64(index: number, value: bigint): void {
        assertFits(this.#bytes, this.#position + SIZEOF_64);
        storeInt64(this.#bytes, this.#position, value);
        this.#setOffset(index);
        this.#position += SIZEOF_64;
    }

    setFloat32(index: number, value: number): void {
        assertFits(this.#bytes, this.#position + SIZEOF_32);
        storeFloat32(this.#bytes, this.#position, value);
        this.#setOffset(index);
        this.#position += SIZEOF_32;
    }

    setFloat64(index: number, value: number): void {
        assertFits(this.#bytes, this.#position + SIZEOF_64);
        storeFloat64(this.#bytes, this.#position, value);
        this.#setOffset(index);
        this.#position += SIZEOF_64;
    }

    setInt8Array(index: number, values: ArrayLike<number> | Int8Array): void {
        const count = values.length;
        const elements = this.#arrayElements(count, SIZEOF_8, SIZEOF_8);

        setInt8Elements(this.#bytes, elements, values, count);
        this.#endArray(index, count, elements, count * SIZEOF_8);
    }

    setUint8Array(index: number, values: ArrayLike<number> | Uint8Array): void {
        const count = values.length;
        const elements = this.#arrayElements(count, SIZEOF_8, SIZEOF_8);

        setUint8Elements(this.#bytes, elements, values, count);
        this.#endArray(index, count, elements, count * SIZEOF_8);
    }

    setInt16Array(index: number, values: ArrayLike<number> | Int16Array): void {
        const count = values.length;
        const elements = this.#arrayElements(count, SIZEOF_16, SIZEOF_16);

        setInt16Elements(this.#bytes, elements, values, count);
        this.#endArray(index, count, elements, count * SIZEOF_16);
    }

    setUint16Array(
        index: number,
        values: ArrayLike<number> | Uint16Array
    ): void {
        const count = values.length;
        const elements = this.#arrayElements(count, SIZEOF_16, SIZEOF_16);

        setUint16Elements(this.#bytes, elements, values, count);
        this.#endArray(index, count, elements, count * SIZEOF_16);
    }

    setInt32Array(index: number, values: ArrayLike<number> | Int32Array): void {
        const count = values.length;
        const elements = this.#arrayElements(count, SIZEOF_32, SIZEOF_32);

        setInt32Elements(this.#bytes, elements, values, count);
        this.#endArray(index, count, elements, count * SIZEOF_32);
    }

    setUint32Array(
        index: number,
        values: ArrayLike<number> | Uint32Array
    ): void {
        const count = values.length;
        const elements = this.#arrayElements(count, SIZEOF_32, SIZEOF_32);

        setUint32Elements(this.#bytes, elements, values, count);
        this.#endArray(index, count, elements, count * SIZEOF_32);
    }

    setInt64Array(
        index: number,
        values: ArrayLike<bigint> | BigInt64Array
    ): void {
        const count = values.length;
        const elements = this.#arrayElements(count, SIZEOF_64, SIZEOF_64);

        setInt64Elements(this.#bytes, elements, values, count);
        this.#endArray(index, count, elements, count * SIZEOF_64);
    }

    setUint64Array(
        index: number,
        values: ArrayLike<bigint> | BigUint64Array
    ): void {
        const count = values.length;
        const elements = this.#arrayElements(count, SIZEOF_64, SIZEOF_64);

        setUint64Elements(this.#bytes, elements, values, count);
        this.#endArray(index, count, elements, count * SIZEOF_64);
    }

    setFloat32Array(
        index: number,
        values: ArrayLike<number> | Float32Array
    ): void {
        const count = values.length;
        const elements = this.#arrayElements(count, SIZEOF_32, SIZEOF_32);

        setFloat32Elements(this.#bytes, elements, values, count);
        this.#endArray(index, count, elements, count * SIZEOF_32);
    }

    setFloat64Array(
        index: number,
        values: ArrayLike<number> | Float64Array
    ): void {
        const count = values.length;
        const elements = this.#arrayElements(count, SIZEOF_64, SIZEOF_64);

        setFloat64Elements(this.#bytes, elements, values, count);
        this.#endArray(index, count, elements, count * SIZEOF_64);
    }

    /** Writes `string` as UTF-8, with lone surrogates replaced by U+FFFD */
    setString(index: number, string: string): void {
        // Its UTF-8 length is known once it's written, so a string that
        // doesn't fit throws before its offset and length are written
        const start = this.#position + this.#size;
        assertFits(this.#bytes, start);
        const length = encodeInto(string, this.#bytes, start);
        this.#setOffset(index);
        this.#setLength(this.#position, length);
        this.#position = start + length;
    }

    /**
     * Writes a struct field from its bytes: its scalars, little-endian, one
     * after another, without padding
     */
    setStruct(index: number, bytes: Uint8Array): void {
        assertFits(this.#bytes, this.#position + bytes.length);
        this.#setOffset(index);

        this.#bytes.set(bytes, this.#position);
        this.#position += bytes.length;
    }

    /**
     * Writes an array of structs of `size` bytes each, from their bytes one
     * after another, padded so that they start at a multiple of `alignment`
     * from the start of the table
     *
     * @throws {RangeError} If `size` isn't a positive integer, `bytes` aren't
     * a whole number of structs, or `alignment` isn't 1, 2, 4 or 8.
     */
    setStructArray(
        index: number,
        bytes: Uint8Array,
        size: number,
        alignment: ElementSize
    ): void {
        const count = structCount(bytes.length, size);
        assertAlignment(alignment);
        const elements = this.#arrayElements(count, size, alignment);

        this.#bytes.set(bytes, elements);
        this.#endArray(index, count, elements, bytes.length);
    }

    /**
     * Writes the offset of a nested table, padded to start at a multiple of
     * `alignment`, whose fields then follow between `startTable()` and
     * `endTable()`
     *
     * @throws {RangeError} If `alignment` isn't 1, 2, 4 or 8.
     */
    setTable(index: number, alignment: ElementSize): void {
        assertAlignment(alignment);
        this.#pad(padding(this.#position - this.#table, alignment));
        this.#setOffset(index);
    }

    /**
     * Writes the offset and count of a vector of `count` tables, whose items
     * then follow, each between `startTable()` and `endTable()` and then
     * `setItemEnd()`.
     *
     * @returns The vector's position, which `setItemEnd()` takes.
     * @throws {RangeError} If `count` isn't a non-negative integer.
     */
    setVector(index: number, count: number): number {
        assertCount(count);
        assertFits(this.#bytes, this.#position + (count + 1) * this.#size);
        this.#setOffset(index);

        const vector = this.#position;
        this.#setLength(vector, count);
        this.#position += (count + 1) * this.#size;

        return vector;
    }

    /** Writes the end of item `item` of the vector, which was just written */
    setItemEnd(vector: number, item: number): void {
        this.#setLength(
            vector + (item + 1) * this.#size,
            this.#position - vector
        );
    }

    // The writer's caller writes the fields that the next three reserve,
    // whose sizes the schema fixes, into target() at the position that each
    // returns, as the code that tafla-gen generates writes them, at
    // positions that it works out. They may change with the generator.

    /**
     * Reserves a nested table of `byteLength` bytes, which the sizer's
     * `addTableBytes()` added, after its parent's `setTable()`, and returns
     * its position in `target()`. The caller writes it whole there: its
     * offsets, and its fields at them. It takes the place of the table's
     * `startTable()`, setters and `endTable()`.
     *
     * @throws {RangeError} If the table doesn't fit, without moving.
     */
    reserveTableBytes(byteLength: number): number {
        const start = this.#position;
        assertFits(this.#bytes, start + byteLength);
        this.#position = start + byteLength;

        return start;
    }

    /**
     * Writes the offset and count of field `index`, a vector of `count`
     * tables of `byteLength` bytes each, which the sizer's `addVector()` and
     * `addTableBytes()` added, and the end of each, and returns the position
     * of the first in `target()`. The caller writes each whole, one after
     * another from there. It takes the place of each item's `startTable()`,
     * setters, `endTable()` and `setItemEnd()`.
     *
     * @throws {RangeError} If `count` isn't a non-negative integer, or the
     * vector doesn't fit, without writing any of it.
     */
    reserveTableVector(
        index: number,
        count: number,
        byteLength: number
    ): number {
        assertCount(count);
        const size = this.#size;
        const vector = this.#position;
        const items = vector + (count + 1) * size;
        assertFits(this.#bytes, items + count * byteLength);
        this.#setOffset(index);

        this.#setLength(vector, count);
        let end = (count + 1) * size;
        for (let item = 1; item <= count; item++) {
            end += byteLength;
            this.#setLength(vector + item * size, end);
        }
        this.#position = items + count * byteLength;

        return items;
    }

    /**
     * Writes the offset and count of field `index`, an array of `count`
     * structs of `size` bytes each, padded so that they start at a multiple
     * of `alignment` from the start of the table, and returns their position
     * in `target()`. The caller writes each struct's scalars there,
     * little-endian, one struct after another, rather than collecting their
     * bytes for `setStructArray()`.
     *
     * @throws {RangeError} If `count` isn't a non-negative integer, `size` a
     * positive one, `alignment` 1, 2, 4 or 8, or the array doesn't fit,
     * without writing any of it.
     */
    reserveStructArray(
        index: number,
        count: number,
        size: number,
        alignment: ElementSize
    ): number {
        assertCount(count);
        assertStructSize(size);
        assertAlignment(alignment);
        const elements = this.#arrayElements(count, size, alignment);

        this.#endArray(index, count, elements, count * size);

        return elements;
    }

    /**
     * The bytes that the writer writes the table into, from its start, which
     * the positions that the reserving methods return are in: new bytes, or
     * the start of those that `TableSizer.writer(bytes)` took
     */
    target(): Uint8Array {
        return this.#bytes;
    }

    /** {@link reserveScalar}, which reaches the writer's fields */
    static #reserveScalar(
        this: void,
        writer: TableWriter,
        index: number,
        size: ElementSize
    ): number {
        const bytes = writer.#bytes;
        const position = writer.#position;
        assertFits(bytes, position + size);

        const offsetSize = writer.#size;
        setSlot(
            bytes,
            writer.#slots + index * offsetSize,
            offsetSize,
            position - writer.#table
        );
        writer.#position = position + size;

        return position;
    }

    /** {@link copyTable}'s write, which only this class's fields can reach */
    #copyTable(table: Uint8Array): void {
        assertFits(this.#bytes, this.#position + table.length);

        this.#bytes.set(table, this.#position);
        this.#position += table.length;
    }

    /**
     * Where an array's elements go, after the padding that aligns them and
     * its count, once its count and its elements fit. It writes nothing, so
     * that elements that throw leave the writer where it was.
     */
    #arrayElements(
        count: number,
        elementSize: number,
        alignment: ElementSize
    ): number {
        const start = this.#position + this.#size;
        const elements = start + padding(start - this.#table, alignment);
        assertFits(this.#bytes, elements + count * elementSize);

        return elements;
    }

    /**
     * Writes the offset and count of an array whose elements were written
     * at `elements`, and moves past them
     */
    #endArray(
        index: number,
        count: number,
        elements: number,
        byteLength: number
    ): void {
        this.#pad(elements - this.#size - this.#position);
        this.#setOffset(index);
        this.#setLength(this.#position, count);
        this.#position = elements + byteLength;
    }

    /**
     * Writes `length` bytes of zero padding at the position, and moves past
     * them. The bytes needn't be new, so their padding is written rather
     * than left as it was.
     *
     * Most arrays and nested tables need none, and calling `fill()` for no
     * bytes took a fifth of the time that writing a vector of 100 tables of
     * scalars took, in Node 27 and d8.
     */
    #pad(length: number): void {
        if (length === 0) {
            return;
        }

        const position = this.#position;
        this.#bytes.fill(0, position, position + length);
        this.#position = position + length;
    }

    #setOffset(index: number): void {
        this.#setLength(
            this.#slots + index * this.#size,
            this.#position - this.#table
        );
    }

    /** Writes an offset, length or count at `position`, as `setSlot()` does */
    #setLength(position: number, value: number): void {
        setSlot(this.#bytes, position, this.#size, value);
    }
}

/**
 * Writes an offset, length, count or header slot of `size` bytes at
 * `position`, little-endian. A value too large for the width is wrapped, as
 * DataView's setters wrap it; the sizer rejects a table long enough to need
 * one.
 */
function setSlot(
    bytes: Uint8Array,
    position: number,
    size: OffsetSize,
    value: number
): void {
    if (size === SIZEOF_32) {
        storeInt32(bytes, position, value);
    } else if (size === SIZEOF_16) {
        storeInt16(bytes, position, value);
    } else {
        storeInt8(bytes, position, value);
    }
}

/**
 * Writes a table's header at the start of `bytes`: their length when the
 * layout is sized, and then `version` when it's versioned
 */
function writeHeader(
    bytes: Uint8Array,
    layout: TableLayout,
    version: number | undefined
): void {
    const size = layout.offsetSize();

    let slot = 0;
    if (layout.sized()) {
        setSlot(bytes, slot, size, bytes.length);
        slot += size;
    }

    if (version !== undefined) {
        setSlot(bytes, slot, size, version);
    }
}

/**
 * `TableWriter.tableBytes()` and `versionedTableBytes()`, which check the
 * header and the length as a sizer and its `writer()` check them.
 *
 * A layout without header slots, as the code that tafla-gen generates for
 * most messages has, never calls `writeHeader()`, so V8 leaves it out of
 * the bytecode that it inlines into the generated serializers.
 */
function newTableBytes(
    fields: number,
    layout: TableLayout,
    version: number | undefined,
    length: number,
    bytes: Uint8Array | undefined
): Uint8Array {
    headerLength(fields, layout, version);
    assertAddressable(length, layout);

    const table =
        bytes === undefined
            ? new Uint8Array(length)
            : tableBytes(bytes, length);
    if (layout.headerSize() !== 0) {
        writeHeader(table, layout, version);
    }

    return table;
}

/**
 * Checks that a table of `length` bytes is no longer than offsets of the
 * layout's type address
 *
 * @throws {RangeError} If it's longer: past 255 bytes, 64 KiB or 4 GiB.
 */
function assertAddressable(length: number, layout: TableLayout): void {
    if (!(length <= maxLength(layout.offsetSize()))) {
        throw tableLengthError(length, layout);
    }
}

function tableLengthError(length: number, layout: TableLayout): RangeError {
    return new RangeError(
        `The table's fields make it ${length} bytes, too long for ${layout.offsetType()} offsets`
    );
}

/**
 * A writer of a table of `fields` fields into `bytes`, for `TableSizer` and
 * `TableBuilder`. Their constructors checked the fields and the version
 * against the layout, and they checked the table's length as they counted
 * it, so the writer's constructor doesn't check them again. That saves about
 * 15 ns a table: in Node 27, `writer()` took 34 ns rather than 49 ns, and a
 * generated record's write 0.140 µs rather than 0.156 µs.
 */
export function newTableWriter(
    bytes: Uint8Array,
    fields: number,
    layout: TableLayout,
    version: number | undefined
): TableWriter {
    return new (TableWriter as CheckedTableWriter)(
        bytes,
        fields,
        layout,
        version,
        CHECKED
    );
}

/**
 * The view of the start of `bytes` that a table of `length` bytes is written
 * into
 *
 * @throws {RangeError} If the table is longer than `bytes`.
 */
function tableBytes(bytes: Uint8Array, length: number): Uint8Array {
    if (length > bytes.length) {
        throw new RangeError(
            `The table is ${length} bytes, longer than the ${bytes.length} bytes given`
        );
    }

    return bytes.subarray(0, length);
}

/**
 * Checks what `TableWriter`'s constructor takes from a caller outside the
 * package: the header, as `headerLength()` does, and the bytes' length
 * against the offset type
 */
function assertWritable(
    bytes: Uint8Array,
    fields: number,
    layout: TableLayout,
    version: number | undefined
): void {
    headerLength(fields, layout, version);

    if (!(bytes.length <= maxLength(layout.offsetSize()))) {
        throw new RangeError(
            `A table of ${bytes.length} bytes is too long for ${layout.offsetType()} offsets`
        );
    }
}

/**
 * Checks that the table's bytes reach `end`, before a field writes up to it.
 * An array's elements are written through a view of the bytes' buffer, which
 * would write past bytes that are only part of it.
 *
 * @throws {RangeError} If they don't.
 */
function assertFits(bytes: Uint8Array, end: number): void {
    if (!(end <= bytes.length)) {
        throw tooLong(bytes, end);
    }
}

function tooLong(bytes: Uint8Array, end: number): RangeError {
    return new RangeError(
        `The fields need ${end} bytes, more than the table's ${bytes.length}`
    );
}

/**
 * The number of structs of `size` bytes in `byteLength` bytes
 *
 * @throws {RangeError} If `size` isn't a positive integer, or the bytes
 * aren't a whole number of structs.
 */
export function structCount(byteLength: number, size: number): number {
    assertStructSize(size);
    if (byteLength % size !== 0) {
        throw new RangeError(
            `${byteLength} bytes aren't a whole number of ${size}-byte structs`
        );
    }

    return byteLength / size;
}
