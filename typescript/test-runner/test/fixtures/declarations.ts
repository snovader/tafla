// Tests that `declarations.test.ts` runs, and checks the results of, by
// their titles. Its tests fail where they should.
import {
    afterAll,
    afterEach,
    beforeAll,
    beforeEach,
    describe,
    expect,
    it,
    test,
    vi
} from 'test-runner';
import { fc, it as property } from 'test-runner/fast-check';

it.each([
    [1, 'a'],
    [2, 'b']
])('each spreads %i and %s', (count, text) => {
    expect([typeof count, typeof text]).toEqual(['number', 'string']);
});

it.each([1, [2]])(
    'each passes row %# whole unless every row is an array',
    row => {
        expect(row === 1 || Array.isArray(row)).toBe(true);
    }
);

it.each([{ name: 'a', value: -0 }])(
    'each names $name with $value',
    ({ value }) => {
        expect(value).toBe(-0);
    }
);

it.for([[1, 2]])(
    'for passes [%i, %i] whole, and the context',
    (row, context) => {
        expect(row).toEqual([1, 2]);
        expect(context.name).toBe('for passes [1, 2] whole, and the context');
    }
);

it.skip('skip skips', () => {
    expect('ran').toBe('skipped');
});

it.skip.each([1])('skip.each skips %i', () => {
    expect('ran').toBe('skipped');
});

it.skipIf(true)('skipIf skips if true', () => {
    expect('ran').toBe('skipped');
});

it.skipIf(false)('skipIf runs if false', () => {
    expect(true).toBe(true);
});

it.runIf(false)('runIf skips if false', () => {
    expect('ran').toBe('skipped');
});

it.runIf(true)('runIf runs if true', () => {
    expect(true).toBe(true);
});

it.todo('todo is to do');

it('a test without a function is to do');

it('options before the function apply', { skip: 'reason' }, () => {
    expect('ran').toBe('skipped');
});

it('a timeout after the function applies', async () => {
    // A timer that doesn't keep the process running after the test fails
    await new Promise(resolve => {
        setTimeout(resolve, 1000).unref();
    });
}, 10);

test('test is it', () => {
    expect(test).toBe(it);
});

describe.each(['A'])('describe.each %s', name => {
    it('declares tests in the suite', () => {
        expect(name).toBe('A');
    });
});

describe.skip('describe.skip skips', () => {
    it('in a skipped suite', () => {
        expect('ran').toBe('skipped');
    });
});

const calls: string[] = [];

describe('hooks', () => {
    beforeAll(() => {
        calls.push('beforeAll');
        return () => {
            calls.push('beforeAll cleanup');
        };
    });
    beforeEach(() => {
        calls.push('beforeEach');
        return () => {
            calls.push('beforeEach cleanup');
        };
    });
    afterEach(() => {
        calls.push('afterEach');
    });
    afterAll(() => {
        calls.push('afterAll');
    });

    it('first', () => {
        calls.push('first');
        expect(calls).toEqual(['beforeAll', 'beforeEach', 'first']);
    });

    it('second', () => {
        calls.push('second');
        expect(calls.at(-1)).toBe('second');
    });
});

it('hooks run in the order that node:test runs them', () => {
    // node:test runs after hooks in the order that they were declared, and
    // each cleanup is declared with its hook
    expect(calls).toEqual([
        'beforeAll',
        'beforeEach',
        'first',
        'beforeEach cleanup',
        'afterEach',
        'beforeEach',
        'second',
        'beforeEach cleanup',
        'afterEach',
        'beforeAll cleanup',
        'afterAll'
    ]);
});

it('vi.fn records calls for the matchers', () => {
    const spy = vi.fn((value: number) => value * 2);
    spy(2);

    expect(spy).toHaveBeenCalledWith(2);
    expect(spy).toHaveReturnedWith(4);
});

describe('properties', () => {
    let runs = 0;

    property.prop([fc.nat()], { numRuns: 7 })(
        'prop takes parameters',
        count => {
            runs++;
            expect(count).toBeGreaterThanOrEqual(0);
        }
    );

    it('prop ran 7 times', () => {
        expect(runs).toBe(7);
    });

    property.prop({ count: fc.nat(), text: fc.string() })(
        'prop takes a record',
        ({ count, text }) => {
            expect([typeof count, typeof text]).toEqual(['number', 'string']);
        }
    );

    // Fails, which it wouldn't if the assertion ran after the property
    property.prop([fc.nat()])('prop awaits an async predicate', async count => {
        await Promise.resolve();
        expect(count).toBeLessThan(0);
    });

    property.prop([fc.nat()])(
        'prop rejects a promise from a function that is not async',
        count => {
            expect(count).toBeGreaterThanOrEqual(0);
            return Promise.resolve(true);
        }
    );

    // oxlint-disable-next-line tafla/expect-expect -- A predicate fails the property by returning false
    property.prop([fc.nat()])('prop fails on false', count => count < 0);

    property.skip.prop([fc.nat()])('skip.prop skips', () => {
        expect('ran').toBe('skipped');
    });

    property.each([1])('each is on the property API too, %i', value => {
        expect(value).toBe(1);
    });
});
