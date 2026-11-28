// The `tafla-gen` command of the package's build, which runs the generator's
// WebAssembly module, with the cases of the native command's tests in
// rust/gen/tests/cli.rs that a host could get wrong: the files it reads and
// writes, what it prints, and its exit codes.
import { spawnSync } from 'node:child_process';
import {
    existsSync,
    mkdirSync,
    mkdtempSync,
    readdirSync,
    readFileSync,
    writeFileSync
} from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, relative } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'hypotes';

const BIN = fileURLToPath(new URL('../dist/bin.js', import.meta.url));
const REPOSITORY = fileURLToPath(new URL('../../..', import.meta.url));
const DIAGNOSTICS = join(REPOSITORY, 'rust/gen/tests/diagnostics');
const VERSION = (
    JSON.parse(readFileSync(new URL('../package.json', import.meta.url), 'utf8')) as {
        version: string;
    }
).version;

const USAGE =
    'usage: tafla-gen <typescript|javascript|python|java|rust|go|cpp|swift|csharp> <schema>... ' +
    '[--out <dir>] [--runtime <module>] [--package <package>]';

/** The directories of the golden files that tafla-gen generates from `schemas/shapes.tafla` */
const GOLDEN: Array<[target: string, directory: string]> = [
    ['typescript', 'typescript/core/test/generated/shapes/typescript'],
    ['javascript', 'typescript/core/test/generated/shapes/javascript'],
    ['python', 'python/core/tests/generated/shapes'],
    ['rust', 'rust/gen/tests/golden/shapes/rust']
];

interface Output {
    readonly status: number | null;
    readonly stdout: string;
    readonly stderr: string;
}

function taflaGen(directory: string, commandArguments: readonly string[]): Output {
    return spawnSync(process.execPath, [BIN, ...commandArguments], {
        cwd: directory,
        encoding: 'utf8'
    });
}

function temporaryDirectory(): string {
    return mkdtempSync(join(tmpdir(), 'tafla-gen-'));
}

/** A new directory of schemas, by their paths in it */
function writeSchemas(schemas: Record<string, string | Uint8Array>): string {
    const directory = temporaryDirectory();
    for (const [path, source] of Object.entries(schemas)) {
        mkdirSync(dirname(join(directory, path)), { recursive: true });
        writeFileSync(join(directory, path), source);
    }
    return directory;
}

/** Every file under `directory`, by its path, but for Python's cache of modules */
function readTree(directory: string): Record<string, string> {
    if (!existsSync(directory)) {
        return {};
    }

    const files: Array<[string, string]> = [];
    const entries = readdirSync(directory, { recursive: true, withFileTypes: true });
    for (const entry of entries) {
        const path = join(entry.parentPath, entry.name);
        if (entry.isFile() && !path.includes('__pycache__')) {
            files.push([relative(directory, path), readFileSync(path, 'utf8')]);
        }
    }
    return Object.fromEntries(files.sort(([a], [b]) => (a < b ? -1 : 1)));
}

