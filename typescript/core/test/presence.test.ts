import { readdir, readFile } from 'node:fs/promises';
import { describe, it as example, expect } from 'hypotes';
import { fc, it } from 'hypotes/fast-check';
import { DecodeError, TableBuilder } from 'tafla';
import {
    corruptionOf,
    fitsItsLayout,
    sliceOf,
    tables,
    BYTE_OFFSETS,
    CORRUPTIONS
} from './conformance/arbitraries.ts';
import {
    corrupt,
    type FieldDescription,
    type NestedTable,
    type Suite,
    type TableDescription
} from './conformance/description.ts';
import { writeSuite, PRESENCE_DIRECTORY } from './conformance/files.ts';
import { PRESENCE_SUITE } from './conformance/presence-cases.ts';
import {
    buildTable,
    buildTableInPlace,
    layoutOf,
    nestedInPlace,
    readTable,
    writeTable,
    DescriptionMismatch,
    DescriptionReader
} from './conformance/tables.ts';

// The cases of presence, which lie apart from the conformance suite's other
// cases, and the properties of absent fields, whose offsets are 0, and of
// the fields that a reader of a later revision of a table finds missing.
// The tables are those of the conformance suite's model, with fields left
// out.

const directory = PRESENCE_DIRECTORY;
const SUBDIRECTORIES = ['valid/', 'invalid/'];

const json = await readFile(new URL('cases.json', directory), 'utf8');
const suite = JSON.parse(json) as Suite;
const files = await readFiles();

async function readFiles(): Promise<Map<string, Uint8Array>> {
    const files = new Map<string, Uint8Array>();

    for (const subdirectory of SUBDIRECTORIES) {
        for (const name of await readdir(new URL(subdirectory, directory))) {
            const path = `${subdirectory}${name}`;
            files.set(path, new Uint8Array(await readFile(new URL(path, directory))));
        }
    }

    return files;
}

function bytesOf(file: string): Uint8Array {
    const bytes = files.get(file);
    if (bytes == null) {
        throw new Error(`Missing ${file}`);
    }

    return bytes;
}

describe('the checked-in cases of presence (run `yarn conformance:generate` to update them)', () => {
    const generated = writeSuite(PRESENCE_SUITE);

    example('should describe the cases that the generator writes', () => {
        expect(json).toBe(generated.json);
    });

    example('should hold the tables that the generator writes', () => {
        expect(new Set(files.keys())).toStrictEqual(new Set(generated.tables.keys()));

        for (const [path, bytes] of generated.tables) {
            expect(files.get(path), path).toStrictEqual(bytes);
        }
    });
});

describe('valid cases of presence', () => {
    example.each(suite.valid)('should read $name', ({ file, table }) => {
        expect(readTable(bytesOf(file), table)).toStrictEqual(table);
    });

    example.each(suite.valid)('should rebuild $name byte for byte', ({ file, table }) => {
        expect(buildTable(table).bytes()).toStrictEqual(bytesOf(file));
    });
});

describe('invalid cases of presence', () => {
    example.each(suite.invalid)(
        'should read $name up to field $field and then throw DecodeError',
        ({ file, case: name, field }) => {
            const { table } = suite.valid.find(validCase => validCase.name === name)!;
            const reader = new DescriptionReader(bytesOf(file), table.layout);

            for (let index = 0; index < field; index++) {
                const read = reader.readRootField(index, table.fields[index]);
                expect(read).toStrictEqual(table.fields[index]);
            }

            expect(() => reader.readRootField(field, table.fields[field])).toThrow(DecodeError);
        }
    );
});

const ABSENT: FieldDescription = { type: 'absent' };

/**
 * Tables of the conformance suite's model with fields left out, at every
 * depth: each field in turn is absent when the next of `absences` says so
 */
const PRESENCE_TABLES: fc.Arbitrary<TableDescription> = fc
    .tuple(tables({ minFields: 1 }), fc.array(fc.boolean(), { minLength: 1, maxLength: 24 }))
    .map(([table, absences]) => withAbsences(table, absences))
    .filter(fitsItsLayout);

function withAbsences(table: TableDescription, absences: boolean[]): TableDescription {
    let next = 0;
    const isAbsent = (): boolean => absences[next++ % absences.length];

    const within = (table: TableDescription): TableDescription => ({
        ...table,
        fields: table.fields.map(field)
    });
    const item = ({ from, table }: NestedTable): NestedTable => ({ from, table: within(table) });
    const field = (description: FieldDescription): FieldDescription => {
        if (isAbsent()) {
            return ABSENT;
        }

        switch (description.type) {
            case 'table':
                return { ...description, table: within(description.table) };

            case 'vector':
                return { ...description, items: description.items.map(item) };

            default:
                return description;
        }
    };

    return within(table);
}

