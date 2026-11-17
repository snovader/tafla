import * as fc from 'fast-check';

// Node 26.13 and 27's `TextEncoder.encodeInto()` sometimes stops a UTF-16 code
// unit short of bytes of exactly a string's UTF-8 length, for strings of more
// than 256 units that aren't all ASCII. Tables and `writeUtf8()` size strings
// exactly, so these strings check that they're written whole regardless.

/** Strings that `encodeInto()` stops a unit short of, by how they're formed */
export const SHORTFALL_STRINGS: Readonly<Record<string, string>> = {
    'well-formed': `${'aé'.repeat(133)}a`,
    'lone-surrogate': `${'\ud800😀'.repeat(86)}a`
};

/**
 * Strings of 257 to 513 code points of 1 to 4 bytes in UTF-8, lone
 * surrogates too, the last of them ASCII, of which `encodeInto()` stops short
 * of about 5%, against 1% when the last is any of them
 */
export const LONG_STRINGS: fc.Arbitrary<string> = fc
    .string({
        unit: fc.constantFrom('a', 'é', '€', '😀', '\ud800', '\udc00'),
        minLength: 256,
        maxLength: 512
    })
    .map(string => `${string}a`);