describe('tafla-gen', () => {
    it.each(GOLDEN)(
        'should write the golden files of an example schema for %s',
        (target, golden) => {
            const out = temporaryDirectory();

            const output = taflaGen(REPOSITORY, [target, 'schemas/shapes.tafla', '--out', out]);

            expect(output).toMatchObject({ status: 0, stdout: '', stderr: '' });
            expect(readTree(out)).toStrictEqual(readTree(join(REPOSITORY, golden)));
        }
    );

    it('should write every schema it is given, under the current directory by default', () => {
        const directory = writeSchemas({
            'point.tafla': 'message Point { x: uint8; }\n',
            'nested/pair.tafla': 'message Pair { a: uint8; }\n'
        });

        const output = taflaGen(directory, ['py', 'point.tafla', 'nested/pair.tafla']);

        expect(output).toMatchObject({ status: 0, stdout: '', stderr: '' });
        expect(Object.keys(readTree(directory))).toStrictEqual([
            'nested/pair.tafla',
            'pair.py',
            'point.py',
            'point.tafla'
        ]);
    });

    it('should write the classes of a Java package in its directories', () => {
        const directory = writeSchemas({ 'point.tafla': 'message Point { x: uint8; }\n' });
        const out = temporaryDirectory();

        const output = taflaGen(directory, [
            'java',
            'point.tafla',
            '--package',
            'com.example.shapes',
            '--out',
            out
        ]);

        expect(output).toMatchObject({ status: 0, stderr: '' });
        expect(Object.keys(readTree(out))).toStrictEqual(['com/example/shapes/Point.java']);
    });

    it('should print the errors of an invalid schema and fail', () => {
        const out = temporaryDirectory();

        const output = taflaGen(DIAGNOSTICS, ['rust', 'unknown_types.tafla', '--out', out]);

        expect(output.status).toBe(1);
        expect(output.stderr).toBe(readFileSync(join(DIAGNOSTICS, 'unknown_types.stderr'), 'utf8'));
        expect(readTree(out)).toStrictEqual({});
    });

    it('should write neither of two schemas that write the same file', () => {
        const point = 'message Point { x: uint8; }\n';
        const directory = writeSchemas({
            'a/point.tafla': point,
            'b/point.tafla': point,
            'point.v2.tafla': point,
            'pair.tafla': 'message Pair { a: uint8; }\n'
        });
        const out = temporaryDirectory();

        const output = taflaGen(directory, [
            'py',
            'a/point.tafla',
            'b/point.tafla',
            'point.v2.tafla',
            'pair.tafla',
            '--out',
            out
        ]);

        expect(output.status).toBe(1);
        expect(output.stderr).toBe(
            'a/point.tafla and b/point.tafla both write point.py\n\n' +
                'a/point.tafla and point.v2.tafla both write point.py\n'
        );
        expect(Object.keys(readTree(out))).toStrictEqual(['pair.py']);
    });

    it('should report a schema it cannot read and write the others', () => {
        const out = temporaryDirectory();

        const output = taflaGen(REPOSITORY, [
            'rust',
            'schemas/missing.tafla',
            'schemas/shapes.tafla',
            '--out',
            out
        ]);

        expect(output.status).toBe(1);
        expect(output.stderr).toBe('schemas/missing.tafla: ENOENT: no such file or directory\n');
        expect(Object.keys(readTree(out))).toStrictEqual(['shapes.rs']);
    });

    it('should report a file it cannot write, and fail', () => {
        const directory = writeSchemas({
            'point.tafla': 'message Point { x: uint8; }\n',
            // A file where the directory to write under should be
            out: ''
        });

        const output = taflaGen(directory, ['py', 'point.tafla', '--out', 'out']);

        expect(output.status).toBe(1);
        // Without the path again, which Node's message ends with
        expect(output.stderr).toMatch(/^out\/point\.py: [^'\n]+\n$/);
    });

    it("should reject a schema that isn't UTF-8, at its first such bytes", () => {
        const directory = writeSchemas({
            // A doc comment in Latin-1 rather than UTF-8
            'point.tafla': Buffer.from(
                'message Point {\n    /// café au lait\n    x: float64;\n}\n',
                'latin1'
            )
        });
        const out = temporaryDirectory();

        const output = taflaGen(directory, ['rust', 'point.tafla', '--out', out]);

        expect(output.status).toBe(1);
        expect(output.stderr).toBe(
            "error: the schema isn't UTF-8\n" +
                ' --> point.tafla:2:12\n' +
                '  |\n' +
                '2 |     /// caf� au lait\n' +
                "  |            ^ this isn't a character in UTF-8\n"
        );
        expect(readTree(out)).toStrictEqual({});
    });

    it('should take options before the target, and schemas after a double dash', () => {
        const directory = writeSchemas({ '-point.tafla': 'message Point { x: uint8; }\n' });
        const out = temporaryDirectory();

        const output = taflaGen(directory, ['--out', out, 'py', '--', '-point.tafla']);

        expect(output).toMatchObject({ status: 0, stderr: '' });
        expect(Object.keys(readTree(out))).toStrictEqual(['-point.py']);
    });

    it.each([[['--help']], [['-h']], [['rust', 'schema.tafla', '--help']]])(
        'should print its help to stdout for %j',
        commandArguments => {
            const output = taflaGen(REPOSITORY, commandArguments);

            expect(output).toMatchObject({ status: 0, stderr: '' });
            expect(output.stdout.startsWith(`tafla-gen ${VERSION}\n`)).toBe(true);
            expect(output.stdout).toContain(`\n${USAGE}\n`);
            expect(output.stdout).toContain('\n  2  The arguments are wrong\n');
        }
    );

    it.each(['--version', '-V'])(
        "should print the package's version to stdout for %s",
        argument => {
            const output = taflaGen(REPOSITORY, [argument]);

            expect(output).toStrictEqual(
                expect.objectContaining({
                    status: 0,
                    stdout: `tafla-gen ${VERSION}\n`,
                    stderr: ''
                })
            );
        }
    );

    it.each<[string[], string]>([
        [[], 'no target is given'],
        [['rust'], 'no schema is given'],
        [['rust', 'point.tafla', '--out'], '`--out` needs a value'],
        [['rust', 'point.tafla', '--verbose'], 'unknown option `--verbose`'],
        [
            ['java', 'point.tafla', '--package', '../generated'],
            "`../generated` isn't a Java package, which is identifiers in ASCII separated by dots, such as `com.example.shapes`"
        ]
    ])(
        'should reject the arguments %j with the usage and exit code 2',
        (commandArguments, error) => {
            const directory = writeSchemas({
                'point.tafla': 'message Point { x: uint8; }\n'
            });

            const output = taflaGen(directory, commandArguments);

            expect(output.status).toBe(2);
            expect(output.stdout).toBe('');
            expect(output.stderr).toBe(
                `error: ${error}\n${USAGE}\nRun \`tafla-gen --help\` for more.\n`
            );
            expect(Object.keys(readTree(directory))).toStrictEqual(['point.tafla']);
        }
    );
});
