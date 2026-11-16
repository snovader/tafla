import { toJsonNumber } from '../conformance/description.ts';
import { formatSuiteJson } from '../conformance/files.ts';
import type {
    DocumentFields,
    ItemListFields,
    PointFields,
    SampleFields
} from './shapes/typescript/shapes.ts';
import * as typescript from './shapes/typescript/shapes.ts';
import {
    readDocument,
    readItemList,
    readPoint,
    readSample,
    type Shapes
} from './shapesTables.ts';

// The cases of conformance/generated: values of the messages and the struct of
// schemas/shapes.tafla, whose tables the code generated in every language
// writes byte for byte and reads back. The generated TypeScript writes the
// tables; run `yarn conformance:generate` after changing a case, and every
// language's tests. conformance/generated/README.md describes the files.

export type ShapesCase = { name: string; description: string } & (
    | { type: 'Point'; fields: PointFields }
    | { type: 'Document'; fields: DocumentFields }
    | { type: 'ItemList'; fields: ItemListFields }
    | { type: 'Sample'; fields: SampleFields }
);

const INT64_MIN = -(2n ** 63n);
const INT64_MAX = 2n ** 63n - 1n;
const UINT32_MAX = 0xffff_ffff;
const FLOAT32_MAX = 2 ** 128 - 2 ** 104;
/** The smallest positive float32, a subnormal */
const FLOAT32_MIN = 2 ** -149;

export const SHAPES_CASES: readonly ShapesCase[] = [
    {
        name: 'point',
        description:
            'Signed zero, NaN, the smallest int64 and characters of one to four bytes of UTF-8',
        type: 'Point',
        fields: {
            x: -0,
            y: Number.NaN,
            id: INT64_MIN,
            name: 'a\u00e5\u4e16\u{1f980}'
        }
    },
    {
        name: 'point-limits',
        description:
            'The smallest float64, negative infinity, the largest int64 and an empty string',
        type: 'Point',
        fields: {
            x: Number.MIN_VALUE,
            y: Number.NEGATIVE_INFINITY,
            id: INT64_MAX,
            name: ''
        }
    },
    {
        name: 'document',
        description:
            'A sized and versioned table, with a string that starts with U+FEFF and a nested table of arrays of the largest uint32, signed zero, NaN, the extremes of float32 and the infinities',
        type: 'Document',
        fields: {
            x: 1.5,
            y: -2.25,
            name: '\ufeffdocument',
            columns: {
                base: -1n,
                keys: [0, 1, UINT32_MAX],
                a: [0.5, -0, Number.NaN, FLOAT32_MAX],
                b: [Number.POSITIVE_INFINITY, FLOAT32_MIN, -FLOAT32_MAX]
            }
        }
    },
    {
        name: 'document-empty-arrays',
        description:
            'A nested table of empty arrays, after the largest float64 and an empty string',
        type: 'Document',
        fields: {
            x: 0,
            y: Number.MAX_VALUE,
            name: '',
            columns: { base: INT64_MAX, keys: [], a: [], b: [] }
        }
    },
    {
        name: 'item-list',
        description:
            'A vector of three tables, each with a nested table of two float32',
        type: 'ItemList',
        fields: {
            items: [
                { value: 1, pair: { first: -0, second: Number.NaN } },
                {
                    value: -1.5,
                    pair: { first: FLOAT32_MAX, second: FLOAT32_MIN }
                },
                {
                    value: Number.POSITIVE_INFINITY,
                    pair: { first: 0.25, second: Number.NEGATIVE_INFINITY }
                }
            ]
        }
    },
    {
        name: 'item-list-empty',
        description: 'An empty vector',
        type: 'ItemList',
        fields: { items: [] }
    },
    {
        name: 'sample',
        description: 'A struct of the largest uint32, signed zero and NaN',
        type: 'Sample',
        fields: { key: UINT32_MAX, a: -0, b: Number.NaN }
    }
];

/** Writes a case's table or struct with a language's generated code */
export function serialize(shapes: Shapes, shapesCase: ShapesCase): Uint8Array {
    switch (shapesCase.type) {
        case 'Point':
            return shapes.Point.serialize(shapesCase.fields);

        case 'Document':
            return shapes.Document.serialize(shapesCase.fields);

        case 'ItemList':
            return shapes.ItemList.serialize(shapesCase.fields);

        case 'Sample':
            return shapes.Sample.serialize(shapesCase.fields);
    }
}

/** Reads every field of bytes as a case's type, with a language's generated code */
export function read(
    shapes: Shapes,
    type: ShapesCase['type'],
    bytes: Uint8Array
): ShapesCase['fields'] {
    switch (type) {
        case 'Point':
            return readPoint(new shapes.Point(bytes));

        case 'Document':
            return readDocument(new shapes.Document(bytes));

        case 'ItemList':
            return readItemList(new shapes.ItemList(bytes));

        case 'Sample':
            return readSample(new shapes.Sample(bytes));
    }
}

/** The directory of the cases, at the root of the repository */
export const SHAPES_DIRECTORY = new URL(
    '../../../../conformance/generated/',
    import.meta.url
);

export interface ShapesFiles {
    /** The text of `shapes.json` */
    json: string;
    /** The bytes of each case, by their paths relative to `shapes.json` */
    tables: Map<string, Uint8Array>;
}

/** Writes the cases' description, and their tables with the generated TypeScript */
export function writeShapesCases(cases: readonly ShapesCase[]): ShapesFiles {
    const tables = new Map<string, Uint8Array>();
    const descriptions = cases.map(shapesCase => {
        const { name, description, type, fields } = shapesCase;
        const file = `shapes/${name}.bin`;
        tables.set(file, serialize(typescript, shapesCase));

        return { name, description, type, file, fields: toJson(fields) };
    });

    const json = formatSuiteJson({
        schema: 'schemas/shapes.tafla',
        cases: descriptions
    });
    return { json, tables };
}

/**
 * Returns fields as JSON holds them: 64-bit integers as decimal strings, and
 * the floats that JSON has no number for by name, as in `cases.json`
 */
function toJson(value: unknown): unknown {
    if (typeof value === 'bigint') {
        return String(value);
    }

    if (typeof value === 'number') {
        return toJsonNumber(value);
    }

    if (Array.isArray(value)) {
        return value.map(toJson);
    }

    if (typeof value === 'object' && value !== null) {
        const entries = Object.entries(value).map(([key, item]) => [
            key,
            toJson(item)
        ]);
        return Object.fromEntries(entries);
    }

    return value;
}
