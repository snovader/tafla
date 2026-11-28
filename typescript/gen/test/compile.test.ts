import { readFileSync } from 'node:fs';
import { compile, CompileError } from '@tafla/gen';
import { describe, expect, it } from 'hypotes';

const REPOSITORY = new URL('../../../', import.meta.url);

describe('compile()', () => {
    it('should generate the golden file of an example schema', () => {
        const source = readFileSync(new URL('schemas/shapes.tafla', REPOSITORY), 'utf8');
        const golden = readFileSync(
            new URL('python/core/tests/generated/shapes/shapes.py', REPOSITORY),
            'utf8'
        );

        const files = compile('schemas/shapes.tafla', source, 'python');

        expect(files).toStrictEqual([{ path: 'shapes.py', contents: golden }]);
    });

    it('should name the files after the schema, and the classes after the package', () => {
        const files = compile('point.tafla', 'message Point { x: float64; }', 'java', {
            package: 'com.example.shapes'
        });

        expect(files.map(file => file.path)).toStrictEqual(['com/example/shapes/Point.java']);
        expect(files[0]?.contents).toContain('package com.example.shapes;');
    });

    it('should import the runtime that the options name', () => {
        const [file] = compile('point.tafla', 'message Point { x: float64; }', 'ts', {
            runtime: '@example/tafla'
        });

        expect(file?.contents).toContain("from '@example/tafla'");
    });

    it("should throw a CompileError of a schema's errors, as the command prints them", () => {
        const source = 'message Point { x: flaot64; }';

        expect(() => compile('point.tafla', source, 'rust')).toThrow(CompileError);
        expect(() => compile('point.tafla', source, 'rust')).toThrow(
            'error: unknown type `flaot64`\n --> point.tafla:1:20\n'
        );
    });

    it('should throw a CompileError of an unknown target', () => {
        // @ts-expect-error A target that the type doesn't name
        const compileCobol = (): unknown => compile('point.tafla', '', 'cobol');

        expect(compileCobol).toThrow(CompileError);
        expect(compileCobol).toThrow('unknown target `cobol`: this build generates');
    });

    it("should throw a CompileError of a package that isn't a Java package", () => {
        const compileIt = (): unknown =>
            compile('point.tafla', 'message Point { x: uint8; }', 'java', {
                package: '../generated'
            });

        expect(compileIt).toThrow(CompileError);
        expect(compileIt).toThrow("`../generated` isn't a Java package");
    });

    it('should generate again after a schema larger than the memory it started with', () => {
        // Grows the module's memory, which detaches a host's views of it
        const comment = `/// ${'tafla '.repeat(1_000_000)}`;
        const large = `${comment}\nmessage Large { x: float64; }`;

        const [largeFile] = compile('large.tafla', large, 'python');
        const [file] = compile('point.tafla', 'message Point { x: float64; }', 'python');

        expect(largeFile?.contents.length).toBeGreaterThan(comment.length);
        expect(file?.path).toBe('point.py');
    });
});
