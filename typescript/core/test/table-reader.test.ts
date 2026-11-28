import { describe, expect, test } from 'hypotes';
import { fc, it } from 'hypotes/fast-check';
import { DecodeError, TableArena, TableBuilder, TableLayout, TableReader } from 'tafla';
import { sliceOf, MAX_VALUES, SLICES, type Slice } from './conformance/arbitraries.ts';
import type { OffsetType } from './conformance/description.ts';

// Tests that allocate buffers of more than 4 GiB run only when asked for
const LARGE_BUFFER_TESTS = process.env.TAFLA_LARGE_BUFFER_TESTS === '1';

const IS_LITTLE_ENDIAN = new Uint8Array(new Uint16Array([1]).buffer)[0] === 1;

it('should share the storage of aligned arrays, except swapped copies on a big-endian host', () => {
    const builder = new TableBuilder(TableArena.DEFAULT, 2);
    builder.setUint8Array(new Uint8Array([1, 2, 3]));
    builder.setFloat64Array(new Float64Array([1.5]));

    const bytes = builder.bytes();
    const reader = new TableReader(bytes);
    const uint8s = reader.getUint8Array(reader.rootOffset(0));
    const float64s = reader.getFloat64Array(reader.rootOffset(1));

    expect(uint8s.buffer).toBe(bytes.buffer);
    expect(float64s.buffer === bytes.buffer).toBe(IS_LITTLE_ENDIAN);
});

it('should reject impossible vector counts before anything allocates for them', () => {
    const reader = new TableReader(new Uint8Array([255, 255, 255, 255]));

    // Allocating the offsets first would fail with a different error
    expect(() => reader.getVectorLength(0)).toThrow('Read exceeds the supplied buffer');
    expect(() => reader.getVectorOffsets(0)).toThrow('Read exceeds the supplied buffer');
    expect(() => reader.getVector(0)).toThrow('Read exceeds the supplied buffer');
});

it.each([-1, 2, 0.5])('should reject item %s of a vector of 2 with a RangeError', index => {
    const builder = new TableBuilder(TableArena.DEFAULT, 1);
    builder.setVector([new Uint8Array([1]), new Uint8Array([2, 3])]);
    const reader = builder.finalize();
    const error = new RangeError(`Item ${index} is outside the vector of 2 at 4`);

    expect(() => reader.getVectorOffset(reader.rootOffset(0), index)).toThrow(error);
    expect(() => reader.getVectorItemOffset(reader.rootOffset(0), 2, index)).toThrow(error);
});

it.each([0, 3])(
    "should throw DecodeError for item %s of a vector whose count the bytes can't hold",
    index => {
        // A count of 3, whose ends need 12 bytes after it, where 4 are left
        const reader = new TableReader(new Uint8Array([3, 0, 0, 0, 8, 0, 0, 0]));

        expect(() => reader.getVectorOffset(0, index)).toThrow(DecodeError);
    }
);

it('should find each vector item from its ends, as reading every item finds it', () => {
    const builder = new TableBuilder(TableArena.DEFAULT, 1);
    builder.setVector([new Uint8Array([1]), new Uint8Array(0), new Uint8Array([2, 3])]);
    const reader = builder.finalize();
    const vector = reader.rootOffset(0);

    // The count and the three ends, at 4, then the items from 20
    expect(reader.getVectorOffsets(vector)).toStrictEqual([20, 21, 21]);
    expect([0, 1, 2].map(index => reader.getVectorOffset(vector, index))).toStrictEqual([
        20, 21, 21
    ]);
    expect([0, 1, 2].map(index => reader.getVectorItemOffset(vector, 3, index))).toStrictEqual([
        20, 21, 21
    ]);
    expect(reader.getVectorLength(vector)).toBe(3);
    expect(reader.getVector(vector)).toStrictEqual([
        new Uint8Array([1]),
        new Uint8Array(0),
        new Uint8Array([2, 3])
    ]);
});

// A buffer of many tables can hold a table past 4 GiB, whose positions don't
// fit in 32 bits, though its own offsets do
test.runIf(LARGE_BUFFER_TESTS)('should find vector items past 4 GiB in a larger buffer', () => {
    const builder = new TableBuilder(TableArena.DEFAULT, 1);
    builder.setVector([new Uint8Array([1]), new Uint8Array([2, 3])]);
    const table = builder.finalize();
    const position = 2 ** 32;

    // Allocated zeroed, so the bytes before the table take no memory
    // until they're written
    const bytes = new Uint8Array(position + table.bytes().length);
    bytes.set(table.bytes(), position);

    const reader = new TableReader(bytes);
    const vector = position + table.rootOffset(0);
    // The count and the two ends, after the vector at 4
    const expected = [position + 16, position + 17];

    expect(reader.getVectorOffsets(vector)).toStrictEqual(expected);
    expect([0, 1].map(index => reader.getVectorOffset(vector, index))).toStrictEqual(expected);
    expect([0, 1].map(index => reader.getVectorItemOffset(vector, 2, index))).toStrictEqual(
        expected
    );
    expect(reader.getVector(vector)).toStrictEqual([new Uint8Array([1]), new Uint8Array([2, 3])]);
});

