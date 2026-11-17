import plugin from './plugin.ts';
import { tester } from './ruleTester.ts';

// What the rules recognize: tests, describe blocks and expect() from
// test-runner, and tests with `prop` from its fast-check subpath
const IMPORTS = [
    "import { describe, expect, suite } from 'test-runner';",
    "import { it, test } from 'test-runner/fast-check';"
].join('\n');

/** Code that imports what the rules recognize */
function imported(code: string): string {
    return `${IMPORTS}\n${code}`;
}

const NO_ASSERTION =
    'Expected the test to call expect(), or a function whose name starts with expect';

tester.run('expect-expect', plugin.rules['expect-expect'], {
    valid: [
        imported("it('asserts', () => { expect(1).toBe(1); });"),
        imported("test('asserts', { skip: true }, () => { expect(1).toBe(1); });"),
        imported("it('asserts through a helper', () => { expectOne(1); });"),
        imported(
            "it('asserts in a callback', () => { [1].forEach(value => { expect(value).toBe(1); }); });"
        ),
        imported("it.prop([arbitrary])('asserts', value => { expect(value).toBe(value); });"),
        imported("it.each([1])('asserts %i', value => { expect(value).toBe(1); });"),
        imported("it.for([1])('asserts %i', (value, context) => { expect(value).toBe(1); });"),
        imported("it.skipIf(skipped).only('asserts', () => { expect(1).toBe(1); });"),
        imported("it.todo('is to be written');"),
        imported("it('runs a function declared elsewhere', check);"),
        imported("describe('has no assertions of its own', () => {});"),
        // An `it` that isn't test-runner's
        "function it(name: string, run: () => void): void { run(); }\nit('runs', () => {});"
    ],
    invalid: [
        {
            name: 'a test that asserts nothing',
            code: imported("it('asserts nothing', () => {});"),
            errors: [{ message: NO_ASSERTION }]
        },
        {
            name: 'a test in a describe block',
            code: imported(
                "describe('a block', () => { test('computes', () => { const value = 1; }); });"
            ),
            errors: [{ message: NO_ASSERTION }]
        },
        {
            name: 'a variant of a test',
            code: imported("it.only('asserts nothing', function () {});"),
            errors: [{ message: NO_ASSERTION }]
        },
        {
            name: 'a property that returns a boolean',
            code: imported("it.prop([arbitrary])('holds', value => value >= 0);"),
            errors: [{ message: NO_ASSERTION }]
        },
        {
            name: 'a test of each row',
            code: imported("it.skip.each([1, 2])('asserts nothing about %i', value => {});"),
            errors: [{ message: NO_ASSERTION }]
        },
        {
            name: 'a test that runs on a condition',
            code: imported("test.runIf(enabled)('asserts nothing', () => {});"),
            errors: [{ message: NO_ASSERTION }]
        },
        {
            name: 'a test imported under another name',
            code: "import { it as check } from 'test-runner';\ncheck('asserts nothing', () => {});",
            errors: [{ message: NO_ASSERTION }]
        },
        {
            name: "a test of node:test's",
            code: "import { it } from 'node:test';\nit('asserts nothing', () => {});",
            errors: [{ message: NO_ASSERTION }]
        },
        {
            name: 'a call of an expect helper beside the test',
            code: imported("expectOne(1);\nit('asserts nothing', () => {});"),
            errors: [{ message: NO_ASSERTION }]
        }
    ]
});

const STANDALONE =
    'Expected expect() in a test, or in a function of its own, rather than in a describe block or at the top of the module';

