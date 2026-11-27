// The description of the conformance suite in `cases.json`, which every
// implementation loads to read and rebuild the tables. README.md explains
// the format.

export type ScalarType =
    | 'int8'
    | 'uint8'
    | 'int16'
    | 'uint16'
    | 'int32'
    | 'uint32'
    | 'int64'
    | 'uint64'
    | 'float32'
    | 'float64';

export type ArrayType = `${ScalarType}[]`;

/**
 * A value as JSON holds it: a number, a 64-bit integer as a decimal string,
 * since JSON numbers lose precision past 2^53 in JavaScript, or one of the
 * floats that JSON has no number for: "NaN", "Infinity", "-Infinity" or "-0".
 */
export type JsonNumber = number | string;

export type OffsetType = 'uint8' | 'uint16' | 'uint32';

/** The offset size, the width in bytes of an offset type */
export type OffsetSize = 1 | 2 | 4;

export type Alignment = 1 | 2 | 4 | 8;

export interface LayoutDescription {
    sized: boolean;
    versioned: boolean;
    offsetType: OffsetType;
}

export interface TableDescription {
    layout: LayoutDescription;
    /** The version in the header, present when the layout is versioned */
    version?: number;
    fields: FieldDescription[];
}

/** A table nested from its builder or from the bytes that its builder writes */
export interface NestedTable {
    from: 'builder' | 'bytes';
    table: TableDescription;
}

export interface TableField extends NestedTable {
    type: 'table';
    /** The alignment passed to `setTable()`, absent for its default */
    alignment?: Alignment;
}

/** A scalar of a struct, with its value */
export interface StructScalar {
    type: ScalarType;
    value: JsonNumber;
}

/**
 * An array of structs: the types of the scalars of each, and the values of
 * every struct's scalars in that order
 */
export interface StructArrayField {
    type: 'struct[]';
    fields: ScalarType[];
    items: JsonNumber[][];
}

export type FieldDescription =
    | { type: ScalarType; value: JsonNumber }
    | BoolField
    | { type: ArrayType; value: JsonNumber[] }
    | BoolArrayField
    | { type: 'string'; value: string }
    | { type: 'struct'; fields: StructScalar[] }
    | StructArrayField
    | TableField
    | { type: 'vector'; items: NestedTable[] };

/**
 * A bool, which the runtimes write as the uint8 1 or 0 and read as true when
 * its byte isn't 0
 */
export interface BoolField {
    type: 'bool';
    value: boolean;
}

/**
 * An array of bools, which the runtimes write as a uint8[] of 1 and 0 and
 * read as true where a byte isn't 0
 */
export interface BoolArrayField {
    type: 'bool[]';
    value: boolean[];
}

export type ArrayField = Extract<FieldDescription, { type: ArrayType }>;

/**
 * Whether a field is an array of integers or floats, rather than of structs
 * or bools
 */
export function isArrayField(field: FieldDescription): field is ArrayField {
    return (
        field.type !== 'struct[]' &&
        field.type !== 'bool[]' &&
        field.type.endsWith('[]')
    );
}

/** The element type of an array type */
export function elementTypeOf(type: ArrayType): ScalarType {
    return type.slice(0, -2) as ScalarType;
}

export interface ValidCase {
    name: string;
    description: string;
    /** The table's bytes, relative to `cases.json` */
    file: string;
    table: TableDescription;
}

/** How an invalid case's bytes differ from its valid case's */
export type Corruption =
    | { kind: 'truncate'; length: number }
    | { kind: 'overwrite'; at: number; bytes: number[] };

/**
 * Bytes that read the fields of a valid case before `field` as it describes
 * them, and then throw a decode error reading `field`
 */
export interface InvalidCase {
    name: string;
    description: string;
    file: string;
    /** The name of the valid case whose table description reads the bytes */
    case: string;
    corruption: Corruption;
    field: number;
}

/** Returns a copy of `bytes` with the corruption applied */
export function corrupt(bytes: Uint8Array, corruption: Corruption): Uint8Array {
    if (corruption.kind === 'truncate') {
        return bytes.slice(0, corruption.length);
    }

    const corrupted = bytes.slice();
    corrupted.set(corruption.bytes, corruption.at);

    return corrupted;
}

/**
 * Bytes that differ from a valid case's but read the values that it
 * describes, since readers accept more than writers write, such as any byte
 * other than 0 as a true bool. They aren't rebuilt.
 */
export interface EquivalentCase {
    name: string;
    description: string;
    file: string;
    /** The name of the valid case whose table description reads the bytes */
    case: string;
    corruption: Corruption;
}

export interface Suite {
    valid: ValidCase[];
    equivalent: EquivalentCase[];
    invalid: InvalidCase[];
}

/**
 * Returns a number as JSON holds it, with the floats that JSON has no number
 * for as their names
 */
export function toJsonNumber(value: number): JsonNumber {
    if (Number.isNaN(value)) {
        return 'NaN';
    }

    if (value === Number.POSITIVE_INFINITY) {
        return 'Infinity';
    }

    if (value === Number.NEGATIVE_INFINITY) {
        return '-Infinity';
    }

    return Object.is(value, -0) ? '-0' : value;
}

/**
 * Returns a number that JSON holds as a number or the name of a float.
 *
 * @throws {TypeError} If `value` is any other string, such as a 64-bit
 * integer's.
 */
export function fromJsonNumber(value: JsonNumber): number {
    switch (value) {
        case 'NaN':
            return Number.NaN;

        case 'Infinity':
            return Number.POSITIVE_INFINITY;

        case '-Infinity':
            return Number.NEGATIVE_INFINITY;

        case '-0':
            return -0;
    }

    if (typeof value !== 'number') {
        throw new TypeError(`Not a number: ${value}`);
    }

    return value;
}
