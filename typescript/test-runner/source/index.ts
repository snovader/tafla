// Vitest's API for tests, on Node's test runner: tests and suites, hooks,
// Vitest's matchers, and its spies
import {
    clearAllMocks,
    fn,
    isMockFunction,
    resetAllMocks,
    restoreAllMocks,
    spyOn
} from '@vitest/spy';

export { expect, type Expect } from './expect.ts';
export {
    afterAll,
    afterEach,
    beforeAll,
    beforeEach,
    type AfterHook,
    type Hook
} from './hooks.ts';
export {
    describe,
    it,
    suite,
    test,
    type Declare,
    type Each,
    type For,
    type Modifiers,
    type SuiteAPI,
    type SuiteContext,
    type SuiteFunction,
    type TestAPI,
    type TestContext,
    type TestFunction,
    type TestOptions
} from './tests.ts';
export type { Mock, MockInstance } from '@vitest/spy';

/**
 * Vitest's spies, which its matchers, such as `toHaveBeenCalledWith`, check.
 * node:test's `mock` mocks modules and timers.
 */
export const vi = {
    clearAllMocks,
    fn,
    isMockFunction,
    resetAllMocks,
    restoreAllMocks,
    spyOn
};
