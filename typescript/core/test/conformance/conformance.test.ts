import { readdir, readFile } from 'node:fs/promises';
import { describe, expect, it } from 'hypotes';
import { DecodeError, TableReader } from 'tafla';
import { SUITE } from './cases.ts';
import type { Suite, ValidCase } from './description.ts';
import { writeSuite, SUITE_DIRECTORY } from './files.ts';
import { buildTable, readTable, DescriptionReader } from './tables.ts';

const directory = SUITE_DIRECTORY;

// The checked-in suite, which the other implementations of tafla load too
const json = await readFile(new URL('cases.json', directory), 'utf8');
const suite = JSON.parse(json) as Suite;
const tables = await readTables();

async function readTables(): Promise<Map<string, Uint8Array>> {
    const tables = new Map<string, Uint8Array>();

    for (const subdirectory of ['valid/', 'equivalent/', 'invalid/']) {
        for (const name of await readdir(new URL(subdirectory, directory))) {
            const path = `${subdirectory}${name}`;
            // A plain Uint8Array rather than a Buffer, to compare with tafla's
            tables.set(path, new Uint8Array(await readFile(new URL(path, directory))));
        }
    }

    return tables;
}

function validCaseNamed(name: string): ValidCase {
    const validCase = suite.valid.find(validCase => validCase.name === name);
    if (validCase == null) {
        throw new Error(`There's no valid case ${name}`);
    }

    return validCase;
}

function bytesOf(file: string): Uint8Array {
    const bytes = tables.get(file);
    if (bytes == null) {
        throw new Error(`Missing ${file}`);
    }

    return bytes;
}

describe('the checked-in suite (run `yarn conformance:generate` to update it)', () => {
    const generated = writeSuite(SUITE);

    it('should describe the cases that the generator writes', () => {
        expect(json).toBe(generated.json);
    });

    it('should hold the tables that the generator writes', () => {
        expect(new Set(tables.keys())).toStrictEqual(new Set(generated.tables.keys()));

        for (const [path, bytes] of generated.tables) {
            expect(tables.get(path), path).toStrictEqual(bytes);
        }
    });
});

describe('valid cases', () => {
    it.each(suite.valid)('should read $name', ({ file, table }) => {
        expect(readTable(bytesOf(file), table)).toStrictEqual(table);
    });

    it.each(suite.valid)('should rebuild $name byte for byte', ({ file, table }) => {
        expect(buildTable(table).bytes()).toStrictEqual(bytesOf(file));
    });
});

describe('equivalent cases', () => {
    it.each(suite.equivalent)('should read $name as $case', ({ file, case: name }) => {
        const { table } = validCaseNamed(name);
        expect(readTable(bytesOf(file), table)).toStrictEqual(table);
    });
});

describe('invalid cases', () => {
    it.each(suite.invalid)(
        'should read $name up to field $field and then throw DecodeError',
        ({ file, case: name, field }) => {
            const { table } = validCaseNamed(name);
            const reader = new DescriptionReader(bytesOf(file), table.layout);

            for (let index = 0; index < field; index++) {
                const read = reader.readRootField(index, table.fields[index]);
                expect(read).toStrictEqual(table.fields[index]);
            }

            expect(() => reader.readRootField(field, table.fields[field])).toThrow(DecodeError);
        }
    );
});

describe('the vector of strings', () => {
    it('should hold the bytes of a vector of items that are each string’s UTF-8', () => {
        // What a vector of strings adds to the format is how its items are
        // read, so the builders that write vectors from bytes write it too
        const { table } = validCaseNamed('string-vectors');
        const [strings] = table.fields;
        if (strings.type !== 'string[]') {
            throw new Error('The first field of string-vectors is its strings');
        }

        const encoder = new TextEncoder();
        const built = buildTable({ ...table, fields: [strings] }).bytes();
        const reader = new DescriptionReader(built, table.layout);
        const items = strings.value.map(string => encoder.encode(string));

        expect(itemsOf(built)).toStrictEqual(items);
        expect(reader.readRootField(0, strings)).toStrictEqual(strings);
    });
});

/** The items of the vector that is the first field of a table of `bytes` */
function itemsOf(bytes: Uint8Array): Uint8Array[] {
    const reader = new TableReader(bytes);

    return reader.getVector(reader.rootOffset(0)).map(item => item.slice());
}
