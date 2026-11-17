# test-runner

Vitest's API for tests, on Node's test runner, `node:test`. Tests declare
themselves with `describe` and `it`, as they would with Vitest, and assert
with Vitest's matchers, from `@vitest/expect`. Node runs them, with no
bundler or transform: its module loader strips the types of `.ts` files.

```ts
import { describe, expect, it } from 'test-runner';
import { fc, it as property } from 'test-runner/fast-check';

describe('sum', () => {
    it.each([
        [1, 2, 3],
        [2, -2, 0]
    ])('should add %i and %i into %i', (a, b, sum) => {
        expect(a + b).toBe(sum);
    });

    property.prop([fc.integer(), fc.integer()])('should commute', (a, b) => {
        expect(a + b).toBe(b + a);
    });
});
```

```sh
node --test 'test/**/*.test.ts'
```

The package is written in TypeScript that Node runs as it is, so it can't
be installed from npm as it is: Node doesn't strip the types of files under
`node_modules`. As a workspace's package, which Yarn links, it isn't one.
Publishing it would mean building JavaScript and declarations, as `tafla`
does.

## Tests and suites

`test-runner` exports `it` and `test`, which declare tests, `describe` and
`suite`, which declare suites, and `beforeAll`, `afterAll`, `beforeEach` and
`afterEach`, which declare hooks. They declare through node:test's, so its
reporters, `--test-name-pattern`, `--test-only` and `--watch` apply.

As in Vitest:

- `it.each(rows)(name, fn)` declares a test for each row. Its function takes
  the row's values, if every row is an array, and otherwise the row. Its
  title formats the row's values as Vitest 5.1 formats them, with the same
  functions: `%s`, `%d`, `%i`, `%f`, `%j`, `%o` and `%O` print them in order,
  `%#` and `%$` print the row's index, and `$name` prints a property of an
  object row.
- `it.for(rows)(name, fn)` passes each row whole, and the test's context.
- `it.skip`, `it.only` and `it.todo`, and `it.skipIf(condition)` and
  `it.runIf(condition)`, apply to any combination, such as
  `it.skip.each(rows)`. `describe` has the same.
- A test's options come after its function, as `it(name, fn, { timeout })`
  or `it(name, fn, timeout)`, or before it, as node:test takes them. A test
  without a function is to do.
- A `beforeAll` or `beforeEach` hook can return a function that cleans up
  after the tests, or each test.
- `vi.fn()` and `vi.spyOn()` make Vitest's spies, which matchers such as
  `toHaveBeenCalledWith` check. node:test's `mock` mocks modules and timers.

Unlike Vitest:

- A test's function takes node:test's `TestContext`.
- node:test runs after hooks in the order that they were declared, and the
  cleanup that a before hook returns runs as an after hook declared with it.
  Vitest runs them in the reverse order.
- A hook declared at the top of a file runs for every test in its process:
  one file's, unless tests run in one process, with `--test-isolation=none`.
  Declare hooks in a `describe` block to keep them to its tests.
- `--test-only`, or a run in one process, runs only the tests marked only.

node:test records where each test was declared from the code that called it,
which would be this package's. So each declaration calls node:test from a
function compiled at the position of the test file's call, which reporters
then print, as `test at file:line:column`, and which JUnit's names each
test's file with.

## Matchers

`expect` has Vitest's matchers and asymmetric matchers, such as
`toStrictEqual` and `expect.any()`, and `toSatisfy` and `toBeOneOf`, which
Vitest's own `expect` adds. `expect.extend()` adds matchers, which a test
types by augmenting `@vitest/expect`'s `Matchers`:

```ts
declare module '@vitest/expect' {
    interface Matchers<R> {
        toBeEven: () => R;
    }
}
```

`expect.addEqualityTesters()` adds custom equality testers. The members of
Vitest's `expect` that need its runner, such as `expect.soft`,
`expect.poll` and `expect.assertions`, aren't there.

node:test's reporters print an assertion's message, but not the diff of its
values that Vitest's prints, so a failed matcher that compares values adds
one to its message: the lines of the values' inspections that differ, as
`+ actual - expected`, with 3 lines around each change.

### Equality

`@vitest/expect` compares values for `toEqual` and `toStrictEqual` with a
generic recursion: at each value it runs every equality tester, lists keys
with `for...in`, and compares typed arrays element by element, after listing
their keys. `test-runner` adds a tester that runs first and answers for the
plain objects, arrays, typed arrays and primitives that it reaches, with the
result that `@vitest/expect` would give, through faster paths: it compares
the elements of arrays without holes or other keys by index, and typed
arrays by their bytes. A property test checks that `toEqual` and
`toStrictEqual` give the same results with it as without it, for values that
differ in the ways that they tell apart: signed zero, holes and undefined,
prototypes, symbol keys, the NaNs of float arrays, and the properties of
arrays.

It leaves to `@vitest/expect` anything else, such as a Map, an error or an
asymmetric matcher, and what's inside it, other matchers, such as
`toMatchObject`, and every comparison once a test adds testers of its own.
It takes values nested more than 256 deep for cycles, and leaves them too.

## Properties

`test-runner/fast-check` exports fast-check as `fc`, and `it` and `test`
with `prop`, as `@fast-check/vitest` does:

```ts
it.prop([fc.nat(), fc.string()])('should ...', (count, text) => { ... });
it.prop({ count: fc.nat() }, { numRuns: 1000 })('should ...', ({ count }) => { ... });
```

A property passes unless its predicate throws or returns false. A failure
reports the counterexample, shrunk, with the seed and path that replay it.
An async predicate runs as an async property, which fast-check awaits; a
predicate that returns a promise without being async fails, as fast-check
would take the promise as passing. Unlike `@fast-check/vitest`'s, the test's
title doesn't name the seed, so that it's the same on every run.
