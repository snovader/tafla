import { SIZEOF_8, SIZEOF_16, SIZEOF_32, SIZEOF_64 } from './constants.ts';
import type { ElementSize, OffsetSize, OffsetType } from './types.ts';

export interface TableLayoutOptions<Versioned extends boolean = boolean> {
    /** Whether the header starts with the table's total byte length */
    sized?: boolean;
    /**
     * Whether the header has a slot for a version, after the length when
     * `sized`. Readers can check it before reading any field.
     */
    versioned?: Versioned;
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
 * `ByteBuffer` that reads it, so that the two can't disagree. Nested tables
 * use {@link TableLayout.DEFAULT}.
 *
 * `Versioned` lets TypeScript require a version where a versioned table is
 * built.
 */
export class TableLayout<Versioned extends boolean = boolean> {
    /** No header slots, and offsets and lengths of the offset type `uint32` */
    static readonly DEFAULT = new TableLayout<false>(false, false, 'uint32');

    readonly #sized: boolean;
    readonly #versioned: Versioned;
    readonly #offsetType: OffsetType;
    readonly #offsetSize: OffsetSize;
    readonly #headerSize: number;

    private constructor(
        sized: boolean,
        versioned: Versioned,
        offsetType: OffsetType
    ) {
        this.#sized = sized;
        this.#versioned = versioned;
        this.#offsetType = offsetType;
        this.#offsetSize = OFFSET_SIZES[offsetType];
        this.#headerSize =
            ((sized ? 1 : 0) + (versioned ? 1 : 0)) * this.#offsetSize;
    }

    /**
     * @throws {TypeError} If `options` has another option than `sized`,
     * `versioned` and `offsetType`, such as a misspelled one.
     * @throws {RangeError} If `offsetType` isn't `'uint8'`, `'uint16'` or
     * `'uint32'`.
     */
    static create<Versioned extends boolean = false>({
        sized = false,
        versioned = false as Versioned,
        offsetType = 'uint32',
        ...others
    }: TableLayoutOptions<Versioned> = {}): TableLayout<Versioned> {
        // An option that isn't read would leave its default in its place,
        // and readers of the intended layout would misread the table
        const [other] = Object.keys(others);
        if (other !== undefined) {
            throw new TypeError(
                `TableLayout.create() takes sized, versioned and offsetType, not ${other}`
            );
        }

        assertOffsetType(offsetType);

        return new TableLayout(sized, versioned, offsetType);
    }

    sized(): boolean {
        return this.#sized;
    }

    versioned(): Versioned {
        return this.#versioned;
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
 * The byte length of the header of a table of `fields` fields in `layout`,
 * whose writer puts `version` in the header
 *
 * It checks everything in one condition, and finds which check failed out
 * of line, to keep the bytecode that V8 inlines into `TableWriter.tableBytes()`
 * small: TurboFan inlines up to 920 bytes of it into a function, and the
 * serializers that tafla-gen generates spend that on the table's checks,
 * its fields' writes and its string.
 *
 * @throws {TypeError} If `version` is given for a layout without a version
 * slot, or missing for one with it.
 * @throws {RangeError} If `fields` isn't a non-negative integer, `version`
 * isn't an integer that fits in the layout's offset type, or the header
 * alone is longer than the layout addresses.
 */
export function headerLength(
    fields: number,
    layout: TableLayout,
    version: number | undefined
): number {
    const size = layout.offsetSize();
    const length = layout.headerSize() + fields * size;

    if (
        !(
            isCount(fields) &&
            length <= maxLength(size) &&
            layout.versioned() === (version !== undefined) &&
            (version === undefined || fitsIn(version, size))
        )
    ) {
        throw headerError(fields, layout, version);
    }

    return length;
}

/** The error that `headerLength()` throws, for the first check that fails */
function headerError(
    fields: number,
    layout: TableLayout,
    version: number | undefined
): Error {
    if (!isCount(fields)) {
        return countError(fields);
    }

    if (layout.versioned() !== (version !== undefined)) {
        return new TypeError(
            layout.versioned()
                ? 'A versioned layout needs a version'
                : 'The layout has no version slot'
        );
    }

    if (version !== undefined && !fitsIn(version, layout.offsetSize())) {
        return new RangeError(
            `Version ${version} doesn't fit in a ${layout.offsetType()} slot`
        );
    }

    return headerLengthError(fields, layout);
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
