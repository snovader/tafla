import { fc } from 'hypotes/fast-check';
import {
    toJsonNumber,
    type Alignment,
    type ArrayField,
    type Corruption,
    type FieldDescription,
    type JsonNumber,
    type LayoutDescription,
    type NestedTable,
    type OffsetType,
    type ScalarType,
    type TableDescription,
    type TableField
} from './description.ts';
import { buildTable } from './tables.ts';

// Arbitrary descriptions of tables, in the conformance suite's model, for
// property tests to build with `buildTable()` and read with `readTable()`.
// They cover every layout, value and nesting that the format allows, in tables
// small enough to keep the tests fast.

export const OFFSET_TYPES: readonly OffsetType[] = [
    'uint8',
    'uint16',
    'uint32'
];

/**
 * The largest value of each offset type, which is also the longest table that
 * its offsets address
 */
export const MAX_VALUES: Readonly<Record<OffsetType, number>> = {
    uint8: 0xff,
    uint16: 0xffff,
    uint32: 0xffff_ffff
};

export const SCALAR_TYPES: readonly ScalarType[] = [
    'int8',
    'uint8',
    'int16',
    'uint16',
    'int32',
    'uint32',
    'int64',
    'uint64',
    'float32',
    'float64'
];

const FROMS: ReadonlyArray<NestedTable['from']> = ['builder', 'bytes'];
const ALIGNMENTS: readonly Alignment[] = [1, 2, 4, 8];

const FIELDS_PER_TABLE = 6;
const ITEMS_PER_VECTOR = 3;
/** How deep tables nest below the root, with setTable() or in vectors */
const NESTING_DEPTH = 2;

/** The value of a scalar type as tafla's builders take it */
export type ScalarValue<Type extends ScalarType> = Type extends
    | 'int64'
    | 'uint64'
    ? bigint
    : number;

/**
 * Values of each scalar type over its full range, as tafla's builders take
 * them. fast-check reaches the extremes and the special floats only now and
 * then, so they're drawn as often as other values: a run holds a few fields
 * of each type, and a bug may show in one value only.
 */
export const SCALAR_VALUES: {
    readonly [Type in ScalarType]: fc.Arbitrary<ScalarValue<Type>>;
} = {
    int8: integers(-0x80, 0x7f),
    uint8: integers(0, 0xff),
    int16: integers(-0x8000, 0x7fff),
    uint16: integers(0, 0xffff),
    int32: integers(-0x8000_0000, 0x7fff_ffff),
    uint32: integers(0, 0xffff_ffff),
    int64: bigIntegers(-(2n ** 63n), 2n ** 63n - 1n),
    uint64: bigIntegers(0n, 2n ** 64n - 1n),
    float32: floats(fc.float(), [2 ** 128 - 2 ** 104, 2 ** -149]),
    float64: floats(fc.double(), [Number.MAX_VALUE, Number.MIN_VALUE])
};

/** Values of each scalar type as descriptions hold them */
const VALUES: Readonly<Record<ScalarType, fc.Arbitrary<JsonNumber>>> = {
    int8: SCALAR_VALUES.int8,
    uint8: SCALAR_VALUES.uint8,
    int16: SCALAR_VALUES.int16,
    uint16: SCALAR_VALUES.uint16,
    int32: SCALAR_VALUES.int32,
    uint32: SCALAR_VALUES.uint32,
    // 64-bit integers as decimal strings, which JSON holds exactly
    int64: SCALAR_VALUES.int64.map(String),
    uint64: SCALAR_VALUES.uint64.map(String),
    float32: SCALAR_VALUES.float32.map(toJsonNumber),
    float64: SCALAR_VALUES.float64.map(toJsonNumber)
};

function integers(min: number, max: number): fc.Arbitrary<number> {
    return withEdges(fc.integer({ min, max }), [min, max, 0]);
}

function bigIntegers(min: bigint, max: bigint): fc.Arbitrary<bigint> {
    return withEdges(fc.bigInt({ min, max }), [min, max, 0n]);
}

/** Floats with -0, the infinities, NaN and the extremes given */
function floats(
    values: fc.Arbitrary<number>,
    extremes: number[]
): fc.Arbitrary<number> {
    const special = [
        -0,
        Number.POSITIVE_INFINITY,
        Number.NEGATIVE_INFINITY,
        Number.NaN,
        ...extremes
    ];

    return withEdges(values, special);
}

function withEdges<T>(values: fc.Arbitrary<T>, edges: T[]): fc.Arbitrary<T> {
    return fc.oneof(values, fc.constantFrom(...edges));
}

const BYTE_ORDER_MARK = String.fromCharCode(0xfeff);

/**
 * Strings of any code points, mostly outside the Basic Multilingual Plane,
 * and of graphemes, also with a leading U+FEFF, which is part of the string.
 * Lone surrogates are left out, since UTF-8 can't hold them.
 */
