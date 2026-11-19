import { describe, expect } from 'hypotes';
import { fc, it } from 'hypotes/fast-check';
import { DecodeError } from 'tafla';
import {
    corruptionOf,
    sliceOf,
    CORRUPTIONS,
    MAX_VALUES,
    SCALAR_VALUES,
    SLICES,
    STRINGS
} from '../conformance/arbitraries.ts';
import {
    corrupt,
    type LayoutDescription,
    type TableDescription
} from '../conformance/description.ts';
import { buildTable } from '../conformance/tables.ts';
import { array, message, scalar, string } from './descriptions.ts';
import { expectValuesOrDecodeError } from './expectations.ts';
import * as javascript from './layouts/javascript/layouts.js';
import * as typescript from './layouts/typescript/layouts.ts';
import type { Uint8OffsetsFields } from './layouts/typescript/layouts.ts';

// The code generated from schemas/layouts.tafla, in TypeScript and in
// JavaScript, writes and reads messages of every layout with tafla. The
// messages have the same fields, and their layouts and versions are written
// out here from the schema rather than taken from the generated code.

type Layouts = typeof typescript | typeof javascript;

const LAYOUTS: ReadonlyArray<readonly [string, Layouts]> = [
    ['TypeScript', typescript],
    ['JavaScript', javascript]
];

type Fields = Uint8OffsetsFields;

interface LayoutMessage {
    name: keyof Layouts;
    layout: LayoutDescription;
    version?: number;
}

const MESSAGES: readonly LayoutMessage[] = [
    { name: 'Uint8Offsets', layout: layoutOf('uint8') },
    { name: 'Uint8OffsetsSized', layout: layoutOf('uint8', { sized: true }) },
    { name: 'Uint8OffsetsVersioned', layout: layoutOf('uint8', { versioned: true }), version: 255 },
    {
        name: 'Uint8OffsetsSizedVersioned',
        layout: layoutOf('uint8', { sized: true, versioned: true }),
        version: 0
    },
    { name: 'Uint16Offsets', layout: layoutOf('uint16') },
    { name: 'Uint16OffsetsSized', layout: layoutOf('uint16', { sized: true }) },
    {
        name: 'Uint16OffsetsVersioned',
        layout: layoutOf('uint16', { versioned: true }),
        version: 65535
    },
    {
        name: 'Uint16OffsetsSizedVersioned',
        layout: layoutOf('uint16', { sized: true, versioned: true }),
        version: 1
    },
    { name: 'Uint32Offsets', layout: layoutOf('uint32') },
    { name: 'Uint32OffsetsSized', layout: layoutOf('uint32', { sized: true }) },
    {
        name: 'Uint32OffsetsVersioned',
        layout: layoutOf('uint32', { versioned: true }),
        version: 4294967295
    },
    {
        name: 'Uint32OffsetsSizedVersioned',
        layout: layoutOf('uint32', { sized: true, versioned: true }),
        version: 2
    }
];

const VERSIONED = MESSAGES.filter(({ version }) => version !== undefined);
const UINT16_OFFSETS = MESSAGES.find(({ name }) => name === 'Uint16Offsets')!;

function layoutOf(
    offsetType: LayoutDescription['offsetType'],
    header: Partial<LayoutDescription> = {}
): LayoutDescription {
    return { sized: false, versioned: false, offsetType, ...header };
}

const ENCODER = new TextEncoder();

/**
 * Fields of tables short enough for uint8 offsets in every layout: a name of
 * up to 120 bytes of UTF-8 and up to 8 values take at most 150 bytes
 */
const FIELDS: fc.Arbitrary<Fields> = fc.record({
    id: SCALAR_VALUES.uint32,
    name: STRINGS.filter(name => ENCODER.encode(name).length <= 120),
    values: fc.array(SCALAR_VALUES.uint16, { maxLength: 8 })
});

/** Fields of tables that are too long for uint8 offsets about half the time */
const LONGER_FIELDS: fc.Arbitrary<Fields> = fc.record({
    id: SCALAR_VALUES.uint32,
    name: fc.string({ unit: 'binary', maxLength: 100 }),
    values: fc.array(SCALAR_VALUES.uint16, { maxLength: 100 })
});

function describeTable(
    { layout, version }: LayoutMessage,
    { id, name, values }: Fields
): TableDescription {
    return message(layout, [scalar('uint32', id), string(name), array('uint16', values)], version);
}

function read(layouts: Layouts, { name }: LayoutMessage, bytes: Uint8Array): Fields {
    const table = new layouts[name](bytes);

    return { id: table.id(), name: table.name(), values: Array.from(table.values()) };
}

/** The bytes that `write` returns, or the class of the error that it throws */
function outcomeOf(write: () => Uint8Array): Uint8Array | string {
    try {
        return write();
    } catch (error) {
        return error instanceof Error ? error.constructor.name : String(error);
    }
}

describe.each(LAYOUTS)('%s', (_, layouts) => {
    describe.each(MESSAGES)('$name', layoutMessage => {
        const { name } = layoutMessage;

        it.prop([FIELDS, SLICES])(
            'should read back its fields, wherever its bytes lie',
            (fields, slice) => {
                const bytes = sliceOf(layouts[name].serialize(fields), slice);

                expect(read(layouts, layoutMessage, bytes)).toEqual(fields);
            }
        );

        it.prop([LONGER_FIELDS])(
            'should write the bytes that TableBuilder writes in its layout, or throw as it does for a table too long for its offsets',
            fields => {
                const written = outcomeOf(() => layouts[name].serialize(fields));
                const byHand = outcomeOf(() =>
                    buildTable(describeTable(layoutMessage, fields)).bytes()
                );

                expect(written).toStrictEqual(byHand);
            }
        );

        it.prop([FIELDS, CORRUPTIONS])(
            'should read corrupted bytes as values or throw DecodeError',
            (fields, corruption) => {
                const bytes = layouts[name].serialize(fields);
                const corrupted = corrupt(bytes, corruptionOf(corruption, bytes.length));

                expectValuesOrDecodeError(() => read(layouts, layoutMessage, corrupted));
            }
        );

        it.prop([fc.uint8Array({ maxLength: 64 })])(
            'should read arbitrary bytes as values or throw DecodeError',
            bytes => {
                expectValuesOrDecodeError(() => read(layouts, layoutMessage, bytes));
            }
        );
    });

    describe.each(VERSIONED)('$name', layoutMessage => {
        const { layout, version } = layoutMessage;
        const otherVersions = fc
            .integer({ min: 0, max: MAX_VALUES[layout.offsetType] })
            .filter(other => other !== version);

        it.prop([FIELDS, otherVersions])(
            'should reject a table of another version with DecodeError',
            (fields, other) => {
                const bytes = buildTable(
                    describeTable({ ...layoutMessage, version: other }, fields)
                ).bytes();

                expect(() => read(layouts, layoutMessage, bytes)).toThrow(DecodeError);
            }
        );
    });

    it('should write the longest table that uint16 offsets address, and throw RangeError for a longer one', () => {
        // 3 offsets, the id, an empty string's length and the array's count
        // take 14 bytes, which leave 65,521 for 32,760 elements
        const longest = { id: 0, name: '', values: new Array<number>(32_760).fill(0xffff) };
        const tooLong = { ...longest, values: [...longest.values, 0] };

        const bytes = layouts.Uint16Offsets.serialize(longest);
        expect(bytes).toHaveLength(65_534);
        expect(read(layouts, UINT16_OFFSETS, bytes)).toEqual(longest);
        expect(() => layouts.Uint16Offsets.serialize(tooLong)).toThrow(RangeError);
    });
});
