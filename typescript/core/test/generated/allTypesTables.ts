import { fc } from '@fast-check/vitest';
import { SCALAR_VALUES, STRINGS } from '../conformance/arbitraries.ts';
import type {
    FieldDescription,
    ScalarType,
    TableDescription
} from '../conformance/description.ts';
import {
    array,
    bool,
    DEFAULT_LAYOUT,
    message,
    scalar,
    string,
    struct,
    structArray,
    table,
    vector
} from './descriptions.ts';
import { readItems, readItemsFromLast } from './views.ts';
import * as javascript from './all_types/javascript/allTypes.js';
import * as typescript from './all_types/typescript/allTypes.ts';
import type {
    ArraysFields,
    DocumentedFields,
    InnerFields,
    KeywordsFields,
    NestedFields,
    OuterFields,
    PackedFields,
    PathFields,
    PointFields,
    ReservedFields,
    ScalarsFields,
    SegmentFields,
    StructsFields,
    TreeFields
} from './all_types/typescript/allTypes.ts';

// The code generated from schemas/all_types.tafla, how tests read its
// messages back and describe them to build their tables by hand, and
// arbitrary values of their fields.

/**
 * The generated TypeScript or JavaScript. Tests call either through this
 * union, which type-checks only while the JavaScript's JSDoc types match the
 * TypeScript's.
 */
export type AllTypes = typeof typescript | typeof javascript;

export const ALL_TYPES: ReadonlyArray<readonly [string, AllTypes]> = [
    ['TypeScript', typescript],
    ['JavaScript', javascript]
];

type Scalars = typescript.Scalars | javascript.Scalars;
type Arrays = typescript.Arrays | javascript.Arrays;
type Nested = typescript.Nested | javascript.Nested;
type Outer = typescript.Outer | javascript.Outer;
type Inner = typescript.Inner | javascript.Inner;
type Tree = typescript.Tree | javascript.Tree;
type Reserved = typescript.Reserved | javascript.Reserved;
type Keywords = typescript.Keywords | javascript.Keywords;
type Packed = typescript.Packed | javascript.Packed;
type Documented = typescript.Documented | javascript.Documented;
type Structs = typescript.Structs | javascript.Structs;
type Path = typescript.Path | javascript.Path;
type Segment = typescript.Segment | javascript.Segment;
type Point = typescript.Point | javascript.Point;

/**
 * The scalar types in the order of the fields of `Arrays`, and of `Scalars`
 * and `Packed`, which end with a bool
 */
const SCALAR_TYPES = [
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
] as const satisfies readonly ScalarType[];

// Reading every field, with arrays copied to plain arrays to compare with the
// fields that were written

/** Reads the fields of `Scalars` or `Packed`, which have the same getters */
export function readScalars(scalars: Scalars | Packed): ScalarsFields {
    return {
        int8: scalars.int8(),
        uint8: scalars.uint8(),
        int16: scalars.int16(),
        uint16: scalars.uint16(),
        int32: scalars.int32(),
        uint32: scalars.uint32(),
        int64: scalars.int64(),
        uint64: scalars.uint64(),
        float32: scalars.float32(),
        float64: scalars.float64(),
        bool: scalars.bool()
    };
}

export function readArrays(arrays: Arrays): ArraysFields {
    return {
        int8: Array.from(arrays.int8()),
        uint8: Array.from(arrays.uint8()),
        int16: Array.from(arrays.int16()),
        uint16: Array.from(arrays.uint16()),
        int32: Array.from(arrays.int32()),
        uint32: Array.from(arrays.uint32()),
        int64: Array.from(arrays.int64()),
        uint64: Array.from(arrays.uint64()),
        float32: Array.from(arrays.float32()),
        float64: Array.from(arrays.float64())
    };
}

export function readNested(nested: Nested): NestedFields {
    return {
        label: nested.label(),
        outer: readOuter(nested.outer()),
        trees: readItems(nested.trees(), readTree)
    };
}

function readOuter(outer: Outer): OuterFields {
    return { count: outer.count(), inner: readInner(outer.inner()) };
}

function readInner(inner: Inner): InnerFields {
    return { name: inner.name(), values: Array.from(inner.values()) };
}

/**
 * Reads a tree and its children, each vector's from the first to the last.
 * Each item then starts where the one before it ends, which reading it
 * checks, as `getVectorOffsets()` does, so each tree lies further
 * into the bytes than its parent, and a walk ends within them.
 */
function readTree(tree: Tree): TreeFields {
    return {
        value: tree.value(),
        children: readItems(tree.children(), readTree)
    };
}

export function readReserved(reserved: Reserved): ReservedFields {
    return { keywords: readKeywords(reserved.keywords()) };
}

function readKeywords(keywords: Keywords): KeywordsFields {
    return {
        class: keywords.class(),
        default: keywords.default(),
        in: keywords.in(),
        match: keywords.match(),
        self: keywords.self(),
        type: keywords.type()
    };
}

