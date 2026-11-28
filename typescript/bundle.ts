// Bundles the package as it's published, for tests and benchmarks to import
// with the `tafla-bundle` condition: it bundles the package, runs the command
// in its arguments, and removes the bundle. Both processes strip types with
// `@snovader/node-hooks`, as the README describes:
//
//     node --no-strip-types --import @snovader/node-hooks/register bundle.ts \
//         node --no-strip-types --import @snovader/node-hooks/register \
//         --conditions=tafla-bundle benchmarks/comparison/main.ts
import { spawn } from 'node:child_process';
import { rm } from 'node:fs/promises';
import { join } from 'node:path';
import { build } from 'rolldown';
import { bundleOptions } from './core/rolldown.config.ts';

/** The directory of the bundle that the `tafla-bundle` condition resolves */
const BUNDLE_DIRECTORY = join(import.meta.dirname, 'core/.bundle');

/** Writes the bundle, over the one that's there */
async function bundle(): Promise<void> {
    await build(bundleOptions(join(BUNDLE_DIRECTORY, 'index.js')));
}

async function removeBundle(): Promise<void> {
    await rm(BUNDLE_DIRECTORY, { recursive: true, force: true });
}

/** Runs the command with the bundle, and returns its exit code */
async function runWithBundle(command: string[]): Promise<number> {
    const [file, ...args] = command;

    if (file == null) {
        throw new Error('Expected a command to run with the bundle');
    }

    await bundle();

    try {
        return await run(file, args);
    } finally {
        await removeBundle();
    }
}

function run(file: string, args: string[]): Promise<number> {
    // The command receives Ctrl-C itself, and this script waits for it to
    // exit, so that the bundle is still removed
    const ignore = (): void => {};
    process.on('SIGINT', ignore);

    const child = spawn(file, args, { stdio: 'inherit' });

    return new Promise((resolve, reject) => {
        child.on('error', reject);
        child.on('exit', code => {
            process.off('SIGINT', ignore);
            resolve(code ?? 1);
        });
    });
}

if (import.meta.main) {
    process.exitCode = await runWithBundle(process.argv.slice(2));
}
