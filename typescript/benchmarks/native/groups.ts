import * as native from '@tafla/native';
import { tafla } from '../comparison/codecs/tafla.ts';
import { taflaGen } from '../comparison/codecs/taflaGen.ts';
import type { Group, Variant } from '../comparison/harness.ts';
import {
    getterAccess,
    sumItem,
    tableReaderAccess,
    writeItems
} from '../comparison/itemAccess.ts';
import { listInput, type ListInput } from '../comparison/workloads.ts';
import {
    assertSameValues,
    compile,
    randomValues,
    tableOfFields,
    LENGTH,
    type ElementType,
    type TypedArray
} from '../fields/groups.ts';
import { sumNewGeneratedColumns, GeneratedItems } from './generatedItems.ts';
import {
    createColumns,
    createReusedColumnsSum,
    sumNewColumns
} from './itemColumns.ts';

/**
 * The types whose fields are read: a number, a BigInt, and a float64, which
 * V8 keeps as a heap number where it doesn't remove it
 */
const FIELD_TYPES = [
    ['Int32', Int32Array],
    ['Int64', BigInt64Array],
    ['Float64', Float64Array]
] as const satisfies ElementType[];

/** The items of each vector of nested tables */
const ITEM_COUNTS = [100, 10_000];

/**
 * The groups that compare the addon's reads of many values in one call with
 * the TypeScript runtime's getters, each checked to read the values it
 * should. The first variant of each group is the TypeScript runtime, and the
 * last is it again, an A/A pair that shows the harness's noise.
 */
export function nativeGroups(): Group[] {
    return [
        ...FIELD_TYPES.map(fieldsGroup),
        ...ITEM_COUNTS.flatMap(count => [
            itemsGroup(count),
            deserializeGroup(count),
            serializeGroup(count)
        ])
    ];
}

/**
 * Reads every field of a table of one type into a typed array of that type,
 * with `TableReader`'s getter in a loop and with the addon in one call
 */
function fieldsGroup([type, TypedArray]: ElementType): Group {
    const values = randomValues(TypedArray);
    const reader = tableOfFields(values, type);

    const bytes = reader.bytes();
    const getFields = native[`get${type}Fields`];

    const getterVariant = (name: string): Variant => {
        const results = new TypedArray(LENGTH);
        const run = compile(
            { reader, results },
            `for (let i = 0; i < results.length; i++) {
                results[i] = reader.get${type}(reader.rootOffset(i));
            }
            return results;`
        );

        return checkedFields(name, run, results, values);
    };

    const nativeVariant = (name: string): Variant => {
        const results = new TypedArray(LENGTH);
        const run = compile(
            { getFields, bytes, results },
            `getFields(bytes, results);
            return results;`
        );

        return checkedFields(name, run, results, values);
    };

    return {
        name: `${LENGTH} ${type} fields`,
        variants: [
            getterVariant(`get${type}`),
            nativeVariant(`native get${type}Fields`),
            getterVariant(`get${type} A/A`)
        ]
    };
}

/**
 * A variant of `run`, which must read `values` into `results` in its first
 * call
 */
function checkedFields(
    name: string,
    run: () => unknown,
    results: TypedArray,
    values: TypedArray
): Variant {
    run();
    assertSameValues(results, values, name);

    return { name, run };
}

/**
 * Sums every item of a vector of nested tables: with `TableReader`'s
 * getters by hand and with tafla-gen's views, as the comparison does, and
 * from columns that the addon reads, by the paths of their fields or with
 * its function of the schema, created in each call or kept between calls
 */
function itemsGroup(count: number): Group {
    const list = listInput(count);
    const { tables } = writeItems(list);
    const sumReusedColumns = createReusedColumnsSum(count);
    const generated = new GeneratedItems(count);

    let expected = 0;
    for (const item of list.items) {
        expected += sumItem(item);
    }

    const variant = (name: string, run: () => number): Variant => {
        const actual = run();
        if (!Object.is(actual, expected)) {
            throw new Error(
                `${name} summed the items to ${actual}, rather than ${expected}`
            );
        }

        return { name, run };
    };

    return {
        name: `vector of ${count} nested tables: sum every item`,
        variants: [
            variant('tafla', () => tableReaderAccess.everyTable(tables)),
            variant('tafla-gen', () => getterAccess.everyTable(tables)),
            variant('native columns', () => sumNewColumns(tables)),
            variant('native columns, reused', () => sumReusedColumns(tables)),
            variant('native generated', () => sumNewGeneratedColumns(tables)),
            variant('native generated, reused', () => generated.sum(tables)),
            variant('tafla A/A', () => tableReaderAccess.everyTable(tables))
        ]
    };
}

/**
 * Reads a vector of nested tables as the workload's objects: with the
 * comparison's codecs, and from the columns that the addon's function of the
 * schema reads
 */
function deserializeGroup(count: number): Group {
    const list = listInput(count);
    const bytes = taflaGen.list.serialize(list);
    const generated = new GeneratedItems(count);

    const variant = (name: string, run: () => ListInput): Variant => {
        assertSameItems(name, run(), list);

        return { name, run };
    };

    return {
        name: `vector of ${count} nested tables: deserialize`,
        variants: [
            variant('tafla', () => tafla.list.deserialize(bytes)),
            variant('tafla-gen', () => taflaGen.list.deserialize(bytes)),
            variant('native generated', () => generated.deserialize(bytes)),
            variant('tafla A/A', () => tafla.list.deserialize(bytes))
        ]
    };
}

/**
 * Writes a vector of nested tables from the workload's objects: with the
 * comparison's codecs, and with the addon's function of the schema, through
 * columns that it fills from the objects, or from columns that the caller
 * holds already
 */
function serializeGroup(count: number): Group {
    const list = listInput(count);
    const expected = taflaGen.list.serialize(list);
    const generated = new GeneratedItems(count);

    const columns = createColumns(count);
    list.items.forEach(({ value, first, second }, i) => {
        columns.value[i] = value;
        columns.first[i] = first;
        columns.second[i] = second;
    });

    const variant = (name: string, run: () => Uint8Array): Variant => {
        assertSameValues(run(), expected, name);

        return { name, run };
    };

    return {
        name: `vector of ${count} nested tables: serialize`,
        variants: [
            variant('tafla', () => tafla.list.serialize(list)),
            variant('tafla-gen', () => taflaGen.list.serialize(list)),
            variant('native generated', () => generated.serialize(list)),
            variant('native generated, from columns', () =>
                generated.serializeColumns(count, columns)
            ),
            variant('tafla A/A', () => tafla.list.serialize(list))
        ]
    };
}

function assertSameItems(
    name: string,
    actual: ListInput,
    expected: ListInput
): void {
    const fieldsOf = ({ items }: ListInput): number[] =>
        items.flatMap(({ value, first, second }) => [value, first, second]);

    assertSameValues(fieldsOf(actual), fieldsOf(expected), name);
}