export function readDocumented(documented: Documented): DocumentedFields {
    return {
        one: documented.one(),
        two: documented.two(),
        undocumented: documented.undocumented()
    };
}

export function readStructs(structs: Structs): StructsFields {
    return {
        packed: readScalars(structs.packed()),
        documented: readItems(structs.documented(), readDocumented),
        segment: readSegment(structs.segment()),
        segments: readItems(structs.segments(), readSegment),
        path: readPath(structs.path())
    };
}

function readPath(path: Path): PathFields {
    return {
        start: readPoint(path.start()),
        points: readItems(path.points(), readPoint)
    };
}

// Reading the vectors and arrays of structs from the last item to the first
// instead, as a reader that skips items would

/**
 * Reads a `Nested` of `byteLength` bytes from the last item of each vector to
 * the first, with trees nested as many levels deep as the bytes are long at
 * most.
 *
 * A view reads an item without the items before it, so corrupted ends can
 * give it a tree that holds itself, as SPEC.md's Validation describes, which
 * a walk would follow without end. A tree, read in place, lies at or after
 * its parent, and one at its parent's position reads as its parent, which
 * then holds itself. So a tree nested deeper than the bytes are long lies
 * below one that holds itself, and reads as a tree a level above it, which
 * the walk reads.
 */
export function readNestedFromLast(
    nested: Nested,
    byteLength: number
): NestedFields {
    return {
        label: nested.label(),
        outer: readOuter(nested.outer()),
        trees: readItemsFromLast(nested.trees(), (tree: Tree) =>
            readTreeFromLast(tree, byteLength)
        )
    };
}

/** Reads a tree and its children from the last to the first, `levels` levels deep */
function readTreeFromLast(tree: Tree, levels: number): TreeFields {
    const value = tree.value();
    const children =
        levels > 1
            ? readItemsFromLast(tree.children(), (child: Tree) =>
                  readTreeFromLast(child, levels - 1)
              )
            : [];

    return { value, children };
}

export function readStructsFromLast(structs: Structs): StructsFields {
    const path = structs.path();

    return {
        packed: readScalars(structs.packed()),
        documented: readItemsFromLast(structs.documented(), readDocumented),
        segment: readSegment(structs.segment()),
        segments: readItemsFromLast(structs.segments(), readSegment),
        path: {
            start: readPoint(path.start()),
            points: readItemsFromLast(path.points(), readPoint)
        }
    };
}

function readSegment(segment: Segment): SegmentFields {
    return { start: readPoint(segment.start()), end: readPoint(segment.end()) };
}

function readPoint(point: Point): PointFields {
    return { x: point.x(), y: point.y() };
}

// The tables that the generated code writes, in the conformance suite's
// model

export function describeScalars(fields: ScalarsFields): TableDescription {
    return message(DEFAULT_LAYOUT, [
        ...SCALAR_TYPES.map(type => scalar(type, fields[type])),
        bool(fields.bool)
    ]);
}

export function describeArrays(fields: ArraysFields): TableDescription {
    return message(
        DEFAULT_LAYOUT,
        SCALAR_TYPES.map(type => array(type, fields[type]))
    );
}

export function describeNested({
    label,
    outer,
    trees
}: NestedFields): TableDescription {
    const { count, inner } = outer;

    return message(DEFAULT_LAYOUT, [
        string(label),
        table([
            scalar('uint32', count),
            table([string(inner.name), array('float64', inner.values)])
        ]),
        vector(trees.map(describeTree))
    ]);
}

function describeTree({ value, children }: TreeFields): FieldDescription[] {
    return [scalar('int32', value), vector(children.map(describeTree))];
}

export function describeReserved({
    keywords
}: ReservedFields): TableDescription {
    return message(DEFAULT_LAYOUT, [
        table([
            scalar('uint8', keywords.class),
            scalar('uint8', keywords.default),
            scalar('uint8', keywords.in),
            scalar('uint8', keywords.match),
            scalar('uint8', keywords.self),
            string(keywords.type)
        ])
    ]);
}

/** The scalars of a `Segment`, those of its two points in their place */
const SEGMENT_TYPES: ScalarType[] = [
    'float32',
    'float32',
    'float32',
    'float32'
];

export function describeStructs({
    packed,
    documented,
    segment,
    segments,
    path
}: StructsFields): TableDescription {
    return message(DEFAULT_LAYOUT, [
        // A struct's bool is the uint8 that stores it
        struct([
            ...SCALAR_TYPES.map(type => [type, packed[type]] as const),
            ['uint8', packed.bool ? 1 : 0]
        ]),
        structArray(
            ['uint8', 'uint16', 'uint32'],
            documented.map(({ one, two, undocumented }) => [
                one,
                two,
                undocumented
            ])
        ),
        struct(
            segmentValues(segment).map((value, index) => [
                SEGMENT_TYPES[index],
                value
            ])
        ),
        structArray(SEGMENT_TYPES, segments.map(segmentValues)),
        table([
            struct([
                ['float32', path.start.x],
                ['float32', path.start.y]
            ]),
            structArray(
                ['float32', 'float32'],
                path.points.map(({ x, y }) => [x, y])
            )
        ])
    ]);
}

