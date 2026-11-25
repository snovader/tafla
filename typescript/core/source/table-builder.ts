import { SIZEOF_8, SIZEOF_16, SIZEOF_32, SIZEOF_64 } from './constants.ts';
import { utf8Length } from './encoding.ts';
import {
    assertAlignment,
    assertCount,
    headerLength,
    maxLength,
    nestedHeaderLength,
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
import { TableReader } from './table-reader.ts';
import {
    copyTable,
    newTableWriter,
    reserveScalar,
    structCount,
    FLOAT32,
    FLOAT64,
    INT8,
    INT16,
    INT32,
    INT64,
    UINT8,
    UINT16,
    UINT32,
    UINT64,
    type TableWriter
} from './table-writer.ts';
import type { ElementSize, OffsetSize, OffsetType } from './types.ts';

// The kinds of field, which finalize() switches on to write each value: the
// writer's scalar types, INT8 to FLOAT64, which are 0 to 9, and then the
// others
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
/** A table recorded in place, whose fields' records follow its own */
const TABLE_IN_PLACE = 25;
/** A vector recorded in place, whose items' fields' records follow its own */
const VECTOR_IN_PLACE = 26;
/** A field left out, which takes an offset of 0 and no bytes */
const ABSENT = 27;

/** The size of a scalar of each kind, INT8 to FLOAT64 */
const SIZES: readonly ElementSize[] = [1, 1, 2, 2, 4, 4, 8, 8, 4, 8];

/** The longest that an array can be, 2^32 - 1 */
const MAX_ARRAY_LENGTH = 0xffff_ffff;

/**
 * The fields of a table that a {@link TableBuilder} records in place, which
 * its `setTableInPlace()` and `setVectorInPlace()` give to `fill`: the
 * builder itself, without `bytes()` and `finalize()`, whose setters set the
 * nested table's fields while `fill` runs.
 */
export type TableItem = Omit<TableBuilder, 'bytes' | 'finalize'>;

/**
 * Builds a table: a header with one offset per field, after the table's total
 * length and version when its layout has them, followed by the fields in the
 * order they're set. Arrays, strings and vectors start with their count, and a
 * vector then holds the end of each item. Arrays of scalars and of structs are
 * padded so their elements are aligned from the start of the table. Fields are read back by index with `TableReader.rootOffset()`, or
 * `TableReader.fieldOffset()` for a table nested in another.
 *
 * Setting a field records it and counts the table's length. `finalize()` then
 * writes every field, in order, through a {@link TableWriter} into bytes of
 * exactly that length, so the values must not change in between: arrays whose
 * lengths changed so that the fields no longer fit the table, or no longer
 * fill it, make it throw a `RangeError` and leave the builder unfinalized.
 * Tables nested from their builders are written by the same writer, straight
 * into this one. `setTableInPlace()` and `setVectorInPlace()` record a
 * nested table, or each item of a vector, among this builder's own fields
 * instead, without a builder of its own, and write the same bytes as nested
 * builders. A setter throws a `RangeError`, and leaves the builder as it
 * was, when its field would make the table longer than the layout's
 * `offsetType` addresses: 255 bytes, 64 KiB or 4 GiB. Typed arrays of a
 * field's element type are copied as bytes; any other array-like, such as a
 * `number[]` or a typed array of another type, is converted as the typed
 * array's constructor would.
 */
export class TableBuilder {
    /**
     * The table once written: its bytes, and then the reader of them that
     * `finalize()` returns
     */
    declare private _written: Uint8Array | TableReader | undefined;

    declare private readonly _layout: TableLayout;
    /** The offset size, the width of offsets, lengths and header slots */
    declare private readonly _size: OffsetSize;
    /** The longest table that the layout's offsets and lengths address */
    declare private readonly _maxLength: number;

    // What the builder counts of the table whose fields it's recording: its
    // own, or, while a `fill` runs, those of the table that it records in
    // place, until it restores its own
    declare private _fields: number;
    /** The number of fields set so far */
    private _count = 0;
    /** The table's length so far, which is where the next field goes */
    declare private _offset: number;
    /**
     * The largest alignment that a field needs from the table's start, which
     * a parent pads this table to when it nests it
     */
    private _alignment: ElementSize = SIZEOF_8;
    /** The number of tables being recorded in place, one in another */
    private _nesting = 0;

    // A record of three slots per field, in one plain array sized up front
    // for the table's own fields: its kind, the alignment that it's padded
    // to or 0, and its value. The writer pads each field and writes its
    // offset as it reaches it, so the record needn't hold either. A table
    // recorded in place holds its number of fields, and its fields' records
    // follow, and a vector recorded in place holds its count, and its items'
    // number of fields in place of an alignment, since items aren't padded,
    // and each item's records follow. Small typed arrays took four times as
    // long to allocate, and an array per slot made setting a small table's
    // fields take twice as long. The slots are literals, since named
    // constants made building a small table 5-9% slower in d8.
    declare private _records: unknown[];
    /** The number of slots recorded, where the next record goes */
    private _recorded = 0;
    /**
     * The slots that the records have room for, at least: those recorded,
     * and three for each field that the tables being recorded have left to
     * set. Every record is stored within them, so that a store never grows
     * the array: once one did, V8 slowed every builder's stores, and a small
     * table took a third longer to build.
     */
    declare private _reserved: number;

    /**
     * @param fields - The number of fields that the table will hold.
     * @param layout - The table's header and offset type, which its readers
     * must use too, with the version to write to the header when it has one.
     * @throws {RangeError} If `fields` isn't a non-negative integer, or the
     * header alone is longer than the layout addresses.
     */
    constructor(fields: number, layout: TableLayout = TableLayout.DEFAULT) {
        const size = layout.offsetSize();

        this._layout = layout;
        this._size = size;
        this._maxLength = maxLength(size);
        this._fields = fields;
        this._offset = headerLength(fields, layout);

        this._records = new Array(fields * 3);
        this._reserved = fields * 3;
    }

    /**
     * Writes the table, once, and returns a `TableReader` that reads it; later
     * calls return the same reader.
     *
     * @throws {RangeError} If fewer fields were set than the table holds, or
     * the fields no longer fit the table or fill it.
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
     * which `finalize()` then returns a reader of
     *
     * @throws {RangeError} If fewer fields were set than the table holds, or
     * the fields no longer fit the table or fill it.
     */
    bytes(): Uint8Array {
        const written = this._written ?? this.#write();
        return written instanceof TableReader ? written.bytes() : written;
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
        this.#addField(STRING, string, this._size + utf8Length(string), 0, 0);
    }

    /**
     * Sets the next field as absent: an offset of 0 and no value. A reader
     * finds it by comparing the field's offset, from
     * `TableReader.rootOffset()`, with 0, or one from `fieldOffset()` with
     * the nested table's position, before it reads the field, so only a
     * format whose readers make that check for the field can leave it out.
     */
    setAbsent(): void {
        this.#addField(ABSENT, undefined, 0, 0, 0);
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
            this._size + bytes.length,
            padding(this._offset + this._size, alignment),
            alignment
        );
        this.#alignTo(alignment);
    }

    /**
     * Embeds a table, from its builder or its bytes, without a length prefix.
     * Readers find its fields with `TableReader.fieldOffset()` from the field's
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
            ? // oxlint-disable-next-line eslint/no-underscore-dangle -- another builder's own field, which the rule allows only after `this`
              table._alignment
            : SIZEOF_32
    ): void {
        assertAlignment(alignment);

        this.#addField(
            TABLE,
            table,
            TableBuilder.#lengthOf(table),
            padding(this._offset, alignment),
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
        let byteLength = this._size + vector.length * this._size;
        for (let i = 0; i < vector.length; i++) {
            byteLength += TableBuilder.#lengthOf(vector[i]);
        }

        this.#addField(VECTOR, vector, byteLength, 0, 0);
    }

    /**
     * Records a table of `fields` fields nested in this one, in place,
     * without a builder of its own: `fill` sets its fields, in order, on the
     * {@link TableItem} that it's given, and each is recorded straight among
     * this builder's fields. It's written as a nested builder of this
     * table's offset type and no header is: padded to start at a multiple of
     * its largest alignment, as `setTable()` pads a builder by default, and
     * into the same bytes.
     *
     * The item is this builder, whose setters set the nested table's fields
     * until `fill` returns. The table can't be finalized or nested meanwhile.
     *
     * @throws {RangeError} If `fields` isn't a non-negative integer, `fill`
     * leaves fields of the table unset, or as the other setters throw.
     * @throws What `fill` throws. Either way, the builder is left as it was.
     */
    setTableInPlace(fields: number, fill: (table: TableItem) => void): void {
        this.#recordTable(fields, fill);
    }

    /**
     * Records a vector of `count` tables of `fields` fields each, in place,
     * as `setTableInPlace()` records one: `fill(item, index)` sets the fields
     * of item `index`, for each in order. Items aren't padded, as
     * `setVector()` doesn't pad them.
     *
     * @throws {RangeError} If `count` or `fields` isn't a non-negative
     * integer, the vector's count and item ends and its items' offsets make
     * the table too long, before any item is recorded, `fill` leaves fields
     * of an item unset, or as the other setters throw.
     * @throws What `fill` throws. Either way, the builder is left as it was.
     */
    setVectorInPlace(
        count: number,
        fields: number,
        fill: (item: TableItem, index: number) => void
    ): void {
        this.#recordVector(count, fields, fill);
    }

    /** `setTableInPlace()` */
    #recordTable(fields: number, fill: (table: TableItem) => void): void {
        this.#assertRoom();
        const header = nestedHeaderLength(fields, this._layout);

        const parentFields = this._fields;
        const count = this._count;
        const offset = this._offset;
        const alignment = this._alignment;
        const record = this._recorded;
        const reserved = this._reserved;

        this.#startInPlace(TABLE_IN_PLACE, 0, fields);
        try {
            this.#startTable(fields, header);
            fill(this);
            this.#assertSet();
        } catch (error) {
            this.#restore(parentFields, count, offset, alignment);
            this.#truncate(record, reserved);
            throw error;
        }

        const length = this._offset;
        const tableAlignment = this._alignment;
        this.#restore(parentFields, count, offset, alignment);

        // Padded from this table's length, as a nested builder is
        const total = offset + padding(offset, tableAlignment) + length;
        if (!(total <= this._maxLength)) {
            this.#truncate(record, reserved);
            throw tooLong(count, total, this._layout.offsetType());
        }

        this._records[record + 1] = tableAlignment;
        this._offset = total;
        this._count = count + 1;
        this.#alignTo(tableAlignment);
    }

    /** `setVectorInPlace()` */
    #recordVector(
        count: number,
        fields: number,
        fill: (item: TableItem, index: number) => void
    ): void {
        assertCount(count);
        this.#assertRoom();

        // The count, the items' ends and their offsets, which fail before
        // any item is recorded, rather than after a `fill` for each
        const header = nestedHeaderLength(fields, this._layout);
        const index = this._count;
        const offset = this._offset;
        const start = offset + this._size + count * this._size;
        const headers = start + count * header;
        if (!(headers <= this._maxLength)) {
            throw tooLong(index, headers, this._layout.offsetType());
        }

        const parentFields = this._fields;
        const alignment = this._alignment;
        const record = this._recorded;
        const reserved = this._reserved;

        // Room for the fewest records that the items record, one for each
        // of their fields, which is all of them unless they nest tables of
        // their own. Their offsets fit in the table, so there are at most a
        // few slots for each of its bytes.
        const fewest = reserved + count * fields * 3;
        if (fewest > this._records.length) {
            this._records.length = fewest;
        }

        let length = start;
        this.#startInPlace(VECTOR_IN_PLACE, fields, count);
        try {
            for (let item = 0; item < count; item++) {
                this.#startTable(fields, header);
                fill(this, item);
                this.#assertSet();

                length += this._offset;
            }
        } catch (error) {
            this.#restore(parentFields, index, offset, alignment);
            this.#truncate(record, reserved);
            throw error;
        }

        this.#restore(parentFields, index, offset, alignment);
        if (!(length <= this._maxLength)) {
            this.#truncate(record, reserved);
            throw tooLong(index, length, this._layout.offsetType());
        }

        this._offset = length;
        this._count = index + 1;
    }

    /**
     * Records a table or vector in place, as a field of the table being
     * recorded, whose fields' or items' records follow, and starts counting
     * their fields rather than this table's
     */
    #startInPlace(kind: number, alignment: number, value: number): void {
        const records = this._records;
        const record = this._recorded;
        records[record] = kind;
        records[record + 1] = alignment;
        records[record + 2] = value;

        this._recorded = record + 3;
        this._nesting++;
    }

    /**
     * Counts the fields of a table of `fields` fields recorded in place,
     * whose header of `header` bytes holds only their offsets
     */
    #startTable(fields: number, header: number): void {
        const reserved = this._reserved + fields * 3;
        if (reserved > this._records.length) {
            this.#grow(reserved);
        }
        this._reserved = reserved;

        this._fields = fields;
        this._count = 0;
        this._offset = header;
        this._alignment = SIZEOF_8;
    }

    /**
     * Counts the table's own fields again, or its parent's, after recording
     * one in place: those set so far, its length and its alignment
     */
    #restore(
        fields: number,
        count: number,
        offset: number,
        alignment: ElementSize
    ): void {
        this._fields = fields;
        this._count = count;
        this._offset = offset;
        this._alignment = alignment;
        this._nesting--;
    }

    /**
     * Grows the records to at least `slots` slots, at least doubling them
     * up to the longest array, so that recording many tables in place grows
     * them a few times
     *
     * @throws {RangeError} If `slots` is longer than an array can be.
     */
    #grow(slots: number): void {
        const records = this._records;
        records.length = Math.max(
            slots,
            Math.min(2 * records.length, MAX_ARRAY_LENGTH)
        );
    }

    /**
     * Removes the records from `recorded` on, which leaves the builder as it
     * was before a table that failed, with room for `reserved` slots, and
     * releases their values, which may be large arrays
     */
    #truncate(recorded: number, reserved: number): void {
        this._records.fill(undefined, recorded, this._recorded);
        this._recorded = recorded;
        this._reserved = reserved;
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
            this._size + byteLength,
            padding(this._offset + this._size, alignment),
            alignment
        );
        this.#alignTo(alignment);
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
        // oxlint-disable-next-line eslint/no-underscore-dangle -- another builder's own field, which the rule allows only after `this`
        return table._offset;
    }

    /**
     * @throws {TypeError} If a table is being recorded in place, whose fields
     * the builder is counting rather than its own.
     * @throws {RangeError} If the table has fields left to set.
     */
    #assertComplete(): void {
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
     * Checks that a field can be added, as `#addField()` does, before
     * recording a table in place
     *
     * @throws {RangeError} If the table already holds all its fields.
     * @throws {TypeError} If the table is already written.
     */
    #assertRoom(): void {
        if (this._written !== undefined) {
            throw new TypeError('The table was already finalized');
        }

        if (this._count === this._fields) {
            throw tooManyFields(this._fields);
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
        if (this._written !== undefined) {
            throw new TypeError('The table was already finalized');
        }

        const index = this._count;
        if (index === this._fields) {
            throw tooManyFields(this._fields);
        }

        const length = this._offset + padding + byteLength;
        if (!(length <= this._maxLength)) {
            throw tooLong(index, length, this._layout.offsetType());
        }

        const records = this._records;
        const record = this._recorded;
        records[record] = kind;
        records[record + 1] = alignment;
        records[record + 2] = value;

        this._recorded = record + 3;
        this._offset = length;
        this._count = index + 1;
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
            new Uint8Array(this._offset),
            this._fields,
            this._layout
        );
        this.#writeFields(writer, 0, this._fields);
        const bytes = writer.bytes();

        // Release the values, which may be large arrays
        this._records = [];
        this._written = bytes;

        return bytes;
    }

    /**
     * Writes the `fields` fields of a table, whose records start at slot
     * `start`, in order, each with its index in the table, and returns where
     * their records, and those of the tables recorded in place in them, end
     */
    #writeFields(writer: TableWriter, start: number, fields: number): number {
        const records = this._records;
        let record = start;

        for (let index = 0; index < fields; index++) {
            const kind = records[record] as number;
            const alignment = records[record + 1] as ElementSize;
            const value = records[record + 2];
            record += 3;

            if (kind <= FLOAT64) {
                // The writer reserves the scalar and writes its offset, and
                // its store follows here, rather than in a function, which
                // V8 could compile on its own first: TurboFan would then
                // count the stores that it inlined against the budget.
                // #writeFields() itself stays over V8's limit for inlining,
                // 460 bytes of bytecode, as `yarn package:check` checks, so
                // that V8 doesn't inline it into #writeInPlace(), where the
                // scalars would take the budget of the nested tables' calls
                const position = reserveScalar(writer, index, SIZES[kind]);
                const bytes = writer.target();
                switch (kind) {
                    case INT8:
                    case UINT8:
                        storeInt8(bytes, position, value as number);
                        break;

                    case INT16:
                    case UINT16:
                        storeInt16(bytes, position, value as number);
                        break;

                    case INT32:
                    case UINT32:
                        storeInt32(bytes, position, value as number);
                        break;

                    case INT64:
                    case UINT64:
                        storeInt64(bytes, position, value as bigint);
                        break;

                    case FLOAT32:
                        storeFloat32(bytes, position, value as number);
                        break;

                    case FLOAT64:
                        storeFloat64(bytes, position, value as number);
                        break;
                }
            } else if (kind === TABLE) {
                writer.setTable(index, alignment);
                this.#writeTable(writer, value as TableBuilder | Uint8Array);
            } else if (kind === VECTOR) {
                this.#writeVector(
                    writer,
                    index,
                    value as ReadonlyArray<TableBuilder | Uint8Array>
                );
            } else if (kind === TABLE_IN_PLACE) {
                writer.setTable(index, alignment);
                record = this.#writeInPlace(writer, record, value as number);
            } else if (kind === VECTOR_IN_PLACE) {
                // A vector recorded in place holds its items' fields in
                // place of an alignment
                const vector = writer.setVector(index, value as number);
                for (let item = 0; item < (value as number); item++) {
                    record = this.#writeInPlace(writer, record, alignment);
                    writer.setItemEnd(vector, item);
                }
            } else {
                writeField(writer, index, kind, value, alignment);
            }
        }

        return record;
    }

    /**
     * Writes a table recorded in place, of `fields` fields whose records
     * start at slot `start`, and returns where they end
     */
    #writeInPlace(writer: TableWriter, start: number, fields: number): number {
        const parent = writer.startTable(fields);
        const end = this.#writeFields(writer, start, fields);
        writer.endTable(parent);

        return end;
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
        /* oxlint-disable eslint/no-underscore-dangle -- another builder's own field, which the rule allows only after `this` */
        if (!(table instanceof TableBuilder)) {
            copyTable(writer, table);
        } else if (
            table._written === undefined &&
            table._size === this._size &&
            table._layout.headerSize() === 0
        ) {
            const parent = writer.startTable(table._fields);
            table.#writeFields(writer, 0, table._fields);
            writer.endTable(parent);
        } else {
            copyTable(writer, table.bytes());
        }
        /* oxlint-enable eslint/no-underscore-dangle */
    }
}

/**
 * Writes a field of a kind that `#writeFields()` doesn't write itself, each
 * through the writer's setter of its kind.
 *
 * Its bytecode stays over V8's limit for inlining, 460 bytes, as `yarn
 * package:check` checks: 474 bytes in Node 26 and 461 in Node 27. So TurboFan
 * compiles it on its own, with the setters that it calls, rather than
 * inlining it and them into `#writeFields()`, where they take the budget of
 * `#writeInPlace()`, `startTable()` and `endTable()`.
 */
function writeField(
    writer: TableWriter,
    index: number,
    kind: number,
    value: unknown,
    alignment: ElementSize
): void {
    switch (kind) {
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

        case ABSENT:
            writer.setAbsent(index);
            break;
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
