import { describe, it as example, expect } from 'hypotes';
import { fc, it } from 'hypotes/fast-check';
import { DecodeError, TableBuilder, TableLayout } from 'tafla';
import { corruptionOf, sliceOf, CORRUPTIONS, SLICES } from '../conformance/arbitraries.ts';
import { corrupt } from '../conformance/description.ts';
import { expectValuesOrDecodeError } from './expectations.ts';
import * as javascript from './presence/javascript/presence.js';
import * as typescript from './presence/typescript/presence.ts';

// The code generated from test/presence.tafla, in TypeScript and in
// JavaScript: optional fields, which its writers leave out with an offset of
// 0, and fields that a later version of a message or a later revision of a
// table appends, which its readers return as undefined from the tables that
// don't hold them

type Presence = typeof typescript | typeof javascript;

const PRESENCE: ReadonlyArray<readonly [string, Presence]> = [
    ['TypeScript', typescript],
    ['JavaScript', javascript]
];

/** A value, or undefined as often as not */
function optional<T>(values: fc.Arbitrary<T>): fc.Arbitrary<T | undefined> {
    return fc.option(values, { nil: undefined, freq: 2 });
}

const UINT16S = fc.integer({ min: 0, max: 0xffff });
const UINT32S = fc.integer({ min: 0, max: 0xffff_ffff });
const FLOAT32S = fc.float();

const POINT_FIELDS = fc.record({ x: fc.double(), y: fc.double(), z: fc.double() });
const ENTRY_FIELDS = fc.record({ key: UINT16S, note: optional(fc.string()), weight: FLOAT32S });

const RECORD_FIELDS: fc.Arbitrary<typescript.RecordFields> = fc.record({
    id: UINT32S,
    name: optional(fc.string()),
    values: optional(fc.array(fc.double(), { maxLength: 4 })),
    origin: optional(POINT_FIELDS),
    flag: optional(fc.boolean()),
    entries: optional(fc.array(ENTRY_FIELDS, { maxLength: 3 })),
    total: optional(fc.bigInt({ min: -(2n ** 63n), max: 2n ** 63n - 1n })),
    label: fc.string(),
    extent: optional(fc.record({ width: FLOAT32S, height: FLOAT32S }))
});

// The defaults that the schema gives the scalars that may be missing
const FLAG_DEFAULT = true;
const TOTAL_DEFAULT = -1n;
const Z_DEFAULT = -0.5;
const WEIGHT_DEFAULT = Math.fround(0.1);
const COUNT_DEFAULT = 7;

/** The first version of a record's fields, without those of version 2 */
function firstVersionOf(fields: typescript.RecordFields): typescript.RecordV1Fields {
    const { origin, entries } = fields;

    return {
        id: fields.id,
        name: fields.name,
        values: fields.values,
        origin: origin && { x: origin.x, y: origin.y },
        flag: fields.flag,
        entries: entries?.map(({ key, note }) => ({ key, note })),
        total: fields.total
    };
}

type Record = typescript.Record | javascript.Record;
type RecordV1 = typescript.RecordV1 | javascript.RecordV1;

/**
 * Reads every field of a record through its getters, with its arrays and
 * views as plain arrays, undefined for each field but a scalar that it
 * doesn't hold, and each scalar that may be missing as its getter's value
 * and whether `has<Field>()` finds it
 */
function readRecord(record: Record): object {
    const extent = record.extent();

    return {
        ...readFirstVersion(record),
        origin: readPoint(record.origin()),
        entries: readEntries(record.entries()),
        label: record.label(),
        extent: extent && { width: extent.width(), height: extent.height() }
    };
}

function readFirstVersion(record: Record | RecordV1): object {
    const values = record.values();

    return {
        id: record.id(),
        name: record.name(),
        values: values && Array.from(values),
        origin: record.origin() && { x: record.origin()!.x(), y: record.origin()!.y() },
        flag: [record.flag(), record.hasFlag()],
        entries: readEntriesV1(record.entries()),
        total: [record.total(), record.hasTotal()]
    };
}

function readPoint(point: typescript.Point | javascript.Point | undefined): unknown {
    return point && { x: point.x(), y: point.y(), z: [point.z(), point.hasZ()] };
}

function readEntries(
    entries: typescript.EntryVector | javascript.EntryVector | undefined
): unknown {
    if (entries == null) {
        return undefined;
    }

    return Array.from({ length: entries.length() }, (_, index) => {
        const entry = entries.get(index);
        return {
            key: entry.key(),
            note: entry.note(),
            weight: [entry.weight(), entry.hasWeight()]
        };
    });
}

function readEntriesV1(
    entries:
        | typescript.EntryVector
        | javascript.EntryVector
        | typescript.EntryV1Vector
        | javascript.EntryV1Vector
        | undefined
): unknown {
    if (entries == null) {
        return undefined;
    }

    return Array.from({ length: entries.length() }, (_, index) => {
        const entry = entries.get(index);
        return { key: entry.key(), note: entry.note() };
    });
}

/** A scalar that may be missing, as its getter and `has<Field>()` read it */
function scalar<T>(value: T | undefined, missing: T): [T, boolean] {
    return value === undefined ? [missing, false] : [value, true];
}

/**
 * A record's fields as its getters return them: absent fields undefined,
 * and each absent scalar its default
 */
