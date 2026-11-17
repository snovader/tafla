// Property tests with fast-check, as @fast-check/vitest declares them:
// `it.prop([fc.nat()])('should ...', value => { ... })`
import * as fc from 'fast-check';
import { callerSite } from './site.ts';
import {
    createTestAPI,
    parseArguments,
    type Declare,
    type TestAPI,
    type TestFunction,
    type TestOptions
} from './tests.ts';

export { fc };

type Awaitable<T> = PromiseLike<T> | T;

/** A property's predicate, which fails it by throwing or returning false */
type Predicate<Values extends unknown[]> = (
    ...values: Values
) => Awaitable<boolean | void>;

export interface PropAPI {
    /**
     * Declares a test of a property, which checks the predicate against the
     * values of the arbitraries, in a tuple or a record. A failure reports
     * the counterexample, shrunk, with the seed and path that replay it. An
     * async predicate runs as an async property.
     *
     *     it.prop([fc.nat(), fc.string()])('should ...', (count, text) => { ... });
     *     it.prop({ count: fc.nat() })('should ...', ({ count }) => { ... });
     */
    prop<Values extends [unknown, ...unknown[]]>(
        arbitraries: { [K in keyof Values]: fc.Arbitrary<Values[K]> },
        parameters?: fc.Parameters<Values>
    ): Declare<Predicate<Values>>;
    prop<Values extends Record<string, unknown>>(
        arbitraries: { [K in keyof Values]: fc.Arbitrary<Values[K]> },
        parameters?: fc.Parameters<[Values]>
    ): Declare<Predicate<[Values]>>;
}

type AnyPredicate = (...values: unknown[]) => Awaitable<boolean | void>;
type Arbitraries =
    | ReadonlyArray<fc.Arbitrary<unknown>>
    | Record<string, fc.Arbitrary<unknown>>;

/** `it` and `test`, whose modifiers, such as `skip`, have `prop` too */
export const it: TestAPI<PropAPI> & PropAPI = createTestAPI(define => ({
    prop:
        (arbitraries: Arbitraries, parameters?: fc.Parameters<unknown[]>) =>
        (
            name: string,
            first?: AnyPredicate | TestOptions | number,
            second?: AnyPredicate | TestOptions | number
        ): void => {
            const site = callerSite();
            const { fn: predicate, options } = parseArguments(first, second);
            const test =
                predicate &&
                ((() =>
                    assertProperty(
                        name,
                        arbitraries,
                        parameters,
                        predicate
                    )) as TestFunction);

            define(name, options, test, site);
        }
})) as TestAPI<PropAPI> & PropAPI;
export const test: TestAPI<PropAPI> & PropAPI = it;

/**
 * Runs the property, async if its predicate is an async function, which
 * fast-check has to await
 */
function assertProperty(
    name: string,
    arbitraries: Arbitraries,
    parameters: fc.Parameters<unknown[]> | undefined,
    predicate: AnyPredicate
): Promise<void> | undefined {
    const values = Array.isArray(arbitraries)
        ? arbitraries
        : [fc.record(arbitraries as Record<string, fc.Arbitrary<unknown>>)];

    if (
        Object.prototype.toString.call(predicate) === '[object AsyncFunction]'
    ) {
        const property = fc.asyncProperty(
            ...(values as [fc.Arbitrary<unknown>]),
            predicate as (...values: unknown[]) => Promise<boolean | void>
        );
        return fc.assert(property, parameters);
    }

    const property = fc.property(
        ...(values as [fc.Arbitrary<unknown>]),
        (...args: unknown[]) => checkSync(name, predicate(...args))
    );
    fc.assert(property, parameters);
    return undefined;
}

/**
 * A synchronous property's result, which a promise isn't: fast-check would
 * take it as passing without awaiting it
 */
function checkSync(
    name: string,
    result: Awaitable<boolean | void>
): boolean | void {
    if (
        typeof (result as PromiseLike<unknown> | undefined)?.then === 'function'
    ) {
        throw new TypeError(
            `The predicate of "${name}" returned a promise, which only an async function's is awaited`
        );
    }

    return result as boolean | void;
}
