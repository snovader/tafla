import { fc } from 'hypotes/fast-check';
import { SCALAR_VALUES, STRINGS } from '../conformance/arbitraries.ts';
import type {
    FieldDescription,
    ScalarType,
    TableDescription
} from '../conformance/description.ts';
import type { AllTypes } from './all-types-tables.ts';
import type * as javascript from './all_types/javascript/all-types.js';
import type * as typescript from './all_types/typescript/all-types.ts';
import type {
    EnumsFields,
    MarkFields,
    StepFields
} from './all_types/typescript/all-types.ts';
import {
    array,
    message,
    scalar,
    string,
    struct,
    structArray,
    vector,
    DEFAULT_LAYOUT
} from './descriptions.ts';
import { readItems } from './views.ts';

// The message of enums that the code generated from schemas/all_types.tafla
// writes, how tests read it back and describe it to build its table by hand,
// of the integer types that store its enums, and arbitrary values of its
// fields.

type Enums = typescript.Enums | javascript.Enums;
type Mark = typescript.Mark | javascript.Mark;
type Step = typescript.Step | javascript.Step;

/**
 * The fields of `Enums` that hold one enum and an array of it, with the
 * integer type that stores the enum, in the order declared
 */
const ENUM_FIELDS = [
    ['level', 'levels', 'uint8'],
    ['direction', 'directions', 'int8'],
    ['unit', 'units', 'uint16'],
    ['change', 'changes', 'int16'],
    ['status', 'statuses', 'uint32'],
    ['sign', 'signs', 'int32'],
    ['mask', 'masks', 'uint64'],
    ['extent', 'extents', 'int64']
] as const;

/** The integer types that store the scalars of a `Mark`, in order */
const MARK_TYPES: ScalarType[] = [
    'uint8',
    'int16',
    'float32',
    'int64',
    'int8',
    'uint64'
];

/** The byte length of a `Mark`: its scalars' sizes in all */
export const MARK_SIZE = 24;

/**
 * The enums of all_types.tafla, with the values that the schema names, by
 * their names in PascalCase, and the integer type that stores them
 */
export const ENUM_VALUES = [
    ['Level', 'uint8', { Low: 0, Medium: 1, High: 2 }],
    [
        'Direction',
        'int8',
        { Backward: -1, None: 0, Forward: 1, FurthestBack: -128 }
    ],
    ['Unit', 'uint16', { Last: 65_535, First: 0, V2: 2 }],
    ['Change', 'int16', { Least: -32_768, Greatest: 32_767 }],
    ['Status', 'uint32', { Ok: 0, NotFound: 404, Greatest: 4_294_967_295 }],
    ['Sign', 'int32', { Least: -2_147_483_648, Greatest: 2_147_483_647 }],
    [
        'Mask',
        'uint64',
        {
            None: 0n,
            One: 1n,
            BeyondFloat64: 9_007_199_254_740_993n,
            All: 2n ** 64n - 1n
        }
    ],
    [
        'Extent',
        'int64',
        { Least: -(2n ** 63n), Zero: 0n, Greatest: 2n ** 63n - 1n }
    ],
    [
        'Word',
        'uint8',
        {
            Class: 0,
            Default: 1,
            In: 2,
            Match: 3,
            Self: 4,
            Type: 5,
            True: 6,
            Null: 7,
            None: 8,
            Init: 9,
            New: 10,
            Name: 11,
            Value: 12
        }
    ]
] as const satisfies ReadonlyArray<
    readonly [keyof AllTypes, ScalarType, Record<string, number | bigint>]
>;

// Reading every field, with arrays copied to plain arrays to compare with the
// fields that were written

