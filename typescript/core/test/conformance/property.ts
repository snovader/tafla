import { it } from 'node:test';
import * as fc from 'fast-check';

/**
 * Declares a test of a property, which checks the predicate against values
 * that the arbitraries generate. A failure reports the counterexample, shrunk,
 * with the seed and path that replay it.
 *
 *     prop([fc.nat()])('should ...', value => { ... });
 */
export function prop<Ts extends [unknown, ...unknown[]]>(
    arbitraries: { [K in keyof Ts]: fc.Arbitrary<Ts[K]> },
    parameters?: fc.Parameters<Ts>
): (name: string, predicate: (...values: Ts) => boolean | void) => void {
    return (name, predicate) => {
        // oxlint-disable-next-line tafla/expect-expect -- fc.assert() checks the predicate, which expect-expect checks where prop() is called
        it(name, () => {
            const property = fc.property<Ts>(...arbitraries, predicate);

            fc.assert(property, parameters);
        });
    };
}