export const STRINGS: fc.Arbitrary<string> = fc.oneof(
    fc.string({ unit: 'binary' }),
    fc.string({ unit: 'grapheme' }),
    fc.string({ unit: 'binary' }).map(string => BYTE_ORDER_MARK + string)
);

const scalarFields = fc.oneof(
    ...SCALAR_TYPES.map(type =>
        VALUES[type].map((value): FieldDescription => ({ type, value }))
    )
);

/** Arrays of every element type, empty too */
export const ARRAY_FIELDS: fc.Arbitrary<ArrayField> = fc.oneof(
    ...SCALAR_TYPES.map(type =>
        fc
            .array(VALUES[type])
            .map((value): ArrayField => ({ type: `${type}[]`, value }))
    )
);

const stringFields = STRINGS.map((value): FieldDescription => ({
    type: 'string',
    value
}));

const ITEMS_PER_STRUCT_ARRAY = 4;

/** The scalar types of a struct, of one to four scalars */
const STRUCT_TYPES: fc.Arbitrary<ScalarType[]> = fc.array(
    fc.constantFrom(...SCALAR_TYPES),
    { minLength: 1, maxLength: 4 }
);

/** The values of a struct of scalars of these types */
function structValues(types: ScalarType[]): fc.Arbitrary<JsonNumber[]> {
    return fc.tuple(...types.map(type => VALUES[type]));
}

const structFields = STRUCT_TYPES.chain(types =>
    structValues(types).map((values): FieldDescription => ({
        type: 'struct',
        fields: types.map((type, index) => ({ type, value: values[index] }))
    }))
);

/** Arrays of structs of any scalars, empty too */
export const STRUCT_ARRAY_FIELDS: fc.Arbitrary<FieldDescription> =
    STRUCT_TYPES.chain(types =>
        fc
            .array(structValues(types), { maxLength: ITEMS_PER_STRUCT_ARRAY })
            .map((items): FieldDescription => ({
                type: 'struct[]',
                fields: types,
                items
            }))
    );

/** A table's layout, and its version when it's versioned */
type Header = Pick<TableDescription, 'layout' | 'version'>;

/** Every layout of the offset types, with versions that fit their slots */
function headers(offsetTypes: readonly OffsetType[]): fc.Arbitrary<Header> {
    return fc.oneof(
        ...offsetTypes.map(offsetType =>
            fc
                .record({
                    sized: fc.boolean(),
                    versioned: fc.boolean(),
                    version: integers(0, MAX_VALUES[offsetType])
                })
                .map(({ sized, versioned, version }) =>
                    headerOf({ sized, versioned, offsetType }, version)
                )
        )
    );
}

function headerOf(layout: LayoutDescription, version: number): Header {
    return layout.versioned ? { layout, version } : { layout };
}

/**
 * The layouts of a table nested with `setTable()`, which has no header. One
 * with its parent's offset type is read in place, and any other from its
 * position, as the conformance suite reads them.
 */
function headerless(offsetTypes: readonly OffsetType[]): fc.Arbitrary<Header> {
    return fc.constantFrom(...offsetTypes).map((offsetType): Header => ({
        layout: { sized: false, versioned: false, offsetType }
    }));
}

/** Layouts of every header and offset type */
export const LAYOUTS: fc.Arbitrary<LayoutDescription> = headers(
    OFFSET_TYPES
).map(({ layout }) => layout);

/**
 * A field of any type. Fields nest tables up to {@link NESTING_DEPTH} deep:
 * with `setTable()`, from their builders or bytes, at their default alignment
 * or another, and in vectors, whose items may have any layout.
 */
export const FIELDS: fc.Arbitrary<FieldDescription> = fc.letrec<{
    field: FieldDescription;
    nestedTable: TableDescription;
    item: TableDescription;
}>(tie => ({
    field: fc.oneof(
        // At the maximum depth, only the first arbitrary is drawn
        { maxDepth: NESTING_DEPTH, withCrossShrink: true },
        {
            arbitrary: fc.oneof(
                scalarFields,
                ARRAY_FIELDS,
                stringFields,
                structFields,
                STRUCT_ARRAY_FIELDS
            ),
            weight: 4
        },
        { arbitrary: tableFields(tie('nestedTable')), weight: 1 },
        { arbitrary: vectorFields(tie('item')), weight: 1 }
    ),
    nestedTable: tablesOf(headerless(OFFSET_TYPES), tie('field')),
    item: tablesOf(headers(OFFSET_TYPES), tie('field'))
})).field;

function tableFields(
    tables: fc.Arbitrary<TableDescription>
): fc.Arbitrary<FieldDescription> {
    return fc
        .record({
            from: fc.constantFrom(...FROMS),
            alignment: fc.option(fc.constantFrom(...ALIGNMENTS), {
                nil: undefined
            }),
            table: tables
        })
        .map(tableField);
}

function tableField({
    from,
    alignment,
    table
}: Omit<TableField, 'type'>): TableField {
    return alignment == null
        ? { type: 'table', from, table }
        : { type: 'table', from, alignment, table };
}

