import { TableReader } from 'tafla';
import {
    toJsonNumber,
    type Alignment,
    type Corruption,
    type EquivalentCase,
    type FieldDescription,
    type InvalidCase,
    type JsonNumber,
    type LayoutDescription,
    type NestedTable,
    type OffsetSize,
    type OffsetType,
    type ScalarType,
    type Suite,
    type TableDescription,
    type TableField,
    type ValidCase
} from './description.ts';
import { buildTable, layoutOf } from './tables.ts';

// The cases of the conformance suite, which `generate.ts` writes to
// `cases.json` and a file of bytes each. README.md explains them.

const DEFAULT = layout();

// The extremes of each integer type, and a value whose bytes all differ, so
// that their order shows
const INTEGERS: ReadonlyArray<{
    type: ScalarType;
    min: number | bigint;
    max: number | bigint;
    pattern: number | bigint;
}> = [
    { type: 'int8', min: -0x80, max: 0x7f, pattern: 0x12 },
    { type: 'uint8', min: 0, max: 0xff, pattern: 0xfe },
    { type: 'int16', min: -0x8000, max: 0x7fff, pattern: 0x1234 },
    { type: 'uint16', min: 0, max: 0xffff, pattern: 0xfedc },
    {
        type: 'int32',
        min: -0x8000_0000,
        max: 0x7fff_ffff,
        pattern: 0x1234_5678
    },
    { type: 'uint32', min: 0, max: 0xffff_ffff, pattern: 0xfedc_ba98 },
    {
        type: 'int64',
        min: -(2n ** 63n),
        max: 2n ** 63n - 1n,
        pattern: 0x1234_5678_9abc_def0n
    },
    {
        type: 'uint64',
        min: 0n,
        max: 2n ** 64n - 1n,
        pattern: 0xfedc_ba98_7654_3210n
    }
];

// A float that rounds, the zeros' signs, the infinities, NaN, the largest
// finite value and the smallest subnormal one. NaN is the quiet NaN that
// JavaScript writes: 0x7fc00000 as a float32 and 0x7ff8000000000000 as a
// float64.
const FLOAT32S = [
    1.5,
    0.1,
    -0,
    Number.POSITIVE_INFINITY,
    Number.NEGATIVE_INFINITY,
    Number.NaN,
    2 ** 128 - 2 ** 104,
    2 ** -149
];
const FLOAT64S = [
    1.5,
    0.1,
    -0,
    Number.POSITIVE_INFINITY,
    Number.NEGATIVE_INFINITY,
    Number.NaN,
    Number.MAX_VALUE,
    Number.MIN_VALUE
];

const ELEMENT_TYPES: readonly ScalarType[] = [
    ...INTEGERS.map(({ type }) => type),
    'float32',
    'float64'
];

// The largest version that the header slot of each offset type holds
const MAX_VERSIONS = {
    uint8: 0xff,
    uint16: 0xffff,
    uint32: 0xffff_ffff
} as const;

const STRINGS = [
    '',
    'a',
    'Göteborg',
    '東京',
    '🌍',
    '\uFEFFStockholm',
    '\uFEFF',
    'a\u0000b',
    'Zürich, 東京 and 🌍'
];

const EMPTY = table([]);

/** A table nested in others, whose float64[] aligns it to 8 bytes */
const INNER = table([
    scalar('int16', -2),
    array('float64', [0.5, -0.25]),
    string('inner')
]);

/** The rows of the vectors, whose float64[]s are aligned within each row */
const ROWS = [
    table([scalar('uint32', 1), array('float64', [0.5]), string('one')]),
    table([scalar('uint32', 2), array('float64', []), string('')]),
    table([
        scalar('uint32', 3),
        array('float64', [-1.5, 2.25, 1e300]),
        string('three')
    ])
];