export function readEnums(enums: Enums): EnumsFields {
    return {
        level: enums.level(),
        levels: Array.from(enums.levels()),
        direction: enums.direction(),
        directions: Array.from(enums.directions()),
        unit: enums.unit(),
        units: Array.from(enums.units()),
        change: enums.change(),
        changes: Array.from(enums.changes()),
        status: enums.status(),
        statuses: Array.from(enums.statuses()),
        sign: enums.sign(),
        signs: Array.from(enums.signs()),
        mask: enums.mask(),
        masks: Array.from(enums.masks()),
        extent: enums.extent(),
        extents: Array.from(enums.extents()),
        word: enums.word(),
        mark: readMark(enums.mark()),
        marks: readItems(enums.marks(), readMark),
        steps: readItems(enums.steps(), readStep)
    };
}

export function readMark(mark: Mark): MarkFields {
    const turn = mark.turn();

    return {
        level: mark.level(),
        change: mark.change(),
        weight: mark.weight(),
        extent: mark.extent(),
        turn: { direction: turn.direction(), mask: turn.mask() }
    };
}

function readStep(step: Step): StepFields {
    return {
        level: step.level(),
        mark: readMark(step.mark()),
        name: step.name()
    };
}

// The table that the generated code writes, in the conformance suite's
// model, where an enum is the integer type that stores it

export function describeEnums(fields: EnumsFields): TableDescription {
    const enums = ENUM_FIELDS.flatMap(([one, many, type]) => [
        scalar(type, fields[one]),
        array(type, fields[many] as ArrayLike<number> | ArrayLike<bigint>)
    ]);

    return message(DEFAULT_LAYOUT, [
        ...enums,
        scalar('uint8', fields.word),
        describeMark(fields.mark),
        structArray(MARK_TYPES, fields.marks.map(markValues)),
        vector(fields.steps.map(describeStep))
    ]);
}

function describeStep({ level, mark, name }: StepFields): FieldDescription[] {
    return [scalar('uint8', level), describeMark(mark), string(name)];
}

function describeMark(mark: MarkFields): FieldDescription {
    return struct(
        markValues(mark).map((value, index) => [MARK_TYPES[index], value])
    );
}

function markValues({
    level,
    change,
    weight,
    extent,
    turn
}: MarkFields): Array<number | bigint> {
    return [level, change, weight, extent, turn.direction, turn.mask];
}

// Arbitrary values of the fields: any integer of the type that stores each
// enum, most of which the enum doesn't name

const MAX_LENGTH = 8;

function arrays<T>(values: fc.Arbitrary<T>): fc.Arbitrary<T[]> {
    return fc.array(values, { maxLength: MAX_LENGTH });
}

export const MARK_FIELDS: fc.Arbitrary<MarkFields> = fc.record({
    level: SCALAR_VALUES.uint8,
    change: SCALAR_VALUES.int16,
    weight: SCALAR_VALUES.float32,
    extent: SCALAR_VALUES.int64,
    turn: fc.record({
        direction: SCALAR_VALUES.int8,
        mask: SCALAR_VALUES.uint64
    })
});

export const ENUMS_FIELDS: fc.Arbitrary<EnumsFields> = fc.record({
    level: SCALAR_VALUES.uint8,
    levels: arrays(SCALAR_VALUES.uint8),
    direction: SCALAR_VALUES.int8,
    directions: arrays(SCALAR_VALUES.int8),
    unit: SCALAR_VALUES.uint16,
    units: arrays(SCALAR_VALUES.uint16),
    change: SCALAR_VALUES.int16,
    changes: arrays(SCALAR_VALUES.int16),
    status: SCALAR_VALUES.uint32,
    statuses: arrays(SCALAR_VALUES.uint32),
    sign: SCALAR_VALUES.int32,
    signs: arrays(SCALAR_VALUES.int32),
    mask: SCALAR_VALUES.uint64,
    masks: arrays(SCALAR_VALUES.uint64),
    extent: SCALAR_VALUES.int64,
    extents: arrays(SCALAR_VALUES.int64),
    word: SCALAR_VALUES.uint8,
    mark: MARK_FIELDS,
    marks: arrays(MARK_FIELDS),
    steps: arrays(
        fc.record({
            level: SCALAR_VALUES.uint8,
            mark: MARK_FIELDS,
            name: STRINGS
        })
    )
});
