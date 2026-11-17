import { execFile } from 'node:child_process';
import { readFileSync } from 'node:fs';
import process from 'node:process';
import { fileURLToPath } from 'node:url';
import { promisify } from 'node:util';
import { describe, expect, it } from 'test-runner';

interface Result {
    readonly name: string;
    readonly passed: boolean;
    readonly skipped: boolean;
    readonly todo: boolean;
    readonly file: string | undefined;
    readonly line: number | undefined;
    readonly error: string;
}

const RESULTS = fileURLToPath(new URL('fixtures/results.ts', import.meta.url));
const DECLARATIONS = fileURLToPath(new URL('fixtures/declarations.ts', import.meta.url));
const ONLY = fileURLToPath(new URL('fixtures/only.ts', import.meta.url));

/**
 * The results of a file's tests and suites, by title, from a run of it in
 * another process: node:test runs no files from a test file's
 */
async function resultsOf(file: string, only = false): Promise<Map<string, Result>> {
    const env = { ...process.env };
    // Which tells node:test that it runs in a test file's process
    delete env.NODE_TEST_CONTEXT;

    const args = [RESULTS, file, ...(only ? ['only'] : [])];
    const { stdout } = await promisify(execFile)(process.execPath, args, { env });
    const results = JSON.parse(stdout) as Result[];

    return new Map(results.map(result => [result.name, result]));
}

/** The line of the fixture on which the text first appears */
function lineOf(text: string): number {
    const lines = readFileSync(DECLARATIONS, 'utf8').split('\n');
    return lines.findIndex(line => line.includes(text)) + 1;
}

const [results, onlyResults] = await Promise.all([resultsOf(DECLARATIONS), resultsOf(ONLY, true)]);

/** The result of the test or suite of the title, which ran */
function resultOf(title: string): Result {
    const result = results.get(title);
    expect(result, title).toBeDefined();
    return result as Result;
}

describe('declarations', () => {
    it.each([
        'each spreads 1 and a',
        'each spreads 2 and b',
        'each passes row 0 whole unless every row is an array',
        'each passes row 1 whole unless every row is an array',
        'each names a with -0',
        'for passes [1, 2] whole, and the context',
        'skipIf runs if false',
        'runIf runs if true',
        'test is it',
        'describe.each A',
        'declares tests in the suite',
        'hooks',
        'hooks run in the order that node:test runs them',
        'vi.fn records calls for the matchers',
        'prop takes parameters',
        'prop ran 7 times',
        'prop takes a record',
        'each is on the property API too, 1'
    ])('should run and pass "%s"', title => {
        expect(resultOf(title)).toMatchObject({ passed: true, skipped: false, todo: false });
    });

    it.each([
        'skip skips',
        'skip.each skips 1',
        'skipIf skips if true',
        'runIf skips if false',
        'options before the function apply',
        'describe.skip skips',
        'skip.prop skips'
    ])('should skip "%s"', title => {
        expect(resultOf(title)).toMatchObject({ skipped: true });
    });

    it.each(['todo is to do', 'a test without a function is to do'])(
        'should mark "%s" as to do',
        title => {
            expect(resultOf(title)).toMatchObject({ todo: true });
        }
    );

    it.each([
        ['a timeout after the function applies', 'timed out after 10ms'],
        ['prop awaits an async predicate', 'Property failed'],
        ['prop rejects a promise from a function that is not async', 'returned a promise'],
        ['prop fails on false', 'Property failed']
    ])('should fail "%s"', (title, message) => {
        const result = resultOf(title);

        expect(result.passed).toBe(false);
        expect(result.error).toContain(message);
    });

    it.each([
        ['each spreads 1 and a', "])('each spreads %i and %s'"],
        ['skip skips', "it.skip('skip skips'"],
        ['test is it', "test('test is it'"],
        ['prop takes parameters', 'property.prop([fc.nat()], { numRuns: 7 })(']
    ])('should report "%s" where the test file declares it', (title, declaration) => {
        expect(resultOf(title)).toMatchObject({ file: DECLARATIONS, line: lineOf(declaration) });
    });

    it('should run only the tests marked only, when asked to', () => {
        expect(onlyResults.get('only runs')?.passed).toBe(true);
        expect(onlyResults.get('the others are skipped')?.passed).not.toBe(false);
    });
});
