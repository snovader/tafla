import { expect } from 'hypotes';
import { fc, it } from 'hypotes/fast-check';
import {
    readFloat32,
    readFloat64,
    readInt8,
    readInt16,
    readInt32,
    readInt64,
    readUint8,
    readUint16,
    readUint32,
    readUint64,
    utf8Length,
    viewOf,
    writeFloat32,
    writeFloat64,
    writeInt8,
    writeInt16,
    writeInt32,
    writeInt64,
    writeUint8,
    writeUint16,
    writeUint32,
    writeUint64,
    writeUtf8,
    DecodeError,
    StructWriter
} from 'tafla';
import { sliceOf, SLICES, STRINGS } from './conformance/arbitraries.ts';
import { LONG_STRINGS, SHORTFALL_STRINGS } from './long-strings.ts';

type Setter = Extract<keyof StructWriter, `set${string}`>;

/** A struct's field: the setter that writes it and its value */
interface Field {
    setter: Setter;
    value: number | bigint;
}

interface Scalar {
    size: number;
    values: fc.Arbitrary<number | bigint>;
    /** Writes the value little-endian, as a reference */
    write: (view: DataView, position: number, value: never) => void;
}

const SCALARS: Readonly<Record<Setter, Scalar>> = {
    setInt8: {
        size: 1,
        values: fc.integer({ min: -0x80, max: 0x7f }),
        write: (view, position, value: number) => view.setInt8(position, value)
    },
    setUint8: {
        size: 1,
        values: fc.integer({ min: 0, max: 0xff }),
        write: (view, position, value: number) => view.setUint8(position, value)
    },
    setInt16: {
        size: 2,
        values: fc.integer({ min: -0x8000, max: 0x7fff }),
        write: (view, position, value: number) => view.setInt16(position, value, true)
    },
    setUint16: {
        size: 2,
        values: fc.integer({ min: 0, max: 0xffff }),
        write: (view, position, value: number) => view.setUint16(position, value, true)
    },
    setInt32: {
        size: 4,
        values: fc.integer({ min: -0x8000_0000, max: 0x7fff_ffff }),
        write: (view, position, value: number) => view.setInt32(position, value, true)
    },
    setUint32: {
        size: 4,
        values: fc.integer({ min: 0, max: 0xffff_ffff }),
        write: (view, position, value: number) => view.setUint32(position, value, true)
    },
    setInt64: {
        size: 8,
        values: fc.bigInt({ min: -(2n ** 63n), max: 2n ** 63n - 1n }),
        write: (view, position, value: bigint) => view.setBigInt64(position, value, true)
    },
    setUint64: {
        size: 8,
        values: fc.bigInt({ min: 0n, max: 2n ** 64n - 1n }),
        write: (view, position, value: bigint) => view.setBigUint64(position, value, true)
    },
    setFloat32: {
        size: 4,
        values: fc.float(),
        write: (view, position, value: number) => view.setFloat32(position, value, true)
    },
    setFloat64: {
        size: 8,
        values: fc.double(),
        write: (view, position, value: number) => view.setFloat64(position, value, true)
    }
};

/** Fields of each type, with the value of the same field of another struct */
const FIELD_PAIRS = fc.oneof(
    ...Object.entries(SCALARS).map(([setter, { values }]) =>
        fc.tuple(values, values).map(([first, second]): [Field, Field] => [
            { setter: setter as Setter, value: first },
            { setter: setter as Setter, value: second }
        ])
    )
);

it.prop([fc.array(FIELD_PAIRS, { maxLength: 8 })])(
    'should write scalars one after another, little-endian, and copy each struct',
    pairs => {
        const firsts = pairs.map(([first]) => first);
        const seconds = pairs.map(([, second]) => second);
        const writer = new StructWriter(new Uint8Array(sizeOf(firsts)));

        writeStruct(writer, firsts);
        const first = writer.bytes();
        writeStruct(writer, seconds);
        const second = writer.bytes();

        // bytes() rewinds, so the second struct starts at the start too, and
        // copies, so writing it left the first struct's bytes as they were
        expect(first).toStrictEqual(littleEndian(firsts));
        expect(second).toStrictEqual(littleEndian(seconds));
    }
);

