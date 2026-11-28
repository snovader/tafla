import type { Group, Variant } from 'benk';
import { TableArena, TableBuilder, type TableReader } from 'tafla';

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

/** The fields of a table of scalars, and the elements of each array */
const LENGTH = 1000;

/** The fields of a table of arrays */
const ARRAYS = 100;

const TYPES = [
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

type ElementType = (typeof TYPES)[number];
type TypeName = ElementType[0];

/** Numbers the sources of the variants' calls, so that no two are the same */
let sourceId = 0;

/**
 * The groups of `TableReader`'s getters and `TableBuilder`'s setters, with a
 * variant for each element type, each checked to read or write the values it
 * should. The array setters, of `number[]` and `bigint[]`, come last, since
 * they change the type feedback of the code that the typed array setters
 * share.
 */
export function fieldGroups(): Group[] {
    return [
        { name: 'primitive getters', variants: TYPES.map(getterVariant) },
        {
            name: 'typed array getters',
            variants: TYPES.map(arrayGetterVariant)
        },
        { name: 'primitive setters', variants: TYPES.map(setterVariant) },
        {
            name: 'typed array setters',
            variants: TYPES.map(typedArraySetterVariant)
        },
        { name: 'array setters', variants: TYPES.map(plainArraySetterVariant) }
    ];
}

/**
 * Reads every field of a table of one type with its getter, into a typed
 * array of that type, which keeps every value without allocating, so that V8
 * can't discard the reads
 */
function getterVariant([type, TypedArray]: ElementType): Variant {
    const name = `get${type}`;
    const values = randomValues(TypedArray);
    const reader = tableOfFields(values, type);
    const results = new TypedArray(LENGTH);

    const run = compile(
        { reader, results },
        `for (let i = 0; i < results.length; i++) {
            results[i] = reader.get${type}(reader.rootOffset(i));
        }
        return results;`
    );

    run();
    assertSameValues(results, values, name);

    return { name, run };
}

/**
 * Reads every array of a table of arrays of one type, into an array that
 * keeps them
 */
function arrayGetterVariant([type, TypedArray]: ElementType): Variant {
    const name = `get${type}Array`;
    const arrays = randomArrays(TypedArray);
    const reader = tableOfArrays(arrays, type);
    const results = Array.from<TypedArray | undefined>({ length: ARRAYS });

    const run = compile(
        { reader, results },
        `for (let i = 0; i < results.length; i++) {
            results[i] = reader.get${type}Array(reader.rootOffset(i));
        }
        return results;`
    );

    run();
    arrays.forEach((array, i) => {
        assertSameValues(results[i] ?? [], array, name);
    });

    return { name, run };
}

/** Writes a table of one type's fields */
function setterVariant([type, TypedArray]: ElementType): Variant {
    const name = `set${type}`;
    const values = randomValues(TypedArray);

    const run = compile(
        { TableArena, TableBuilder, values },
        `const builder = new TableBuilder(TableArena.DEFAULT, values.length);
        for (let i = 0; i < values.length; i++) {
            builder.set${type}(values[i]);
        }
        return builder.finalize();`
    );

    assertSameValues(fieldsOf(run() as TableReader, type), values, name);

    return { name, run };
}

/** Writes a table of arrays of one type from typed arrays of that type */
function typedArraySetterVariant([type, TypedArray]: ElementType): Variant {
    const arrays = randomArrays(TypedArray);
    return arraySetterVariant(type, arrays, arrays);
}

/** Writes a table of arrays of one type from `number[]` or `bigint[]` */
function plainArraySetterVariant([type, TypedArray]: ElementType): Variant {
    const arrays = randomArrays(TypedArray);
    const plainArrays = arrays.map(array => [...array]);

    return arraySetterVariant(type, plainArrays, arrays);
}

/**
 * Writes a table of `inputs`, each an array field, and checks that it reads
 * back as `arrays`
 */
function arraySetterVariant(
    type: TypeName,
    inputs: Array<ArrayLike<bigint | number>>,
    arrays: TypedArray[]
): Variant {
    const name = `set${type}Array`;

    const run = compile(
        { TableArena, TableBuilder, inputs },
        `const builder = new TableBuilder(TableArena.DEFAULT, inputs.length);
        for (let i = 0; i < inputs.length; i++) {
            builder.set${type}Array(inputs[i]);
        }
        return builder.finalize();`
    );

    const reader = run() as TableReader;
    arrays.forEach((array, i) => {
        assertSameValues(
            reader[`get${type}Array`](reader.rootOffset(i)),
            array,
            name
        );
    });

    return { name, run };
}

/**
 * Compiles a variant's call from a source of its own, which calls the getter
 * or setter by name, as a caller of that one method would. V8 shares type
 * feedback between the closures of one function literal, and between
 * identical sources through its compilation cache: a call site shared by
 * every type's getter goes megamorphic, and the getters after the first three
 * then took 14 to 19 µs per 1000 reads, against 3.2 to 3.5 µs each from a
 * source of its own. Each source ends with a number of its own, since the
 * typed array and array setters have the same code.
 */
function compile(
    parameters: Record<string, unknown>,
    body: string
): () => unknown {
    // oxlint-disable-next-line typescript/no-implied-eval -- A call of its own for each variant
    const create = new Function(
        ...Object.keys(parameters),
        `return () => {
        ${body}
        }; // variant ${sourceId++}`
    ) as (...values: unknown[]) => () => unknown;

    return create(...Object.values(parameters));
}

/** A table of a field of `values`' type for each of them */
function tableOfFields(values: TypedArray, type: TypeName): TableReader {
    const builder = new TableBuilder(TableArena.DEFAULT, values.length);
    const set = builder[`set${type}`].bind(builder) as (
        value: bigint | number
    ) => void;

    for (const value of values) {
        set(value);
    }

    return builder.finalize();
}

/** A table of an array field of `arrays`' type for each of them */
function tableOfArrays(arrays: TypedArray[], type: TypeName): TableReader {
    const builder = new TableBuilder(TableArena.DEFAULT, arrays.length);
    const set = builder[`set${type}Array`].bind(builder) as (
        values: TypedArray
    ) => void;

    for (const array of arrays) {
        set(array);
    }

    return builder.finalize();
}

/** Reads every field of a table of one type */
function fieldsOf(reader: TableReader, type: TypeName): Array<bigint | number> {
    const fields: Array<bigint | number> = [];
    for (let i = 0; i < LENGTH; i++) {
        fields.push(reader[`get${type}`](reader.rootOffset(i)));
    }

    return fields;
}

/**
 * Throws unless `actual` holds the same values as `expected`, compared with
 * `Object.is`, which tells -0 from 0
 */
function assertSameValues(
    actual: ArrayLike<unknown>,
    expected: ArrayLike<unknown>,
    name: string
): void {
    if (actual.length !== expected.length) {
        throw new Error(
            `${name} read ${actual.length} values, rather than ${expected.length}`
        );
    }

    for (let i = 0; i < expected.length; i++) {
        if (!Object.is(actual[i], expected[i])) {
            throw new Error(
                `${name} read ${String(actual[i])} at ${i}, rather than ${String(expected[i])}`
            );
        }
    }
}

function randomArrays(TypedArray: TypedArrayConstructor): TypedArray[] {
    return Array.from({ length: ARRAYS }, () => randomValues(TypedArray));
}

/**
 * Random values over the whole range of an integer type, or from -1 to 1 for
 * floats
 */
function randomValues(TypedArray: TypedArrayConstructor): TypedArray {
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