function vectorFields(
    tables: fc.Arbitrary<TableDescription>
): fc.Arbitrary<FieldDescription> {
    // fc.record() makes objects without a prototype, which toStrictEqual()
    // tells apart from the plain objects that readTable() returns
    const items = fc
        .tuple(fc.constantFrom(...FROMS), tables)
        .map(([from, table]): NestedTable => ({ from, table }));

    return fc
        .array(items, { maxLength: ITEMS_PER_VECTOR })
        .map((items): FieldDescription => ({ type: 'vector', items }));
}

function tablesOf(
    headers: fc.Arbitrary<Header>,
    fields: fc.Arbitrary<FieldDescription>,
    minFields = 0
): fc.Arbitrary<TableDescription> {
    const tables = fc.tuple(
        headers,
        fc.array(fields, { minLength: minFields, maxLength: FIELDS_PER_TABLE })
    );

    return (
        tables
            // oxlint-disable-next-line oxc/no-map-spread -- fast-check reuses generated values as it shrinks
            .map(([header, fields]) => ({ ...header, fields }))
            .filter(fitsItsLayout)
    );
}

/**
 * Whether a table and the tables nested in it are short enough for their
 * offset types, as their builders judge. The builder-limit properties check
 * that the builders reject the fields that make tables too long, at the
 * limit.
 */
export function fitsItsLayout(table: TableDescription): boolean {
    try {
        buildTable(table);
        return true;
    } catch (error) {
        if (error instanceof RangeError) {
            return false;
        }

        throw error;
    }
}

export interface TableOptions {
    /** The offset types of the table, and not of the tables nested in it */
    offsetTypes?: readonly OffsetType[];
    /** Whether the table has no header, as `setTable()` requires */
    headerless?: boolean;
    minFields?: number;
}

/** Descriptions of tables that fit their layouts */
export function tables({
    offsetTypes = OFFSET_TYPES,
    headerless: withoutHeader = false,
    minFields = 0
}: TableOptions = {}): fc.Arbitrary<TableDescription> {
    const layouts = withoutHeader
        ? headerless(offsetTypes)
        : headers(offsetTypes);

    return tablesOf(layouts, FIELDS, minFields);
}

/** Where a reader may find a table's bytes, as a slice of a larger buffer */
export interface Slice {
    /** The slice's offset in the buffer, which may leave it unaligned */
    byteOffset: number;
    /** The value of the buffer's bytes around the slice */
    fill: number;
}

/** Offsets of a slice, which align it to each element size or none */
export const BYTE_OFFSETS: fc.Arbitrary<number> = fc.integer({
    min: 0,
    max: 15
});

export const SLICES: fc.Arbitrary<Slice> = fc.record({
    byteOffset: BYTE_OFFSETS,
    fill: fc.integer({ min: 0, max: 0xff })
});

/** Copies `bytes` into a slice of a larger buffer */
export function sliceOf(
    bytes: Uint8Array,
    { byteOffset, fill }: Slice
): Uint8Array {
    const buffer = new Uint8Array(byteOffset + bytes.length + 16).fill(fill);
    buffer.set(bytes, byteOffset);

    return buffer.subarray(byteOffset, byteOffset + bytes.length);
}

/**
 * A corruption of bytes of any length, which {@link corruptionOf} fits to
 * them: a truncation that cuts 1 + `cut` bytes from the end, or a run of
 * bytes written at `at`
 */
export type AnyCorruption =
    | { kind: 'truncate'; cut: number }
    | { kind: 'overwrite'; at: number; bytes: number[] };

/**
 * Corruptions of bytes. fast-check draws small numbers more often, so most
 * truncations cut a few bytes, which leaves a field that ends a table short.
 */
export const CORRUPTIONS: fc.Arbitrary<AnyCorruption> = fc.oneof(
    fc.nat().map((cut): AnyCorruption => ({ kind: 'truncate', cut })),
    fc
        .tuple(
            fc.nat(),
            fc.array(fc.integer({ min: 0, max: 0xff }), {
                minLength: 1,
                maxLength: 4
            })
        )
        .map(([at, bytes]): AnyCorruption => ({ kind: 'overwrite', at, bytes }))
);

/**
 * Fits a corruption to bytes of `length`: a truncation cuts at least one of
 * them, and an overwrite stays within them
 */
export function corruptionOf(
    corruption: AnyCorruption,
    length: number
): Corruption {
    if (corruption.kind === 'truncate') {
        const cut = 1 + wrap(corruption.cut, length);
        return { kind: 'truncate', length: Math.max(length - cut, 0) };
    }

    const at = wrap(corruption.at, length);
    return {
        kind: 'overwrite',
        at,
        bytes: corruption.bytes.slice(0, length - at)
    };
}

/** `value` within [0, length), or 0 if `length` is 0 */
function wrap(value: number, length: number): number {
    return length === 0 ? 0 : value % length;
}
