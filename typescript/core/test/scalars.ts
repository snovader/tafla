import { fc } from 'hypotes/fast-check';
import {
    writeFloat32,
    writeFloat64,
    writeInt8,
    writeInt16,
    writeInt32,
    writeInt64,
    writeUint8,
    writeUint16,
    writeUint32,
    writeUint64,
    TableWriter
} from 'tafla';
import { SCALAR_TYPES } from './conformance/arbitraries.ts';
import type { ScalarType } from './conformance/description.ts';
import { ELEMENT_SIZES } from './conformance/tables.ts';

// The scalar writes that take a value as `DataView`'s setters take it: the
// write functions and `TableWriter`'s setters, with values of any type for
// them, and the setters as their reference.

/** A scalar type's writes, and `DataView`'s setter of it */
export interface ScalarWrites {
    type: ScalarType;
    size: number;
    /**
     * Any value that the setter takes: numbers or bigints of any size, values
     * that it converts to them, and values that it rejects
     */
    values: fc.Arbitrary<unknown>;
    /** 1, as the setter takes it: a `bigint` or a `number` */
    one: unknown;
    /** 1 as the other kind, which the setter rejects with a `TypeError` */
    wrongType: unknown;
    /** `DataView`'s setter, little-endian */
    reference: (view: DataView, offset: number, value: unknown) => void;
    /** The write function, such as `writeInt8()` */
    write: (bytes: Uint8Array, offset: number, value: unknown) => void;
    /** `TableWriter`'s setter, such as `setInt8()` */
    set: (writer: TableWriter, index: number, value: unknown) => void;
}

/** An object that converts to `value` */
function convertingTo<T>(value: T): { valueOf(): T } {
    return { valueOf: () => value };
}

/**
 * Numbers in and out of every type's range, fractions and special floats,
 * values that convert to numbers, and a `bigint` and a `Symbol`, which throw
 */
const NUMBERS: fc.Arbitrary<unknown> = fc.oneof(
    fc.double(),
    fc.integer(),
    fc.maxSafeInteger(),
    fc.constantFrom(
        -0,
        0.5,
        -1.9,
        128,
        -129,
        256,
        32_768,
        -32_769,
        65_536,
        2 ** 31,
        -(2 ** 31) - 1,
        2 ** 32,
        2 ** 32 + 5,
        2 ** 53 + 2,
        -1e20,
        // The largest float32, and numbers that round past it and to 0
        2 ** 128 - 2 ** 104,
        2 ** 128 - 2 ** 103,
        2 ** -150,
        Number.MIN_VALUE,
        Number.NaN,
        Number.POSITIVE_INFINITY,
        Number.NEGATIVE_INFINITY
    ),
    fc.constantFrom<unknown>(
        '-70000',
        ' 0x10 ',
        'tafla',
        true,
        null,
        undefined,
        [7],
        convertingTo(70_000),
        5n,
        Symbol('value')
    )
);

/**
 * Bigints in and out of the 64-bit types' range, values that convert to
 * bigints, and numbers, `undefined` and strings that aren't integers, which
 * throw
 */
const BIGINTS: fc.Arbitrary<unknown> = fc.oneof(
    fc.bigInt({ min: -(2n ** 70n), max: 2n ** 70n }),
    fc.constantFrom(
        -1n,
        2n ** 63n,
        -(2n ** 63n) - 1n,
        2n ** 64n,
        2n ** 64n + 5n
    ),
    fc.constantFrom<unknown>(
        true,
        '-123',
        ' 0x10 ',
        convertingTo(2n ** 64n + 7n),
        1,
        undefined,
        '1.5',
        Symbol('value')
    )
);

const WRITE_FUNCTIONS: Readonly<Record<ScalarType, ScalarWrites['write']>> = {
    int8: writeInt8,
    uint8: writeUint8,
    int16: writeInt16,
    uint16: writeUint16,
    int32: writeInt32,
    uint32: writeUint32,
    int64: writeInt64,
    uint64: writeUint64,
    float32: writeFloat32,
    float64: writeFloat64
} as Record<ScalarType, ScalarWrites['write']>;

/** Each scalar type's writes */
export const SCALAR_WRITES: readonly ScalarWrites[] =
    SCALAR_TYPES.map(writesOf);

function writesOf(type: ScalarType): ScalarWrites {
    const name = `${type[0].toUpperCase()}${type.slice(1)}`;
    const bigint = type === 'int64' || type === 'uint64';
    const viewSetter = `set${bigint ? 'Big' : ''}${name}` as const;
    const writerSetter = `set${name}` as const;

    return {
        type,
        size: ELEMENT_SIZES[type],
        values: bigint ? BIGINTS : NUMBERS,
        one: bigint ? 1n : 1,
        wrongType: bigint ? 1 : 1n,
        reference: (view, offset, value) => {
            (
                view[viewSetter as keyof DataView] as (
                    ...args: unknown[]
                ) => void
            ).call(view, offset, value, true);
        },
        write: WRITE_FUNCTIONS[type],
        set: (writer, index, value) => {
            (
                writer[writerSetter as keyof TableWriter] as (
                    ...args: unknown[]
                ) => void
            ).call(writer, index, value);
        }
    };
}

/** What a write left: the class of the error that it threw, if any, and the bytes */
export interface WriteAttempt {
    error?: string;
    /** The whole buffer of the bytes written into, so that a stray write shows */
    buffer: Uint8Array;
}

/** What the writes left, and what `DataView`'s setter left in their place */
export interface WriteAttempts {
    actual: WriteAttempt;
    expected: WriteAttempt;
}

/**
 * What the write function leaves of `value` at `offset` in `bytes`, and what
 * `DataView`'s setter leaves in a copy of their buffer
 */
export function writeAttempts(
    { write, reference }: ScalarWrites,
    bytes: Uint8Array,
    offset: number,
    value: unknown
): WriteAttempts {
    const copy = new Uint8Array(
        bytes.buffer.slice(0),
        bytes.byteOffset,
        bytes.length
    );
    const view = new DataView(copy.buffer, copy.byteOffset, copy.length);

    return {
        actual: attempt(bytes, () => write(bytes, offset, value)),
        expected: attempt(copy, () => reference(view, offset, value))
    };
}

/** Where the default layout's table of one field holds it, after its offset */
const FIELD = 4;

/**
 * What `TableWriter`'s setter leaves of `value` in a table of the one field,
 * in bytes of `fill`, and what `DataView`'s setter leaves, followed by the
 * field's offset
 */
export function setAttempts(
    { size, set, reference }: ScalarWrites,
    value: unknown,
    fill: number
): WriteAttempts {
    const bytes = new Uint8Array(FIELD + size).fill(fill);
    const writer = new TableWriter(bytes, 1);
    const expected = new Uint8Array(FIELD + size).fill(fill);

    return {
        actual: attempt(bytes, () => set(writer, 0, value)),
        expected: attempt(expected, () => {
            reference(new DataView(expected.buffer), FIELD, value);
            expected.set([FIELD, 0, 0, 0]);
        })
    };
}

/** Runs `write` into `bytes`, and returns what it left */
function attempt(bytes: Uint8Array, write: () => void): WriteAttempt {
    let error: string | undefined;
    try {
        write();
    } catch (thrown) {
        error =
            thrown instanceof Error ? thrown.constructor.name : String(thrown);
    }

    return { error, buffer: new Uint8Array(bytes.buffer.slice(0)) };
}
