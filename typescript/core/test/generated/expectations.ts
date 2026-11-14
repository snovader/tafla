import { expect } from 'vitest';
import { DecodeError } from 'tafla';

/**
 * Expects reading bytes to return values or throw `DecodeError`, rather than
 * any other error
 */
export function expectValuesOrDecodeError(read: () => unknown): void {
    try {
        read();
    } catch (error) {
        expect(error).toBeInstanceOf(DecodeError);
    }
}

/**
 * A value with each typed array in it copied to a plain array, to compare what
 * `fields()` reads, whose arrays are typed, with fields written from plain
 * arrays
 */
export function withPlainArrays(value: unknown): unknown {
    if (ArrayBuffer.isView(value)) {
        return Array.from(value as unknown as ArrayLike<unknown>);
    }
    if (Array.isArray(value)) {
        return value.map(withPlainArrays);
    }
    if (typeof value === 'object' && value !== null) {
        const entries = Object.entries(value).map(([key, field]) => [
            key,
            withPlainArrays(field)
        ]);
        return Object.fromEntries(entries);
    }

    return value;
}
