import { mkdir, rm, writeFile } from 'node:fs/promises';
import { SUITE } from './cases.ts';
import { writeSuite, SUITE_DIRECTORY } from './files.ts';

// Writes the conformance suite to its directory at the root of the
// repository; run it with `yarn conformance:generate`.

const directory = SUITE_DIRECTORY;
const { json, tables } = writeSuite(SUITE);

// The tables are written afresh, so that a renamed or removed case leaves no
// file behind
for (const subdirectory of ['valid/', 'equivalent/', 'invalid/']) {
    const url = new URL(subdirectory, directory);
    await rm(url, { recursive: true, force: true });
    await mkdir(url);
}

await writeFile(new URL('cases.json', directory), json);
for (const [path, bytes] of tables) {
    await writeFile(new URL(path, directory), bytes);
}
