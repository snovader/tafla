import { SIZEOF_8, SIZEOF_16, SIZEOF_32, SIZEOF_64 } from './constants.ts';
import type { ElementSize, OffsetSize, OffsetType } from './types.ts';

export interface TableLayoutOptions {
    /** Whether the header starts with the table's total byte length */
    sized?: boolean;
    /**
     * The version that the header holds, after the length when `sized`,
     * which gives it a slot for one: an integer that fits in the offset
     * type. Writers write it, and readers can check the one that a table
     * holds, with `TableReader.getVersion()`, before reading any field.
     */
    version?: number;
    /**
     * The offset type, the unsigned integer type of the header slots, field
     * offsets, counts, lengths and item ends, which limits the table's length:
     * `'uint8'` for up to 255 bytes, `'uint16'` for 65,535 and `'uint32'`, the
     * default, for 4 GiB.
     */
    offsetType?: OffsetType;
}

/** The offset size of each offset type */
const OFFSET_SIZES: Readonly<Record<OffsetType, OffsetSize>> = {
    uint8: SIZEOF_8,
    uint16: SIZEOF_16,
    uint32: SIZEOF_32
};

/**
 * The header and offset type of a table. A format creates its layout once
 * and passes it to both the `TableBuilder` that writes the table and the
 * `TableReader` that reads it, so that the two can't disagree. Nested tables
 * use {@link TableLayout.DEFAULT}.
 *
 * A layout with a version is the layout of that version of the format: its
 * writers write the version, and its readers read tables of any version,
 * so that an application can read a table's version before it chooses how
 * to read the fields. The layouts of a format's versions should differ in
 * the version only, as {@link TableLayout.withVersion} makes them.
 */
export class TableLayout {
    /** No header slots, and offsets and lengths of the offset type `uint32` */
    static readonly DEFAULT = new TableLayout(false, undefined, 'uint32');

    readonly #sized: boolean;
    readonly #version: number | undefined;
    readonly #offsetType: OffsetType;
    readonly #offsetSize: OffsetSize;
    readonly #headerSize: number;

    private constructor(
        sized: boolean,
        version: number | undefined,
        offsetType: OffsetType
    ) {
        this.#sized = sized;
        this.#version = version;
        this.#offsetType = offsetType;
        this.#offsetSize = OFFSET_SIZES[offsetType];
        this.#headerSize =
            ((sized ? 1 : 0) + (version === undefined ? 0 : 1)) *
            this.#offsetSize;
    }

    /**
     * @throws {TypeError} If `options` has another option than `sized`,
     * `version` and `offsetType`, such as a misspelled one.
     * @throws {RangeError} If `offsetType` isn't `'uint8'`, `'uint16'` or
     * `'uint32'`, or `version` isn't an integer that fits in it.
     */
    static create({
        sized = false,
        version,
        offsetType = 'uint32',
        ...others
    }: TableLayoutOptions = {}): TableLayout {
        // An option that isn't read would leave its default in its place,
        // and readers of the intended layout would misread the table
        const [other] = Object.keys(others);
        if (other != null) {
            throw new TypeError(
                `TableLayout.create() takes sized, version and offsetType, not ${other}`
            );
        }

        assertOffsetType(offsetType);
        if (
            version !== undefined &&
            !fitsIn(version, OFFSET_SIZES[offsetType])
        ) {
            throw new RangeError(
                `Version ${version} doesn't fit in a ${offsetType} slot`
            );
        }

        return new TableLayout(sized, version, offsetType);
    }

    /**
     * The layout with `version` in place of its own, for another version of
     * the same format
     *
     * @throws {RangeError} If `version` isn't an integer that fits in the
     * offset type.
     */
    withVersion(version: number): TableLayout {
        return TableLayout.create({
            sized: this.#sized,
            version,
            offsetType: this.#offsetType
        });
    }

    sized(): boolean {
        return this.#sized;
    }

    /** Whether the header has a slot for a version */
    versioned(): boolean {
        return this.#version !== undefined;
    }

    /** The version that writers write to the header, when it has a slot */
    version(): number | undefined {
        return this.#version;
    }

    offsetType(): OffsetType {
        return this.#offsetType;
    }

    /** The offset size, the width in bytes of the offset type: 1, 2 or 4 */
    offsetSize(): OffsetSize {
        return this.#offsetSize;
    }

    /** The byte length of the header slots before the field offsets */
    headerSize(): number {
        return this.#headerSize;
    }
}