const VALID_CASES: ValidCase[] = [
    validCase(
        'integer-scalars',
        'The minimum, maximum and a value of distinct bytes of every integer type',
        table(
            INTEGERS.flatMap(({ type, min, max, pattern }) => [
                scalar(type, min),
                scalar(type, max),
                scalar(type, pattern)
            ])
        )
    ),
    validCase(
        'float-scalars',
        "For each float type: a value that's exact, one that rounds, -0, both infinities, NaN, the largest finite value and the smallest subnormal one",
        table([
            ...FLOAT32S.map(value => scalar('float32', value)),
            ...FLOAT64S.map(value => scalar('float64', value))
        ])
    ),
    validCase(
        'bools',
        'false and true, which writers write as the bytes 0 and 1, and two more true bools, whose bytes bool-nonzero changes',
        table([bool(false), bool(true), bool(true), bool(true)])
    ),
    validCase(
        'integer-arrays',
        'An array of every integer type, of its extremes, -1, 0, 1 and a value of distinct bytes',
        table(
            INTEGERS.map(({ type, min, max, pattern }) =>
                array(
                    type,
                    min === 0 || min === 0n
                        ? [min, 1, pattern, max]
                        : [min, -1, 0, 1, pattern, max]
                )
            )
        )
    ),
    validCase(
        'float-arrays',
        'Arrays of the float values of float-scalars and 0',
        table([
            array('float32', [0, ...FLOAT32S]),
            array('float64', [0, ...FLOAT64S])
        ])
    ),
    validCase(
        'empty-arrays',
        'An empty array of every element type. Their lengths are padded as they would be for elements, so the int64[] and uint64[] each follow 4 bytes of padding.',
        table(ELEMENT_TYPES.map(type => array(type, [])))
    ),
    validCase(
        'padding',
        'A uint8[] of 0 to 7 bytes before each of eight float64[], of 0 to 3 bytes before each of four int32[] and of 0 to 1 bytes before each of two uint16[], so that the arrays are padded by every amount that they can be',
        table(paddingFields())
    ),
    ...layoutCases(),
    validCase(
        'offset-type-uint8-full',
        'The longest table of uint8 offsets, 255 bytes: its size, the length 250 of its uint8[] and the offset 254 of its last field are only read correctly unsigned',
        table(
            [
                array(
                    'uint8',
                    Array.from({ length: 250 }, (_, index) => index)
                ),
                scalar('uint8', 0xff)
            ],
            layout({ sized: true, offsetType: 'uint8' })
        )
    ),
    validCase(
        'strings',
        'Strings: empty, ASCII, with 2-, 3- and 4-byte UTF-8 sequences, with a leading U+FEFF, which is part of the string rather than a byte order mark, U+FEFF alone, with a NUL inside and mixing every sequence length',
        table(STRINGS.map(string))
    ),
    validCase(
        'structs',
        'Struct fields, which are unpadded like scalars: of one uint8 after a uint8; of every scalar type, each a value of distinct bytes, so that the order of the scalars and of their bytes shows; and of a float32, a float64 and an int16 of -0, NaN and -2',
        table([
            scalar('uint8', 1),
            struct([['uint8', 0xfe]]),
            struct(INTEGERS.map(({ type, pattern }) => [type, pattern])),
            struct([
                ['float32', -0],
                ['float64', Number.NaN],
                ['int16', -2]
            ])
        ])
    ),
    validCase(
        'struct-arrays',
        "Arrays of structs, each padded so that its structs start at a multiple of their largest scalar's size: an empty array of structs of a uint8 and a float64, padded to 8; after a uint8 each, arrays of structs of two uint8s, aligned to 1; of a uint16 and a uint8, of 3 bytes aligned to 2; of three float32s, aligned to 4, with -0 and NaN; and of a uint8 and a float64, of 9 bytes aligned to 8, whose float64s aren't aligned in the structs after the first; then an array of a struct of every integer type",
        table([
            structArray(['uint8', 'float64'], []),
            scalar('uint8', 1),
            structArray(
                ['uint8', 'uint8'],
                [
                    [1, 2],
                    [3, 4]
                ]
            ),
            scalar('uint8', 2),
            structArray(
                ['uint16', 'uint8'],
                [
                    [0xfedc, 1],
                    [2, 0xff]
                ]
            ),
            scalar('uint8', 3),
            structArray(
                ['float32', 'float32', 'float32'],
                [
                    [1.5, -0, Number.NaN],
                    [0.1, 2, Number.NEGATIVE_INFINITY]
                ]
            ),
            scalar('uint8', 4),
            structArray(
                ['uint8', 'float64'],
                [
                    [1, 1.5],
                    [2, -0.25],
                    [3, 1e300]
                ]
            ),
            structArray(
                INTEGERS.map(({ type }) => type),
                [
                    INTEGERS.map(({ min }) => min),
                    INTEGERS.map(({ max }) => max),
                    INTEGERS.map(({ pattern }) => pattern)
                ]
            )
        ])
    ),
    validCase(
        'nested-tables',
        'The same table nested from its builder, padded to 8 for its float64[]; from its bytes, padded to the default of 4 and then to 8; and from its builder with an alignment of 1, unpadded. Then an empty table from its builder, unpadded, and from its bytes, padded to 4.',
        table([
            scalar('uint8', 1),
            nested(fromBuilder(INNER)),
            scalar('uint8', 2),
            nested(fromBytes(INNER)),
            scalar('uint8', 3),
            nested(fromBytes(INNER), 8),
            scalar('uint8', 4),
            nested(fromBuilder(INNER), 1),
            nested(fromBuilder(EMPTY)),
            nested(fromBytes(EMPTY))
        ])
    ),
    validCase(
        'nested-two-deep',
        "A table nested in a table nested in a sized and versioned table, from the middle table's builder and from its bytes. The int64[] at the bottom pads both nested tables to 8.",
        nestedTwoDeep()
    ),
    validCase(
        'nested-narrower-offsets',
        'Tables of uint8 and uint16 offsets nested in a table of uint32 offsets, from their builders, which are written with their own offsets, and from bytes. Each is read from its position with its own offset type.',
        table([
            scalar('uint8', 1),
            nested(
                fromBuilder(
                    table(
                        [
                            array('uint8', [1, 2, 3]),
                            string('one'),
                            array('int16', [-1])
                        ],
                        layout({ offsetType: 'uint8' })
                    )
                )
            ),
            nested(
                fromBuilder(
                    table(
                        [array('int32', [-1]), string('two')],
                        layout({ offsetType: 'uint16' })
                    )
                )
            ),
            nested(
                fromBytes(
                    table([string('bytes')], layout({ offsetType: 'uint8' }))
                )
            )
        ])
    ),
    validCase(
        'nested-wider-offsets',
        'Tables of uint32 and uint16 offsets nested from their builders in a table of uint8 offsets. Each is read from its position with its own offset type.',
        table(
            [
                scalar('uint8', 1),
                nested(
                    fromBuilder(
                        table([scalar('uint32', 0xfedc_ba98), string('four')])
                    )
                ),
                nested(
                    fromBuilder(
                        table(
                            [array('uint16', [0xfedc]), string('two')],
                            layout({ offsetType: 'uint16' })
                        )
                    )
                )
            ],
            layout({ offsetType: 'uint8' })
        )
    ),
    validCase(
        'vectors',
        "Vectors of rows from their builders and of the same rows from their bytes, after the end of each row, whose float64[]s are aligned within each row, but mostly not in the table, since items aren't padded; an empty vector, which is its count alone; and a vector of an empty table from its builder and from its bytes, whose ends are equal",
        table([
            scalar('uint8', 1),
            vector(ROWS.map(fromBuilder)),
            vector(ROWS.map(fromBytes)),
            vector([]),
            vector([fromBuilder(EMPTY), fromBytes(EMPTY)])
        ])
    ),
    validCase(
        'vector-item-layouts',
        "A vector of a table read in place and then tables of other layouts, which are read from their items' bytes: of uint16 offsets, which its builder writes and the vector copies; with a size and a version, written straight into the vector; and of uint8 offsets with both, from bytes",
        table([
            vector([
                fromBuilder(table([scalar('uint8', 1)])),
                fromBuilder(
                    table(
                        [scalar('uint8', 2), string('two')],
                        layout({ offsetType: 'uint16' })
                    )
                ),
                fromBuilder(
                    table(
                        [scalar('uint8', 3)],
                        layout({ sized: true, versioned: true }),
                        3
                    )
                ),
                fromBytes(
                    table(
                        [scalar('uint8', 4)],
                        layout({
                            sized: true,
                            versioned: true,
                            offsetType: 'uint8'
                        }),
                        4
                    )
                )
            ])
        ])
    ),
    validCase('empty-table', 'A table of no fields, which has no bytes', EMPTY),
    validCase(
        'empty-table-sized-versioned',
        'A table of no fields with uint16 size and version slots',
        table(
            [],
            layout({ sized: true, versioned: true, offsetType: 'uint16' }),
            0x0102
        )
    )
];