it.prop([fc.array(FIELD_PAIRS, { minLength: 1, maxLength: 8 })])(
    'should write the next struct from the start after reset() discards an unfinished one',
    pairs => {
        const firsts = pairs.map(([first]) => first);
        const seconds = pairs.map(([, second]) => second);
        const writer = new StructWriter(new Uint8Array(sizeOf(firsts)));

        writeStruct(writer, firsts.slice(0, -1));
        writer.reset();
        writeStruct(writer, seconds);

        expect(writer.bytes()).toStrictEqual(littleEndian(seconds));
    }
);

it.prop([fc.array(FIELD_PAIRS, { minLength: 1, maxLength: 8 }), SLICES])(
    'should write within a slice of a larger buffer, and throw RangeError for a scalar past it without writing any of it',
    (pairs, slice) => {
        const fields = pairs.map(([first]) => first);
        const fits = fields.slice(0, -1);
        const bytes = sliceOf(new Uint8Array(sizeOf(fields) - 1), slice);
        const writer = new StructWriter(bytes);

        writeStruct(writer, fits);
        const before = new Uint8Array(bytes.buffer.slice(0));

        expect(() => writeStruct(writer, fields.slice(-1))).toThrow(RangeError);
        expect(new Uint8Array(bytes.buffer)).toStrictEqual(before);
        expect(writer.bytes().subarray(0, sizeOf(fits))).toStrictEqual(littleEndian(fits));
    }
);

function writeStruct(writer: StructWriter, fields: Field[]): void {
    for (const { setter, value } of fields) {
        (writer[setter] as (value: number | bigint) => void).call(writer, value);
    }
}

/** The bytes of a struct of `fields`, as DataView writes them */
function littleEndian(fields: Field[]): Uint8Array {
    const bytes = new Uint8Array(sizeOf(fields));
    const view = new DataView(bytes.buffer);

    let position = 0;
    for (const { setter, value } of fields) {
        const { size, write } = SCALARS[setter];
        write(view, position, value as never);
        position += size;
    }

    return bytes;
}

function sizeOf(fields: Field[]): number {
    return fields.reduce((size, { setter }) => size + SCALARS[setter].size, 0);
}

/** Each read helper, the size it reads and DataView's little-endian read */
const READS: ReadonlyArray<
    [
        read: (bytes: Uint8Array, offset: number) => number | bigint,
        size: number,
        reference: (view: DataView, offset: number) => number | bigint
    ]
> = [
    [readInt8, 1, (view, offset) => view.getInt8(offset)],
    [readUint8, 1, (view, offset) => view.getUint8(offset)],
    [readInt16, 2, (view, offset) => view.getInt16(offset, true)],
    [readUint16, 2, (view, offset) => view.getUint16(offset, true)],
    [readInt32, 4, (view, offset) => view.getInt32(offset, true)],
    [readUint32, 4, (view, offset) => view.getUint32(offset, true)],
    [readInt64, 8, (view, offset) => view.getBigInt64(offset, true)],
    [readUint64, 8, (view, offset) => view.getBigUint64(offset, true)],
    [readFloat32, 4, (view, offset) => view.getFloat32(offset, true)],
    [readFloat64, 8, (view, offset) => view.getFloat64(offset, true)]
];

it.prop([fc.uint8Array({ maxLength: 12 }), fc.integer({ min: -2, max: 14 }), SLICES])(
    'should read little-endian values within the bytes, and throw DecodeError past them',
    (bytes, offset, slice) => {
        const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
        const sliced = sliceOf(bytes, slice);

        for (const [read, size, reference] of READS) {
            if (offset >= 0 && offset + size <= bytes.length) {
                // toBe() compares with Object.is, so NaN equals NaN and -0 isn't 0
                expect(read(sliced, offset)).toBe(reference(view, offset));
            } else {
                expect(() => read(sliced, offset)).toThrow(DecodeError);
                expect(() => read(sliced, offset)).toThrow(
                    `Read exceeds the supplied buffer: ${size} bytes at ${offset} of ${bytes.length}`
                );
            }
        }
    }
);

type Write = (bytes: Uint8Array, offset: number, value: never) => void;

