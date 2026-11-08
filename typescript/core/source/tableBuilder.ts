import { ByteBuffer, structCount } from './byteBuffer.ts';
import { SIZEOF_8, SIZEOF_16, SIZEOF_32, SIZEOF_64 } from './constants.ts';
import { utf8Length } from './encoding.ts';
import {
    assertAlignment,
    headerLength,
    maxLength,
    padding,
    TableLayout
} from './layout.ts';
import { copyTable, newTableWriter, type TableWriter } from './tableWriter.ts';
import type { ElementSize, OffsetSize, OffsetType } from './types.ts';

// The kinds of field, which finalize() switches on to write each value
const INT8 = 0;
const UINT8 = 1;
const INT16 = 2;
const UINT16 = 3;
const INT32 = 4;
const UINT32 = 5;
const INT64 = 6;
const UINT64 = 7;
const FLOAT32 = 8;
const FLOAT64 = 9;
const INT8_ARRAY = 10;
const UINT8_ARRAY = 11;
const INT16_ARRAY = 12;
const UINT16_ARRAY = 13;
const INT32_ARRAY = 14;
const UINT32_ARRAY = 15;
const INT64_ARRAY = 16;
const UINT64_ARRAY = 17;
const FLOAT32_ARRAY = 18;
const FLOAT64_ARRAY = 19;
const STRING = 20;
const TABLE = 21;
const VECTOR = 22;
const STRUCT = 23;
const STRUCT_ARRAY = 24;

/**
 * Builds a table: a header with one offset per field, after the table's total
 * length and version when its layout has them, followed by the fields in the
 * order they're set. Arrays, strings and vectors start with their count, and a
 * vector then holds the end of each item. Arrays of scalars and of structs are
 * padded so their elements are aligned from the start of the table. Fields are read back by index with `ByteBuffer.rootOffset()`, or
 * `ByteBuffer.fieldOffset()` for a table nested in another.
 *
 * Setting a field records it and counts the table's length. `finalize()` then
 * writes every field, in order, through a {@link TableWriter} into bytes of
 * exactly that length, so the values must not change in between: arrays whose
 * lengths changed so that the fields no longer fit the table, or no longer
 * fill it, make it throw a `RangeError` and leave the builder unfinalized.
 * Tables nested from their builders are written by the same writer, straight
 * into this one. A setter throws a `RangeError`, and leaves the builder as it
 * was, when its field would make the table longer than the layout's
 * `offsetType` addresses: 255 bytes, 64 KiB or 4 GiB. Typed arrays of a
 * field's element type are copied as bytes; any other array-like, such as a
 * `number[]` or a typed array of another type, is converted as the typed
 * array's constructor would.
 */
export class TableBuilder {
    /**
     * The table once written: its bytes, and then the buffer of them that
     * `finalize()` returns
     */
    #written: Uint8Array | ByteBuffer | undefined;

    readonly #layout: TableLayout;
    readonly #version: number | undefined;
    /** The offset size, the width of offsets, lengths and header slots */
    readonly #size: OffsetSize;
    readonly #fields: number;
    /** The longest table that the layout's offsets and lengths address */
    readonly #maxLength: number;
    /** The table's length so far, which is where the next field goes */
    #offset: number;
    /**
     * The largest alignment that a field needs from the table's start, which
     * a parent pads this table to when it nests it
     */
    #alignment: ElementSize = SIZEOF_8;

    // A record of three slots per field, in one plain array sized up front:
    // its kind, the alignment that it's padded to or 0, and its value. The
    // writer pads each field and writes its offset as it reaches it, so the
    // record needn't hold either. Small typed arrays took four times as long
    // to allocate, and an array per slot made setting a small table's fields
    // take twice as long. The slots are literals, since named constants made
    // building a small table 5-9% slower in d8.
    #count = 0;
    #records: unknown[];

