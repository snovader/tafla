import { describe, expect } from 'vitest';
import { fc, it } from '@fast-check/vitest';
import { TableBuilder, TableWriter } from 'tafla';

type NumberArrayType =
    | 'Float32'
    | 'Float64'
    | 'Int8'
    | 'Int16'
    | 'Int32'
    | 'Uint8'
    | 'Uint16'
    | 'Uint32';

const numberTypes = [
    ['Float32', Float32Array],
    ['Float64', Float64Array],
    ['Int8', Int8Array],
    ['Int16', Int16Array],
    ['Int32', Int32Array],
    ['Uint8', Uint8Array],
    ['Uint16', Uint16Array],
    ['Uint32', Uint32Array]
] as const;

/** The input kinds that `values` can arrive as, holding the same numbers */
function numberInputs(values: number[]): Array<[string, ArrayLike<number>]> {
    const holey = new Array<number>(values.length);
    for (let i = 0; i < values.length; i++) {
        holey[i] = values[i];
    }

    const smis = values.map(value => (Number.isFinite(value) ? value | 0 : 0));

    return [
        ['number[] of doubles', values],
        ['number[] of SMIs', smis],
        ['holey number[]', holey],
        ['array-like object', { ...values, length: values.length }],
        ['Float64Array', Float64Array.from(values)],
        ['Float32Array', Float32Array.from(values)],
        ['Int32Array', Int32Array.from(values)],
        ['Uint8Array', Uint8Array.from(values)]
    ];
}

const doubles = fc.array(
    fc.oneof(
        fc.double(),
        fc.constantFrom(-0, Number.NaN, 2 ** 31, -(2 ** 31) - 1, 2 ** 32),
        fc.integer()
    )
);

/** Writes `values` with the builder and reads them back with the buffer */
function roundTrip(type: NumberArrayType, values: ArrayLike<number>): ArrayLike<number> {
    const builder = new TableBuilder(1);
    (builder[`set${type}Array`] as (values: ArrayLike<number>) => void)(values);

    const buffer = builder.finalize();
    return buffer[`get${type}Array`](buffer.rootOffset(0));
}

function expectElements(
    actual: ArrayLike<unknown>,
    expected: ArrayLike<unknown>,
    kind?: string
): void {
    expect(actual.length, kind).toBe(expected.length);

    // toBe compares with Object.is, so -0 and NaN must match exactly
    for (let i = 0; i < expected.length; i++) {
        expect(actual[i], kind).toBe(expected[i]);
    }
}

describe.each(numberTypes)('set%sArray()', (type, TypedArray) => {
    it.prop([doubles])(
        'should convert every input kind like the typed array constructor',
        values => {
            // The element type's own typed array is copied as bytes
            const inputs = [...numberInputs(values), [type, TypedArray.from(values)] as const];

            for (const [, input] of inputs) {
                expectElements(roundTrip(type, input), TypedArray.from(input));
            }
        }
    );

    it('should convert holes like the typed array constructor', () => {
        // biome-ignore lint/suspicious/noSparseArray: Holes are the input
        const values = [1.5, , -0, Number.NaN] as number[];

        expectElements(roundTrip(type, values), TypedArray.from(values));
    });
});

describe.each([
    ['Int64', BigInt64Array],
    ['Uint64', BigUint64Array]
] as const)('set%sArray()', (type, TypedArray) => {
    it.prop([fc.array(fc.bigInt({ min: -(2n ** 64n), max: 2n ** 64n }))])(
        'should convert every input kind like the typed array constructor',
        values => {
            const inputs: Array<ArrayLike<bigint>> = [
                values,
                { ...values, length: values.length },
                BigInt64Array.from(values, value => BigInt.asIntN(64, value)),
                BigUint64Array.from(values, value => BigInt.asUintN(64, value))
            ];

            for (const input of inputs) {
                const builder = new TableBuilder(1);
                builder[`set${type}Array`](input);

                const buffer = builder.finalize();
                expectElements(
                    buffer[`get${type}Array`](buffer.rootOffset(0)),
                    TypedArray.from(input)
                );
            }
        }
    );

    it('should reject numbers', () => {
        const builder = new TableBuilder(1);

        expect(() => {
            builder[`set${type}Array`]([1] as unknown as bigint[]);
            builder.finalize();
        }).toThrow(TypeError);
    });
});

it.prop([doubles, fc.integer({ min: 1, max: 7 })])(
    'should convert into a slice that is unaligned for the element type',
    (values, byteOffset) => {
        // The array's offset, its count at 4, and its elements from 8
        const length = 8 + values.length * 8;

        for (const [, input] of numberInputs(values)) {
            const aligned = writeFloat64Array(new Uint8Array(length), input);

            const backing = new Uint8Array(byteOffset + length);
            const unaligned = writeFloat64Array(backing.subarray(byteOffset), input);

            expect(unaligned).toStrictEqual(aligned);
        }
    }
);

/** Writes a table of one float64 array into `bytes`, which it fills */
function writeFloat64Array(bytes: Uint8Array, values: ArrayLike<number>): Uint8Array {
    const writer = new TableWriter(bytes, 1);
    writer.setFloat64Array(0, values);

    return writer.bytes();
}