tester.run('no-standalone-expect', plugin.rules['no-standalone-expect'], {
    valid: [
        imported("it('asserts', () => { expect(1).toBe(1); });"),
        imported("it.prop([arbitrary])('asserts', value => { expect(value).toBe(value); });"),
        imported("it.each([1])('asserts %i', value => { expect(value).toBe(1); });"),
        imported(
            "test('asserts in a callback', () => { [1].forEach(value => { expect(value).toBe(1); }); });"
        ),
        imported('function expectOne(value: number): void { expect(value).toBe(1); }'),
        imported(
            "describe('a block', () => { const check = (): void => { expect(1).toBe(1); }; it('checks', check); });"
        ),
        // An asymmetric matcher, rather than an assertion
        imported('const ANY_NUMBER = expect.any(Number);'),
        // An `expect` that isn't imported
        'declare function expect(value: unknown): void;\nexpect(1);'
    ],
    invalid: [
        {
            name: 'an assertion at the top of the module',
            code: imported('expect(1).toBe(1);'),
            errors: [{ message: STANDALONE }]
        },
        {
            name: 'an assertion in a describe block',
            code: imported("describe('a block', () => { expect(1).toBe(1); });"),
            errors: [{ message: STANDALONE }]
        },
        {
            name: 'an assertion in a suite',
            code: imported("suite('a suite', () => { expect(1).toBe(1); });"),
            errors: [{ message: STANDALONE }]
        },
        {
            name: 'an assertion in a callback at the top of the module',
            code: imported('[1].forEach(value => { expect(value).toBe(1); });'),
            errors: [{ message: STANDALONE }]
        },
        {
            name: 'an assertion in a callback in a describe block',
            code: imported(
                "describe.skip('a block', () => { [1].forEach(value => { expect(value).toBe(1); }); });"
            ),
            errors: [{ message: STANDALONE }]
        },
        {
            name: 'an assertion in a describe block of each row',
            code: imported(
                "describe.each([1])('a block for %i', value => { expect(value).toBe(1); });"
            ),
            errors: [{ message: STANDALONE }]
        }
    ]
});

const NO_MATCHER = 'Expected a matcher, such as toBe(), to be called on expect()';
const NOT_AWAITED = 'Expected the promise of .resolves or .rejects to be awaited or returned';

tester.run('valid-expect', plugin.rules['valid-expect'], {
    valid: [
        imported('expect(1).toBe(1);'),
        imported('expect(1).not.toBe(2);'),
        imported("expect(1, 'a message').toBe(1);"),
        imported('await expect(promise).resolves.toBe(1);'),
        imported(
            'async function check(): Promise<void> { return expect(promise).rejects.toThrow(); }'
        ),
        imported('const check = (): Promise<void> => expect(promise).resolves.not.toBe(1);')
    ],
    invalid: [
        {
            name: 'no matcher',
            code: imported('expect(1);'),
            errors: [{ message: NO_MATCHER }]
        },
        {
            name: 'a matcher that is named but not called',
            code: imported('expect(1).toBe;'),
            errors: [{ message: NO_MATCHER }]
        },
        {
            name: 'a modifier without a matcher',
            code: imported('expect(1).not;'),
            errors: [{ message: NO_MATCHER }]
        },
        {
            name: 'a promise that is neither awaited nor returned',
            code: imported('expect(promise).resolves.toBe(1);'),
            errors: [{ message: NOT_AWAITED }]
        },
        {
            name: 'a promise in a function that returns something else',
            code: imported(
                'async function check(): Promise<number> { expect(promise).rejects.toThrow(); return 1; }'
            ),
            errors: [{ message: NOT_AWAITED }]
        }
    ]
});