export const SUITE: Suite = {
    valid: VALID_CASES,
    equivalent: equivalentCases(),
    invalid: invalidCases()
};

function equivalentCases(): EquivalentCase[] {
    const bools = inspect('bools');

    return [
        equivalentCase(
            'bool-nonzero',
            'The bytes of the last two bools of bools are 2 and 0x80, which read as true, like 1',
            bools,
            overwrite(bools.buffer.rootOffset(2), [0x02, 0x80])
        )
    ];
}

function invalidCases(): InvalidCase[] {
    const integerScalars = inspect('integer-scalars');
    const integerArrays = inspect('integer-arrays');
    const floatArrays = inspect('float-arrays');
    const strings = inspect('strings');
    const full = inspect('offset-type-uint8-full');
    const uint16Offsets = inspect('offset-type-uint16');
    const nestedTables = inspect('nested-tables');
    const vectors = inspect('vectors');
    const itemLayouts = inspect('vector-item-layouts');
    const structs = inspect('structs');
    const structArrays = inspect('struct-arrays');

    // The vector of vector-item-layouts holds four items
    const itemsVector = itemLayouts.buffer.rootOffset(0);
    const lastEnd = itemsVector + 4 * 4;

    return [
        invalidCase(
            'truncated-offsets',
            'Cut inside the offset of the first field',
            integerScalars,
            truncate(2),
            0
        ),
        invalidCase(
            'truncated-array',
            'Cut inside the last element of the last array',
            integerArrays,
            truncate(integerArrays.bytes.length - 1),
            7
        ),
        invalidCase(
            'truncated-string',
            'Cut inside the last character of the last string',
            strings,
            truncate(strings.bytes.length - 1),
            8
        ),
        invalidCase(
            'truncated-offset-type-uint8',
            'offset-type-uint8-full without its last byte, its last field',
            full,
            truncate(full.bytes.length - 1),
            1
        ),
        invalidCase(
            'offset-past-end',
            'The offset of the first field, an int8, points at the end of the table',
            integerScalars,
            overwrite(0, littleEndian(integerScalars.bytes.length, 4)),
            0
        ),
        invalidCase(
            'array-length-past-end',
            'The length of the last array counts one element more than the table holds',
            floatArrays,
            lengthenByOne(floatArrays, floatArrays.buffer.rootOffset(1)),
            1
        ),
        invalidCase(
            'array-length-max',
            'The length of the first array is 2^32 - 1, which a reader must not allocate',
            integerArrays,
            overwrite(
                integerArrays.buffer.rootOffset(0),
                littleEndian(0xffff_ffff, 4)
            ),
            0
        ),
        invalidCase(
            'array-length-max-offset-type-uint16',
            'The uint16 length of the float64[] of offset-type-uint16 is 2^16 - 1',
            uint16Offsets,
            overwrite(
                uint16Offsets.buffer.rootOffset(3),
                littleEndian(0xffff, 2)
            ),
            3
        ),
        invalidCase(
            'string-length-past-end',
            'The length of the last string counts one byte more than the table holds',
            strings,
            lengthenByOne(strings, strings.buffer.rootOffset(8)),
            8
        ),
        invalidCase(
            'invalid-utf8-continuation',
            'A string starts with a continuation byte, 0x80',
            strings,
            overwrite(strings.buffer.rootOffset(2) + 4, [0x80]),
            2
        ),
        invalidCase(
            'invalid-utf8-overlong',
            'A string has an overlong encoding of "/", 0xc0 0xaf',
            strings,
            overwrite(strings.buffer.rootOffset(8) + 5, [0xc0, 0xaf]),
            8
        ),
        invalidCase(
            'invalid-utf8-surrogate',
            'A string has the encoded surrogate U+D800, 0xed 0xa0 0x80',
            strings,
            overwrite(strings.buffer.rootOffset(3) + 4, [0xed, 0xa0, 0x80]),
            3
        ),
        invalidCase(
            'vector-count-past-end',
            'The item count of a vector is 2^32 - 1',
            vectors,
            overwrite(
                vectors.buffer.rootOffset(1),
                littleEndian(0xffff_ffff, 4)
            ),
            1
        ),
        invalidCase(
            'vector-item-past-end',
            'The end of the last item of the vector is one byte past the end of the table',
            itemLayouts,
            lengthenByOne(itemLayouts, lastEnd),
            0
        ),
        invalidCase(
            'vector-item-ends-before-start',
            'The end of the first item of the vector is one byte before its start, which is right after the four ends',
            itemLayouts,
            overwrite(itemsVector + 4, littleEndian(5 * 4 - 1, 4)),
            0
        ),
        invalidCase(
            'struct-past-end',
            'Cut inside the last scalar of the last struct',
            structs,
            truncate(structs.bytes.length - 1),
            3
        ),
        invalidCase(
            'struct-array-count-past-end',
            'The count of the last array of structs counts one struct more than the table holds',
            structArrays,
            lengthenByOne(structArrays, structArrays.buffer.rootOffset(9)),
            9
        ),
        invalidCase(
            'struct-array-count-max',
            'The count of the array of structs of three float32s is 2^32 - 1, which a reader must not allocate for',
            structArrays,
            overwrite(
                structArrays.buffer.rootOffset(6),
                littleEndian(0xffff_ffff, 4)
            ),
            6
        ),
        invalidCase(
            'nested-offset-past-end',
            "The offset of the string of the first nested table is 2^32 - 1, past the end even before it's added to the nested table's position",
            nestedTables,
            overwrite(
                nestedTables.buffer.rootOffset(1) + 2 * 4,
                littleEndian(0xffff_ffff, 4)
            ),
            1
        )
    ];
}