describe('versioned tables', () => {
    it.each([
        ['uint8', MAX_VALUES.uint8 + 1],
        ['uint16', MAX_VALUES.uint16 + 1],
        ['uint32', MAX_VALUES.uint32 + 1],
        ['uint32', -1],
        ['uint32', 1.5]
    ] as const)(
        'should reject a version that a %s slot cannot hold (%s)',
        (offsetType, version) => {
            const error = new RangeError(`Version ${version} doesn't fit in a ${offsetType} slot`);

            expect(() => TableLayout.create({ version, offsetType })).toThrow(error);
            expect(() =>
                TableLayout.create({ version: 0, offsetType }).withVersion(version)
            ).toThrow(error);
        }
    );

    it('should reject reading the version of an unversioned table', () => {
        const layout = TableLayout.create({ sized: true });
        const reader = new TableReader(
            new TableBuilder(TableArena.DEFAULT, 0, layout).bytes(),
            layout
        );

        expect(() => reader.getVersion()).toThrow(new TypeError('The table has no version'));
    });

    it("should read a table's version whatever the version of the reader's layout", () => {
        const first = TableLayout.create({ sized: true, version: 1 });
        const second = first.withVersion(2);

        const table = new TableBuilder(TableArena.DEFAULT, 0, second).bytes();

        expect(second.version()).toBe(2);
        expect(second.sized()).toBe(true);
        expect(new TableReader(table, first).getVersion()).toBe(2);
        expect(new TableReader(table, second).getVersion()).toBe(2);
    });
});

