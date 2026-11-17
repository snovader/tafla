// A faster path through the deep equality of `toEqual` and `toStrictEqual`,
// which @vitest/expect's `equals` computes with a generic recursion: at each
// value it runs the asymmetric matchers and every tester, lists keys with
// `for...in`, and compares typed arrays element by element after listing
// their keys. This tester runs first, as a custom tester, and answers for the
// plain objects, arrays, typed arrays and primitives that it reaches, with
// the result that `equals` would give. Anything else, such as a Map or an
// asymmetric matcher, it leaves to `equals`, which calls it again for what's
// inside.
import { Buffer } from 'node:buffer';
import {
    arrayBufferEquality,
    getCustomEqualityTesters,
    isAsymmetric,
    iterableEquality,
    sparseArrayEquality,
    typeEquality,
    type Tester
} from '@vitest/expect';

/**
 * Nesting past which the tester leaves values to `equals`, which tells
 * cycles from deep values
 */
const MAX_DEPTH = 256;

type TypedArray =
    | BigInt64Array
    | BigUint64Array
    | Float16Array
    | Float32Array
    | Float64Array
    | Int8Array
    | Int16Array
    | Int32Array
    | Uint8Array
    | Uint8ClampedArray
    | Uint16Array
    | Uint32Array;

/**
 * Whether `a` and `b` are equal as `toEqual` compares them, or `toStrictEqual`
 * if `customTesters` are its testers, or undefined to leave them to `equals`.
 * It leaves them all to it with testers of other matchers, such as
 * `toMatchObject`, or other custom testers, whose results it couldn't know.
 */
export const fastEquality: Tester = function (a, b, customTesters) {
    const strict = strictnessOf(customTesters);
    return strict === undefined ? undefined : compare(a, b, strict, 0);
};

/**
 * Whether the testers are those of `toStrictEqual`, rather than `toEqual`, or
 * undefined if they're another matcher's
 */
function strictnessOf(customTesters: readonly Tester[]): boolean | undefined {
    const added = getCustomEqualityTesters();
    if (added.length !== 1 || customTesters[0] !== fastEquality) {
        return undefined;
    }

    if (customTesters.length === 2 && customTesters[1] === iterableEquality) {
        return false;
    }

    const isStrict =
        customTesters.length === 5 &&
        customTesters[1] === iterableEquality &&
        customTesters[2] === typeEquality &&
        customTesters[3] === sparseArrayEquality &&
        customTesters[4] === arrayBufferEquality;
    return isStrict ? true : undefined;
}

function compare(
    a: unknown,
    b: unknown,
    strict: boolean,
    depth: number
): boolean | undefined {
    if (!isObject(a) || !isObject(b)) {
        if (Object.is(a, b)) {
            return true;
        }

        // A primitive or a function equals only itself, unless an asymmetric
        // matcher matches it
        return isAsymmetric(a) || isAsymmetric(b) ? undefined : false;
    }

    if (depth > MAX_DEPTH || isAsymmetric(a) || isAsymmetric(b)) {
        return undefined;
    }

    if (Array.isArray(a)) {
        return Array.isArray(b) ? compareArrays(a, b, strict, depth) : false;
    }

    if (isTypedArray(a)) {
        return isTypedArray(b) && a.constructor === b.constructor
            ? compareTypedArrays(a, b)
            : undefined;
    }

    if (isPlainObject(a) && isPlainObject(b)) {
        return compareObjects(a, b, strict, depth);
    }

    return undefined;
}

function isObject(value: unknown): value is object {
    return typeof value === 'object' && value !== null;
}

function isTypedArray(value: object): value is TypedArray {
    return ArrayBuffer.isView(value) && !(value instanceof DataView);
}

/**
 * Whether `equals` compares the value by its own keys alone: an object of
 * Object's tag, whatever its prototype, that isn't iterable or an error
 */
function isPlainObject(value: object): value is Record<string, unknown> {
    return (
        Object.prototype.toString.call(value) === '[object Object]' &&
        !(Symbol.iterator in value) &&
        !Error.isError(value)
    );
}

/**
 * Objects of the same constructor, if strict, with the same own enumerable
 * keys, and equal values at them: `toEqual` leaves out keys whose values are
 * undefined. Symbol keys are left to `equals`.
 */
function compareObjects(
    a: Record<string, unknown>,
    b: Record<string, unknown>,
    strict: boolean,
    depth: number
): boolean | undefined {
    if (a === b) {
        return true;
    }

    if (strict && a.constructor !== b.constructor) {
        return false;
    }

    if (hasSymbols(a) || hasSymbols(b)) {
        return undefined;
    }

    const aKeys = Object.keys(a);
    const bKeys = Object.keys(b);
    if (strict) {
        return aKeys.length === bKeys.length
            ? compareValues(a, b, aKeys, true, depth)
            : false;
    }

    if (countDefined(a, aKeys) !== countDefined(b, bKeys)) {
        return false;
    }

    return compareValues(a, b, aKeys, false, depth);
}

