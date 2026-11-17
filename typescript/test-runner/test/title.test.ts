import { describe, expect, it } from 'test-runner';
import { formatTitle } from '../source/title.ts';

// Each title is the one that Vitest 5.1's runner formats, quirks and all: it
// signs a `%f` by the value at the `%f`'s own position among the `%f`s, and
// reads `$1.a` as a path only of an object row
describe('formatTitle', () => {
    it.each([
        ['%s and %s', ['a', 1], 'a and 1'],
        ['%s and %s', ['only one'], 'only one and undefined'],
        ['%i, %d and %f', [1.5, -0, -0], '1, 0 and 0'],
        ['%f', [-0], '-0'],
        ['%j', [{ a: [1] }], '{"a":[1]}'],
        ['%o', [{ a: 'b' }], "{ a: 'b' }"],
        ['%s', [new Uint8Array([1, 2])], '1,2'],
        ['row %# of %$, 100%%', ['a'], 'row 2 of 3, 100% a'],
        ['$name is $value', [{ name: 'a', value: -0 }], 'a is -0'],
        ['$value', [{ value: new Uint8Array([1, 2]) }], 'Uint8Array [ 1, 2 ]'],
        ['$a.b[0] and $missing', [{ a: { b: ['c'] } }], "[ 'c' ][0] and undefined"],
        ['$0 and $1.a', [1, { a: 2 }], '1 and $1.a'],
        ['$name', [1], '$name'],
        [
            '$name',
            [{ name: 'a name that goes on past forty characters, by far' }],
            'a name that goes on past forty characte…'
        ]
    ] as const)('should format %j', (template, items, title) => {
        expect(formatTitle(template, items, 2)).toBe(title);
    });
});