describe('strings', () => {
    // What getString() must read: UTF-8 that a fatal decoder accepts, with a
    // leading U+FEFF kept as part of the string
    const decoder = new TextDecoder('utf-8', { fatal: true, ignoreBOM: true });
    const encoder = new TextEncoder();

    const OFFSET_TYPES = fc.constantFrom<OffsetType>('uint8', 'uint16', 'uint32');

    // Lengths up to twice the longest string read a byte at a time, 32 bytes
    const ASCII = fc
        .string({ unit: 'binary-ascii', maxLength: 64, size: 'max' })
        .map(string => encoder.encode(string));
    const UTF8 = fc
        .tuple(
            fc.constantFrom('', '\uFEFF'),
            fc.string({ unit: 'binary', maxLength: 32, size: 'max' })
        )
        .map(([start, string]) => encoder.encode(start + string));
    const BYTES = fc.uint8Array({ maxLength: 64, size: 'max' });
    const ASCII_AND_A_BYTE = fc
        .tuple(ASCII, fc.nat(), fc.integer({ min: 0, max: 0xff }))
        .map(([ascii, at, byte]) => {
            const index = at % (ascii.length + 1);
            return Uint8Array.of(...ascii.subarray(0, index), byte, ...ascii.subarray(index));
        });

    /**
     * Reads `bytes` as the string field of a table of the offset type, in a
     * slice of other bytes, which a read past the string would find
     */
    function readString(
        bytes: Uint8Array,
        offsetType: OffsetType = 'uint32',
        slice: Slice = { byteOffset: 0, fill: 0x41 }
    ): () => string {
        const layout = TableLayout.create({ offsetType });
        const builder = new TableBuilder(TableArena.DEFAULT, 1, layout);
        // A string is stored as an array of its bytes is
        builder.setUint8Array(bytes);
        const reader = new TableReader(sliceOf(builder.bytes(), slice), layout);
        const position = reader.rootOffset(0);

        return () => reader.getString(position);
    }

    /** Expects the read to return what the decoder returns, or to throw where it throws */
    function expectDecoded(read: () => string, bytes: Uint8Array): void {
        let expected: string;
        try {
            expected = decoder.decode(bytes);
        } catch {
            expect(read).toThrow(DecodeError);
            expect(read).toThrow(expect.objectContaining({ cause: expect.any(TypeError) }));
            return;
        }

        expect(read()).toBe(expected);
    }

    it.prop([fc.oneof(ASCII, UTF8, BYTES, ASCII_AND_A_BYTE), OFFSET_TYPES, SLICES])(
        'should read any bytes as a fatal UTF-8 decoder reads them, or throw DecodeError where it throws',
        (bytes, offsetType, slice) => {
            expectDecoded(readString(bytes, offsetType, slice), bytes);
        }
    );

    // Either side of the longest strings read from the arguments of one call,
    // 8 bytes, and a byte at a time, 32 bytes
    it.each([0, 1, 8, 9, 31, 32, 33, 64])(
        'should read %i bytes of ASCII, or with a character of 2 bytes first or last, as UTF-8',
        length => {
            const ascii = 'tafla-0123456789'.repeat(4).slice(0, length);
            const strings = [ascii];
            if (length >= 2) {
                strings.push(`é${ascii.slice(2)}`, `${ascii.slice(2)}é`);
            }

            for (const string of strings) {
                const bytes = encoder.encode(string);
                expect(bytes).toHaveLength(length);
                expect(readString(bytes)()).toBe(string);
            }
        }
    );

    // Each length that's read as ASCII and up to the first that isn't,
    // before a character of each width, an invalid sequence, and a
    // surrogate, which UTF-8 can't hold
    it('should read ASCII of 0 to 65 bytes followed by any character, or throw for invalid UTF-8, as the decoder does', () => {
        const ascii = 'tafla-0123456789'.repeat(5);
        const sequences = [
            [0xc3, 0xa9],
            [0xe2, 0x82, 0xac],
            [0xf0, 0x9f, 0x98, 0x80],
            [0xc3],
            [0x80],
            [0xe0, 0x9f, 0xbf],
            [0xed, 0xa0, 0x80],
            [0xed, 0xbf, 0xbf]
        ];

        for (let length = 0; length <= 65; length++) {
            const prefix = encoder.encode(ascii.slice(0, length));

            for (const sequence of sequences) {
                for (const bytes of [
                    Uint8Array.of(...prefix, ...sequence),
                    Uint8Array.of(...prefix, ...sequence, 0x61)
                ]) {
                    expectDecoded(readString(bytes), bytes);
                }
            }
        }
    });

    it.each([9, 16, 32])(
        'should read strings of %i bytes one after another, with one that is not ASCII between them',
        length => {
            const strings = [
                'a'.repeat(length),
                `${'b'.repeat(length - 2)}é`,
                'c'.repeat(length),
                `${'d'.repeat(length - 1)}\u0001`
            ];
            const builder = new TableBuilder(TableArena.DEFAULT, strings.length);
            for (const string of strings) {
                builder.setString(string);
            }

            const reader = new TableReader(builder.bytes());
            const read = strings.map((_, i) => reader.getString(reader.rootOffset(i)));

            expect(read).toEqual(strings);
        }
    );

    it.each([1, 8, 9, 31, 32, 33, 64])(
        'should throw DecodeError for %i bytes of ASCII but a lone continuation byte, first or last',
        length => {
            for (const index of [0, length - 1]) {
                const bytes = new Uint8Array(length).fill(0x61);
                bytes[index] = 0x80;

                expect(readString(bytes)).toThrow(
                    new DecodeError("The string at 4 isn't valid UTF-8")
                );
            }
        }
    );

    // Short strings are decoded a byte at a time, which must accept what the
    // fatal decoder accepts and nothing else
    it.each([
        [
            'the first and last code points of 2, 3 and 4 bytes',
            '\u0080\u07FF\u0800\uFFFF\u{10000}\u{10FFFF}'
        ],
        ['the code points either side of the surrogates', '\uD7FF\uE000'],
        ['code points of every length between ASCII', 'aéb€c😀d']
    ])('should read %s in a short string', (_, string) => {
        expect(readString(encoder.encode(string))()).toBe(string);
    });

    it.each([
        ['a lead byte of 2 bytes at the end', [0xc3]],
        ['a lead byte of 3 bytes a byte short', [0xe2, 0x82]],
        ['a lead byte of 4 bytes a byte short', [0xf0, 0x9f, 0x98]],
        ['a lead byte followed by ASCII', [0xc3, 0x61]],
        ['a third byte that is no continuation byte', [0xe2, 0x82, 0x61]],
        ['a fourth byte that is no continuation byte', [0xf0, 0x9f, 0x98, 0xc0]],
        ['an overlong sequence of 2 bytes', [0xc1, 0xbf]],
        ['an overlong sequence of 3 bytes', [0xe0, 0x9f, 0xbf]],
        ['an overlong sequence of 4 bytes', [0xf0, 0x8f, 0xbf, 0xbf]],
        ['the first surrogate', [0xed, 0xa0, 0x80]],
        ['the last surrogate', [0xed, 0xbf, 0xbf]],
        ['the code point after U+10FFFF', [0xf4, 0x90, 0x80, 0x80]],
        ['a lead byte of 0xF5', [0xf5, 0x80, 0x80, 0x80]],
        ['a lead byte of 0xFF', [0xff]]
    ])('should throw DecodeError for %s in a short string', (_, sequence) => {
        for (const bytes of [sequence, [0x61, ...sequence], [...sequence, 0x61]]) {
            expectDecoded(readString(Uint8Array.from(bytes)), Uint8Array.from(bytes));
            expect(readString(Uint8Array.from(bytes))).toThrow(DecodeError);
        }
    });

    it('should keep a leading U+FEFF and NUL bytes', () => {
        for (const string of ['\uFEFFtafla', '\0', 'ta\0fla\0']) {
            expect(readString(encoder.encode(string))()).toBe(string);
        }
    });
});
