import { describe, expect } from 'hypotes';
import { fc, it } from 'hypotes/fast-check';
import { DecodeError, TableReader } from 'tafla';
import {
    corruptionOf,
    sliceOf,
    BYTE_OFFSETS,
    CORRUPTIONS,
    SLICES,
    STRINGS
} from '../conformance/arbitraries.ts';
import { corrupt, type TableDescription } from '../conformance/description.ts';
import { buildTable } from '../conformance/tables.ts';
import { ALL_TYPES } from './all-types-tables.ts';
import type * as javascript from './all_types/javascript/all-types.js';
import type * as typescript from './all_types/typescript/all-types.ts';
import type {
    EntryFields,
    NarrowStringVectorsFields,
    StringVectorsFields
} from './all_types/typescript/all-types.ts';
import { expectValuesOrDecodeError } from './expectations.ts';

// The vectors of strings of schemas/all_types.tafla, whose getters return a
// StringVector and whose fields() and writers take arrays of strings

const STRING_LISTS = fc.array(STRINGS, { maxLength: 6 });

const ENTRY_FIELDS: fc.Arbitrary<EntryFields> = fc.record(
    { key: STRINGS, aliases: STRING_LISTS },
    { noNullPrototype: true }
);

const STRING_VECTORS_FIELDS: fc.Arbitrary<StringVectorsFields> = fc.record(
    {
        id: fc.integer({ min: 0, max: 0xffff_ffff }),
        tags: STRING_LISTS,
        labels: fc.record({ values: STRING_LISTS }, { noNullPrototype: true }),
        entries: fc.array(ENTRY_FIELDS, { maxLength: 3 })
    },
    { noNullPrototype: true }
);

/** Names of short strings, which fit in a table of uint16 offsets */
const NARROW_FIELDS: fc.Arbitrary<NarrowStringVectorsFields> = fc.record(
    {
        names: fc.array(fc.string({ maxLength: 8 }), { maxLength: 6 }),
        count: fc.integer({ min: 0, max: 0xff })
    },
    { noNullPrototype: true }
);

type StringVectors = typescript.StringVectors | javascript.StringVectors;
type Entry = typescript.Entry | javascript.Entry;

/** Reads every field of a `StringVectors` through its getters and views */
function readStringVectors(table: StringVectors): StringVectorsFields {
    const entries = table.entries();

    return {
        id: table.id(),
        tags: table.tags().toArray(),
        labels: { values: table.labels().values().toArray() },
        entries: Array.from({ length: entries.length() }, (_, index) =>
            readEntry(entries.get(index))
        )
    };
}

/** Reads an `Entry` through its getters, each string of it from its ends */
function readEntry(entry: Entry): EntryFields {
    const aliases = entry.aliases();

    return {
        key: entry.key(),
        aliases: Array.from({ length: aliases.length() }, (_, index) => aliases.get(index))
    };
}

/** The description of the table that a `StringVectors` writes */
function describeStringVectors(fields: StringVectorsFields): TableDescription {
    const layout = { sized: false, versioned: false, offsetType: 'uint32' } as const;

    return {
        layout,
        fields: [
            { type: 'uint32', value: fields.id },
            { type: 'string[]', value: [...fields.tags] },
            {
                type: 'table',
                from: 'builder',
                table: { layout, fields: [{ type: 'string[]', value: [...fields.labels.values] }] }
            },
            {
                type: 'vector',
                items: fields.entries.map(entry => ({
                    from: 'builder',
                    table: {
                        layout,
                        fields: [
                            { type: 'string', value: entry.key },
                            { type: 'string[]', value: [...entry.aliases] }
                        ]
                    }
                }))
            }
        ]
    };
}

describe.each(ALL_TYPES)('%s', (_, allTypes) => {
    it.prop([STRING_VECTORS_FIELDS, SLICES])(
        'should read back the vectors of strings of StringVectors, wherever its bytes lie',
        (fields, slice) => {
            const bytes = sliceOf(allTypes.StringVectors.serialize(fields), slice);

            expect(readStringVectors(new allTypes.StringVectors(bytes))).toStrictEqual(fields);
        }
    );

    it.prop([STRING_VECTORS_FIELDS])('should write the bytes of TableBuilder', fields => {
        expect(allTypes.StringVectors.serialize(fields)).toStrictEqual(
            buildTable(describeStringVectors(fields)).bytes()
        );
    });

    it.prop([ENTRY_FIELDS])('should read back an Entry and its labels with fields()', fields => {
        const bytes = allTypes.StringVectors.serialize({
            id: 0,
            tags: [],
            labels: { values: fields.aliases },
            entries: [fields]
        });
        const table = new allTypes.StringVectors(bytes);

        expect(table.entries().get(0).fields()).toStrictEqual(fields);
        expect(table.labels().fields()).toStrictEqual({ values: fields.aliases });
    });

    it.prop([NARROW_FIELDS])(
        'should read back every field of NarrowStringVectors, of uint16 offsets, with fields()',
        fields => {
            const bytes = allTypes.NarrowStringVectors.serialize(fields);

            expect(new allTypes.NarrowStringVectors(bytes).fields()).toStrictEqual(fields);
        }
    );

    it('should throw RangeError for NarrowStringVectors too long for uint16 offsets', () => {
        const names = ['a'.repeat(0x8000), 'b'.repeat(0x8000)];

        expect(() => allTypes.NarrowStringVectors.serialize({ names, count: 0 })).toThrow(
            RangeError
        );
    });

    it.prop([
        STRING_VECTORS_FIELDS,
        fc.array(CORRUPTIONS, { minLength: 1, maxLength: 3 }),
        BYTE_OFFSETS
    ])(
        'should read a corrupted StringVectors as values or throw DecodeError',
        (fields, corruptions, byteOffset) => {
            let bytes = allTypes.StringVectors.serialize(fields);
            for (const corruption of corruptions) {
                bytes = corrupt(bytes, corruptionOf(corruption, bytes.length));
            }
            const sliced = sliceOf(bytes, { byteOffset, fill: 0xff });

            expectValuesOrDecodeError(() => readStringVectors(new allTypes.StringVectors(sliced)));
            expectValuesOrDecodeError(() => new allTypes.StringVectors(sliced).labels().fields());
        }
    );

    it('should throw DecodeError for a tag that isn’t valid UTF-8, and read the others', () => {
        const bytes = allTypes.StringVectors.serialize({
            id: 1,
            tags: ['ok', 'bad'],
            labels: { values: [] },
            entries: []
        });
        const table = new allTypes.StringVectors(bytes);
        // The second tag's first byte, after the count, two ends and 'ok'
        const reader = new TableReader(bytes);
        bytes[reader.rootOffset(1) + 3 * 4 + 2] = 0xc0;

        expect(table.tags().get(0)).toBe('ok');
        expect(() => table.tags().get(1)).toThrow(DecodeError);
        expect(() => table.tags().toArray()).toThrow(DecodeError);
    });
});
