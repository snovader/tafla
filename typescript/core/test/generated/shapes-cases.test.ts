import { readdir, readFile } from 'node:fs/promises';
import { describe, expect, it } from 'hypotes';
import {
    read,
    serialize,
    writeShapesCases,
    SHAPES_CASES,
    SHAPES_DIRECTORY
} from './shapes-cases.ts';
import { SHAPES } from './shapes-tables.ts';

// The code generated from schemas/shapes.tafla writes the tables of
// conformance/generated, which the code generated in every language writes
// and reads too

const directory = SHAPES_DIRECTORY;

// The checked-in cases, which the other languages' tests load too
const json = await readFile(new URL('shapes.json', directory), 'utf8');
const tables = await readTables();

async function readTables(): Promise<Map<string, Uint8Array>> {
    const tables = new Map<string, Uint8Array>();

    for (const name of await readdir(new URL('shapes/', directory))) {
        const path = `shapes/${name}`;
        // A plain Uint8Array rather than a Buffer, to compare with tafla's
        tables.set(path, new Uint8Array(await readFile(new URL(path, directory))));
    }

    return tables;
}

function bytesOf(name: string): Uint8Array {
    const bytes = tables.get(`shapes/${name}.bin`);
    if (bytes == null) {
        throw new Error(`Missing shapes/${name}.bin`);
    }

    return bytes;
}

describe('the checked-in cases (run `yarn conformance:generate` to update them)', () => {
    const generated = writeShapesCases(SHAPES_CASES);

    it('should describe the cases that the generated TypeScript writes', () => {
        expect(json).toBe(generated.json);
    });

    it('should hold the tables that the generated TypeScript writes', () => {
        expect(new Set(tables.keys())).toStrictEqual(new Set(generated.tables.keys()));

        for (const [path, bytes] of generated.tables) {
            expect(tables.get(path), path).toStrictEqual(bytes);
        }
    });
});

describe.each(SHAPES)('%s', (_, shapes) => {
    it.each(SHAPES_CASES)('should write $name byte for byte', shapesCase => {
        expect(serialize(shapes, shapesCase)).toStrictEqual(bytesOf(shapesCase.name));
    });

    it.each(SHAPES_CASES)('should read back the fields of $name', ({ name, type, fields }) => {
        expect(read(shapes, type, bytesOf(name))).toEqual(fields);
    });
});
