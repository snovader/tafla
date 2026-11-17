import { readdir, readFile } from 'node:fs/promises';
import { describe, it } from 'node:test';
import { expect } from '../conformance/expect.ts';
import {
    read,
    serialize,
    writeShapesCases,
    SHAPES_CASES,
    SHAPES_DIRECTORY
} from './shapesCases.ts';
import { SHAPES } from './shapesTables.ts';

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
    if (bytes === undefined) {
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

for (const [language, shapes] of SHAPES) {
    describe(language, () => {
        for (const shapesCase of SHAPES_CASES) {
            it(`should write ${shapesCase.name} byte for byte`, () => {
                expect(serialize(shapes, shapesCase)).toStrictEqual(bytesOf(shapesCase.name));
            });
        }

        for (const { name, type, fields } of SHAPES_CASES) {
            it(`should read back the fields of ${name}`, () => {
                expect(read(shapes, type, bytesOf(name))).toEqual(fields);
            });
        }
    });
}
