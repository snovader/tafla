/**
 * The lists of strings that the suite writes and reads: for each shape, 10,
 * 100 and 10,000 distinct strings of the same length and mix of UTF-8
 * widths.
 */

export interface ListShape {
    name: string;
    strings: string[];
}

const ASCII = 'abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789';
/** Latin-1 letters, of one byte in V8's strings and two in UTF-8 */
const TWO_BYTES = 'àáâãäåæçèéêëìíîïñòóôõöøùúûüý';
/** The first of the CJK ideographs, of three bytes in UTF-8 */
const THREE_BYTES = 0x4e00;

type Pick = (random: () => number) => number;

const ascii: Pick = random =>
    ASCII.charCodeAt(Math.floor(random() * ASCII.length));
const twoBytes: Pick = random =>
    TWO_BYTES.charCodeAt(Math.floor(random() * TWO_BYTES.length));
const threeBytes: Pick = random => THREE_BYTES + Math.floor(random() * 0x5000);

export const COUNTS = [10, 100, 10_000] as const;

/**
 * Each kind of string, by the picker of each of its characters, from the
 * string's index in the list and the character's in the string
 */
const KINDS: ReadonlyArray<{
    name: string;
    length: number;
    pick: (index: number, string: number) => Pick;
}> = [
    { name: '8 chars of ASCII', length: 8, pick: () => ascii },
    { name: '16 chars of ASCII', length: 16, pick: () => ascii },
    { name: '256 chars of ASCII', length: 256, pick: () => ascii },
    {
        name: '16 chars, two of 2 bytes',
        length: 16,
        pick: index => (index === 2 || index === 11 ? twoBytes : ascii)
    },
    {
        name: '16 chars, every tenth string one of 2 bytes',
        length: 16,
        pick: (index, string) =>
            string % 10 === 9 && index === 5 ? twoBytes : ascii
    },
    { name: '8 chars of 3 bytes', length: 8, pick: () => threeBytes },
    { name: '256 chars of 3 bytes', length: 256, pick: () => threeBytes }
];

export const LIST_SHAPES: ListShape[] = COUNTS.flatMap(count =>
    KINDS.map(({ name, length, pick }, kind) =>
        shape(
            `${count.toLocaleString('en-US')} × ${name}`,
            count,
            length,
            pick,
            // A seed of its own for each shape
            1 + kind + KINDS.length * COUNTS.indexOf(count)
        )
    )
);

/**
 * `count` distinct strings of `length` characters, each a flat string, as a
 * decoder returns it, rather than a concatenation
 */
function shape(
    name: string,
    count: number,
    length: number,
    pick: (index: number, string: number) => Pick,
    seed: number
): ListShape {
    const random = xorshift(seed);
    const strings = Array.from({ length: count }, (_, string) => {
        const codePoints = new Array<number>(length);
        for (let i = 0; i < length; i++) {
            codePoints[i] = pick(i, string)(random);
        }

        return String.fromCodePoint(...codePoints);
    });

    if (new Set(strings).size !== count) {
        throw new Error(`The strings of ${name} aren't distinct`);
    }

    return { name, strings };
}

/** A deterministic generator in [0, 1), so every run sees the same strings */
function xorshift(seed: number): () => number {
    let state = 0x2545f491 ^ seed;

    return () => {
        state ^= state << 13;
        state ^= state >>> 17;
        state ^= state << 5;

        return (state >>> 0) / 2 ** 32;
    };
}
