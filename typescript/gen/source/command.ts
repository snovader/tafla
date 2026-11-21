import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { loadGenerator } from './generator.ts';
import type { GeneratedFile } from './index.ts';
import { Request, type Response } from './wasm-module.ts';

/** What the command prints, and its exit code */
export interface CommandOutput {
    readonly stdout: string;
    readonly stderr: string;
    readonly code: number;
}

/** The schemas that the arguments ask to generate code from, and where to */
interface Generation {
    readonly out: string;
    readonly schemas: string[];
}

interface Generated {
    /** The files of each schema to write, in the order of the schemas */
    readonly fileSets: GeneratedFile[][];
    /** Each error, which ends its last line */
    readonly errors: string[];
}

const FAILURE = 1;

/**
 * Runs the `tafla-gen` command with its arguments, without the command's own
 * name: reads the schemas, writes the generated files, and returns what to
 * print. The generator's module decides everything else, as it does for the
 * command's native binary, so that the two agree.
 */
export function runCommand(commandArguments: readonly string[]): CommandOutput {
    const generation = parse(commandArguments);
    if ('code' in generation) {
        return generation;
    }

    const generated = generate(commandArguments, generation.schemas);
    if ('code' in generated) {
        return generated;
    }

    const errors = generated.errors;
    for (const files of generated.fileSets) {
        const error = writeAll(generation.out, files);
        if (error !== undefined) {
            errors.push(error);
        }
    }

    return {
        stdout: '',
        stderr: errors.join('\n'),
        code: errors.length > 0 ? FAILURE : 0
    };
}

function parse(
    commandArguments: readonly string[]
): CommandOutput | Generation {
    const request = new Request();
    for (const argument of commandArguments) {
        request.string(argument);
    }

    return loadGenerator().call('tafla_command', request, (status, response) =>
        status === 0
            ? { out: response.string(), schemas: response.strings() }
            : readOutput(response)
    );
}

function generate(
    commandArguments: readonly string[],
    schemas: readonly string[]
): CommandOutput | Generated {
    const request = new Request().count(commandArguments.length);
    for (const argument of commandArguments) {
        request.string(argument);
    }
    for (const schema of schemas) {
        addSource(request, schema);
    }

    return loadGenerator().call('tafla_generate', request, (status, response) =>
        status === 0
            ? {
                  fileSets: Array.from({ length: response.count() }, () =>
                      response.files()
                  ),
                  errors: response.strings()
              }
            : readOutput(response)
    );
}

/** Adds a schema's bytes to the request, or why it can't be read */
function addSource(request: Request, schema: string): void {
    let bytes: Uint8Array;
    try {
        bytes = readFileSync(schema);
    } catch (error) {
        request.count(1).string(reasonOf(error));
        return;
    }
    request.count(0).bytes(bytes);
}

/** The response of a command that's over */
function readOutput(response: Response): CommandOutput {
    return {
        code: response.count(),
        stdout: response.string(),
        stderr: response.string()
    };
}

/**
 * Writes a schema's files under `out`, up to the first that fails.
 *
 * @returns The error of that file, if any
 */
function writeAll(
    out: string,
    files: readonly GeneratedFile[]
): string | undefined {
    for (const file of files) {
        const path = join(out, file.path);
        try {
            mkdirSync(dirname(path), { recursive: true });
            writeFileSync(path, file.contents);
        } catch (error) {
            return `${path}: ${reasonOf(error)}\n`;
        }
    }
    return undefined;
}

/**
 * Why a file can't be read or written, in Node's words, without the system
 * call and the file's path that Node's message ends with, since the command
 * prints the path before it
 */
function reasonOf(error: unknown): string {
    if (!(error instanceof Error)) {
        return String(error);
    }

    const { path, syscall } = error as NodeJS.ErrnoException;
    const suffix = `, ${syscall} '${path}'`;
    return error.message.endsWith(suffix)
        ? error.message.slice(0, -suffix.length)
        : error.message;
}