function expectedRecord(fields: typescript.RecordFields): object {
    const { origin, entries } = fields;

    return {
        ...expectedFirstVersion(firstVersionOf(fields)),
        origin: origin && { ...origin, z: scalar(origin.z, Z_DEFAULT) },
        entries: entries?.map(({ key, note, weight }) => ({
            key,
            note,
            weight: scalar(weight, WEIGHT_DEFAULT)
        })),
        label: fields.label,
        extent: fields.extent
    };
}

function expectedFirstVersion(fields: typescript.RecordV1Fields): object {
    return {
        ...fields,
        values: fields.values && Array.from(fields.values),
        flag: scalar(fields.flag, FLAG_DEFAULT),
        total: scalar(fields.total, TOTAL_DEFAULT)
    };
}

/** The fields of version 1, as a reader of version 2 reads them */
function expectedFromFirstVersion(fields: typescript.RecordV1Fields): unknown {
    return {
        ...expectedFirstVersion(fields),
        origin: fields.origin && { ...fields.origin, z: [Z_DEFAULT, false] },
        entries: fields.entries?.map(({ key, note }) => ({
            key,
            note,
            weight: [WEIGHT_DEFAULT, false]
        })),
        label: undefined,
        extent: undefined
    };
}

describe.each(PRESENCE)('%s', (_, presence) => {
    describe('optional fields', () => {
        it.prop([RECORD_FIELDS, SLICES])(
            'should read back every field of a Record, and undefined or its default for each absent one, wherever its bytes lie',
            (fields, slice) => {
                const bytes = sliceOf(presence.Record.serialize(fields), slice);

                expect(readRecord(new presence.Record(bytes))).toEqual(expectedRecord(fields));
            }
        );

        it.prop([RECORD_FIELDS])(
            'should write into reused bytes what it writes into new ones, absent fields included',
            fields => {
                const written = presence.Record.serialize(fields);
                const reused = new Uint8Array(written.length + 8).fill(0xff);

                expect(presence.Record.serializeInto(fields, reused)).toStrictEqual(written);
            }
        );

        example('should write an absent field as TableBuilder.setAbsent() does', () => {
            const written = presence.Record.serialize({ id: 7, label: 'a' });

            const builder = new TableBuilder(
                9,
                TableLayout.create({ sized: true, version: presence.Record.FORMAT_VERSION })
            );
            builder.setUint32(7);
            for (let index = 1; index < 7; index++) {
                builder.setAbsent();
            }
            builder.setString('a');
            builder.setAbsent();

            expect(written).toStrictEqual(builder.bytes());
        });
    });

    describe('versions', () => {
        it.prop([RECORD_FIELDS])(
            'should read a Record of the first version, with undefined or the default for each field that the second added, in nested tables too',
            fields => {
                const first = firstVersionOf(fields);
                const bytes = presence.RecordV1.serialize(first);

                expect(readRecord(new presence.Record(bytes))).toEqual(
                    expectedFromFirstVersion(first)
                );
            }
        );

        it.prop([fc.integer({ min: 0, max: 0xffff_ffff }), RECORD_FIELDS])(
            'should reject a Record of a version before the first or after its own',
            (version, fields) => {
                const bytes = presence.Record.serialize(fields);
                new DataView(bytes.buffer, bytes.byteOffset).setUint32(4, version, true);

                const reads = version >= 1 && version <= 2;
                if (reads) {
                    expect(() => new presence.Record(bytes)).not.toThrow();
                } else {
                    expect(() => new presence.Record(bytes)).toThrow(DecodeError);
                }
                expect(presence.Record.versionOf(bytes)).toBe(version);
            }
        );

        example('should keep rejecting other versions with a reader of one version', () => {
            const bytes = presence.Record.serialize({ id: 1, label: '' });

            expect(() => new presence.RecordV1(bytes)).toThrow(DecodeError);
        });

        it.prop([UINT32S, UINT16S])(
            'should read a Sample of the first revision, without a version, with the default for the field that the second appended',
            (key, count) => {
                const first = presence.SampleV1.serialize({ key });
                const second = presence.Sample.serialize({ key, count });

                expect(new presence.Sample(first).key()).toBe(key);
                expect(new presence.Sample(first).count()).toBe(COUNT_DEFAULT);
                expect(new presence.Sample(first).hasCount()).toBe(false);
                expect(new presence.Sample(second).count()).toBe(count);
                expect(new presence.Sample(second).hasCount()).toBe(true);
                // A reader of the first revision reads its fields of a later one
                expect(new presence.SampleV1(second).key()).toBe(key);
            }
        );
    });

    describe('untrusted bytes', () => {
        it.prop([RECORD_FIELDS, fc.array(CORRUPTIONS, { minLength: 1, maxLength: 3 })])(
            'should read a corrupted Record as values, undefined or DecodeError, never another error',
            (fields, corruptions) => {
                let bytes = presence.Record.serialize(fields);
                for (const corruption of corruptions) {
                    bytes = corrupt(bytes, corruptionOf(corruption, bytes.length));
                }

                expectValuesOrDecodeError(() => readRecord(new presence.Record(bytes)));
            }
        );

        it.prop([fc.uint8Array({ maxLength: 48 })])(
            'should read arbitrary bytes as a Sample without a version as values, undefined or DecodeError',
            bytes => {
                expectValuesOrDecodeError(() => new presence.Sample(bytes).count());
                expectValuesOrDecodeError(() => new presence.Sample(bytes).hasCount());
                expectValuesOrDecodeError(() => new presence.Sample(bytes).key());
            }
        );
    });
});
