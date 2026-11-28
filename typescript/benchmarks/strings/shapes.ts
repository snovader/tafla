/**
 * The strings that the suite writes and reads: for each shape, 16 distinct
 * strings of the same length and mix of UTF-8 widths, from ASCII to
 * characters of 4 bytes, and 16 empty strings, which time what a call takes
 * without any.
 */

/** The strings of each table */
export const COUNT = 16;

export interface Shape {
    name: string;
    strings: string[];
}

const ASCII = 'abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789';
/** Latin-1 letters, of one byte in V8's strings and two in UTF-8 */
const TWO_BYTES = 'àáâãäåæçèéêëìíîïñòóôõöøùúûüý';
/** The first of the CJK ideographs, of three bytes in UTF-8 */
const THREE_BYTES = 0x4e00;
/** The first of the emoji, of a surrogate pair and four bytes in UTF-8 */
const FOUR_BYTES = 0x1f600;

type Pick = (random: () => number) => number;

const twoBytes: Pick = random =>
    TWO_BYTES.charCodeAt(Math.floor(random() * TWO_BYTES.length));
const threeBytes: Pick = random => THREE_BYTES + Math.floor(random() * 0x5000);
const fourBytes: Pick = random => FOUR_BYTES + Math.floor(random() * 0x50);

const every = (): boolean => true;

export const SHAPES: Shape[] = [
    { name: 'empty', strings: new Array<string>(COUNT).fill('') },
    shape('8 chars of ASCII', 8, 1),
    shape('8 chars, one of 2 bytes', 8, 2, i => i === 3, twoBytes),
    shape('8 chars of 3 bytes', 8, 3, every, threeBytes),
    shape(
        '16 chars, two of 2 bytes',
        16,
        4,
        i => i === 2 || i === 11,
        twoBytes
    ),
    shape('15 chars, one of 4 bytes', 15, 5, i => i === 7, fourBytes),
    shape('16 chars of ASCII', 16, 17),
    shape('31 chars, last of 2 bytes', 31, 18, i => i === 30, twoBytes),
    shape('32 chars of ASCII', 32, 19),
    shape('32 chars of 3 bytes', 32, 6, every, threeBytes),
    shape('33 chars of ASCII', 33, 11),
    shape('33 chars, last of 2 bytes', 33, 12, i => i === 32, twoBytes),
    shape('64 chars of ASCII', 64, 7),
    shape('256 chars of ASCII', 256, 8),
    shape('256 chars of 3 bytes', 256, 9, every, threeBytes),
    shape('1,024 chars of ASCII', 1024, 10)
];

/**
 * 16 distinct strings of `length` characters, of which those at the indexes
 * that `special` selects come from `pick`, and the others from ASCII. Each
 * is a flat string, as a decoder returns it, rather than a concatenation.
 */
function shape(
    name: string,
    length: number,
    seed: number,
    special: (index: number) => boolean = () => false,
    pick: Pick = twoBytes
): Shape {
    const random = xorshift(seed);
    const strings = Array.from({ length: COUNT }, () => {
        const codePoints = new Array<number>(length);
        for (let i = 0; i < length; i++) {
            codePoints[i] = special(i)
                ? pick(random)
                : ASCII.charCodeAt(Math.floor(random() * ASCII.length));
        }

        return String.fromCodePoint(...codePoints);
    });

    if (new Set(strings).size !== COUNT) {
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
