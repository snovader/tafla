// Vitest's matchers, from @vitest/expect, which adds them to chai as plugins,
// on an `expect` of the members of Vitest's that don't need its runner
import { diff, inspect, type InspectOptions } from 'node:util';
import {
    addCustomEqualityTesters,
    chai,
    customMatchers,
    JestAsymmetricMatchers,
    JestChaiExpect,
    JestExtend,
    type Assertion,
    type AsymmetricMatchersContaining,
    type ExpectStatic,
    type MatchersObject,
    type Tester
} from '@vitest/expect';
import { fastEquality } from './equals.ts';

chai.use(JestExtend);
chai.use(JestChaiExpect);
chai.use(JestAsymmetricMatchers);

/**
 * Vitest's `expect`, with its matchers, such as `toStrictEqual`, its
 * asymmetric matchers, such as `expect.any`, and `extend` and
 * `addEqualityTesters`. The members that need Vitest's runner, such as
 * `expect.soft` and `expect.assertions`, aren't here.
 */
export interface Expect
    extends
        AsymmetricMatchersContaining,
        Pick<ExpectStatic, 'any' | 'anything' | 'extend' | 'not'> {
    <T>(actual: T, message?: string): Assertion<void, T>;
    /** Adds testers, which `toEqual` and other matchers try first */
    addEqualityTesters: (testers: Tester[]) => void;
}

/** chai's `expect`, whose `extend` takes the `expect` to add matchers to */
interface ChaiExpect {
    (actual: unknown, message?: string): unknown;
    extend: (expect: Expect, matchers: MatchersObject) => void;
}

const chaiExpect = chai.expect as unknown as ChaiExpect;

export const expect = Object.assign(
    <T>(actual: T, message?: string): Assertion<void, T> =>
        chaiExpect(actual, message) as Assertion<void, T>,
    // The asymmetric matchers, and `not`
    chaiExpect,
    {
        extend: (matchers: MatchersObject): void => {
            chaiExpect.extend(expect, matchers);
        },
        addEqualityTesters: (testers: Tester[]): void => {
            addCustomEqualityTesters(testers);
        }
    }
) as unknown as Expect;

// `toSatisfy` and `toBeOneOf`, which Vitest's `expect` adds
expect.extend(customMatchers);
addCustomEqualityTesters([fastEquality]);

const INSPECT_OPTIONS: InspectOptions = {
    depth: Infinity,
    maxArrayLength: Infinity,
    maxStringLength: Infinity,
    sorted: true
};

/** Unchanged lines kept around each change */
const CONTEXT = 3;

/** The lines of two values' inspections that differ, with some around them */
function describeDiff(actual: unknown, expected: unknown): string {
    const changes = diff(
        inspect(actual, INSPECT_OPTIONS).split('\n'),
        inspect(expected, INSPECT_OPTIONS).split('\n')
    );
    const changed = changes.map(([operation]) => operation !== 0);
    const nearChange = (index: number): boolean =>
        changed
            .slice(Math.max(0, index - CONTEXT), index + CONTEXT + 1)
            .includes(true);

    const lines: string[] = [];
    for (const [index, [operation, line]] of changes.entries()) {
        if (nearChange(index)) {
            lines.push(
                `${operation === 1 ? '+' : operation === -1 ? '-' : ' '} ${line}`
            );
        } else if (lines.at(-1) !== '  ...') {
            lines.push('  ...');
        }
    }

    return lines.join('\n');
}

// node:test's reporters print an assertion error's message and values, but
// not the diff of the values that Vitest's reporter prints, so the message of
// each failed matcher that compares values carries one
/** Marks chai's `assert` once it adds diffs, if another copy of this module loads */
const WITH_DIFF = Symbol.for('test-runner.withDiff');

type Assert = ((this: Chai.AssertionStatic, ...args: unknown[]) => void) & {
    [WITH_DIFF]?: true;
};

// oxlint-disable-next-line typescript/unbound-method -- Called with each assertion as `this`
const assert = chai.Assertion.prototype.assert as unknown as Assert;

if (assert[WITH_DIFF] !== true) {
    const assertWithDiff: Assert = function (this, ...args) {
        try {
            assert.apply(this, args);
        } catch (error) {
            if (
                error instanceof chai.AssertionError &&
                error.showDiff === true
            ) {
                const changes = describeDiff(error.actual, error.expected);
                error.message += `\n\n+ actual - expected\n\n${changes}`;
            }
            throw error;
        }
    };
    assertWithDiff[WITH_DIFF] = true;

    chai.Assertion.prototype.assert = assertWithDiff;
}
