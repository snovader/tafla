// Builds the addon with Cargo, and copies it beside the package's sources as
// `tafla-native.node`, the file that `source/index.ts` loads:
//
//     node build.ts
import { execFileSync } from 'node:child_process';
import { copyFileSync } from 'node:fs';
import { join } from 'node:path';

/** The library that Cargo builds from the crate, by `process.platform` */
const LIBRARIES: Partial<Record<NodeJS.Platform, string>> = {
    darwin: 'libtafla_native.dylib',
    linux: 'libtafla_native.so',
    win32: 'tafla_native.dll'
};

const directory = import.meta.dirname;
const library = LIBRARIES[process.platform];

if (library === undefined) {
    throw new Error(`No build of the addon for ${process.platform}`);
}

execFileSync('cargo', ['build', '--release', '--locked'], {
    cwd: directory,
    stdio: 'inherit'
});

copyFileSync(
    join(directory, 'target/release', library),
    join(directory, 'tafla-native.node')
);
