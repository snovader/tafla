import { mkdir, rm, writeFile } from 'node:fs/promises';
import { PROPOSED_SUITE, SUITE } from './cases.ts';
import type { Suite } from './description.ts';
import { writeSuite, PROPOSED_DIRECTORY, SUITE_DIRECTORY } from './files.ts';

// Writes the conformance suite to its directory at the root of the
// repository, and the cases of what's proposed for the format to
// `proposed/` in it; run it with `yarn conformance:generate`.

await writeFiles(SUITE, SUITE_DIRECTORY);
await writeFiles(PROPOSED_SUITE, PROPOSED_DIRECTORY);

async function writeFiles(suite: Suite, directory: URL): Promise<void> {
    const { json, tables } = writeSuite(suite);

    // The tables are written afresh, so that a renamed or removed case
    // leaves no file behind
    for (const subdirectory of ['valid/', 'equivalent/', 'invalid/']) {
        const url = new URL(subdirectory, directory);
        await rm(url, { recursive: true, force: true });
        await mkdir(url, { recursive: true });
    }

    await writeFile(new URL('cases.json', directory), json);
    for (const [path, bytes] of tables) {
        await writeFile(new URL(path, directory), bytes);
    }
}