/** A valid case's bytes, to find where to corrupt them */
interface Inspected {
    name: string;
    bytes: Uint8Array;
    buffer: TableReader;
}

function inspect(name: string): Inspected {
    const { table } = VALID_CASES.find(validCase => validCase.name === name)!;
    const bytes = buildTable(table).bytes();

    return {
        name,
        bytes,
        buffer: new TableReader(bytes, layoutOf(table.layout))
    };
}

function equivalentCase(
    name: string,
    description: string,
    { name: validName }: Inspected,
    corruption: Corruption
): EquivalentCase {
    return {
        name,
        description,
        file: `equivalent/${name}.bin`,
        case: validName,
        corruption
    };
}

function invalidCase(
    name: string,
    description: string,
    { name: validName }: Inspected,
    corruption: Corruption,
    field: number
): InvalidCase {
    return {
        name,
        description,
        file: `invalid/${name}.bin`,
        case: validName,
        corruption,
        field
    };
}

/** Counts one more element or byte in the uint32 length at `position` */
function lengthenByOne({ buffer }: Inspected, position: number): Corruption {
    return overwrite(position, littleEndian(buffer.getUint32(position) + 1, 4));
}

function truncate(length: number): Corruption {
    return { kind: 'truncate', length };
}

function overwrite(at: number, bytes: number[]): Corruption {
    return { kind: 'overwrite', at, bytes };
}

