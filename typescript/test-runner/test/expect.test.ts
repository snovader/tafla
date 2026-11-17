import { describe, expect, it, vi } from 'test-runner';

// How a test file types the matchers that it adds
declare module '@vitest/expect' {
    interface Matchers<R> {
        toBeEven: () => R;
    }
}

expect.extend({
    toBeEven(received: unknown) {
        return {
            pass: typeof received === 'number' && received % 2 === 0,
            message: () => `expected ${String(received)} to be even`
        };
    }
});

/** The message of the error that the function throws */
function messageOf(fn: () => void): string {
    try {
        fn();
    } catch (error) {
        return (error as Error).message;
    }

    throw new Error('Expected the function to throw');
}

describe('expect', () => {
    it('should add matchers with extend', () => {
        expect(2).toBeEven();
        expect(3).not.toBeEven();
        expect(() => expect(3).toBeEven()).toThrow('expected 3 to be even');
    });

    it('should add asymmetric matchers with extend', () => {
        // Asymmetric matchers are typed as any, which values take as unknown
        const even: unknown = expect.toBeEven();
        const odd: unknown = expect.not.toBeEven();

        expect({ count: 4 }).toEqual({ count: even });
        expect({ count: 3 }).toEqual({ count: odd });
    });

    it("should have the matchers that Vitest's expect adds", () => {
        expect(2).toBeOneOf([1, 2]);
        expect(2).toSatisfy((value: number) => value > 1);
    });

    it('should have the asymmetric matchers', () => {
        const number: unknown = expect.any(Number);
        const matching: unknown = expect.stringMatching(/b/);
        const containing: unknown = expect.arrayContaining([2]);
        const anything: unknown = expect.anything();

        expect({ a: 1, b: 'b', c: [1, 2] }).toEqual({ a: number, b: matching, c: containing });
        expect({ a: 1, b: 2 }).toEqual(expect.objectContaining({ a: anything }));
    });

    it("should check Vitest's spies", () => {
        const spy = vi.fn();
        spy('a');

        expect(spy).toHaveBeenCalledExactlyOnceWith('a');
        expect(vi.isMockFunction(spy)).toBe(true);
    });

    it('should add a diff to the message of a matcher that compares values', () => {
        const message = messageOf(() => {
            expect({ a: [1, -0], b: 'same' }).toStrictEqual({ a: [1, 0], b: 'same' });
        });

        expect(message).toContain(
            "+ actual - expected\n\n+ { a: [ 1, -0 ], b: 'same' }\n- { a: [ 1, 0 ], b: 'same' }"
        );
    });
});