/**
 * The byte length of the header of a table of `fields` fields in `layout`
 *
 * It checks everything in one condition, and finds which check failed out
 * of line, to keep the bytecode that V8 inlines into `TableWriter.tableBytes()`
 * small: TurboFan inlines up to 920 bytes of it into a function, and the
 * serializers that tafla-gen generates spend that on the table's checks,
 * its fields' writes and its string.
 *
 * @throws {RangeError} If `fields` isn't a non-negative integer, or the
 * header alone is longer than the layout addresses.
 */
export function headerLength(fields: number, layout: TableLayout): number {
    const size = layout.offsetSize();
    const length = layout.headerSize() + fields * size;

    if (!(isCount(fields) && length <= maxLength(size))) {
        throw headerError(fields, layout);
    }

    return length;
}

/** The error that `headerLength()` throws, for the first check that fails */
function headerError(fields: number, layout: TableLayout): RangeError {
    return isCount(fields)
        ? headerLengthError(fields, layout)
        : countError(fields);
}

/**
 * The byte length of the header of a table of `fields` fields nested in
 * place in a table of `layout`: only the offsets of its fields, of the
 * layout's offset type
 *
 * @throws {RangeError} If `fields` isn't a non-negative integer, or the
 * offsets alone are longer than the layout addresses.
 */
export function nestedHeaderLength(
    fields: number,
    layout: TableLayout
): number {
    assertCount(fields);
    return fittedHeaderLength(0, fields, layout);
}

/**
 * The byte length of a header of `slots` bytes before the offsets of
 * `fields` fields
 *
 * @throws {RangeError} If it's longer than the layout addresses.
 */
function fittedHeaderLength(
    slots: number,
    fields: number,
    layout: TableLayout
): number {
    const size = layout.offsetSize();

    const length = slots + fields * size;
    if (!(length <= maxLength(size))) {
        throw headerLengthError(fields, layout);
    }

    return length;
}

function headerLengthError(fields: number, layout: TableLayout): RangeError {
    return new RangeError(
        `A header of ${fields} fields doesn't fit in ${layout.offsetType()} offsets`
    );
}

/**
 * The longest table that offsets of `size` bytes address, which is also the
 * largest number that they hold unsigned, without a power: 0xff, 0xffff or
 * 0xffff_ffff, the 32 set bits shifted right by those that `size` bytes
 * don't hold. A `switch` takes three times the bytecode for V8 to inline.
 */
export function maxLength(size: OffsetSize): number {
    return 0xffff_ffff >>> (32 - 8 * size);
}

/**
 * Checks that `count`, of a table's fields, an array's elements or a vector's
 * items, is a non-negative integer, which a table can be written with
 *
 * @throws {RangeError} If it isn't.
 */
export function assertCount(count: number): void {
    if (!isCount(count)) {
        throw countError(count);
    }
}

function isCount(count: number): boolean {
    return Number.isSafeInteger(count) && count >= 0;
}

function countError(count: number): RangeError {
    return new RangeError(
        `A count must be a non-negative integer, not ${count}`
    );
}

/**
 * Checks an alignment that a caller gave, which arrays of structs and nested
 * tables are padded to. An array's alignment is its element size, which
 * needs no check.
 *
 * @throws {RangeError} If `alignment` isn't 1, 2, 4 or 8.
 */
export function assertAlignment(alignment: ElementSize): void {
    if (
        !(
            alignment === SIZEOF_8 ||
            alignment === SIZEOF_16 ||
            alignment === SIZEOF_32 ||
            alignment === SIZEOF_64
        )
    ) {
        throw new RangeError(
            `An alignment of ${alignment} isn't 1, 2, 4 or 8 bytes`
        );
    }
}

/**
 * The bytes that bring `offset`, from a table's start, to a multiple of
 * `alignment`, which is 1, 2, 4 or 8
 */
export function padding(offset: number, alignment: ElementSize): number {
    const remainder = offset % alignment;
    return remainder === 0 ? 0 : alignment - remainder;
}

/**
 * Checks that `value` is one of the offset types. It must be the string itself:
 * `Object.hasOwn()` would also find `['uint8']`, which converts to its key.
 *
 * @throws {RangeError} If it isn't.
 */
function assertOffsetType(value: unknown): asserts value is OffsetType {
    if (typeof value !== 'string') {
        throw new RangeError(
            `An offset type must be a string such as 'uint16', not a value of type ${typeof value}`
        );
    }

    if (!Object.hasOwn(OFFSET_SIZES, value)) {
        throw new RangeError(`${value} isn't an offset type`);
    }
}

/** Whether `value` is a whole number that `size` bytes hold unsigned */
function fitsIn(value: number, size: OffsetSize): boolean {
    return Number.isInteger(value) && value >= 0 && value <= maxLength(size);
}
