// Tests and suites, declared through node:test with Vitest's API: `.each`,
// `.for`, `.skipIf` and `.runIf`, and test options after the function.
import * as nodeTest from 'node:test';
import { toArray } from '@vitest/utils/helpers';
import { callerSite, callFrom, type Site } from './site.ts';
import { formatTitle } from './title.ts';

type Awaitable<T> = PromiseLike<T> | T;

/** The options of a test or a suite, which node:test takes */
export interface TestOptions {
    /** Milliseconds after which the test fails */
    readonly timeout?: number;
    /** Skips it, with the reason if a string */
    readonly skip?: boolean | string;
    /**
     * Marks it as to do: it runs, and its failure is reported but doesn't
     * fail the run
     */
    readonly todo?: boolean | string;
    /**
     * Runs it, and the others marked only, rather than every test, when
     * `--test-only` is passed or tests run in one process
     */
    readonly only?: boolean;
    /** How many of its subtests run at once, or all at once if true */
    readonly concurrency?: boolean | number;
}

export type TestContext = nodeTest.TestContext;
export type SuiteContext = nodeTest.SuiteContext;
export type TestFunction = (context: TestContext) => Awaitable<unknown>;
export type SuiteFunction = (context: SuiteContext) => Awaitable<unknown>;

/**
 * Declares a test or a suite, with its options after its function, as
 * Vitest takes them, or before it, as node:test does. A timeout alone can
 * take their place. Without a function, it's to do.
 */
export interface Declare<Fn> {
    (name: string, fn?: Fn, options?: TestOptions | number): void;
    (name: string, options: TestOptions, fn?: Fn): void;
}

/**
 * Declares a test or a suite for each row, which `formatTitle` names. Its
 * function takes the row's values, if every row is an array, and otherwise
 * the row. A readonly row's values are spread into parameters, which can't be
 * readonly.
 */
export interface Each<Result> {
    <Row extends readonly unknown[] | [unknown]>(
        rows: readonly Row[]
    ): Declare<(...values: [...Row]) => Result>;
    <Row>(rows: readonly Row[]): Declare<(row: Row) => Result>;
}

/**
 * Declares a test or a suite for each row, which `formatTitle` names. Its
 * function takes the row, whether or not it's an array, and a test's
 * context.
 */
export type For<Context extends unknown[]> = <Row>(
    rows: readonly Row[]
) => Declare<(row: Row, ...context: Context) => Awaitable<unknown>>;

/** The variants of a test or a suite, which apply to any combination */
export interface Modifiers<API> {
    /** Skips it */
    readonly skip: API;
    /** Runs it, and the others marked only, rather than every test */
    readonly only: API;
    /** Marks it as to do */
    readonly todo: API;
    /** Skips it if the condition holds */
    skipIf(condition: unknown): API;
    /** Skips it unless the condition holds */
    runIf(condition: unknown): API;
}

/** `it` and `test`, with members that a subpath, such as fast-check's, adds */
export interface TestAPI<Extra = unknown>
    extends Declare<TestFunction>, Modifiers<TestAPI<Extra> & Extra> {
    readonly each: Each<Awaitable<unknown>>;
    readonly for: For<[context: TestContext]>;
}

/** `describe` and `suite` */
export interface SuiteAPI extends Declare<SuiteFunction>, Modifiers<SuiteAPI> {
    readonly each: Each<Awaitable<unknown>>;
    readonly for: For<[]>;
}

/** Declares a test or a suite through node:test, from the site */
type Register = (
    name: string,
    options: TestOptions,
    fn: ((context: never) => unknown) | undefined,
    site: Site | undefined
) => void;

/**
 * Declares a test or a suite with the options that its modifiers set, from
 * the site where a test file declared it
 */
export type Define<Fn> = (
    name: string,
    options: TestOptions,
    fn: Fn | undefined,
    site: Site | undefined
) => void;

type Argument<Fn> = Fn | TestOptions | number | undefined;

/** The function and options of a declaration, in either order */
export function parseArguments<Fn>(
    first: Argument<Fn>,
    second: Argument<Fn>
): { fn: Fn | undefined; options: TestOptions } {
    if (typeof first === 'object') {
        return { fn: second as Fn | undefined, options: first as TestOptions };
    }

    const fn = first as Fn | undefined;
    if (typeof second === 'number') {
        return { fn, options: { timeout: second } };
    }

    return { fn, options: (second as TestOptions | undefined) ?? {} };
}

