import { describe, expect } from 'hypotes';
import { fc, it } from 'hypotes/fast-check';
import { DecodeError, TableLayout, TableReader } from 'tafla';
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
import { expectValuesOrDecodeError } from '../generated/expectations.ts';
import * as javascript from './lists/javascript/lists.js';
import * as typescript from './lists/typescript/lists.ts';
import type { EntryFields, LabelsFields, TaggedFields } from './lists/typescript/lists.ts';

// The code generated from schemas/proposed/lists.tafla, in TypeScript and
// in JavaScript, writes and reads the proposed vectors of strings and arrays
// of bools with tafla

type Lists = typeof typescript | typeof javascript;

const LISTS: ReadonlyArray<readonly [string, Lists]> = [
    ['TypeScript', typescript],
    ['JavaScript', javascript]
];

const STRING_LISTS = fc.array(STRINGS, { maxLength: 6 });
const BOOL_LISTS = fc.array(fc.boolean(), { maxLength: 12 });

const ENTRY_FIELDS: fc.Arbitrary<EntryFields> = fc.record(
    { key: STRINGS, aliases: STRING_LISTS, enabled: BOOL_LISTS },
    { noNullPrototype: true }
);

const TAGGED_FIELDS: fc.Arbitrary<TaggedFields> = fc.record(
    {
        id: fc.integer({ min: 0, max: 0xffff_ffff }),
        tags: STRING_LISTS,
        flags: BOOL_LISTS,
        names: fc.record({ values: STRING_LISTS }, { noNullPrototype: true }),
        entries: fc.array(ENTRY_FIELDS, { maxLength: 3 })
    },
    { noNullPrototype: true }
);

/** Labels of short strings, which fit in a table of uint16 offsets */
const LABELS_FIELDS: fc.Arbitrary<LabelsFields> = fc.record(
    {
        labels: fc.array(fc.string({ maxLength: 8 }), { maxLength: 6 }),
        bits: BOOL_LISTS,
        count: fc.integer({ min: 0, max: 0xff })
    },
    { noNullPrototype: true }
);

type Tagged = typescript.Tagged | javascript.Tagged;
type Entry = typescript.Entry | javascript.Entry;

/** The bools that the bytes of an array of bools read as */
function boolsOf(bytes: Uint8Array): boolean[] {
    return Array.from(bytes, byte => byte !== 0);
}

/** Reads every field of a `Tagged` through its getters and views */
function readTagged(tagged: Tagged): TaggedFields {
    const entries = tagged.entries();

    return {
        id: tagged.id(),
        tags: tagged.tags().toArray(),
        flags: boolsOf(tagged.flags()),
        names: { values: tagged.names().values().toArray() },
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
        aliases: Array.from({ length: aliases.length() }, (_, index) => aliases.get(index)),
        enabled: boolsOf(entry.enabled())
    };
}

/** The description of the table that a `Tagged` writes */
function describeTagged(fields: TaggedFields): TableDescription {
    const layout = { sized: false, versioned: false, offsetType: 'uint32' } as const;

    return {
        layout,
        fields: [
            { type: 'uint32', value: fields.id },
            { type: 'string[]', value: [...fields.tags] },
            { type: 'bool[]', value: [...fields.flags] },
            {
                type: 'table',
                from: 'builder',
                table: { layout, fields: [{ type: 'string[]', value: [...fields.names.values] }] }
            },
            {
                type: 'vector',
                items: fields.entries.map(entry => ({
                    from: 'builder',
                    table: {
                        layout,
                        fields: [
                            { type: 'string', value: entry.key },
                            { type: 'string[]', value: [...entry.aliases] },
                            { type: 'bool[]', value: [...entry.enabled] }
                        ]
                    }
                }))
            }
        ]
    };
}

describe.each(LISTS)('%s', (_, lists) => {
    it.prop([TAGGED_FIELDS, SLICES])(
        'should read back the strings and bools of a Tagged, wherever its bytes lie',
        (fields, slice) => {
            const bytes = sliceOf(lists.Tagged.serialize(fields), slice);

            expect(readTagged(new lists.Tagged(bytes))).toStrictEqual(fields);
        }
    );

    it.prop([TAGGED_FIELDS])('should write the bytes of TableBuilder', fields => {
        expect(lists.Tagged.serialize(fields)).toStrictEqual(
            buildTable(describeTagged(fields)).bytes()
        );
    });

    it.prop([LABELS_FIELDS])(
        'should read back every field of Labels, of uint16 offsets, with fields()',
        fields => {
            expect(new lists.Labels(lists.Labels.serialize(fields)).fields()).toStrictEqual(fields);
        }
    );

    it.prop([ENTRY_FIELDS])('should read back an Entry with fields()', fields => {
        const bytes = lists.Tagged.serialize({
            id: 0,
            tags: [],
            flags: [],
            names: { values: [] },
            entries: [fields]
        });

        expect(new lists.Tagged(bytes).entries().get(0).fields()).toStrictEqual(fields);
    });

    it('should read a byte of a bool array that isn’t 0 or 1 as true', () => {
        const bytes = lists.Labels.serialize({ labels: [], bits: [true, false], count: 0 });
        const reader = new TableReader(bytes, TableLayout.create({ offsetType: 'uint16' }));
        // The bits' elements follow their uint16 count
        const elements = reader.rootOffset(1) + 2;
        bytes[elements] = 0x80;
        bytes[elements + 1] = 2;

        expect(new lists.Labels(bytes).fields().bits).toStrictEqual([true, true]);
    });

    it('should throw RangeError for Labels too long for uint16 offsets', () => {
        const labels = ['a'.repeat(0x8000), 'b'.repeat(0x8000)];

        expect(() => lists.Labels.serialize({ labels, bits: [], count: 0 })).toThrow(RangeError);
    });

    it.prop([TAGGED_FIELDS, fc.array(CORRUPTIONS, { minLength: 1, maxLength: 3 }), BYTE_OFFSETS])(
        'should read a corrupted Tagged as values or throw DecodeError',
        (fields, corruptions, byteOffset) => {
            let bytes = lists.Tagged.serialize(fields);
            for (const corruption of corruptions) {
                bytes = corrupt(bytes, corruptionOf(corruption, bytes.length));
            }
            const sliced = sliceOf(bytes, { byteOffset, fill: 0xff });

            expectValuesOrDecodeError(() => readTagged(new lists.Tagged(sliced)));
        }
    );

    it('should throw DecodeError for a tag that isn’t valid UTF-8', () => {
        const bytes = lists.Tagged.serialize({
            id: 1,
            tags: ['ok', 'bad'],
            flags: [],
            names: { values: [] },
            entries: []
        });
        const tagged = new lists.Tagged(bytes);
        // The second tag's first byte, after the count, two ends and 'ok'
        const reader = new TableReader(bytes);
        bytes[reader.rootOffset(1) + 3 * 4 + 2] = 0xc0;

        expect(tagged.tags().get(0)).toBe('ok');
        expect(() => tagged.tags().get(1)).toThrow(DecodeError);
    });
});