function littleEndian(value: number, size: OffsetSize): number[] {
    return Array.from(
        { length: size },
        (_, index) => Math.floor(value / 256 ** index) % 256
    );
}

function validCase(
    name: string,
    description: string,
    table: TableDescription
): ValidCase {
    return { name, description, file: `valid/${name}.bin`, table };
}

/** Every combination of offset type and header, with the same fields */
function layoutCases(): ValidCase[] {
    const headers = [
        { suffix: '', sized: false, versioned: false },
        { suffix: '-sized', sized: true, versioned: false },
        { suffix: '-versioned', sized: false, versioned: true },
        { suffix: '-sized-versioned', sized: true, versioned: true }
    ];

    const offsetTypes: readonly OffsetType[] = ['uint8', 'uint16', 'uint32'];

    return offsetTypes.flatMap(offsetType =>
        headers.map(({ suffix, sized, versioned }) =>
            validCase(
                `offset-type-${offsetType}${suffix}`,
                describeLayout(offsetType, sized, versioned),
                table(
                    layoutFields(offsetType),
                    layout({ sized, versioned, offsetType }),
                    versioned ? MAX_VERSIONS[offsetType] : undefined
                )
            )
        )
    );
}

function describeLayout(
    offsetType: OffsetType,
    sized: boolean,
    versioned: boolean
): string {
    const slots = [
        ...(sized ? ['a size slot'] : []),
        ...(versioned
            ? [
                  `a version slot of ${MAX_VERSIONS[offsetType]}, the largest that it holds`
              ]
            : [])
    ];
    const header = slots.length > 0 ? slots.join(' and ') : 'no header';

    return `${offsetType} offsets, lengths, counts and item ends after ${header}, with a field of each kind: a scalar, arrays, a string, a struct, an array of structs, and a nested table and a vector of tables of the same offsets`;
}

