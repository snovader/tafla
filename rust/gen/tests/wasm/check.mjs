// Checks the generator's WebAssembly module: it has no imports, so it runs
// with an empty import object, and its `tafla_compile` export generates the
// golden files of every example schema and renders the errors of every
// invalid schema in tests/diagnostics, as the native library does. Its
// `tafla_command` and `tafla_generate` exports run the command for a host
// that reads the schemas and writes the files, its subcommands `describe` and
// `check` included, as tests/cli.rs checks of the native command.
//
// node gen/tests/wasm/check.mjs <tafla_gen.wasm>

import { existsSync, readdirSync, readFileSync, statSync } from 'node:fs';
import { join, relative } from 'node:path';
import { fileURLToPath } from 'node:url';

const TESTS = fileURLToPath(new URL('..', import.meta.url));
const REPOSITORY = join(TESTS, '..', '..', '..');
const SCHEMAS = join(REPOSITORY, 'schemas');
const DIAGNOSTICS = join(TESTS, 'diagnostics');

/**
 * Where each target's golden files lie: a directory of each example schema
 * under `root`, and `subdirectory` of it when targets share the root, as
 * `GOLDEN_ROOTS` in tests/common/mod.rs lists them
 *
 * @type {Record<string, { root: string, subdirectory?: string }>}
 */
const GOLDEN_ROOTS = {
    typescript: {
        root: 'typescript/core/test/generated',
        subdirectory: 'typescript'
    },
    javascript: {
        root: 'typescript/core/test/generated',
        subdirectory: 'javascript'
    },
    python: { root: 'python/core/tests/generated' },
    java: {
        root: 'java/core/src/test/java/io/github/snovader/tafla/test/generated'
    },
    rust: { root: 'rust/gen/tests/golden', subdirectory: 'rust' },
    go: { root: 'go/tests/generated' },
    cpp: { root: 'cpp/tests/generated' },
    swift: { root: 'swift/Tests/Generated' },
    csharp: { root: 'csharp/tests/Tafla.Generated' }
};

/** The Java test module's package of the classes of each example schema */
const JAVA_PACKAGE = 'io.github.snovader.tafla.test.generated';

/**
 * The golden files of a schema's code for a target: their directory, the
 * Java package that the generator takes, or empty for the default, and the
 * directory of the generated paths that the directory stands for
 *
 * @param {string} schema
 * @param {string} target
 * @returns {{ directory: string, packageName: string, prefix: string }}
 */
function goldenOf(schema, target) {
    const { root, subdirectory = '' } = GOLDEN_ROOTS[target];
    const directory = join(REPOSITORY, root, schema, subdirectory);

    if (target !== 'java') {
        return { directory, packageName: '', prefix: '' };
    }

    const packageName = `${JAVA_PACKAGE}.${schema}`;
    return {
        directory,
        packageName,
        prefix: `${packageName.replaceAll('.', '/')}/`
    };
}

const encoder = new TextEncoder();
const decoder = new TextDecoder();

/**
 * Calls one of the module's operations: a request in memory from
 * `tafla_alloc`, and a response that starts with its own length and a
 * status, which `tafla_free` frees once `read` has read it.
 *
 * @template Result
 * @param {WebAssembly.Instance} instance
 * @param {'tafla_compile' | 'tafla_command' | 'tafla_generate'} operation
 * @param {Array<number | string | Uint8Array>} parts the request's `u32`
 * counts, and its strings, of which bytes needn't be UTF-8
 * @param {(status: number, response: { count: () => number, string: () => string }) => Result} read
 * @returns {Result}
 */
function send(instance, operation, parts, read) {
    const { memory, tafla_alloc, tafla_free } = instance.exports;
    const encoded = parts.map(part =>
        typeof part === 'string' ? encoder.encode(part) : part
    );
    const length = encoded.reduce(
        (sum, part) => sum + 4 + (typeof part === 'number' ? 0 : part.length),
        0
    );

    const request = tafla_alloc(length);
    // Allocating may grow the memory, which detaches earlier views of it
    let view = new DataView(memory.buffer);
    let offset = request;
    for (const part of encoded) {
        if (typeof part === 'number') {
            view.setUint32(offset, part, true);
            offset += 4;
        } else {
            view.setUint32(offset, part.length, true);
            new Uint8Array(memory.buffer, offset + 4, part.length).set(part);
            offset += 4 + part.length;
        }
    }

    const response = instance.exports[operation](request, length);
    view = new DataView(memory.buffer);
    const responseLength = view.getUint32(response, true);
    const status = view.getUint32(response + 4, true);
    offset = response + 8;
    const count = () => {
        const value = view.getUint32(offset, true);
        offset += 4;
        return value;
    };
    const string = () => {
        const byteLength = count();
        const bytes = new Uint8Array(memory.buffer, offset, byteLength);
        offset += byteLength;
        return decoder.decode(bytes);
    };

    try {
        return read(status, { count, string });
    } finally {
        tafla_free(response, responseLength);
    }
}