/** Each write function, and the scalar that it writes */
const WRITES: ReadonlyArray<[write: Write, scalar: Scalar]> = [
    [writeInt8, SCALARS.setInt8],
    [writeUint8, SCALARS.setUint8],
    [writeInt16, SCALARS.setInt16],
    [writeUint16, SCALARS.setUint16],
    [writeInt32, SCALARS.setInt32],
    [writeUint32, SCALARS.setUint32],
    [writeInt64, SCALARS.setInt64],
    [writeUint64, SCALARS.setUint64],
    [writeFloat32, SCALARS.setFloat32],
    [writeFloat64, SCALARS.setFloat64]
];

/** A write function, and a value of the scalar that it writes */
const WRITES_OF_VALUES = fc.oneof(
    ...WRITES.map(([write, scalar]) => scalar.values.map(value => ({ write, scalar, value })))
);

it.prop([WRITES_OF_VALUES, fc.nat({ max: 12 }), fc.integer({ min: -2, max: 14 }), SLICES])(
    'should write little-endian values within the bytes, and throw RangeError past them, writing nothing',
    ({ write, scalar, value }, length, offset, slice) => {
        const bytes = sliceOf(new Uint8Array(length), slice);
        // The whole buffer, so that a write around the slice shows
        const expected = new Uint8Array(bytes.buffer.slice(0));

        if (offset >= 0 && offset + scalar.size <= length) {
            const view = new DataView(expected.buffer, bytes.byteOffset, length);
            scalar.write(view, offset, value as never);

            write(bytes, offset, value as never);
        } else {
            expect(() => write(bytes, offset, value as never)).toThrow(
                new RangeError(
                    `Write exceeds the supplied buffer: ${scalar.size} bytes at ${offset} of ${length}`
                )
            );
        }

        expect(new Uint8Array(bytes.buffer)).toStrictEqual(expected);
    }
);

it.each([0.5, Number.NaN, Number.POSITIVE_INFINITY, 2 ** 53])(
    'should throw RangeError for an offset of %s, writing nothing',
    offset => {
        const bytes = new Uint8Array(8);

        expect(() => writeUint8(bytes, offset, 1)).toThrow(RangeError);
        expect(() => writeFloat32(bytes, offset, 1)).toThrow(RangeError);
        expect(() => writeUint64(bytes, offset, 1n)).toThrow(RangeError);
        expect(bytes).toStrictEqual(new Uint8Array(8));
    }
);

const ENCODER = new TextEncoder();

it.prop([
    fc.oneof(STRINGS, fc.string({ unit: 'binary' })),
    fc.nat({ max: 8 }),
    fc.nat({ max: 40 }),
    SLICES
])(
    'should write a string as UTF-8 at an offset within the bytes, returning its utf8Length(), and throw RangeError past them, having written only what fits of it',
    (string, offset, length, slice) => {
        const bytes = sliceOf(new Uint8Array(length), slice);
        // The whole buffer, so that a write around the slice shows
        const around = new Uint8Array(bytes.buffer.slice(0));
        // TextEncoder replaces lone surrogates with U+FFFD, as tables hold them
        const encoded = ENCODER.encode(string);
        expect(utf8Length(string)).toBe(encoded.length);

        if (offset + encoded.length <= length) {
            expect(writeUtf8(bytes, offset, string)).toBe(encoded.length);
            expect(bytes.subarray(offset, offset + encoded.length)).toStrictEqual(encoded);
        } else {
            expect(() => writeUtf8(bytes, offset, string)).toThrow(RangeError);
            expectStartOf(bytes.subarray(offset), encoded);
        }

        around.set(bytes, bytes.byteOffset);
        expect(new Uint8Array(bytes.buffer)).toStrictEqual(around);
    }
);

/**
 * Expects `bytes`, which were zeros, to hold bytes from the start of
 * `encoded` and zeros after them, as `writeUtf8()` leaves them when the
 * string doesn't fit
 */
function expectStartOf(bytes: Uint8Array, encoded: Uint8Array): void {
    let written = 0;
    while (written < bytes.length && bytes[written] === encoded[written]) {
        written++;
    }

    expect(bytes.subarray(written)).toStrictEqual(new Uint8Array(bytes.length - written));
}

it.each([
    ['a character of 3 bytes', 'a€', [0x61, 0xe2, 0x82]],
    ['a surrogate pair', 'a😀', [0x61, 0xf0, 0x9f, 0x98]]
])(
    'should write the first bytes of %s that ends past the bytes of a short string, and throw RangeError',
    (_, string, start) => {
        const bytes = new Uint8Array(start.length);

        expect(() => writeUtf8(bytes, 0, string)).toThrow(RangeError);
        expect(bytes).toStrictEqual(Uint8Array.from(start));
    }
);

