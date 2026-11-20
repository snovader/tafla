import { loadGenerator } from './generator.ts';
import { Request, type Response } from './wasm-module.ts';

/** A language that tafla-gen generates code for, by its name or abbreviation */
export type Target =
    | 'cpp'
    | 'cs'
    | 'csharp'
    | 'go'
    | 'java'
    | 'javascript'
    | 'js'
    | 'py'
    | 'python'
    | 'rs'
    | 'rust'
    | 'swift'
    | 'ts'
    | 'typescript';

export interface CompileOptions {
    /**
     * The module, package or crate that the generated code imports tafla's
     * runtime from, by default the target's own, such as `tafla`
     */
    readonly runtime?: string;
    /**
     * The package of the Java classes, by default `generated`, and the
     * namespace of the C++ and the C#, by default the schema's name. Other
     * targets ignore it.
     */
    readonly package?: string;
}

export interface GeneratedFile {
    /** Relative to the directory that the files are written under, and within it */
    readonly path: string;
    readonly contents: string;
}

/**
 * A schema's errors, an unknown target, or a package that isn't a Java
 * package. Its message is the errors as the `tafla-gen` command prints them.
 */
export class CompileError extends Error {
    public override readonly name = 'CompileError';
}

/**
 * Generates the code for a schema, as the `tafla-gen` command does for each
 * schema it's given, without reading or writing a file.
 *
 * @param fileName The schema's file name, which names the generated files
 *   and the schema in their headers and in errors
 * @param source The schema
 * @throws {CompileError} If the schema has errors, or the target or the
 *   package is invalid
 */
export function compile(
    fileName: string,
    source: string,
    target: Target,
    options: CompileOptions = {}
): GeneratedFile[] {
    const request = new Request()
        .string(target)
        .string(fileName)
        .string(source)
        // An empty runtime or package takes the default
        .string(options.runtime ?? '')
        .string(options.package ?? '');

    return loadGenerator().call(
        'tafla_compile',
        request,
        (status, response) => {
            if (status !== 0) {
                throw new CompileError(response.string());
            }
            return readFiles(response);
        }
    );
}

/** A count of files and each file's path and contents */
export function readFiles(response: Response): GeneratedFile[] {
    return Array.from({ length: response.count() }, () => ({
        path: response.string(),
        contents: response.string()
    }));
}
