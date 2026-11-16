import { describe, test } from 'vitest';
import { TableBuilder, type ByteBuffer } from 'tafla';

type TypedArray =
    | Int8Array<ArrayBuffer>
    | Uint8Array<ArrayBuffer>
    | Int16Array<ArrayBuffer>
    | Uint16Array<ArrayBuffer>
    | Int32Array<ArrayBuffer>
    | Uint32Array<ArrayBuffer>
    | BigInt64Array<ArrayBuffer>
    | BigUint64Array<ArrayBuffer>
    | Float32Array<ArrayBuffer>
    | Float64Array<ArrayBuffer>;

type TypedArrayConstructor = new (length: number) => TypedArray;

const BENCHMARK_OPTIONS = {
    iterations: 10,
    time: 250,
    warmupIterations: 5,
    warmupTime: 50
} as const;

const LENGTH = 1000;
const ARRAYS = 100;

const cases = [
    ['Int8', Int8Array],
    ['Uint8', Uint8Array],
    ['Int16', Int16Array],
    ['Uint16', Uint16Array],
    ['Int32', Int32Array],
    ['Uint32', Uint32Array],
    ['Int64', BigInt64Array],
    ['Uint64', BigUint64Array],
    ['Float32', Float32Array],
    ['Float64', Float64Array]
] as const;

describe('primitive getters', () => {
    for (const [type, TypedArray] of cases) {
        const array = randomArray(TypedArray);
        const buffer = bufferFromTypedArray(array, `set${type}`);

        const get = buffer[`get${type}`].bind(buffer) as (
            position: number
        ) => number;

        test(`get${type}`, async ({ bench }) => {
            await bench(`get${type}`, () => {
                for (let i = 0; i < array.length; i++) {
                    get(buffer.rootOffset(i));
                }
            }).run(BENCHMARK_OPTIONS);
        });
    }
});

describe('typed array getters', () => {
    for (const [type, TypedArray] of cases) {
        const arrays = Array.from({ length: ARRAYS }, () =>
            randomArray(TypedArray)
        );
        const buffer = bufferFromTypedArray(arrays, `set${type}Array`);

        const get = buffer[`get${type}Array`].bind(buffer);

        test(`get${type}Array`, async ({ bench }) => {
            await bench(`get${type}Array`, () => {
                for (let i = 0; i < arrays.length; i++) {
                    get(buffer.rootOffset(i));
                }
            }).run(BENCHMARK_OPTIONS);
        });
    }
});

describe('primitive setters', () => {
    for (const [type, TypedArray] of cases) {
        const array = randomArray(TypedArray);

        test(`set${type}`, async ({ bench }) => {
            await bench(`set${type}`, () => {
                bufferFromTypedArray(array, `set${type}`);
            }).run(BENCHMARK_OPTIONS);
        });
    }
});

describe('typed array setters', () => {
    for (const [type, TypedArray] of cases) {
        const arrays = Array.from({ length: ARRAYS }, () =>
            randomArray(TypedArray)
        );

        test(`set${type}Array`, async ({ bench }) => {
            await bench(`set${type}Array`, () => {
                bufferFromTypedArray(arrays, `set${type}Array`);
            }).run(BENCHMARK_OPTIONS);
        });
    }
});

describe('array setters', () => {
    for (const [type, TypedArray] of cases) {
        const arrays = Array.from({ length: ARRAYS }, () => [
            ...randomArray(TypedArray)
        ]);

        test(`set${type}Array`, async ({ bench }) => {
            await bench(`set${type}Array`, () => {
                bufferFromTypedArray(arrays, `set${type}Array`);
            }).run(BENCHMARK_OPTIONS);
        });
    }
});

type Setters<V extends TypedArray | unknown[]> =
    V extends ArrayLike<bigint | number>
        ? Exclude<Extract<keyof TableBuilder, `set${string}`>, `${string}Array`>
        : Extract<keyof TableBuilder, `set${string}Array`>;

function bufferFromTypedArray<V extends TypedArray | unknown[]>(
    array: V,
    setter: Setters<V>
): ByteBuffer {
    const builder = new TableBuilder(array.length);
    const set = builder[setter].bind(builder) as (value: V[number]) => void;

    for (const value of array) {
        set(value);
    }

    return builder.finalize();
}

/**
 * An array of random values over the whole range of an integer type, or from
 * -1 to 1 for floats
 */
function randomArray(TypedArray: TypedArrayConstructor): TypedArray {
    const array = new TypedArray(LENGTH);

    if (array instanceof Float32Array || array instanceof Float64Array) {
        for (let i = 0; i < array.length; i++) {
            array[i] = Math.random() * 2 - 1;
        }

        return array;
    }

    // Random bytes are random integers of every width
    crypto.getRandomValues(new Uint8Array(array.buffer));

    return array;
}
