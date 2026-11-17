// Vitest's hooks, through node:test's. A hook declared in a suite runs for
// its tests, and one declared at the top of a file runs for every test in the
// process: a file of its own, unless tests run in one process, with
// `--test-isolation=none`. node:test runs after hooks in the order that they
// were declared, and the cleanup that a before hook returns runs as an after
// hook declared with it, where Vitest runs them in the reverse order.
import * as nodeTest from 'node:test';

type Awaitable<T> = PromiseLike<T> | T;

/** A hook, which can return a function that cleans up after it */
export type Hook<Context> = (
    context: Context
) => Awaitable<(() => Awaitable<unknown>) | void>;

/** A hook, which runs after the tests or each test */
export type AfterHook<Context> = (context: Context) => Awaitable<unknown>;

type HookContext = nodeTest.SuiteContext | nodeTest.TestContext;

/** Runs `cleanup` if a hook returned a function */
async function cleanUp(cleanup: unknown): Promise<void> {
    if (typeof cleanup === 'function') {
        await (cleanup as () => Awaitable<unknown>)();
    }
}

/** Runs the hook before the suite's tests, and its cleanup after them */
export function beforeAll(fn: Hook<HookContext>, timeout?: number): void {
    let cleanup: unknown;

    nodeTest.before(
        async context => {
            cleanup = await fn(context);
        },
        { timeout }
    );
    nodeTest.after(() => cleanUp(cleanup), { timeout });
}

/** Runs the hook after the suite's tests */
export function afterAll(fn: AfterHook<HookContext>, timeout?: number): void {
    nodeTest.after(async context => void (await fn(context)), { timeout });
}

/** Runs the hook before each of the suite's tests, and its cleanup after it */
export function beforeEach(
    fn: Hook<nodeTest.TestContext>,
    timeout?: number
): void {
    const cleanups = new WeakMap<nodeTest.TestContext, unknown>();

    nodeTest.beforeEach(
        async context => {
            const test = context as nodeTest.TestContext;
            cleanups.set(test, await fn(test));
        },
        { timeout }
    );
    nodeTest.afterEach(
        context => cleanUp(cleanups.get(context as nodeTest.TestContext)),
        { timeout }
    );
}

/** Runs the hook after each of the suite's tests */
export function afterEach(
    fn: AfterHook<nodeTest.TestContext>,
    timeout?: number
): void {
    nodeTest.afterEach(
        async context => void (await fn(context as nodeTest.TestContext)),
        { timeout }
    );
}
