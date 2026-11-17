// Vitest's matchers, from @vitest/expect, which the tests assert with under
// node:test. @vitest/expect adds them to chai, as a plugin, and exports the
// chai that it adds them to.
import { diff, inspect, type InspectOptions } from 'node:util';
import {
    chai,
    JestAsymmetricMatchers,
    JestChaiExpect,
    JestExtend,
    type Assertion,
    type AsymmetricMatchersContaining,
    type ExpectStatic
} from '@vitest/expect';

chai.use(JestExtend);
chai.use(JestChaiExpect);
chai.use(JestAsymmetricMatchers);

/**
 * chai's `expect`, with Vitest's matchers, such as `toStrictEqual`, and its
 * asymmetric matchers, such as `expect.any`. The members of Vitest's own
 * `expect` that its runner provides, such as `expect.soft`, aren't here.
 */
interface Expect
    extends
        AsymmetricMatchersContaining,
        Pick<ExpectStatic, 'any' | 'anything' | 'not'> {
    <T>(actual: T, message?: string): Assertion<void, T>;
}

export const expect = chai.expect as unknown as Expect;

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
type Assert = (this: Chai.AssertionStatic, ...args: unknown[]) => void;
// oxlint-disable-next-line typescript/unbound-method -- Called with each assertion as `this`
const assert = chai.Assertion.prototype.assert as unknown as Assert;

chai.Assertion.prototype.assert = function (
    this: Chai.AssertionStatic,
    ...args: unknown[]
): void {
    try {
        assert.apply(this, args);
    } catch (error) {
        if (error instanceof chai.AssertionError && error.showDiff === true) {
            const changes = describeDiff(error.actual, error.expected);
            error.message += `\n\n+ actual - expected\n\n${changes}`;
        }
        throw error;
    }
} as typeof chai.Assertion.prototype.assert;