    /**
     * @param fields - The number of fields that the table will hold.
     * @param layout - The table's header and offset type, which its readers
     * must use too.
     * @param version - The version to write to the header, required when the
     * layout is versioned. Readers can check it with `ByteBuffer.getVersion()`
     * before reading any field.
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
        const size = layout.offsetSize();

        this.#layout = layout;
        this.#version = version;
        this.#size = size;
        this.#fields = fields;
        this.#maxLength = maxLength(size);
        this.#offset = headerLength(fields, layout, version);

        this.#records = new Array(fields * 3);
    }

    /**
     * Writes the table, once, and returns a buffer of its bytes; later calls
     * return the same buffer.
     *
     * @throws {RangeError} If fewer fields were set than the table holds, or
     * the fields no longer fit the table or fill it.
     */
    finalize(): ByteBuffer {
        const written = this.#written ?? this.#write();
        if (written instanceof ByteBuffer) {
            return written;
        }

        const buffer = new ByteBuffer(written, this.#layout);
        this.#written = buffer;

        return buffer;
    }

    /**
     * Writes the table, once, as `finalize()` does, and returns its bytes,
     * which `finalize()` then returns a buffer of
     *
     * @throws {RangeError} If fewer fields were set than the table holds, or
     * the fields no longer fit the table or fill it.
     */
    bytes(): Uint8Array {
        const written = this.#written ?? this.#write();
        return written instanceof ByteBuffer ? written.bytes() : written;
    }

    setInt8(value: number): void {
        this.#addField(INT8, value, SIZEOF_8, 0, 0);
    }

    setUint8(value: number): void {
        this.#addField(UINT8, value, SIZEOF_8, 0, 0);
    }

    setInt16(value: number): void {
        this.#addField(INT16, value, SIZEOF_16, 0, 0);
    }

    setUint16(value: number): void {
        this.#addField(UINT16, value, SIZEOF_16, 0, 0);
    }

    setInt32(value: number): void {
        this.#addField(INT32, value, SIZEOF_32, 0, 0);
    }

    setUint32(value: number): void {
        this.#addField(UINT32, value, SIZEOF_32, 0, 0);
    }

    setInt64(value: bigint): void {
        this.#addField(INT64, value, SIZEOF_64, 0, 0);
    }

    setUint64(value: bigint): void {
        this.#addField(UINT64, value, SIZEOF_64, 0, 0);
    }

    setFloat32(value: number): void {
        this.#addField(FLOAT32, value, SIZEOF_32, 0, 0);
    }

    setFloat64(value: number): void {
        this.#addField(FLOAT64, value, SIZEOF_64, 0, 0);
    }

    // The array setters compute their byte lengths each for one element
    // type, so that the length loads see that type's inputs only. Converting
    // 27,028 floats from a number[] took 15-20 µs, against 3.5-5 µs to copy a
    // Float32Array, in Node and d8.

    /**
     * Sets an array field. An `Int8Array` is copied as bytes; any other
     * array-like, such as a `number[]` or another typed array, is converted as
     * `new Int8Array(values)` would, which takes several times as long.
     */
    setInt8Array(values: ArrayLike<number> | Int8Array): void {
        this.#addArray(INT8_ARRAY, values, values.length, SIZEOF_8);
    }

    /**
     * Sets an array field. A `Uint8Array` is copied as bytes; any other
     * array-like, such as a `number[]` or another typed array, is converted as
     * `new Uint8Array(values)` would, which takes several times as long.
     */
    setUint8Array(values: ArrayLike<number> | Uint8Array): void {
        this.#addArray(UINT8_ARRAY, values, values.length, SIZEOF_8);
    }

    /**
     * Sets an array field. An `Int16Array` is copied as bytes; any other
     * array-like, such as a `number[]` or another typed array, is converted as
     * `new Int16Array(values)` would, which takes several times as long.
     */
    setInt16Array(values: ArrayLike<number> | Int16Array): void {
        this.#addArray(
            INT16_ARRAY,
            values,
            values.length * SIZEOF_16,
            SIZEOF_16
        );
    }

