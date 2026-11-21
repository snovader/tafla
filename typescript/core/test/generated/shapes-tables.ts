import { fc } from 'hypotes/fast-check';
import { SCALAR_VALUES, STRINGS } from '../conformance/arbitraries.ts';
import type {
    FieldDescription,
    TableDescription
} from '../conformance/description.ts';
import {
    array,
    message,
    scalar,
    string,
    table,
    vector,
    DEFAULT_LAYOUT
} from './descriptions.ts';
import * as javascript from './shapes/javascript/shapes.js';
import * as typescript from './shapes/typescript/shapes.ts';
import type {
    ColumnsFields,
    DocumentFields,
    ItemFields,
    ItemListFields,
    PairFields,
    PointFields,
    SampleFields
} from './shapes/typescript/shapes.ts';
import { readItems } from './views.ts';

// The code generated from schemas/shapes.tafla, how tests read its messages
// back and describe them to build their tables by hand, and arbitrary values
// of their fields.

/**
 * The generated TypeScript or JavaScript. Tests call either through this
 * union, which type-checks only while the JavaScript's JSDoc types match the
 * TypeScript's.
 */
export type Shapes = typeof typescript | typeof javascript;

export const SHAPES: ReadonlyArray<readonly [string, Shapes]> = [
    ['TypeScript', typescript],
    ['JavaScript', javascript]
];

type Point = typescript.Point | javascript.Point;
type Document = typescript.Document | javascript.Document;
type Columns = typescript.Columns | javascript.Columns;
export type ItemList = typescript.ItemList | javascript.ItemList;
type Item = typescript.Item | javascript.Item;
type Pair = typescript.Pair | javascript.Pair;
type Sample = typescript.Sample | javascript.Sample;

// Reading every field, with arrays copied to plain arrays to compare with the
// fields that were written

export function readPoint(point: Point): PointFields {
    return {
        x: point.x(),
        y: point.y(),
        id: point.id(),
        name: point.name()
    };
}

export function readDocument(document: Document): DocumentFields {
    return {
        x: document.x(),
        y: document.y(),
        name: document.name(),
        columns: readColumns(document.columns())
    };
}

function readColumns(columns: Columns): ColumnsFields {
    return {
        base: columns.base(),
        keys: Array.from(columns.keys()),
        a: Array.from(columns.a()),
        b: Array.from(columns.b())
    };
}

export function readItemList(itemList: ItemList): ItemListFields {
    return { items: readItems(itemList.items(), readItem) };
}

export function readItem(item: Item): ItemFields {
    return { value: item.value(), pair: readPair(item.pair()) };
}

function readPair(pair: Pair): PairFields {
    return { first: pair.first(), second: pair.second() };
}

export function readSample(sample: Sample): SampleFields {
    return { key: sample.key(), a: sample.a(), b: sample.b() };
}

// The tables that the generated code writes, in the conformance suite's
// model

/** The layout and version of a `Document`, from its attributes */
export const DOCUMENT_LAYOUT = {
    layout: { ...DEFAULT_LAYOUT, sized: true, versioned: true },
    version: 1
};

export function describePoint({
    x,
    y,
    id,
    name
}: PointFields): TableDescription {
    return message(DEFAULT_LAYOUT, [
        scalar('float64', x),
        scalar('float64', y),
        scalar('int64', id),
        string(name)
    ]);
}

export function describeDocument(
    { x, y, name, columns }: DocumentFields,
    version = DOCUMENT_LAYOUT.version
): TableDescription {
    const { base, keys, a, b } = columns;

    return message(
        DOCUMENT_LAYOUT.layout,
        [
            scalar('float64', x),
            scalar('float64', y),
            string(name),
            table([
                scalar('int64', base),
                array('uint32', keys),
                array('float32', a),
                array('float32', b)
            ])
        ],
        version
    );
}

export function describeItemList({ items }: ItemListFields): TableDescription {
    return message(DEFAULT_LAYOUT, [vector(items.map(describeItem))]);
}

function describeItem({ value, pair }: ItemFields): FieldDescription[] {
    const { first, second } = pair;

    return [
        scalar('float32', value),
        table([scalar('float32', first), scalar('float32', second)])
    ];
}

/**
 * A `Sample` as its fields lie at fixed offsets, little-endian, written with
 * DataView rather than tafla
 */
export function sampleBytes({ key, a, b }: SampleFields): Uint8Array {
    const bytes = new Uint8Array(12);
    const view = new DataView(bytes.buffer);
    view.setUint32(0, key, true);
    view.setFloat32(4, a, true);
    view.setFloat32(8, b, true);

    return bytes;
}

// Arbitrary values of the fields, over each type's full range, with -0, NaN
// and the infinities, strings outside the Basic Multilingual Plane, and empty
// arrays and vectors

const MAX_LENGTH = 8;

/**
 * The most items of a long vector, which writes its items from 5, 140 bytes,
 * through a DataView rather than the write functions
 */
const LONG_RUN = 24;

function arrays<T>(
    values: fc.Arbitrary<T>,
    maxLength = MAX_LENGTH
): fc.Arbitrary<T[]> {
    return fc.array(values, { maxLength });
}

export const POINT_FIELDS: fc.Arbitrary<PointFields> = fc.record({
    x: SCALAR_VALUES.float64,
    y: SCALAR_VALUES.float64,
    id: SCALAR_VALUES.int64,
    name: STRINGS
});

export const DOCUMENT_FIELDS: fc.Arbitrary<DocumentFields> = fc.record({
    x: SCALAR_VALUES.float64,
    y: SCALAR_VALUES.float64,
    name: STRINGS,
    columns: fc.record({
        base: SCALAR_VALUES.int64,
        keys: arrays(SCALAR_VALUES.uint32),
        a: arrays(SCALAR_VALUES.float32),
        b: arrays(SCALAR_VALUES.float32)
    })
});

const ITEMS: fc.Arbitrary<ItemListFields['items'][number]> = fc.record({
    value: SCALAR_VALUES.float32,
    pair: fc.record({
        first: SCALAR_VALUES.float32,
        second: SCALAR_VALUES.float32
    })
});

export const ITEM_LIST_FIELDS: fc.Arbitrary<ItemListFields> = fc.record({
    items: arrays(ITEMS)
});

/** Item lists of short vectors and of long ones, written through a DataView */
export const LONG_ITEM_LIST_FIELDS: fc.Arbitrary<ItemListFields> = fc.record({
    items: arrays(ITEMS, LONG_RUN)
});

export const SAMPLE_FIELDS: fc.Arbitrary<SampleFields> = fc.record({
    key: SCALAR_VALUES.uint32,
    a: SCALAR_VALUES.float32,
    b: SCALAR_VALUES.float32
});
