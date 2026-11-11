// Checks the generator's WebAssembly module: it has no imports, so it runs
// with an empty import object, and its `tafla_compile` export generates the
// golden files of every example schema and renders the errors of every
// invalid schema in tests/diagnostics, as the native library does.
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
    go: { root: 'go/tests/generated' }
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
 * Compiles through the module's exports: a request of length-prefixed
 * strings in memory from `tafla_alloc`, and a response that starts with its
 * own length and a status, which `tafla_free` frees.
 *
 * @param {WebAssembly.Instance} instance
 * @param {string[]} strings
 * @returns {{ status: number, files?: Map<string, string>, errors?: string }}
 */
function call(instance, strings) {
    const { memory, tafla_alloc, tafla_compile, tafla_free } = instance.exports;
    const encoded = strings.map(string => encoder.encode(string));
    const length = encoded.reduce((sum, bytes) => sum + 4 + bytes.length, 0);

    const request = tafla_alloc(length);
    // Allocating may grow the memory, which detaches earlier views of it
    let view = new DataView(memory.buffer);
    let offset = request;
    for (const bytes of encoded) {
        view.setUint32(offset, bytes.length, true);
        new Uint8Array(memory.buffer, offset + 4, bytes.length).set(bytes);
        offset += 4 + bytes.length;
    }

    const response = tafla_compile(request, length);
    view = new DataView(memory.buffer);
    const responseLength = view.getUint32(response, true);
    const status = view.getUint32(response + 4, true);
    offset = response + 8;
    const readString = () => {
        const byteLength = view.getUint32(offset, true);
        const bytes = new Uint8Array(memory.buffer, offset + 4, byteLength);
        offset += 4 + byteLength;
        return decoder.decode(bytes);
    };

    try {
        if (status !== 0) {
            return { status, errors: readString() };
        }

        const count = view.getUint32(offset, true);
        offset += 4;
        const files = new Map();
        for (let i = 0; i < count; i++) {
            const path = readString();
            files.set(path, readString());
        }
        return { status, files };
    } finally {
        tafla_free(response, responseLength);
    }
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

if (failures.length > 0) {
    console.error(failures.join('\n'));
    process.exit(1);
}
console.log(
    `${wasm.length} bytes, no imports, ${compiles} compiles match the native output`
);