/**
 * @param {{ count: () => number, string: () => string }} response
 * @returns {Map<string, string>} a count of files, by their paths
 */
function readFiles(response) {
    const files = new Map();
    const count = response.count();
    for (let i = 0; i < count; i++) {
        const path = response.string();
        files.set(path, response.string());
    }
    return files;
}

/**
 * Compiles through `tafla_compile`, whose request is five strings
 *
 * @param {WebAssembly.Instance} instance
 * @param {string[]} strings
 * @returns {{ status: number, files?: Map<string, string>, errors?: string }}
 */
function call(instance, strings) {
    return send(instance, 'tafla_compile', strings, (status, response) =>
        status === 0
            ? { status, files: readFiles(response) }
            : { status, errors: response.string() }
    );
}

/**
 * The response of a command that's over
 *
 * @param {{ count: () => number, string: () => string }} response
 */
function readOutput(response) {
    return {
        code: response.count(),
        stdout: response.string(),
        stderr: response.string()
    };
}

/**
 * Parses the command's arguments through `tafla_command`
 *
 * @param {WebAssembly.Instance} instance
 * @param {string[]} commandArguments
 */
function command(instance, commandArguments) {
    return send(
        instance,
        'tafla_command',
        commandArguments,
        (status, response) => {
            if (status !== 0) {
                return { status, ...readOutput(response) };
            }

            const out = response.string();
            const schemas = Array.from(
                { length: response.count() },
                response.string
            );
            return { status, out, schemas };
        }
    );
}

/**
 * Generates through `tafla_generate` from the schemas that a host read
 *
 * @param {WebAssembly.Instance} instance
 * @param {string[]} commandArguments
 * @param {Array<Uint8Array | string | { error: string }>} sources each
 * schema's bytes or source, or why it can't be read
 */
function generate(instance, commandArguments, sources) {
    const parts = [commandArguments.length, ...commandArguments];
    for (const source of sources) {
        if (typeof source === 'object' && 'error' in source) {
            parts.push(1, source.error);
        } else {
            parts.push(0, source);
        }
    }

    return send(instance, 'tafla_generate', parts, (status, response) => {
        if (status !== 0) {
            return { status, ...readOutput(response) };
        }

        const fileSets = Array.from({ length: response.count() }, () => [
            ...readFiles(response).keys()
        ]);
        const errors = Array.from(
            { length: response.count() },
            response.string
        );
        return { status, fileSets, errors };
    });
}

/**
 * @returns {Map<string, string>} every file under `directory`, by path, but
 * for Python's cache of the modules that its tests import
 */
function readTree(directory) {
    const files = new Map();
    if (!existsSync(directory)) {
        return files;
    }

    const visit = current => {
        for (const name of readdirSync(current).sort()) {
            const path = join(current, name);
            if (name === '__pycache__') {
                continue;
            }
            if (statSync(path).isDirectory()) {
                visit(path);
            } else {
                files.set(
                    relative(directory, path),
                    readFileSync(path, 'utf8')
                );
            }
        }
    };
    visit(directory);
    return files;
}

/** @returns {string[]} the names of the files in `directory` with `extension` */
function namesWith(directory, extension) {
    return readdirSync(directory)
        .filter(name => name.endsWith(extension))
        .map(name => name.slice(0, -extension.length))
        .sort();
}

const failures = [];
const fail = message => failures.push(message);

const wasm = readFileSync(process.argv[2]);
const module = new WebAssembly.Module(wasm);
const imports = WebAssembly.Module.imports(module);
if (imports.length > 0) {
    fail(`the module imports ${JSON.stringify(imports)}`);
}
const instance = new WebAssembly.Instance(module, {});