    /**
     * Sets an array field. A `Uint16Array` is copied as bytes; any other
     * array-like, such as a `number[]` or another typed array, is converted as
     * `new Uint16Array(values)` would, which takes several times as long.
     */
    setUint16Array(values: ArrayLike<number> | Uint16Array): void {
        this.#addArray(
            UINT16_ARRAY,
            values,
            values.length * SIZEOF_16,
            SIZEOF_16
        );
    }

    /**
     * Sets an array field. An `Int32Array` is copied as bytes; any other
     * array-like, such as a `number[]` or another typed array, is converted as
     * `new Int32Array(values)` would, which takes several times as long.
     */
    setInt32Array(values: ArrayLike<number> | Int32Array): void {
        this.#addArray(
            INT32_ARRAY,
            values,
            values.length * SIZEOF_32,
            SIZEOF_32
        );
    }

    /**
     * Sets an array field. A `Uint32Array` is copied as bytes; any other
     * array-like, such as a `number[]` or another typed array, is converted as
     * `new Uint32Array(values)` would, which takes several times as long.
     */
    setUint32Array(values: ArrayLike<number> | Uint32Array): void {
        this.#addArray(
            UINT32_ARRAY,
            values,
            values.length * SIZEOF_32,
            SIZEOF_32
        );
    }

    /**
     * Sets an array field. A `BigInt64Array` is copied as bytes; any other
     * array-like, such as a `bigint[]` or another typed array, is converted as
     * `new BigInt64Array(values)` would, which takes several times as long.
     */
    setInt64Array(values: ArrayLike<bigint> | BigInt64Array): void {
        this.#addArray(
            INT64_ARRAY,
            values,
            values.length * SIZEOF_64,
            SIZEOF_64
        );
    }

    /**
     * Sets an array field. A `BigUint64Array` is copied as bytes; any other
     * array-like, such as a `bigint[]` or another typed array, is converted as
     * `new BigUint64Array(values)` would, which takes several times as long.
     */
    setUint64Array(values: ArrayLike<bigint> | BigUint64Array): void {
        this.#addArray(
            UINT64_ARRAY,
            values,
            values.length * SIZEOF_64,
            SIZEOF_64
        );
    }

    /**
     * Sets an array field. A `Float32Array` is copied as bytes; any other
     * array-like, such as a `number[]` or another typed array, is converted as
     * `new Float32Array(values)` would, which takes several times as long.
     */
    setFloat32Array(values: ArrayLike<number> | Float32Array): void {
        this.#addArray(
            FLOAT32_ARRAY,
            values,
            values.length * SIZEOF_32,
            SIZEOF_32
        );
    }

    /**
     * Sets an array field. A `Float64Array` is copied as bytes; any other
     * array-like, such as a `number[]` or another typed array, is converted as
     * `new Float64Array(values)` would, which takes several times as long.
     */
    setFloat64Array(values: ArrayLike<number> | Float64Array): void {
        this.#addArray(
            FLOAT64_ARRAY,
            values,
            values.length * SIZEOF_64,
            SIZEOF_64
        );
    }

    /** Writes `string` as UTF-8, with lone surrogates replaced by U+FFFD */
    setString(string: string): void {
        // The writer encodes it in place, so only its length is counted here
        this.#addField(STRING, string, this.#size + utf8Length(string), 0, 0);
    }

    /**
     * Sets a struct field from its bytes: its scalars, little-endian, one
     * after another, which are written as they are, without padding.
     */
    setStruct(bytes: Uint8Array): void {
        this.#addField(STRUCT, bytes, bytes.length, 0, 0);
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
        structCount(bytes.length, size);
        assertAlignment(alignment);

        this.#addField(
            STRUCT_ARRAY,
            [bytes, size],
            this.#size + bytes.length,
            padding(this.#offset + this.#size, alignment),
            alignment
        );
        this.#alignTo(alignment);
    }

    /**
     * Embeds a table, from its builder or its bytes, without a length prefix.
     * Readers find its fields with `ByteBuffer.fieldOffset()` from the field's
     * offset, so its layout must be {@link TableLayout.DEFAULT}.
     *
     * A builder must have all its fields set. `finalize()` writes it straight
     * into this table, without bytes of its own, so its values must not change
     * until then either. It can still be finalized or nested elsewhere too,
     * since a complete builder doesn't change. A builder that was finalized
     * already, or whose layout has a header or another `offsetType`, is
     * finalized and copied instead.
     *
     * The nested table's typed arrays are aligned from its own start, so it's
     * padded to start at a multiple of `alignment`, which they need to be read
     * as views rather than copies. For a builder, the default is the largest
     * alignment of its arrays and nested tables, or 1 if it has neither. For bytes, the default of 4 suits tables of 8-, 16-
     * and 32-bit elements; pass 8 for a table with 64-bit arrays.
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

        this.#addField(
            TABLE,
            table,
            TableBuilder.#lengthOf(table),
            padding(this.#offset, alignment),
            alignment
        );
        this.#alignTo(alignment);
    }

    /**
     * Writes a vector of tables, from their builders or their bytes, after
     * the end of each, which lets readers find any item without reading the
     * others. Builders must have all their fields set, and are written
     * straight into this table as `setTable()` writes them, except for those
     * finalized already and those whose layout has a header or another
     * `offsetType`, which are finalized and copied. Items aren't padded, so
     * their typed arrays are read as copies when they don't happen to be
     * aligned.
     *
     * @throws {RangeError} If a builder has fields left to set.
     */
    setVector(vector: ReadonlyArray<TableBuilder | Uint8Array>): void {
        let byteLength = this.#size + vector.length * this.#size;
        for (let i = 0; i < vector.length; i++) {
            byteLength += TableBuilder.#lengthOf(vector[i]);
        }

        this.#addField(VECTOR, vector, byteLength, 0, 0);
    }

    /** Pads the array so that its elements, after the length, are aligned */
    #addArray(
        kind: number,
        values: unknown,
        byteLength: number,
        alignment: ElementSize
    ): void {
        this.#addField(
            kind,
            values,
            this.#size + byteLength,
            padding(this.#offset + this.#size, alignment),
            alignment
        );
        this.#alignTo(alignment);
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
        return table.#offset;
    }

    #assertComplete(): void {
        if (this.#count !== this.#fields) {
            throw new RangeError(
                `Only ${this.#count} of the table's ${this.#fields} fields were set`
            );
        }
    }

    /**
     * Records a field of `byteLength` bytes after `padding` bytes, which pad
     * it to `alignment`. Checking the table's length here, rather than in
     * finalize(), reports a field too long for the layout from its own setter
     * and leaves the builder as it was. Every offset and length is shorter
     * than the table, so the table's length is the one value to check.
     *
     * @throws {RangeError} If the table already holds all its fields, or the
     * field would make it longer than the layout addresses.
     * @throws {TypeError} If the table is already written.
     */
    #addField(
        kind: number,
        value: unknown,
        byteLength: number,
        padding: number,
        alignment: ElementSize | 0
    ): void {
        if (this.#written !== undefined) {
            throw new TypeError('The table was already finalized');
        }

        const index = this.#count;
        if (index === this.#fields) {
            throw tooManyFields(this.#fields);
        }

        const length = this.#offset + padding + byteLength;
        if (!(length <= this.#maxLength)) {
            throw tooLong(index, length, this.#layout.offsetType());
        }

        const records = this.#records;
        const record = index * 3;
        records[record] = kind;
        records[record + 1] = alignment;
        records[record + 2] = value;

        this.#offset = length;
        this.#count = index + 1;
    }

    /**
     * Writes the table into bytes of the length that its fields counted
     *
     * @throws {RangeError} If fewer fields were set than the table holds, or
     * the fields no longer fit the bytes or fill them.
     */
    #write(): Uint8Array {
        this.#assertComplete();

        const writer = newTableWriter(
            new Uint8Array(this.#offset),
            this.#fields,
            this.#layout,
            this.#version
        );
        this.#writeFields(writer);
        const bytes = writer.bytes();

        // Release the values, which may be large arrays
        this.#records = [];
        this.#written = bytes;

        return bytes;
    }

    /** Writes the fields in order, each with its index in this table */
    #writeFields(writer: TableWriter): void {
        const records = this.#records;

        for (let index = 0; index < this.#count; index++) {
            const record = index * 3;
            const kind = records[record] as number;
            const alignment = records[record + 1] as ElementSize;
            const value = records[record + 2];

            if (kind === TABLE) {
                writer.setTable(index, alignment);
                this.#writeTable(writer, value as TableBuilder | Uint8Array);
            } else if (kind === VECTOR) {
                this.#writeVector(
                    writer,
                    index,
                    value as ReadonlyArray<TableBuilder | Uint8Array>
                );
            } else {
                writeField(writer, index, kind, value, alignment);
            }
        }
    }

    #writeVector(
        writer: TableWriter,
        index: number,
        items: ReadonlyArray<TableBuilder | Uint8Array>
    ): void {
        const vector = writer.setVector(index, items.length);

        for (let i = 0; i < items.length; i++) {
            this.#writeTable(writer, items[i]);
            writer.setItemEnd(vector, i);
        }
    }

    /**
     * Writes a builder straight into the writer, between `startTable()` and
     * `endTable()`, unless it was finalized already or its layout has a
     * header or another offset type, which a table nested in the writer's
     * doesn't. Those tables, and bytes, are copied.
     */
    #writeTable(writer: TableWriter, table: TableBuilder | Uint8Array): void {
        if (!(table instanceof TableBuilder)) {
            copyTable(writer, table);
        } else if (
            table.#written === undefined &&
            table.#size === this.#size &&
            table.#layout.headerSize() === 0
        ) {
            const parent = writer.startTable(table.#fields);
            table.#writeFields(writer);
            writer.endTable(parent);
        } else {
            copyTable(writer, table.bytes());
        }
    }
}

