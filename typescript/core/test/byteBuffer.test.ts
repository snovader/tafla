import { describe, expect, inject, test } from 'vitest';
import { fc, it } from '@fast-check/vitest';
import { ByteBuffer, DecodeError, TableBuilder, TableLayout } from 'tafla';
import { MAX_VALUES, SLICES, type Slice, sliceOf } from './conformance/arbitraries.ts';
import type { OffsetType } from './conformance/description.ts';

const IS_LITTLE_ENDIAN = new Uint8Array(new Uint16Array([1]).buffer)[0] === 1;

it('should share the storage of aligned arrays, except swapped copies on a big-endian host', () => {
    const builder = new TableBuilder(2);
    builder.setUint8Array(new Uint8Array([1, 2, 3]));
    builder.setFloat64Array(new Float64Array([1.5]));

    const bytes = builder.bytes();
    const buffer = new ByteBuffer(bytes);
    const uint8s = buffer.getUint8Array(buffer.rootOffset(0));
    const float64s = buffer.getFloat64Array(buffer.rootOffset(1));

    expect(uint8s.buffer).toBe(bytes.buffer);
    expect(float64s.buffer === bytes.buffer).toBe(IS_LITTLE_ENDIAN);
});

it('should reject impossible vector counts before anything allocates for them', () => {
    const buffer = new ByteBuffer(new Uint8Array([255, 255, 255, 255]));

    // Allocating the offsets first would fail with a different error
    expect(() => buffer.getVectorLength(0)).toThrow('Read exceeds the supplied buffer');
    expect(() => buffer.getVectorOffsets(0)).toThrow('Read exceeds the supplied buffer');
    expect(() => buffer.getVector(0)).toThrow('Read exceeds the supplied buffer');
});

it.each([-1, 2, 0.5])('should reject item %s of a vector of 2 with a RangeError', index => {
    const builder = new TableBuilder(1);
    builder.setVector([new Uint8Array([1]), new Uint8Array([2, 3])]);
    const buffer = builder.finalize();
    const error = new RangeError(`Item ${index} is outside the vector of 2 at 4`);

    expect(() => buffer.getVectorOffset(buffer.rootOffset(0), index)).toThrow(error);
    expect(() => buffer.getVectorItemOffset(buffer.rootOffset(0), 2, index)).toThrow(error);
});

it.each([0, 3])(
    "should throw DecodeError for item %s of a vector whose count the bytes can't hold",
    index => {
        // A count of 3, whose ends need 12 bytes after it, where 4 are left
        const buffer = new ByteBuffer(new Uint8Array([3, 0, 0, 0, 8, 0, 0, 0]));

        expect(() => buffer.getVectorOffset(0, index)).toThrow(DecodeError);
    }
);

it('should find each vector item from its ends, as reading every item finds it', () => {
    const builder = new TableBuilder(1);
    builder.setVector([new Uint8Array([1]), new Uint8Array(0), new Uint8Array([2, 3])]);
    const buffer = builder.finalize();
    const vector = buffer.rootOffset(0);

    // The count and the three ends, at 4, then the items from 20
    expect(buffer.getVectorOffsets(vector)).toStrictEqual([20, 21, 21]);
    expect([0, 1, 2].map(index => buffer.getVectorOffset(vector, index))).toStrictEqual([
        20, 21, 21
    ]);
    expect([0, 1, 2].map(index => buffer.getVectorItemOffset(vector, 3, index))).toStrictEqual([
        20, 21, 21
    ]);
    expect(buffer.getVectorLength(vector)).toBe(3);
    expect(buffer.getVector(vector)).toStrictEqual([
        new Uint8Array([1]),
        new Uint8Array(0),
        new Uint8Array([2, 3])
    ]);
});

// A buffer of many tables can hold a table past 4 GiB, whose positions don't
// fit in 32 bits, though its own offsets do
test.runIf(inject('largeBufferTests'))(
    'should find vector items past 4 GiB in a larger buffer',
    () => {
        const builder = new TableBuilder(1);
        builder.setVector([new Uint8Array([1]), new Uint8Array([2, 3])]);
        const table = builder.finalize();
        const position = 2 ** 32;

        // Allocated zeroed, so the bytes before the table take no memory
        // until they're written
        const bytes = new Uint8Array(position + table.bytes().length);
        bytes.set(table.bytes(), position);

        const buffer = new ByteBuffer(bytes);
        const vector = position + table.rootOffset(0);
        // The count and the two ends, after the vector at 4
        const expected = [position + 16, position + 17];

        expect(buffer.getVectorOffsets(vector)).toStrictEqual(expected);
        expect([0, 1].map(index => buffer.getVectorOffset(vector, index))).toStrictEqual(expected);
        expect([0, 1].map(index => buffer.getVectorItemOffset(vector, 2, index))).toStrictEqual(
            expected
        );
        expect(buffer.getVector(vector)).toStrictEqual([
            new Uint8Array([1]),
            new Uint8Array([2, 3])
        ]);
    }
);

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
            const layout = TableLayout.create({ versioned: true, offsetType });

            expect(() => new TableBuilder(1, layout, version)).toThrow(
                new RangeError(`Version ${version} doesn't fit in a ${offsetType} slot`)
            );
        }
    );

    it('should reject reading the version of an unversioned table', () => {
        const layout = TableLayout.create({ sized: true });
        const buffer = new ByteBuffer(new TableBuilder(0, layout).bytes(), layout);

        expect(() => buffer.getVersion()).toThrow(new TypeError('The table has no version'));
    });

    it('should require a version exactly when the layout has a slot for it', () => {
        const versioned = TableLayout.create({ versioned: true });
        const unversioned = TableLayout.create({ sized: true });

        // @ts-expect-error: A versioned layout needs a version
        expect(() => new TableBuilder(1, versioned)).toThrow(TypeError);
        // @ts-expect-error: The layout has no version slot
        expect(() => new TableBuilder(1, unversioned, 1)).toThrow(TypeError);
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
        const builder = new TableBuilder(1, layout);
        // A string is stored as an array of its bytes is
        builder.setUint8Array(bytes);
        const buffer = new ByteBuffer(sliceOf(builder.bytes(), slice), layout);
        const position = buffer.rootOffset(0);

        return () => buffer.getString(position);
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

    // Either side of the longest string read a byte at a time, 32 bytes
    it.each([0, 1, 31, 32, 33, 64])(
        'should read %i bytes of ASCII, or with a character of 2 bytes first or last, as UTF-8',
        length => {
            const ascii = 'tafla-0123456789'.repeat(4).slice(0, length);
            const strings = [ascii];
            if (length >= 2) {
                strings.push(`é${ascii.slice(2)}`, `${ascii.slice(2)}é`);
            }

            for (const string of strings) {
                const bytes = encoder.encode(string);
                expect(bytes.length).toBe(length);
                expect(readString(bytes)()).toBe(string);
            }
        }
    );

    it.each([1, 31, 32, 33, 64])(
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

    it('should keep a leading U+FEFF and NUL bytes', () => {
        for (const string of ['\uFEFFtafla', '\0', 'ta\0fla\0']) {
            expect(readString(encoder.encode(string))()).toBe(string);
        }
    });
});