function segmentValues({ start, end }: SegmentFields): number[] {
    return [start.x, start.y, end.x, end.y];
}

/**
 * A `Packed` as its fields lie at fixed offsets, little-endian, written with
 * DataView rather than tafla
 */
export function packedBytes(fields: PackedFields): Uint8Array {
    const bytes = new Uint8Array(43);
    const view = new DataView(bytes.buffer);
    view.setInt8(0, fields.int8);
    view.setUint8(1, fields.uint8);
    view.setInt16(2, fields.int16, true);
    view.setUint16(4, fields.uint16, true);
    view.setInt32(6, fields.int32, true);
    view.setUint32(10, fields.uint32, true);
    view.setBigInt64(14, fields.int64, true);
    view.setBigUint64(22, fields.uint64, true);
    view.setFloat32(30, fields.float32, true);
    view.setFloat64(34, fields.float64, true);
    view.setUint8(42, fields.bool ? 1 : 0);

    return bytes;
}

/** A `Documented` as its fields lie at fixed offsets, little-endian */
export function documentedBytes(fields: DocumentedFields): Uint8Array {
    const bytes = new Uint8Array(7);
    const view = new DataView(bytes.buffer);
    view.setUint8(0, fields.one);
    view.setUint16(1, fields.two, true);
    view.setUint32(3, fields.undocumented, true);

    return bytes;
}

// Arbitrary values of the fields, over each type's full range, with -0, NaN
// and the infinities, strings outside the Basic Multilingual Plane, and empty
// arrays and vectors

const MAX_LENGTH = 8;
/** How deep trees nest below the vector of `Nested` */
const TREE_DEPTH = 3;

function arrays<T>(values: fc.Arbitrary<T>): fc.Arbitrary<T[]> {
    return fc.array(values, { maxLength: MAX_LENGTH });
}

/** A value of every scalar type and a bool, as `Scalars` and `Packed` hold them */
export const SCALARS_FIELDS: fc.Arbitrary<ScalarsFields> = fc.record({
    ...SCALAR_VALUES,
    bool: fc.boolean()
});

export const ARRAYS_FIELDS: fc.Arbitrary<ArraysFields> = fc.record({
    int8: arrays(SCALAR_VALUES.int8),
    uint8: arrays(SCALAR_VALUES.uint8),
    int16: arrays(SCALAR_VALUES.int16),
    uint16: arrays(SCALAR_VALUES.uint16),
    int32: arrays(SCALAR_VALUES.int32),
    uint32: arrays(SCALAR_VALUES.uint32),
    int64: arrays(SCALAR_VALUES.int64),
    uint64: arrays(SCALAR_VALUES.uint64),
    float32: arrays(SCALAR_VALUES.float32),
    float64: arrays(SCALAR_VALUES.float64)
});

/** Trees of vectors of trees, which are empty at the deepest level */
const TREES: fc.Arbitrary<TreeFields> = fc.letrec<{ tree: TreeFields }>(
    tie => ({
        tree: fc.record({
            value: SCALAR_VALUES.int32,
            children: fc.oneof(
                // At the maximum depth, only the first arbitrary is drawn
                { maxDepth: TREE_DEPTH },
                fc.constant([]),
                fc.array(tie('tree'), { maxLength: 3 })
            )
        })
    })
).tree;

export const NESTED_FIELDS: fc.Arbitrary<NestedFields> = fc.record({
    label: STRINGS,
    outer: fc.record({
        count: SCALAR_VALUES.uint32,
        inner: fc.record({
            name: STRINGS,
            values: arrays(SCALAR_VALUES.float64)
        })
    }),
    trees: arrays(TREES)
});

export const RESERVED_FIELDS: fc.Arbitrary<ReservedFields> = fc.record({
    keywords: fc.record({
        class: SCALAR_VALUES.uint8,
        default: SCALAR_VALUES.uint8,
        in: SCALAR_VALUES.uint8,
        match: SCALAR_VALUES.uint8,
        self: SCALAR_VALUES.uint8,
        type: STRINGS
    })
});

export const DOCUMENTED_FIELDS: fc.Arbitrary<DocumentedFields> = fc.record({
    one: SCALAR_VALUES.uint8,
    two: SCALAR_VALUES.uint16,
    undocumented: SCALAR_VALUES.uint32
});

const POINTS: fc.Arbitrary<PointFields> = fc.record({
    x: SCALAR_VALUES.float32,
    y: SCALAR_VALUES.float32
});

const SEGMENTS: fc.Arbitrary<SegmentFields> = fc.record({
    start: POINTS,
    end: POINTS
});

export const STRUCTS_FIELDS: fc.Arbitrary<StructsFields> = fc.record({
    packed: SCALARS_FIELDS,
    documented: arrays(DOCUMENTED_FIELDS),
    segment: SEGMENTS,
    segments: arrays(SEGMENTS),
    path: fc.record({ start: POINTS, points: arrays(POINTS) })
});
