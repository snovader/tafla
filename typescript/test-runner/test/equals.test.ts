import { runInNewContext } from 'node:vm';
import {
    arrayBufferEquality,
    equals,
    getCustomEqualityTesters,
    iterableEquality,
    sparseArrayEquality,
    typeEquality,
    type Tester
} from '@vitest/expect';
import { describe, expect, it } from 'test-runner';
import { fc, it as property } from 'test-runner/fast-check';
import { fastEquality } from '../source/equals.ts';

const TO_EQUAL: Tester[] = [fastEquality, iterableEquality];
const TO_STRICT_EQUAL: Tester[] = [
    fastEquality,
    iterableEquality,
    typeEquality,
    sparseArrayEquality,
    arrayBufferEquality
];

class Point {
    readonly x: unknown;

    constructor(x: unknown) {
        this.x = x;
    }
}

class OtherPoint {
    readonly x: unknown;

    constructor(x: unknown) {
        this.x = x;
    }
}

/** A NaN whose bytes differ from `Number.NaN`'s */
const OTHER_NAN = new Float64Array(new BigUint64Array([0x7f_f8_00_00_00_00_00_01n]).buffer)[0];

/**
 * Values from a small set, so that pairs of them are often equal, or differ in
 * one of the ways that `toEqual` and `toStrictEqual` tell apart: signed zero,
 * holes and undefined, prototypes, and typed arrays' NaNs. Maps, errors and
 * asymmetric matchers are left to `equals`, with what's inside them. None has
 * symbol keys, or properties of an array besides its elements, which the
 * tester doesn't compare.
 */
const { value: VALUES } = fc.letrec(tie => ({
    value: fc.oneof(
        { depthSize: 'small' },
        fc.constantFrom(0, -0, 1, Number.NaN, 1n, undefined, null, 'a', true),
        fc.array(tie('value'), { maxLength: 3 }),
        fc.sparseArray(tie('value'), { maxLength: 3 }),
        // Some with null prototypes
        fc.dictionary(fc.constantFrom('a', 'b'), tie('value'), { maxKeys: 2 }),
        tie('value').map(value => new Point(value)),
        tie('value').map(value => new OtherPoint(value)),
        fc.constantFrom(
            new Float64Array([0]),
            new Float64Array([-0]),
            new Float64Array([Number.NaN]),
            new Float64Array([OTHER_NAN]),
            new Float32Array([-0]),
            new Uint8Array([0]),
            new Uint8Array([1]),
            new Int8Array([0])
        ),
        tie('value').map(value => new Map([['a', value]])),
        tie('value').map(value => new Set([value])),
        fc.constantFrom(new Error('a'), new Date(0)),
        fc.constantFrom(expect.anything(), expect.any(Number))
    )
}));

/**
 * A copy of the value, with the same prototypes, holes and keys, and the same
 * Maps, Sets, errors, dates and matchers, which the tester leaves to `equals`
 */
function copyOf(value: unknown): unknown {
    if (typeof value !== 'object' || value === null || !isCopied(value)) {
        return value;
    }

    if (ArrayBuffer.isView(value)) {
        return (value as Uint8Array).slice();
    }

    // An array of holes, as many as the value's length, until set
    const copy: object = Array.isArray(value)
        ? new Array<unknown>(value.length)
        : (Object.create(Object.getPrototypeOf(value) as object | null) as object);

    for (const key of Reflect.ownKeys(value)) {
        const descriptor = Object.getOwnPropertyDescriptor(value, key);
        if (descriptor?.enumerable === true) {
            Reflect.set(copy, key, copyOf(Reflect.get(value, key)));
        }
    }

    return copy;
}

function isCopied(value: object): boolean {
    return (
        Array.isArray(value) ||
        ArrayBuffer.isView(value) ||
        value instanceof Point ||
        value instanceof OtherPoint ||
        Object.prototype.toString.call(value) === '[object Object]'
    );
}