/**
 * Expects `writeUtf8()` to write `string` at `offset` in bytes that end where
 * it does, as the code that tafla-gen generates sizes them, and to throw
 * RangeError in a byte fewer
 */
function expectWrittenExactly(string: string, offset: number): void {
    const encoded = ENCODER.encode(string);
    const bytes = new Uint8Array(offset + encoded.length);

    expect(writeUtf8(bytes, offset, string)).toBe(encoded.length);
    expect(bytes.subarray(offset)).toStrictEqual(encoded);
    expect(() => writeUtf8(bytes.subarray(0, -1), offset, string)).toThrow(RangeError);
}

it.prop([LONG_STRINGS, fc.nat({ max: 8 })])(
    'should write a long string into bytes of exactly its length, and throw RangeError in a byte fewer',
    expectWrittenExactly
);

it.each(Object.entries(SHORTFALL_STRINGS))(
    'should write a %s string that encodeInto() stops short of into bytes of exactly its length',
    (_, string) => {
        expectWrittenExactly(string, 0);
    }
);

// Short strings are written a code unit at a time, and strings of more than
// 64 units are counted by encoding them into 4,096 bytes at a time
it.each([
    [
        'the first and last code points of 2, 3 and 4 bytes',
        '\u0080\u07FF\u0800\uFFFF\u{10000}\u{10FFFF}'
    ],
    ['lone surrogates first, last, and in the wrong order', '\uDC00a\uDC00\uD800b\uD800'],
    ['32 surrogates, as 16 pairs', '😀'.repeat(16)],
    ['32 lone surrogates', '\uD800'.repeat(32)]
])('should write a short string of %s as TextEncoder encodes it', (_, string) => {
    for (const offset of [0, 3]) {
        expectWrittenExactly(string, offset);
    }
    expect(utf8Length(string)).toBe(ENCODER.encode(string).length);
});

it.prop([
    fc.string({
        unit: fc.constantFrom('a', 'é', '€', '😀', '\uD800', '\uDC00'),
        minLength: 65,
        maxLength: 5000,
        size: 'max'
    })
])('should count the UTF-8 length of a long string as TextEncoder encodes it', string => {
    expect(utf8Length(string)).toBe(ENCODER.encode(string).length);
});

it.each([
    ['ASCII', 'a'],
    ['characters of 2 bytes', 'é'],
    ['characters of 3 bytes', '€'],
    ['surrogate pairs', '😀'],
    ['lone surrogates', '\uD800']
])('should count the UTF-8 length of strings of %s around 4,096 bytes', (_, unit) => {
    const bytes = ENCODER.encode(unit).length;

    for (const count of [
        64, 65, 1023, 1024, 1025, 1365, 1366, 2047, 2048, 2049, 4095, 4096, 4097, 9000
    ]) {
        // With and without a byte before, which moves each character's bytes
        for (const string of [unit.repeat(count), `a${unit.repeat(count)}`]) {
            expect(utf8Length(string)).toBe(bytes * count + string.length - unit.length * count);
        }
    }
});

it.each([-1, 0.5, Number.NaN, 9])(
    'should throw RangeError for a string at an offset of %s',
    offset => {
        expect(() => writeUtf8(new Uint8Array(8), offset, '')).toThrow(RangeError);
    }
);

it.prop([
    fc.nat({ max: 16 }),
    fc.integer({ min: -2, max: 18 }),
    fc.integer({ min: -1, max: 18 }),
    SLICES
])(
    'should make a DataView of bytes within the bytes, and throw RangeError for any past them',
    (length, offset, byteLength, slice) => {
        const bytes = sliceOf(new Uint8Array(length), slice);

        if (offset >= 0 && byteLength >= 0 && offset + byteLength <= length) {
            const view = viewOf(bytes, offset, byteLength);
            expect(view.buffer).toBe(bytes.buffer);
            expect(view.byteOffset).toBe(bytes.byteOffset + offset);
            expect(view.byteLength).toBe(byteLength);
        } else {
            expect(() => viewOf(bytes, offset, byteLength)).toThrow(
                new RangeError(
                    `Write exceeds the supplied buffer: ${byteLength} bytes at ${offset} of ${length}`
                )
            );
        }
    }
);