/** The tables that a `TableSizer` and a `TableWriter` write too */
const WRITTEN_TABLES = PRESENCE_TABLES.map(nestedInPlace).filter(fitsItsLayout);

describe('absent fields', () => {
    it.prop([PRESENCE_TABLES, BYTE_OFFSETS])(
        'should read back the values of a table with absent fields, wherever its bytes lie',
        (table, byteOffset) => {
            const bytes = sliceOf(buildTable(table).bytes(), { byteOffset, fill: 0xff });

            expect(readTable(bytes, table)).toStrictEqual(table);
        }
    );

    it.prop([PRESENCE_TABLES])(
        'should write 0 as the offset of an absent field, and every other offset after the offsets',
        table => {
            const reader = buildTable(table).finalize();
            const layout = layoutOf(table.layout);
            const firstValue = layout.headerSize() + table.fields.length * layout.offsetSize();

            for (const [index, field] of table.fields.entries()) {
                const offset = reader.rootOffset(index);

                if (field.type === 'absent') {
                    expect(offset).toBe(0);
                } else {
                    expect(offset).toBeGreaterThanOrEqual(firstValue);
                }
            }
        }
    );

    it.prop([WRITTEN_TABLES])(
        'should write the bytes that TableBuilder writes with TableSizer and TableWriter, in new bytes and in reused ones',
        table => {
            const built = buildTable(table).bytes();

            expect(writeTable(table)).toStrictEqual(built);
            expect(buildTableInPlace(table).bytes()).toStrictEqual(built);

            // Reused bytes aren't zero, so the writer writes each absent
            // field's offset
            const reused = new Uint8Array(built.length + 8).fill(0xff);
            expect(writeTable(table, reused)).toStrictEqual(built);
        }
    );

    example('should count an absent field as a field of the builder', () => {
        const builder = new TableBuilder(1);
        builder.setAbsent();

        expect(() => builder.setAbsent()).toThrow(RangeError);
        expect(builder.bytes()).toStrictEqual(new Uint8Array(4));
        expect(() => builder.setAbsent()).toThrow(TypeError);
    });
});

/** Tables whose first field is present, as a table with later revisions needs */
const REVISED_TABLES = PRESENCE_TABLES.filter(({ fields }) => fields[0].type !== 'absent');

describe('fields of a later revision', () => {
    it.prop([REVISED_TABLES, fc.integer({ min: 1, max: 4 })])(
        "should find every field after the table's own missing",
        (table, missing) => {
            const read = readTable(buildTable(table).bytes(), { ...table, missing });

            expect(read).toStrictEqual({ ...table, missing });
        }
    );

    it.prop([REVISED_TABLES, fc.nat()])(
        "should read the fields of an earlier revision from a table that holds more, and find the others unless they're absent",
        (table, split) => {
            const known = 1 + (split % table.fields.length);
            const later = table.fields.slice(known);
            const earlier: TableDescription = {
                ...table,
                fields: table.fields.slice(0, known),
                missing: later.length
            };

            const read = readTable(buildTable(table).bytes(), earlier);

            expect(read).toStrictEqual({
                ...earlier,
                missing: later.filter(field => field.type === 'absent').length
            });
        }
    );
});

describe('untrusted bytes with absent and missing fields', () => {
    it.prop([
        REVISED_TABLES,
        fc.integer({ min: 0, max: 4 }),
        fc.array(CORRUPTIONS, { minLength: 1, maxLength: 3 }),
        BYTE_OFFSETS
    ])(
        'should read a table with overwritten or cut bytes as values or throw DecodeError, within its bytes',
        (table, missing, corruptions, byteOffset) => {
            let bytes = buildTable(table).bytes();
            for (const corruption of corruptions) {
                bytes = corrupt(bytes, corruptionOf(corruption, bytes.length));
            }
            const described = { ...table, missing };

            expect(
                readOutcome(sliceOf(bytes, { byteOffset, fill: 0x00 }), described)
            ).toStrictEqual(readOutcome(sliceOf(bytes, { byteOffset, fill: 0xff }), described));
        }
    );
});

/**
 * Reads a table as its description sets it out, and returns what it reads,
 * the mismatch of corrupted bytes that read as another shape, or the
 * DecodeError
 */
function readOutcome(bytes: Uint8Array, table: TableDescription): unknown {
    try {
        return readTable(bytes, table);
    } catch (error) {
        if (!(error instanceof DescriptionMismatch)) {
            // Neither a RangeError, which DecodeError extends, nor any other
            expect(error).toBeInstanceOf(DecodeError);
        }

        return error;
    }
}