/**
 * The API of `it` or `describe`, of options that its modifiers set: each
 * modifier builds another, when it's read. `extend` adds members, such as
 * `prop`, which declare tests with the same options.
 */
function createAPI<Fn>(
    register: Register,
    options: TestOptions,
    extend: (define: Define<Fn>) => object
): object {
    const define: Define<Fn> = (name, ownOptions, fn, site) => {
        const merged = { ...options, ...ownOptions };
        // Vitest marks a test without a function as to do, where node:test
        // would pass it
        const todo = fn === undefined ? { todo: true } : {};
        register(name, { ...merged, ...todo }, fn as never, site);
    };
    const build = (more: TestOptions): object =>
        createAPI(register, { ...options, ...more }, extend);
    const skipIf = (skip: boolean): object => build(skip ? { skip: true } : {});

    const declare = (
        name: string,
        first?: Argument<Fn>,
        second?: Argument<Fn>
    ): void => {
        const parsed = parseArguments(first, second);
        define(name, parsed.options, parsed.fn, callerSite());
    };

    return Object.defineProperties(Object.assign(declare, extend(define)), {
        skip: { get: () => build({ skip: true }) },
        only: { get: () => build({ only: true }) },
        todo: { get: () => build({ todo: true }) },
        skipIf: { value: (condition: unknown) => skipIf(Boolean(condition)) },
        runIf: {
            value: (condition: unknown) => {
                const run = Boolean(condition);
                return skipIf(!run);
            }
        },
        each: { value: each(define) },
        for: { value: forEach(define) }
    });
}

/** `.each`: spreads the rows if every row is an array */
function each<Fn>(
    define: Define<Fn>
): (rows: readonly unknown[]) => Declare<Fn> {
    return rows =>
        (name: string, first?: Argument<Fn>, second?: Argument<Fn>): void => {
            const site = callerSite();
            const { fn, options } = parseArguments(first, second);
            const handler = fn as
                | ((...values: unknown[]) => unknown)
                | undefined;
            const spread = rows.every(row => Array.isArray(row));

            for (const [index, row] of rows.entries()) {
                const values: unknown[] = Array.isArray(row) ? row : [row];
                const title = formatTitle(name, values, index);
                const run = (): unknown =>
                    spread ? handler?.(...values) : handler?.(row);

                define(title, options, handler && (run as Fn), site);
            }
        };
}

/** `.for`: passes each row whole, and a test's context after it */
function forEach<Fn>(
    define: Define<Fn>
): (rows: readonly unknown[]) => Declare<Fn> {
    return rows =>
        (name: string, first?: Argument<Fn>, second?: Argument<Fn>): void => {
            const site = callerSite();
            const { fn, options } = parseArguments(first, second);
            const handler = fn as
                | ((...values: unknown[]) => unknown)
                | undefined;

            for (const [index, row] of rows.entries()) {
                const title = formatTitle(name, toArray(row), index);
                const run = (context: unknown): unknown =>
                    handler?.(row, context);

                define(title, options, handler && (run as Fn), site);
            }
        };
}

/** Declares a test through node:test, which awaits it */
function registerTest(
    name: string,
    options: TestOptions,
    fn: ((context: never) => unknown) | undefined,
    site: Site | undefined
): void {
    // node:test passes a callback as well to a function of two parameters
    const test =
        fn && ((context: TestContext): unknown => fn(context as never));
    void callFrom(
        site,
        nodeTest.it,
        name,
        options,
        test as nodeTest.TestFn | undefined
    );
}

/** Declares a suite through node:test, which awaits it */
function registerSuite(
    name: string,
    options: TestOptions,
    fn: ((context: never) => unknown) | undefined,
    site: Site | undefined
): void {
    void callFrom(
        site,
        nodeTest.describe,
        name,
        options,
        fn as nodeTest.SuiteFn | undefined
    );
}

/**
 * The API of `it` and `test`, with the members that `extend` adds at each
 * modifier, such as fast-check's `prop`
 */
export function createTestAPI<Extra extends object>(
    extend: (define: Define<TestFunction>) => Extra
): TestAPI<Extra> & Extra {
    return createAPI(registerTest, {}, extend) as TestAPI<Extra> & Extra;
}

/** Declares a test, which passes unless its function throws or rejects */
export const it: TestAPI = createTestAPI(() => ({}));
export const test: TestAPI = it;

/** Declares a suite, whose function declares tests and other suites */
export const describe = createAPI<SuiteFunction>(
    registerSuite,
    {},
    () => ({})
) as SuiteAPI;
export const suite: SuiteAPI = describe;
