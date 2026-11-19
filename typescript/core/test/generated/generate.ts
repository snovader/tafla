import { mkdir, rm, writeFile } from 'node:fs/promises';
import {
    writeShapesCases,
    SHAPES_CASES,
    SHAPES_DIRECTORY
} from './shapes-cases.ts';

// Writes the cases of the generated code to conformance/generated at the root
// of the repository; `yarn conformance:generate` runs it after the suite's
// own generator.

const directory = SHAPES_DIRECTORY;
const { json, tables } = writeShapesCases(SHAPES_CASES);

// The tables are written afresh, so that a renamed or removed case leaves no
// file behind
const tablesDirectory = new URL('shapes/', directory);
await rm(tablesDirectory, { recursive: true, force: true });
await mkdir(tablesDirectory, { recursive: true });

await writeFile(new URL('shapes.json', directory), json);
for (const [path, bytes] of tables) {
    await writeFile(new URL(path, directory), bytes);
}
