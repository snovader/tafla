import { mkdir, rm, writeFile } from 'node:fs/promises';
import { SUITE } from './cases.ts';
import type { Suite } from './description.ts';
import { writeSuite, PRESENCE_DIRECTORY, SUITE_DIRECTORY } from './files.ts';
import { PRESENCE_SUITE } from './presence-cases.ts';

// Writes the conformance suite to its directory at the root of the
// repository; run it with `yarn conformance:generate`.

await writeFiles(SUITE, SUITE_DIRECTORY, ['valid/', 'equivalent/', 'invalid/']);
await mkdir(PRESENCE_DIRECTORY, { recursive: true });
await writeFiles(PRESENCE_SUITE, PRESENCE_DIRECTORY, ['valid/', 'invalid/']);

async function writeFiles(
    suite: Suite,
    directory: URL,
    subdirectories: string[]
): Promise<void> {
    const { json, tables } = writeSuite(suite);

    // The tables are written afresh, so that a renamed or removed case
    // leaves no file behind
    for (const subdirectory of subdirectories) {
        const url = new URL(subdirectory, directory);
        await rm(url, { recursive: true, force: true });
        await mkdir(url);
    }

    await writeFile(new URL('cases.json', directory), json);
    for (const [path, bytes] of tables) {
        await writeFile(new URL(path, directory), bytes);
    }
}