let compiles = 0;
for (const schema of namesWith(SCHEMAS, '.tafla')) {
    const source = readFileSync(join(SCHEMAS, `${schema}.tafla`), 'utf8');

    for (const target of Object.keys(GOLDEN_ROOTS)) {
        const fileName = `schemas/${schema}.tafla`;
        const { directory, packageName, prefix } = goldenOf(schema, target);
        const response = call(instance, [
            target,
            fileName,
            source,
            '',
            packageName
        ]);
        compiles++;
        if (response.status !== 0) {
            fail(`${fileName} for ${target}: ${response.errors}`);
            continue;
        }

        const golden = readTree(directory);
        const files = new Map(
            [...response.files].map(([path, contents]) => [
                path.startsWith(prefix) ? path.slice(prefix.length) : path,
                contents
            ])
        );
        const paths = [...files.keys()].sort();
        if (JSON.stringify(paths) !== JSON.stringify([...golden.keys()])) {
            fail(`${fileName} for ${target} generates ${paths.join(', ')}`);
            continue;
        }
        for (const [path, contents] of files) {
            if (contents !== golden.get(path)) {
                fail(`${fileName} for ${target}: ${path} differs`);
            }
        }
    }
}

for (const name of namesWith(DIAGNOSTICS, '.tafla')) {
    const fileName = `${name}.tafla`;
    const source = readFileSync(join(DIAGNOSTICS, fileName), 'utf8');
    const expected = readFileSync(join(DIAGNOSTICS, `${name}.stderr`), 'utf8');

    const response = call(instance, ['rust', fileName, source, '', '']);
    compiles++;
    if (response.status !== 1 || response.errors !== expected) {
        fail(`${fileName}: ${response.errors}`);
    }
}

const unknown = call(instance, ['cobol', 'point.tafla', '', '', '']);
if (
    unknown.status !== 1 ||
    !unknown.errors.startsWith('unknown target `cobol`: this build generates')
) {
    fail(`an unknown target: ${JSON.stringify(unknown)}`);
}

const invalidPackage = call(instance, [
    'java',
    'point.tafla',
    'message Point { x: uint8; }',
    '',
    '../generated'
]);
if (
    invalidPackage.status !== 1 ||
    !invalidPackage.errors.startsWith("`../generated` isn't a Java package")
) {
    fail(`an invalid package: ${JSON.stringify(invalidPackage)}`);
}

// Two strings rather than five
const malformed = call(instance, ['rust', 'point.tafla']);
if (malformed.status !== 1 || malformed.errors !== 'malformed request\n') {
    fail(`a malformed request: ${JSON.stringify(malformed)}`);
}

/**
 * Fails unless `actual` is `expected`, compared as JSON
 *
 * @param {string} what
 * @param {unknown} actual
 * @param {unknown} expected
 */
function expectEqual(what, actual, expected) {
    if (JSON.stringify(actual) !== JSON.stringify(expected)) {
        fail(`${what}: ${JSON.stringify(actual)}`);
    }
}

const POINT = 'message Point { x: uint8; }\n';
const USAGE =
    'usage: tafla-gen <typescript|javascript|python|java|rust|go|cpp|swift|csharp> <schema>... ' +
    '[--out <dir>] [--runtime <module>] [--package <package>]\n' +
    '       tafla-gen describe <schema>...\n' +
    '       tafla-gen check <schema> --against <old schema>';

// The directory to write under and the schemas to read, each once
expectEqual(
    'the schemas of a command',
    command(instance, [
        '--out',
        'generated',
        'py',
        'a.tafla',
        'b.tafla',
        'a.tafla'
    ]),
    { status: 0, out: 'generated', schemas: ['a.tafla', 'b.tafla'] }
);
expectEqual('the default directory', command(instance, ['py', 'a.tafla']), {
    status: 0,
    out: '.',
    schemas: ['a.tafla']
});

const version = command(instance, ['rust', 'point.tafla', '--version']);
if (
    version.status !== 1 ||
    version.code !== 0 ||
    !/^tafla-gen \d+\.\d+\.\d+\n$/.test(version.stdout) ||
    version.stderr !== ''
) {
    fail(`the version: ${JSON.stringify(version)}`);
}
const help = command(instance, ['-h']);
if (help.code !== 0 || !help.stdout.includes(`\n${USAGE}\n`)) {
    fail(`the help: ${JSON.stringify(help)}`);
}

const usageError = {
    status: 1,
    code: 2,
    stdout: '',
    stderr: `error: no schema is given\n${USAGE}\nRun \`tafla-gen --help\` for more.\n`
};
expectEqual('wrong arguments', command(instance, ['rust']), usageError);
// The same answer where a host should have stopped at the first
expectEqual(
    'generating from wrong arguments',
    generate(instance, ['rust'], []),
    usageError
);

