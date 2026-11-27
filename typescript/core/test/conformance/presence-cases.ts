import type {
    Corruption,
    FieldDescription,
    InvalidCase,
    LayoutDescription,
    NestedTable,
    OffsetSize,
    Suite,
    TableDescription,
    ValidCase
} from './description.ts';
import { headerSizeOf, OFFSET_SIZES } from './format.ts';

// The cases of presence: tables with absent fields, whose offsets are 0,
// and tables read by a later revision of their format, which knows fields
// after theirs. `generate.ts` writes them to `presence/` in the conformance
// suite's directory, apart from the cases that every implementation reads.
// README.md explains them.

const ABSENT: FieldDescription = { type: 'absent' };

const DEFAULT: LayoutDescription = {
    sized: false,
    versioned: false,
    offsetType: 'uint32'
};

const VALID_CASES: ValidCase[] = [
    validCase(
        'absent-fields',
        'A scalar, an array and a string, with an absent field after each, whose offset is 0 and which takes no bytes',
        table([
            { type: 'uint8', value: 1 },
            ABSENT,
            { type: 'int32[]', value: [1, -2] },
            ABSENT,
            { type: 'string', value: 'a' },
            ABSENT
        ])
    ),
    validCase(
        'absent-every-field',
        'Three absent fields: the table is its offsets, which are 0',
        table([ABSENT, ABSENT, ABSENT])
    ),
    validCase(
        'absent-first-field',
        'An absent field before a float64, so that the first value is that of the second field',
        table([ABSENT, { type: 'float64', value: 1.5 }])
    ),
    validCase(
        'absent-nested',
        'A nested table with absent fields, an absent field where a table could be, and a vector whose items have absent fields, the last of them every field',
        table([
            nested(table([ABSENT, { type: 'uint16', value: 7 }, ABSENT])),
            ABSENT,
            {
                type: 'vector',
                items: [
                    fromBuilder(table([{ type: 'uint8', value: 1 }, ABSENT])),
                    fromBuilder(table([ABSENT, ABSENT]))
                ]
            }
        ])
    ),
    validCase(
        'absent-nested-last',
        'A nested table of two absent fields at the end of its parent',
        table([{ type: 'uint8', value: 1 }, nested(table([ABSENT, ABSENT]))])
    ),
    validCase(
        'absent-uint8-sized-versioned',
        'Absent fields around a string in a table of uint8 offsets with a length and a version',
        table(
            [ABSENT, { type: 'string', value: 'a' }, ABSENT],
            { sized: true, versioned: true, offsetType: 'uint8' },
            3
        )
    ),
    validCase(
        'absent-uint16',
        'Absent fields around an aligned array in a table of uint16 offsets',
        table([ABSENT, { type: 'float64[]', value: [0.5] }, ABSENT], {
            sized: false,
            versioned: false,
            offsetType: 'uint16'
        })
    ),
    validCase(
        'missing-fields',
        'A table of two fields, read by a reader that knows two more: their offsets would end after the first value starts, at 8',
        {
            ...table([
                { type: 'uint32', value: 1 },
                { type: 'string', value: 'a' }
            ]),
            missing: 2
        }
    ),
    validCase(
        'missing-fields-after-padding',
        'A table whose first value is an aligned array after 4 bytes of padding, read by a reader that knows two more fields: the first of their offsets lies in the padding and reads as 0, and the second would end after the first value starts',
        {
            ...table([
                { type: 'float64[]', value: [1.5] },
                { type: 'uint8', value: 2 }
            ]),
            missing: 2
        }
    ),
    validCase(
        'missing-fields-nested',
        'A nested table and a vector item, each read by a reader that knows more fields than it holds, in a table that such a reader reads too',
        {
            ...table([
                nested({
                    ...table([{ type: 'uint8', value: 1 }]),
                    missing: 1
                }),
                {
                    type: 'vector',
                    items: [
                        fromBuilder({
                            ...table([{ type: 'uint16', value: 2 }]),
                            missing: 2
                        })
                    ]
                }
            ]),
            missing: 1
        }
    ),
    validCase(
        'missing-fields-after-absent',
        'A table whose last two fields are absent, read by a reader that knows one more',
        { ...table([{ type: 'uint8', value: 1 }, ABSENT, ABSENT]), missing: 1 }
    ),
    validCase(
        'missing-fields-uint16-sized-versioned',
        'A table of uint16 offsets with a length and a version, read by a reader that knows three more fields',
        {
            ...table(
                [{ type: 'int8', value: -1 }],
                { sized: true, versioned: true, offsetType: 'uint16' },
                1
            ),
            missing: 3
        }
    )
];

export const PRESENCE_SUITE: Suite = {
    valid: VALID_CASES,
    equivalent: [],
    invalid: invalidCases()
};

function invalidCases(): InvalidCase[] {
    const fields = caseNamed('absent-fields');
    const fieldsEnd = 0xffff_ffff;

    return [
        invalidCase(
            'absent-first-offset-truncated',
            'The table of absent-first-field ends within its first offset, so that the field reads as neither absent nor present',
            'absent-first-field',
            { kind: 'truncate', length: 2 },
            0
        ),
        invalidCase(
            'absent-offset-truncated',
            'The table of absent-every-field ends within its second offset',
            'absent-every-field',
            { kind: 'truncate', length: 6 },
            1
        ),
        invalidCase(
            'absent-nested-offset-truncated',
            'The nested table of absent-nested-last ends within its second offset',
            'absent-nested-last',
            { kind: 'truncate', length: 15 },
            1
        ),
        invalidCase(
            'present-after-absent-offset-past-end',
            'The offset of the string of absent-fields, after two absent fields, points past the end',
            'absent-fields',
            overwrite(offsetPosition(fields, 4), littleEndian(fieldsEnd, 4)),
            4
        )
    ];
}

function caseNamed(name: string): TableDescription {
    return VALID_CASES.find(validCase => validCase.name === name)!.table;
}

/** The position of the offset of root field `index` */
function offsetPosition({ layout }: TableDescription, index: number): number {
    return headerSizeOf(layout) + index * OFFSET_SIZES[layout.offsetType];
}

function validCase(
    name: string,
    description: string,
    table: TableDescription
): ValidCase {
    return { name, description, file: `valid/${name}.bin`, table };
}

function invalidCase(
    name: string,
    description: string,
    validName: string,
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

function overwrite(at: number, bytes: number[]): Corruption {
    return { kind: 'overwrite', at, bytes };
}

function littleEndian(value: number, size: OffsetSize): number[] {
    return Array.from(
        { length: size },
        (_, index) => Math.floor(value / 256 ** index) % 256
    );
}

function table(
    fields: FieldDescription[],
    layout: LayoutDescription = DEFAULT,
    version?: number
): TableDescription {
    return version == null ? { layout, fields } : { layout, version, fields };
}

function nested(table: TableDescription): FieldDescription {
    return { type: 'table', from: 'builder', table };
}

function fromBuilder(table: TableDescription): NestedTable {
    return { from: 'builder', table };
}