tester.run('prefer-to-be', plugin.rules['prefer-to-be'], {
    valid: [
        imported('expect(value).toBe(1);'),
        imported('expect(value).toEqual({ a: 1 });'),
        imported('expect(value).toStrictEqual([1]);'),
        imported('expect(value).toEqual(/a/u);'),
        imported('expect(value).toEqual(`a${b}`);'),
        imported('expect(value).toStrictEqual(-value);'),
        imported('expect(value).toBeNull();'),
        imported('expect(value).not.toBeUndefined();'),
        // A comparison of something other than an imported `expect`
        'declare const check: (value: unknown) => { toEqual(value: unknown): void };\ncheck(1).toEqual(1);'
    ],
    invalid: [
        {
            name: 'a number',
            code: imported('expect(value).toEqual(1);'),
            output: imported('expect(value).toBe(1);'),
            errors: [{ message: 'Use toBe() rather than toEqual(1)' }]
        },
        {
            name: 'negative zero',
            code: imported('expect(value).toStrictEqual(-0);'),
            output: imported('expect(value).toBe(-0);'),
            errors: [{ message: 'Use toBe() rather than toStrictEqual(-0)' }]
        },
        {
            name: 'a string, negated',
            code: imported("expect(value).not.toEqual('a');"),
            output: imported("expect(value).not.toBe('a');"),
            errors: [{ message: "Use toBe() rather than toEqual('a')" }]
        },
        {
            name: 'a bigint, a boolean and a template without expressions',
            code: imported(
                'expect(a).toStrictEqual(1n);\nexpect(b).toEqual(true);\nexpect(c).toEqual(`c`);'
            ),
            output: imported('expect(a).toBe(1n);\nexpect(b).toBe(true);\nexpect(c).toBe(`c`);'),
            errors: [
                { message: 'Use toBe() rather than toStrictEqual(1n)' },
                { message: 'Use toBe() rather than toEqual(true)' },
                { message: 'Use toBe() rather than toEqual(`c`)' }
            ]
        },
        {
            name: 'null',
            code: imported('expect(value).toBe(null);'),
            output: imported('expect(value).toBeNull();'),
            errors: [{ message: 'Use toBeNull() rather than toBe(null)' }]
        },
        {
            name: 'undefined',
            code: imported('expect(value).toEqual(undefined);'),
            output: imported('expect(value).toBeUndefined();'),
            errors: [{ message: 'Use toBeUndefined() rather than toEqual(undefined)' }]
        },
        {
            name: 'not undefined',
            code: imported('expect(value).not.toBe(undefined);'),
            output: imported('expect(value).toBeDefined();'),
            errors: [{ message: 'Use toBeDefined() rather than not.toBe(undefined)' }]
        },
        {
            name: 'not undefined, resolved',
            code: imported('await expect(promise).resolves.not.toBe(undefined);'),
            output: imported('await expect(promise).resolves.toBeDefined();'),
            errors: [{ message: 'Use toBeDefined() rather than not.toBe(undefined)' }]
        },
        {
            name: 'NaN',
            code: imported('expect(value).toStrictEqual(NaN);'),
            output: imported('expect(value).toBeNaN();'),
            errors: [{ message: 'Use toBeNaN() rather than toStrictEqual(NaN)' }]
        }
    ]
});

tester.run('prefer-to-have-length', plugin.rules['prefer-to-have-length'], {
    valid: [
        imported('expect(values).toHaveLength(1);'),
        imported('expect(values.length).toBeGreaterThan(1);'),
        imported('expect(values.size).toBe(1);'),
        imported("expect(values['length']).toBe(1);"),
        imported('expect(values?.length).toBe(1);')
    ],
    invalid: [
        {
            name: 'toBe()',
            code: imported('expect(values.length).toBe(1);'),
            output: imported('expect(values).toHaveLength(1);'),
            errors: [{ message: 'Use toHaveLength() rather than comparing values.length' }]
        },
        {
            name: 'toEqual() of a call',
            code: imported('expect(values.at(0).length).toEqual(2);'),
            output: imported('expect(values.at(0)).toHaveLength(2);'),
            errors: [{ message: 'Use toHaveLength() rather than comparing values.at(0).length' }]
        },
        {
            name: 'not.toStrictEqual()',
            code: imported('expect(values.length).not.toStrictEqual(0);'),
            output: imported('expect(values).not.toHaveLength(0);'),
            errors: [{ message: 'Use toHaveLength() rather than comparing values.length' }]
        }
    ]
});