// A schema's files, another's errors, one that can't be read, one that isn't
// UTF-8 and two that write the same file, in the order of the command's
// errors
const generated = generate(
    instance,
    [
        'py',
        'pair.tafla',
        'bad.tafla',
        'missing.tafla',
        'latin.tafla',
        'a/point.tafla',
        'b/point.tafla'
    ],
    [
        'message Pair { a: uint8; }\n',
        'message Point { x: flaot64; }\n',
        { error: 'No such file' },
        new Uint8Array([0x2f, 0x2f, 0x20, 0xe9, 0x0a]),
        POINT,
        encoder.encode(POINT)
    ]
);
expectEqual('the files of a command', generated.fileSets, [['pair.py']]);
expectEqual(
    'the errors of a command',
    generated.errors?.map(error => error.split('\n')[0]),
    [
        'error: unknown type `flaot64`',
        'missing.tafla: No such file',
        "error: the schema isn't UTF-8",
        'a/point.tafla and b/point.tafla both write point.py'
    ]
);
if (generated.errors?.some(error => !error.endsWith('\n'))) {
    fail(`an error doesn't end its line: ${JSON.stringify(generated.errors)}`);
}

// A subcommand names its schemas as a generation does, the schema to check
// before the old one, and prints rather than writes
expectEqual(
    'the schemas of a subcommand',
    command(instance, ['check', '--against', 'old.tafla', 'new.tafla']),
    { status: 0, out: '.', schemas: ['new.tafla', 'old.tafla'] }
);
const described = generate(instance, ['describe', 'point.tafla'], [POINT]);
if (
    described.status !== 1 ||
    described.code !== 0 ||
    described.stderr !== '' ||
    JSON.parse(described.stdout ?? '').schemas[0].definitions[0].name !==
        'Point'
) {
    fail(`a description: ${JSON.stringify(described)}`);
}
expectEqual(
    'a check',
    generate(
        instance,
        ['check', 'point.tafla', '--against', 'old.tafla'],
        ['message Point { x: uint16; }\n', POINT]
    ),
    {
        status: 1,
        code: 1,
        stdout: '',
        stderr:
            'error: `Point` changes, and has no version\n' +
            ' --> point.tafla:1:9\n' +
            '  |\n' +
            '1 | message Point { x: uint16; }\n' +
            '  |         ^^^^^ `x` changes, and only the version tells its old tables from its new ones\n' +
            '\n' +
            'error: `x` changes its type from `uint8` to `uint16`\n' +
            ' --> point.tafla:1:20\n' +
            '  |\n' +
            '1 | message Point { x: uint16; }\n' +
            '  |                    ^^^^^^ a field keeps its index and its type, and new fields come after the old ones\n'
    }
);
expectEqual(
    'a check of a schema that cannot be read',
    generate(
        instance,
        ['check', 'point.tafla', '--against', 'old.tafla'],
        [POINT, { error: 'No such file' }]
    ),
    { status: 1, code: 1, stdout: '', stderr: 'old.tafla: No such file\n' }
);
expectEqual(
    'a subcommand without its schemas',
    generate(instance, ['describe', 'point.tafla'], []),
    {
        status: 1,
        code: 2,
        stdout: '',
        stderr: 'malformed request\n'
    }
);

// A schema too few, a schema too many, and a source that's neither read nor
// failed
const malformedCommand = {
    status: 1,
    code: 2,
    stdout: '',
    stderr: 'malformed request\n'
};
for (const [what, sources] of [
    ['too few schemas', []],
    ['too many schemas', [POINT, POINT]]
]) {
    expectEqual(
        what,
        generate(instance, ['py', 'point.tafla'], sources),
        malformedCommand
    );
}
expectEqual(
    'an unknown kind of source',
    send(
        instance,
        'tafla_generate',
        [2, 'py', 'point.tafla', 2, POINT],
        (status, response) => ({
            status,
            ...readOutput(response)
        })
    ),
    malformedCommand
);
// A count of arguments beyond the request
expectEqual(
    'a count beyond the request',
    send(
        instance,
        'tafla_generate',
        [0xffffffff, 'py'],
        (status, response) => ({
            status,
            ...readOutput(response)
        })
    ),
    malformedCommand
);

if (failures.length > 0) {
    console.error(failures.join('\n'));
    process.exit(1);
}
console.log(
    `${wasm.length} bytes, no imports, ${compiles} compiles match the native output, and the command runs`
);