function writeField(
    writer: TableWriter,
    index: number,
    kind: number,
    value: unknown,
    alignment: ElementSize
): void {
    switch (kind) {
        case INT8:
            writer.setInt8(index, value as number);
            break;

        case UINT8:
            writer.setUint8(index, value as number);
            break;

        case INT16:
            writer.setInt16(index, value as number);
            break;

        case UINT16:
            writer.setUint16(index, value as number);
            break;

        case INT32:
            writer.setInt32(index, value as number);
            break;

        case UINT32:
            writer.setUint32(index, value as number);
            break;

        case INT64:
            writer.setInt64(index, value as bigint);
            break;

        case UINT64:
            writer.setUint64(index, value as bigint);
            break;

        case FLOAT32:
            writer.setFloat32(index, value as number);
            break;

        case FLOAT64:
            writer.setFloat64(index, value as number);
            break;

        case INT8_ARRAY:
            writer.setInt8Array(index, value as ArrayLike<number>);
            break;

        case UINT8_ARRAY:
            writer.setUint8Array(index, value as ArrayLike<number>);
            break;

        case INT16_ARRAY:
            writer.setInt16Array(index, value as ArrayLike<number>);
            break;

        case UINT16_ARRAY:
            writer.setUint16Array(index, value as ArrayLike<number>);
            break;

        case INT32_ARRAY:
            writer.setInt32Array(index, value as ArrayLike<number>);
            break;

        case UINT32_ARRAY:
            writer.setUint32Array(index, value as ArrayLike<number>);
            break;

        case INT64_ARRAY:
            writer.setInt64Array(index, value as ArrayLike<bigint>);
            break;

        case UINT64_ARRAY:
            writer.setUint64Array(index, value as ArrayLike<bigint>);
            break;

        case FLOAT32_ARRAY:
            writer.setFloat32Array(index, value as ArrayLike<number>);
            break;

        case FLOAT64_ARRAY:
            writer.setFloat64Array(index, value as ArrayLike<number>);
            break;

        case STRING:
            writer.setString(index, value as string);
            break;

        case STRUCT:
            writer.setStruct(index, value as Uint8Array);
            break;

        case STRUCT_ARRAY: {
            const [bytes, size] = value as [Uint8Array, number];
            writer.setStructArray(index, bytes, size, alignment);
            break;
        }
    }
}

// The errors are built outside #addField, which stays small enough to inline
// into every setter

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