function layoutFields(offsetType: OffsetType): FieldDescription[] {
    const nestedLayout = layout({ offsetType });

    return [
        scalar('uint8', 7),
        array('int16', [-2, 3]),
        string('abc'),
        array('float64', [1.5]),
        struct([
            ['uint16', 0x0102],
            ['int8', -3]
        ]),
        structArray(
            ['int8', 'uint32'],
            [
                [-1, 0xfedc_ba98],
                [2, 3]
            ]
        ),
        nested(
            fromBuilder(
                table(
                    [scalar('uint16', 0x0201), array('int32', [-1])],
                    nestedLayout
                )
            )
        ),
        vector([
            fromBuilder(table([scalar('uint8', 1)], nestedLayout)),
            fromBytes(table([string('two')], nestedLayout))
        ])
    ];
}

/**
 * Pairs of a uint8[] of each length up to the alignment of an array and that
 * array. Each group starts aligned to its arrays, after the larger arrays of
 * the previous group.
 */
function paddingFields(): FieldDescription[] {
    const groups = [
        ['float64', 8],
        ['int32', 4],
        ['uint16', 2]
    ] as const;

    return groups.flatMap(([type, alignment]) =>
        Array.from({ length: alignment }, (_, length) => [
            array(
                'uint8',
                Array.from({ length }, (_, index) => index + 1)
            ),
            array(type, [length])
        ]).flat()
    );
}

function nestedTwoDeep(): TableDescription {
    const deepest = table([
        array('int64', [-1n, 2n ** 40n]),
        string('deepest')
    ]);
    const middle = table([scalar('uint32', 7), nested(fromBuilder(deepest))]);

    return table(
        [
            string('outer'),
            nested(fromBuilder(middle)),
            nested(fromBytes(middle), 8),
            scalar('float32', 1.5)
        ],
        layout({ sized: true, versioned: true }),
        1
    );
}

function layout(options: Partial<LayoutDescription> = {}): LayoutDescription {
    return { sized: false, versioned: false, offsetType: 'uint32', ...options };
}

function table(
    fields: FieldDescription[],
    tableLayout: LayoutDescription = DEFAULT,
    version?: number
): TableDescription {
    return version === undefined
        ? { layout: tableLayout, fields }
        : { layout: tableLayout, version, fields };
}

function scalar(type: ScalarType, value: number | bigint): FieldDescription {
    return { type, value: toJson(type, value) };
}

function bool(value: boolean): FieldDescription {
    return { type: 'bool', value };
}

function array(
    type: ScalarType,
    values: ReadonlyArray<number | bigint>
): FieldDescription {
    return {
        type: `${type}[]`,
        value: values.map(value => toJson(type, value))
    };
}

/** Returns a value as JSON holds it, with a float32 as it reads back */
function toJson(type: ScalarType, value: number | bigint): JsonNumber {
    switch (type) {
        case 'int64':
        case 'uint64':
            return String(value);

        case 'float32':
            return toJsonNumber(Math.fround(Number(value)));

        default:
            return toJsonNumber(Number(value));
    }
}

/** A struct of scalars of these types and values */
function struct(
    scalars: ReadonlyArray<readonly [ScalarType, number | bigint]>
): FieldDescription {
    return {
        type: 'struct',
        fields: scalars.map(([type, value]) => ({
            type,
            value: toJson(type, value)
        }))
    };
}

/** An array of structs of scalars of these types, with each struct's values */
function structArray(
    types: ScalarType[],
    items: ReadonlyArray<ReadonlyArray<number | bigint>>
): FieldDescription {
    return {
        type: 'struct[]',
        fields: types,
        items: items.map(values =>
            values.map((value, index) => toJson(types[index], value))
        )
    };
}

function string(value: string): FieldDescription {
    return { type: 'string', value };
}

function nested(
    { from, table }: NestedTable,
    alignment?: Alignment
): TableField {
    return alignment === undefined
        ? { type: 'table', from, table }
        : { type: 'table', from, alignment, table };
}

function vector(items: NestedTable[]): FieldDescription {
    return { type: 'vector', items };
}

function fromBuilder(table: TableDescription): NestedTable {
    return { from: 'builder', table };
}

function fromBytes(table: TableDescription): NestedTable {
    return { from: 'bytes', table };
}