/**
 * The values of `a` at its keys against those of `b`, which has the same
 * number of keys, or of keys with values other than undefined, if not strict
 */
function compareValues(
    a: object,
    b: object,
    aKeys: readonly string[],
    strict: boolean,
    depth: number
): boolean | undefined {
    for (const key of aKeys) {
        const value = (a as Record<string, unknown>)[key];
        if (!strict && value === undefined) {
            continue;
        }

        if (!Object.hasOwn(b, key)) {
            return false;
        }

        const other = (b as Record<string, unknown>)[key];
        if (!strict && other === undefined) {
            return false;
        }

        const equal = compare(value, other, strict, depth + 1);
        if (equal !== true) {
            return equal;
        }
    }

    return true;
}

/**
 * Arrays of the same constructor, if strict, and length, with the same own
 * enumerable keys, and equal values at them. Without holes or other
 * properties, their keys are their indices, and they compare by index.
 * `sparseArrayEquality` also compares the order of the keys of an array that
 * isn't in another array, which the tester leaves to it.
 */
function compareArrays(
    a: unknown[],
    b: unknown[],
    strict: boolean,
    depth: number
): boolean | undefined {
    if (a === b) {
        return true;
    }

    if ((strict && a.constructor !== b.constructor) || a.length !== b.length) {
        return false;
    }

    if (hasSymbols(a) || hasSymbols(b)) {
        return undefined;
    }

    const aKeys = Object.keys(a);
    const bKeys = Object.keys(b);
    if (isDense(a, aKeys) && isDense(b, bKeys)) {
        return compareElements(a, b, strict, depth);
    }

    if (strict) {
        return haveSameOrder(aKeys, bKeys)
            ? compareValues(a, b, aKeys, true, depth)
            : undefined;
    }

    if (countDefined(a, aKeys) !== countDefined(b, bKeys)) {
        return false;
    }

    return compareValues(a, b, aKeys, false, depth);
}

/**
 * Whether an array's keys are its indices, and nothing else: none is a hole,
 * and it has no other property. Index keys come first, in order.
 */
function isDense(array: readonly unknown[], keys: readonly string[]): boolean {
    return (
        keys.length === array.length &&
        (keys.length === 0 || keys.at(-1) === String(array.length - 1))
    );
}

/**
 * The elements of dense arrays of the same length: `toEqual` takes undefined
 * as missing, which it is in both
 */
function compareElements(
    a: readonly unknown[],
    b: readonly unknown[],
    strict: boolean,
    depth: number
): boolean | undefined {
    for (let index = 0; index < a.length; index++) {
        const value = a[index];
        const other = b[index];
        // Equal primitives, but -0 and 0, which `===` takes as equal
        if (value === other && typeof value !== 'object' && value !== 0) {
            continue;
        }

        if (!strict && (value === undefined || other === undefined)) {
            return false;
        }

        const equal = compare(value, other, strict, depth + 1);
        if (equal !== true) {
            return equal;
        }
    }

    return true;
}

/** Whether two arrays of keys hold the same keys, in the same order */
function haveSameOrder(a: readonly string[], b: readonly string[]): boolean {
    return a.length === b.length && a.every((key, index) => key === b[index]);
}

/**
 * Typed arrays of the same constructor and length, with equal elements, as
 * `Object.is` compares them, and the same other properties. Equal bytes make
 * equal elements; a float array's elements can be equal with other bytes, as
 * NaNs.
 */
function compareTypedArrays(a: TypedArray, b: TypedArray): boolean | undefined {
    if (a.length !== b.length) {
        return false;
    }

    // `equals` compares the properties after the indices, which only listing
    // keys finds
    if (
        Object.keys(a).length !== a.length ||
        Object.keys(b).length !== b.length
    ) {
        return undefined;
    }

    if (a === b || Buffer.compare(bytesOf(a), bytesOf(b)) === 0) {
        return true;
    }

    const isFloat =
        a instanceof Float64Array ||
        a instanceof Float32Array ||
        a instanceof Float16Array;
    return isFloat && haveSameElements(a, b);
}

function bytesOf(array: TypedArray): Uint8Array {
    return new Uint8Array(array.buffer, array.byteOffset, array.byteLength);
}

function haveSameElements(a: TypedArray, b: TypedArray): boolean {
    for (let index = 0; index < a.length; index++) {
        if (!Object.is(a[index], b[index])) {
            return false;
        }
    }

    return true;
}

/** How many of the keys have values other than undefined */
function countDefined(object: object, keys: readonly string[]): number {
    let count = 0;
    for (const key of keys) {
        if ((object as Record<string, unknown>)[key] !== undefined) {
            count++;
        }
    }

    return count;
}

/** Whether an object has symbol keys, which the tester leaves to `equals` */
function hasSymbols(object: object): boolean {
    return Object.getOwnPropertySymbols(object).length > 0;
}