describe('equals with the fast tester', () => {
    property.prop([VALUES, VALUES])(
        'should give the result of toEqual without it',
        (actual, expected) => {
            const result = equals(actual, expected, TO_EQUAL.slice(1));

            expect(equals(actual, expected, TO_EQUAL)).toBe(result);
        }
    );

    property.prop([VALUES, VALUES])(
        'should give the result of toStrictEqual without it',
        (actual, expected) => {
            const result = equals(actual, expected, TO_STRICT_EQUAL.slice(1), true);

            expect(equals(actual, expected, TO_STRICT_EQUAL, true)).toBe(result);
        }
    );

    property.prop([VALUES])(
        'should give the results of both matchers without it for a copy',
        value => {
            const copy = copyOf(value);
            const result = equals(value, copy, TO_EQUAL.slice(1));
            const strictResult = equals(value, copy, TO_STRICT_EQUAL.slice(1), true);

            expect(equals(value, copy, TO_EQUAL)).toBe(result);
            expect(equals(value, copy, TO_STRICT_EQUAL, true)).toBe(strictResult);
        }
    );
});

describe('fastEquality', () => {
    property.prop([VALUES, VALUES, fc.boolean()])(
        "should answer only with equals' result",
        (actual, expected, strict) => {
            const testers = strict ? TO_STRICT_EQUAL : TO_EQUAL;
            const answer = fastEquality.call({ equals }, actual, expected, testers);

            if (answer !== undefined) {
                expect(answer).toBe(equals(actual, expected, testers.slice(1), strict));
            }
        }
    );

    it.each([
        ['toEqual', TO_EQUAL],
        ['toStrictEqual', TO_STRICT_EQUAL]
    ])('should answer for plain data compared with %s', (_, testers) => {
        const value = {
            fields: [1, -0, 'a', { nested: [true, null] }],
            bytes: new Uint8Array([1, 2, 3]),
            floats: new Float64Array([Number.NaN, -0])
        };

        expect(fastEquality.call({ equals }, value, structuredClone(value), testers)).toBe(true);
    });

    it.each([
        ['-0 and 0', -0, 0],
        ['arrays with -0 and 0', [-0], [0]],
        ['float arrays with -0 and 0', new Float64Array([-0]), new Float64Array([0])]
    ])('should tell apart %s', (_, actual, expected) => {
        expect(actual).not.toEqual(expected);
        expect(actual).not.toStrictEqual(expected);
    });

    it('should tell a hole from undefined only in toStrictEqual', () => {
        const holey = new Array<unknown>(2);
        holey[1] = 1;

        expect(holey).toEqual([undefined, 1]);
        expect(holey).not.toStrictEqual([undefined, 1]);
    });

    it.each([
        ['the properties of an array besides its elements', /b/u.exec('abc'), ['b']],
        ['symbol keys', { a: 1, [Symbol.for('key')]: 2 }, { a: 1 }],
        [
            'the properties of a typed array besides its elements',
            Object.assign(new Uint8Array([1]), { extra: 1 }),
            new Uint8Array([1])
        ]
    ])('should ignore %s, which equals compares', (_, actual, expected) => {
        expect(equals(actual, expected, TO_EQUAL.slice(1))).toBe(false);
        expect(actual).toEqual(expected);
    });

    it('should take NaNs of other bytes in a float array as equal', () => {
        expect(new Float64Array([OTHER_NAN])).toStrictEqual(new Float64Array([Number.NaN]));
    });

    it('should leave matchers that compare otherwise to equals', () => {
        expect({ a: 1, b: 2 }).toMatchObject({ a: 1 });
        expect([{ a: 1, b: 2 }]).toContainEqual({ a: 1, b: 2 });
    });

    it('should leave cyclic values to equals', () => {
        const actual: Record<string, unknown> = { a: [1] };
        actual.self = actual;
        const expected: Record<string, unknown> = { a: [1] };
        expected.self = expected;

        expect(actual).toEqual(expected);
        expect(actual).toStrictEqual(expected);
    });

    it('should compare objects of another realm by their keys', () => {
        const other: unknown = runInNewContext('({ a: [1, { b: 2 }] })');

        expect(other).toEqual({ a: [1, { b: 2 }] });
        expect(other).not.toStrictEqual({ a: [1, { b: 2 }] });
    });

    it('should leave values to equality testers that a test adds', () => {
        const count = { calls: 0 };
        const counting: Tester = () => {
            count.calls++;
            return undefined;
        };
        expect.addEqualityTesters([counting]);

        try {
            expect({ a: [1] }).toEqual({ a: [1] });
        } finally {
            // Leaves the testers as they were, for the other tests
            const testers = getCustomEqualityTesters();
            testers.splice(testers.indexOf(counting), 1);
        }

        expect(count.calls).toBeGreaterThan(1);
    });
});
