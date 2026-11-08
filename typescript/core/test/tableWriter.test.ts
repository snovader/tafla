import { describe, expect } from 'vitest';
import { fc, it } from '@fast-check/vitest';
import { TableBuilder, TableLayout, TableSizer, TableWriter } from 'tafla';
import { growing } from './arrayLikes.ts';
import {
    ARRAY_FIELDS,
    FIELDS,
    fitsItsLayout,
    MAX_VALUES,
    OFFSET_TYPES,
    SLICES,
    sliceOf,
    tables
} from './conformance/arbitraries.ts';
import {
    type ArrayField,
    elementTypeOf,
    fromJsonNumber,
    type FieldDescription,
    isArrayField,
    type OffsetType,
    type TableDescription
} from './conformance/description.ts';
import { OFFSET_SIZES } from './conformance/format.ts';
import {
    buildTable,
    ELEMENT_SIZES,
    nestedInPlace,
    readTable,
    structAlignment,
    writeTable,
    writeTableInto,
    writeTableThrough
} from './conformance/tables.ts';

/** Tables whose nested tables a TableWriter writes in place */
function inPlaceTables(offsetTypes?: readonly OffsetType[]): fc.Arbitrary<TableDescription> {
    return tables({ offsetTypes }).map(nestedInPlace).filter(fitsItsLayout);
}

/** The bytes that `write` returns, or the class of the error that it throws */
function outcomeOf(write: () => Uint8Array): Uint8Array | string {
    try {
        return write();
    } catch (error) {
        return error instanceof Error ? error.constructor.name : String(error);
    }
}

// TableBuilder writes through a TableWriter too, so these compare the sizer's
// count of each table, padding included, with the builder's, and the calls
// that a sizer's writer takes with those that the builder makes. The bytes
// themselves are the conformance suite's.

it.prop([inPlaceTables()])(
    'should count each table as long as TableBuilder does, and write the same bytes',
    table => {
        expect(writeTable(table)).toStrictEqual(buildTable(table).bytes());
    }
);

it('should count the padding of a struct array, a nested table and a vector as TableBuilder does', () => {
    // A uint8 leaves a uint32[] 3 bytes short of its alignment, and a uint16
    // a nested table of float64s 1 byte short of its own
    const table: TableDescription = {
        layout: { sized: true, versioned: true, offsetType: 'uint16' },
        version: 7,
        fields: [
            { type: 'uint8', value: 1 },
            { type: 'uint32[]', value: [2, 3] },
            { type: 'struct[]', fields: ['uint16', 'float64'], items: [[4, 5]] },
            {
                type: 'table',
                from: 'builder',
                table: {
                    layout: { sized: false, versioned: false, offsetType: 'uint16' },
                    fields: [{ type: 'float64[]', value: [6] }]
                }
            },
            {
                type: 'vector',
                items: [
                    {
                        from: 'builder',
                        table: {
                            layout: { sized: false, versioned: false, offsetType: 'uint16' },
                            fields: [{ type: 'string', value: '🦀' }]
                        }
                    }
                ]
            }
        ]
    };

    const bytes = writeTable(table);
    expect(bytes).toStrictEqual(buildTable(table).bytes());
    expect(readTable(bytes, table)).toStrictEqual(table);
});

describe('strings', () => {
    // Any UTF-16 code units, lone surrogates too, around the length up to
    // which strings are written a byte at a time when they're ASCII
    const codeUnits = fc.integer({ min: 0, max: 0xffff }).map(unit => String.fromCharCode(unit));
    const strings = fc.oneof(
        fc.string({ unit: 'binary-ascii', maxLength: 80 }),
        fc.string({ unit: codeUnits, maxLength: 80 }),
        fc.string({ unit: 'binary', maxLength: 40 })
    );

    // TextEncoder, rather than TableBuilder, which encodes through the writer
    it.prop([strings, fc.constantFrom(...OFFSET_TYPES)])(
        'should write any string as TextEncoder encodes it, with lone surrogates as U+FFFD',
        (value, offsetType) => {
            const table: TableDescription = {
                layout: { sized: false, versioned: false, offsetType },
                fields: [{ type: 'string', value }]
            };
            fc.pre(fitsItsLayout(table));

            // The field's offset and its length, little-endian and as wide as
            // the offset type, and then its bytes
            const size = OFFSET_SIZES[offsetType];
            const unsigned = (integer: number): number[] =>
                Array.from({ length: size }, (_, i) => (integer >>> (8 * i)) & 0xff);
            const encoded = new TextEncoder().encode(value);
            const expected = Uint8Array.from([
                ...unsigned(size),
                ...unsigned(encoded.length),
                ...encoded
            ]);

            expect(writeTable(table)).toStrictEqual(expected);
            expect(buildTable(table).bytes()).toStrictEqual(expected);
        }
    );
});

describe('tables longer than the offsets address', () => {
    const NARROW_OFFSET_TYPES = ['uint8', 'uint16'] as const;

    // Fields after which a table's length doesn't depend on where they start
    const unpaddedFields = FIELDS.filter(
        field =>
            field.type !== 'table' &&
            !(isArrayField(field) && ELEMENT_SIZES[elementTypeOf(field.type)] > 1) &&
            !(field.type === 'struct[]' && structAlignment(field.fields) > 1)
    ).map(field => nestedInPlace({ layout: UINT8_LAYOUT, fields: [field] }).fields[0]);

    const tablesWithLastFields = fc.oneof(
        ...NARROW_OFFSET_TYPES.map(offsetType =>
            fc.record({ table: inPlaceTables([offsetType]), last: unpaddedFields })
        )
    );

    // Each run writes and builds tables of up to 64 KiB
    it.prop([tablesWithLastFields], { numRuns: 30 })(
        'should write a table as long as its offset type addresses, and throw RangeError for one a byte longer',
        ({ table, last }) => {
            const max = MAX_VALUES[table.layout.offsetType];
            const withBytes = (length: number): TableDescription =>
                nestedInPlace({ ...table, fields: [...table.fields, bytesField(length), last] });

            const shortest = withBytes(0);
            fc.pre(fitsItsLayout(shortest));
            const room = max - buildTable(shortest).bytes().length;

            const longest = withBytes(room);
            const bytes = writeTable(longest);
            expect(bytes).toHaveLength(max);
            // Compared in a loop, since the matcher compares 64 KiB slowly
            expect(sameBytes(bytes, buildTable(longest).bytes())).toBe(true);

            const tooLong = withBytes(room + 1);
            expect(() => writeTable(tooLong)).toThrow(
                new RangeError(
                    `The table's fields make it ${max + 1} bytes, too long for ${table.layout.offsetType} offsets`
                )
            );
        }
    );

    it.prop([inPlaceTables(NARROW_OFFSET_TYPES), fc.nat({ max: 300 })])(
        'should throw RangeError for any table that TableBuilder rejects as too long',
        (table, length) => {
            const longer = { ...table, fields: [...table.fields, bytesField(length)] };

            expect(outcomeOf(() => writeTable(longer))).toStrictEqual(
                outcomeOf(() => buildTable(longer).bytes())
            );
        }
    );

    it('should reject a table of uint32 offsets past 4 GiB before allocating it', () => {
        // The table would hold its offset, its length and 2^32 elements
        const sizer = new TableSizer(1);
        sizer.addInt8Array(MAX_VALUES.uint32 + 1);

        expect(() => sizer.writer()).toThrow(
            new RangeError(
                "The table's fields make it 4294967304 bytes, too long for uint32 offsets"
            )
        );
    });

    it.each(['uint8', 'uint16'] as const)(
        'should reject a header longer than %s offsets address, as TableBuilder does',
        offsetType => {
            const max = MAX_VALUES[offsetType];

            for (const sized of [false, true]) {
                const layout = TableLayout.create({ sized, offsetType });
                const fields = Math.floor(max / OFFSET_SIZES[offsetType]) - (sized ? 1 : 0);

                const error = `A header of ${fields + 1} fields doesn't fit in ${offsetType} offsets`;

                expect(() => new TableSizer(fields, layout)).not.toThrow();
                expect(() => new TableSizer(fields + 1, layout)).toThrow(error);
                expect(() => new TableWriter(new Uint8Array(max), fields, layout)).not.toThrow();
                expect(() => new TableWriter(new Uint8Array(max), fields + 1, layout)).toThrow(
                    error
                );
            }
        }
    );

    it.each(['uint8', 'uint16'] as const)(
        'should reject bytes longer than %s offsets address before writing the header',
        offsetType => {
            const max = MAX_VALUES[offsetType];
            const layout = TableLayout.create({ sized: true, offsetType });
            const bytes = new Uint8Array(max + 1).fill(99);
            const lengthSlot = bytes.subarray(0, OFFSET_SIZES[offsetType]);

            expect(() => new TableWriter(new Uint8Array(max), 0, layout)).not.toThrow();
            expect(() => new TableWriter(bytes, 0, layout)).toThrow(
                new RangeError(`A table of ${max + 1} bytes is too long for ${offsetType} offsets`)
            );
            expect(lengthSlot).toStrictEqual(new Uint8Array(lengthSlot.length).fill(99));
        }
    );
});

describe('versions', () => {
    const versioned = TableLayout.create({ versioned: true, offsetType: 'uint8' });

    it.each([
        ['a version that the layout has no slot for', TableLayout.DEFAULT, 1, TypeError],
        ['no version for a versioned layout', versioned, undefined, TypeError],
        ['a version too large for its slot', versioned, 256, RangeError],
        ['a version that is not an integer', versioned, 1.5, RangeError]
    ])('should reject %s, as TableBuilder does', (_, layout, version, error) => {
        const bytes = new Uint8Array(1);

        expect(() => new TableBuilder(0, layout as never, version as never)).toThrow(error);
        expect(() => new TableSizer(0, layout as never, version as never)).toThrow(error);
        expect(() => new TableWriter(bytes, 0, layout as never, version as never)).toThrow(error);
    });
});

describe('counts and sizes', () => {
    // Negative, fractional and non-finite numbers, and integers past those
    // that a double holds exactly
    const invalidCounts = fc.double().filter(count => !(Number.isSafeInteger(count) && count >= 0));

    it.prop([invalidCounts])(
        'should reject a number of fields that is not a non-negative integer, as TableBuilder does',
        fields => {
            const error = new RangeError(`A count must be a non-negative integer, not ${fields}`);

            expect(() => new TableBuilder(fields)).toThrow(error);
            expect(() => new TableSizer(fields)).toThrow(error);
            expect(() => new TableWriter(new Uint8Array(0), fields)).toThrow(error);
        }
    );

    it.prop([invalidCounts])(
        'should reject a count that is not a non-negative integer, and leave the sizer and writer as they were',
        count => {
            const sizer = new TableSizer(2);
            expect(() => sizer.addUint16Array(count)).toThrow(RangeError);
            expect(() => sizer.addStructArray(count, 2, 2)).toThrow(RangeError);
            expect(() => sizer.addVector(count)).toThrow(RangeError);
            expect(() => sizer.startTable(count)).toThrow(RangeError);
            sizer.addUint16Array(1);
            sizer.addVector(0);

            const writer = sizer.writer();
            expect(() => writer.setVector(1, count)).toThrow(RangeError);
            expect(() => writer.startTable(count)).toThrow(RangeError);
            writer.setUint16Array(0, [7]);
            writer.setVector(1, 0);

            const builder = new TableBuilder(2);
            builder.setUint16Array([7]);
            builder.setVector([]);
            expect(writer.bytes()).toStrictEqual(builder.bytes());
        }
    );

    it.each([0, -2, 1.5, Number.NaN, Number.POSITIVE_INFINITY])(
        'should reject an array of structs of %s bytes, as the writer does',
        size => {
            const error = new RangeError(`A struct's size must be a positive integer, not ${size}`);

            expect(() => new TableSizer(1).addStructArray(1, size, 1)).toThrow(error);
            expect(() =>
                new TableSizer(1).writer().setStructArray(0, new Uint8Array(0), size, 1)
            ).toThrow(error);
        }
    );

    it.each([-2, 1.5, Number.NaN, Number.POSITIVE_INFINITY])(
        'should reject a struct of %s bytes',
        size => {
            expect(() => new TableSizer(1).addStruct(size)).toThrow(
                new RangeError(`A struct's size must be a non-negative integer, not ${size}`)
            );
        }
    );

    it('should write a struct of 0 bytes as TableBuilder does', () => {
        const sizer = new TableSizer(2);
        sizer.addStruct(0);
        sizer.addUint8();
        const writer = sizer.writer();
        writer.setStruct(0, new Uint8Array(0));
        writer.setUint8(1, 7);

        const builder = new TableBuilder(2);
        builder.setStruct(new Uint8Array(0));
        builder.setUint8(7);
        expect(writer.bytes()).toStrictEqual(builder.bytes());
    });
});

/** A field padded to an alignment, which a test sizes, writes and builds */
interface FieldKind {
    name: string;
    add(sizer: TableSizer): void;
    set(writer: TableWriter, index: number): void;
    build(builder: TableBuilder): void;
}

const STRUCT = Uint8Array.of(1, 2, 3, 4);

const ALIGNED_KINDS: readonly FieldKind[] = [
    {
        name: 'struct[]',
        add: sizer => sizer.addStructArray(1, STRUCT.length, 4),
        set: (writer, index) => writer.setStructArray(index, STRUCT, STRUCT.length, 4),
        build: builder => builder.setStructArray(STRUCT, STRUCT.length, 4)
    },
    {
        name: 'table',
        add: sizer => {
            sizer.addTable(8);
            sizer.endTable(sizer.startTable(0));
        },
        set: (writer, index) => {
            writer.setTable(index, 8);
            writer.endTable(writer.startTable(0));
        },
        build: builder => builder.setTable(new TableBuilder(0), 8)
    }
];

/**
 * Writes a table of a uint8, a field of `kind` and a uint8, calling `before`
 * once the first uint8 is written, and returns its bytes, which it checks
 * are TableBuilder's
 */
function writeAround(kind: FieldKind, before: (writer: TableWriter) => void): Uint8Array {
    const sizer = new TableSizer(3);
    sizer.addUint8();
    kind.add(sizer);
    sizer.addUint8();

    const writer = sizer.writer();
    writer.setUint8(0, 9);
    before(writer);
    kind.set(writer, 1);
    writer.setUint8(2, 8);

    const builder = new TableBuilder(3);
    builder.setUint8(9);
    kind.build(builder);
    builder.setUint8(8);
    expect(writer.bytes()).toStrictEqual(builder.bytes());

    return writer.bytes();
}

describe('alignments', () => {
    const ALIGNMENTS = [0, 3, -4, 16, 1.5, Number.NaN];

    it.each(ALIGNMENTS)(
        'should reject an alignment of %s from the sizer and builder',
        alignment => {
            const error = new RangeError(`An alignment of ${alignment} isn't 1, 2, 4 or 8 bytes`);
            const invalid = alignment as 1;

            expect(() => new TableSizer(1).addTable(invalid)).toThrow(error);
            expect(() => new TableSizer(1).addStructArray(1, 4, invalid)).toThrow(error);
            expect(() => new TableBuilder(1).setTable(new Uint8Array(0), invalid)).toThrow(error);
            expect(() => new TableBuilder(1).setStructArray(STRUCT, 4, invalid)).toThrow(error);
        }
    );

    it.each(ALIGNMENTS.flatMap(alignment => ALIGNED_KINDS.map(kind => [alignment, kind] as const)))(
        'should reject an alignment of %s from the writer, and leave it where it was',
        (alignment, kind) => {
            const error = new RangeError(`An alignment of ${alignment} isn't 1, 2, 4 or 8 bytes`);
            const invalid = alignment as 1;

            writeAround(kind, writer => {
                if (kind.name === 'table') {
                    expect(() => writer.setTable(1, invalid)).toThrow(error);
                } else {
                    expect(() => writer.setStructArray(1, STRUCT, STRUCT.length, invalid)).toThrow(
                        error
                    );
                }
            });
        }
    );
});

describe('bytes that are part of a larger buffer', () => {
    it.prop([inPlaceTables(), fc.integer({ min: 1, max: 255 })])(
        'should write zero padding into bytes that are not zero',
        (table, fill) => {
            const expected = buildTable(table).bytes();
            const bytes = new Uint8Array(expected.length).fill(fill);

            expect(writeTableInto(table, bytes)).toStrictEqual(expected);
        }
    );

    it('should zero the padding before an array and a nested table in reused bytes', () => {
        // A uint8 leaves the uint32[]'s count 3 bytes short of its alignment,
        // and a uint16 the nested table of float64s 1 byte short of its own
        const table: TableDescription = {
            fields: [
                { type: 'uint8', value: 1 },
                { type: 'uint32[]', value: [2] },
                { type: 'uint16', value: 3 },
                {
                    type: 'table',
                    from: 'builder',
                    table: {
                        layout: { sized: false, versioned: false, offsetType: 'uint32' },
                        fields: [{ type: 'float64[]', value: [4] }]
                    }
                }
            ],
            layout: { sized: false, versioned: false, offsetType: 'uint32' }
        };
        const expected = buildTable(table).bytes();

        expect(writeTableInto(table, new Uint8Array(expected.length).fill(0xee))).toStrictEqual(
            expected
        );
    });

    it.prop([inPlaceTables(), SLICES])(
        'should write a table into a slice of a larger buffer, and nothing around it',
        (table, slice) => {
            const expected = writeTable(table);
            const bytes = sliceOf(new Uint8Array(expected.length), slice);

            expect(writeTableInto(table, bytes)).toStrictEqual(expected);
            expect(bytesAround(bytes)).toStrictEqual(filled(bytes, slice.fill));
        }
    );

    it.prop([inPlaceTables(), SLICES, fc.nat()])(
        'should throw RangeError for a slice shorter than the table, and write nothing past it',
        (table, slice, cut) => {
            const length = writeTable(table).length;
            fc.pre(length > 0);
            const bytes = sliceOf(new Uint8Array(length - 1 - (cut % length)), slice);

            expect(() => writeTableInto(table, bytes)).toThrow(RangeError);
            expect(bytesAround(bytes)).toStrictEqual(filled(bytes, slice.fill));
        }
    );

    it('should throw RangeError for an array past the slice before writing it', () => {
        const buffer = new Uint8Array(32).fill(99);
        const writer = new TableWriter(buffer.subarray(0, 8), 1);

        expect(() => writer.setUint8Array(0, Uint8Array.of(42, 43, 44, 45))).toThrow(
            new RangeError("The fields need 12 bytes, more than the table's 8")
        );
        expect(buffer.subarray(8)).toStrictEqual(new Uint8Array(24).fill(99));
    });
});

describe('arrays that grow after they are counted', () => {
    // Bytes whose offset in their buffer covers aligned and unaligned arrays
    it.prop([ARRAY_FIELDS, fc.nat(), fc.constantFrom(...OFFSET_TYPES), SLICES])(
        'should throw RangeError and write nothing in or past the bytes, leaving the writer where it was',
        (field, cut, offsetType, slice) => {
            fc.pre(field.value.length > 0);
            const counted = { ...field, value: field.value.slice(0, cut % field.value.length) };
            const table: TableDescription = {
                layout: { sized: false, versioned: false, offsetType },
                fields: [counted]
            };
            fc.pre(fitsItsLayout(table));

            const expected = writeTable(table);
            const bytes = sliceOf(new Uint8Array(expected.length), slice);
            const writer = new TableWriter(bytes, 1, TableLayout.create({ offsetType }));
            const before = new Uint8Array(bytes.buffer.slice(0));

            const grown = growing(valuesOf(field), counted.value.length);
            expect(() => setArray(writer, field, grown)).toThrow(RangeError);
            expect(new Uint8Array(bytes.buffer)).toStrictEqual(before);

            setArray(writer, counted, valuesOf(counted));
            expect(writer.bytes()).toStrictEqual(expected);
        }
    );

    it('should throw RangeError for an array-like whose length grows past the bytes', () => {
        const buffer = new Uint8Array(24).fill(99);
        const writer = new TableWriter(buffer.subarray(0, 9), 1);
        const values = growing([42, 43, 44, 45], 1);

        expect(() => writer.setUint8Array(0, values)).toThrow(RangeError);
        expect(buffer.subarray(4)).toStrictEqual(new Uint8Array(20).fill(99));
    });
});

describe('fields that differ from those sized', () => {
    it('should throw RangeError for an array longer than the one sized, and write none of it', () => {
        const sizer = new TableSizer(2);
        sizer.addUint16Array(1);
        sizer.addUint8();
        const writer = sizer.writer();

        expect(() => writer.setUint16Array(0, [1, 2, 3])).toThrow(
            new RangeError("The fields need 18 bytes, more than the table's 15")
        );
        writer.setUint16Array(0, [7]);
        writer.setUint8(1, 8);

        const builder = new TableBuilder(2);
        builder.setUint16Array([7]);
        builder.setUint8(8);
        expect(writer.bytes()).toStrictEqual(builder.bytes());
    });

    it('should throw RangeError for a string longer than the one sized', () => {
        const sizer = new TableSizer(1);
        sizer.addString('a');
        const writer = sizer.writer();

        expect(() => writer.setString(0, 'ab')).toThrow(RangeError);
    });

    it('should throw RangeError from bytes() for fields shorter than those sized', () => {
        const sizer = new TableSizer(1);
        sizer.addString('ab');
        const writer = sizer.writer();
        writer.setString(0, 'a');

        expect(() => writer.bytes()).toThrow(
            new RangeError("The fields wrote 9 of the table's 10 bytes")
        );
    });
});

describe('indices and item ends outside their table', () => {
    // The table's bytes are undefined then, but the writer's bytes bound them
    const positions = fc.oneof(
        fc.integer({ min: -4, max: 64 }),
        fc.integer({ min: -(2 ** 33), max: 2 ** 33 })
    );

    it.prop([inPlaceTables(), SLICES, fc.nat(), positions])(
        'should write nothing outside the bytes, whatever the index of a field or the item of an end',
        (table, slice, call, position) => {
            const length = writeTable(table).length;
            const counter = new IndexReplacer(-1, 0);
            writeTableThrough(table, new Uint8Array(length), writer => counter.wrap(writer));
            fc.pre(counter.calls() > 0);

            const replacer = new IndexReplacer(call % counter.calls(), position);
            const bytes = sliceOf(new Uint8Array(length), slice);
            const outcome = outcomeOf(() =>
                writeTableThrough(table, bytes, writer => replacer.wrap(writer))
            );

            expect(outcome instanceof Uint8Array || outcome === 'RangeError').toBe(true);
            expect(bytesAround(bytes)).toStrictEqual(filled(bytes, slice.fill));
        }
    );
});

/**
 * Passes a writer its calls, with the index of one field or the item of one
 * end replaced by another, as a caller other than the generated code might
 * pass it
 */
class IndexReplacer {
    readonly #call: number;
    readonly #value: number;
    #calls = 0;

    /** Replaces the index or item of call `call` of those that take one */
    constructor(call: number, value: number) {
        this.#call = call;
        this.#value = value;
    }

    /** The calls that took an index or item */
    calls(): number {
        return this.#calls;
    }

    wrap(writer: TableWriter): TableWriter {
        return new Proxy(writer, {
            get: (target, name) => {
                const member = Reflect.get(target, name);
                if (typeof member !== 'function') {
                    return member;
                }

                return (...args: unknown[]) => this.#forward(target, member, name, args);
            }
        });
    }

    #forward(
        writer: TableWriter,
        method: (...args: unknown[]) => unknown,
        name: string | symbol,
        args: unknown[]
    ): unknown {
        // setItemEnd(vector, item), and every other setter set*(index, ...)
        const argument = name === 'setItemEnd' ? 1 : String(name).startsWith('set') ? 0 : -1;
        if (argument >= 0 && this.#calls++ === this.#call) {
            args[argument] = this.#value;
        }

        return method.apply(writer, args);
    }
}

const UINT8_LAYOUT = { sized: false, versioned: false, offsetType: 'uint8' } as const;

/** A uint8[] of `length` bytes */
function bytesField(length: number): FieldDescription {
    return { type: 'uint8[]', value: new Array<number>(length).fill(0) };
}

/** The values of an array field, as its setter takes them */
function valuesOf({ type, value }: ArrayField): Array<number | bigint> {
    return type === 'int64[]' || type === 'uint64[]'
        ? value.map(element => BigInt(element))
        : value.map(fromJsonNumber);
}

/** Writes an array field, field 0, with its type's setter */
function setArray(
    writer: TableWriter,
    { type }: ArrayField,
    values: ArrayLike<number | bigint>
): void {
    const element = elementTypeOf(type);
    const setter = `set${element[0].toUpperCase()}${element.slice(1)}Array`;

    (writer[setter as keyof TableWriter] as (index: number, values: unknown) => void).call(
        writer,
        0,
        values
    );
}

/** The bytes of the buffer before and after `bytes` */
function bytesAround(bytes: Uint8Array): Uint8Array {
    const buffer = new Uint8Array(bytes.buffer);
    const end = bytes.byteOffset + bytes.length;

    return Uint8Array.of(...buffer.subarray(0, bytes.byteOffset), ...buffer.subarray(end));
}

/** As many bytes as there are around `bytes`, each `fill` */
function filled(bytes: Uint8Array, fill: number): Uint8Array {
    return new Uint8Array(bytes.buffer.byteLength - bytes.length).fill(fill);
}

function sameBytes(left: Uint8Array, right: Uint8Array): boolean {
    return left.length === right.length && left.every((byte, index) => byte === right[index]);
}

describe("conversions, as the README's table states them", () => {
    /** The bytes of field 0's value, from a builder and a writer of one field */
    function written(
        add: (sizer: TableSizer) => void,
        set: (writer: TableWriter) => void,
        build: (builder: TableBuilder) => void
    ): number[] {
        const sizer = new TableSizer(1);
        add(sizer);
        const writer = sizer.writer();
        set(writer);

        const builder = new TableBuilder(1);
        build(builder);
        expect(writer.bytes()).toStrictEqual(builder.bytes());

        return Array.from(builder.bytes().subarray(4));
    }

    it('should wrap an integer outside its field’s range', () => {
        for (const [value, byte] of [
            [300, 44],
            [-1, 255]
        ]) {
            expect(
                written(
                    sizer => sizer.addUint8(),
                    writer => writer.setUint8(0, value),
                    builder => builder.setUint8(value)
                )
            ).toStrictEqual([byte]);
        }
        expect(
            written(
                sizer => sizer.addUint8Array(2),
                writer => writer.setUint8Array(0, [300, -1]),
                builder => builder.setUint8Array([300, -1])
            )
        ).toStrictEqual([2, 0, 0, 0, 44, 255]);
        expect(
            written(
                sizer => sizer.addUint64(),
                writer => writer.setUint64(0, 2n ** 64n + 5n),
                builder => builder.setUint64(2n ** 64n + 5n)
            )
        ).toStrictEqual([5, 0, 0, 0, 0, 0, 0, 0]);
    });

    it('should truncate a fraction or NaN for an integer field', () => {
        expect(
            written(
                sizer => sizer.addUint8Array(2),
                writer => writer.setUint8Array(0, [1.5, Number.NaN]),
                builder => builder.setUint8Array([1.5, Number.NaN])
            )
        ).toStrictEqual([2, 0, 0, 0, 1, 0]);
        for (const [value, byte] of [
            [1.5, 1],
            [Number.NaN, 0]
        ]) {
            expect(
                written(
                    sizer => sizer.addUint8(),
                    writer => writer.setUint8(0, value),
                    builder => builder.setUint8(value)
                )
            ).toStrictEqual([byte]);
        }
    });

    it('should write a number too large for a float32 as infinity', () => {
        // Little-endian, as the format writes them on every host
        const infinity = [0x00, 0x00, 0x80, 0x7f];
        const negativeInfinity = [0x00, 0x00, 0x80, 0xff];

        expect(
            written(
                sizer => sizer.addFloat32(),
                writer => writer.setFloat32(0, 1e300),
                builder => builder.setFloat32(1e300)
            )
        ).toStrictEqual(infinity);
        expect(
            written(
                sizer => sizer.addFloat32Array(1),
                writer => writer.setFloat32Array(0, [-1e300]),
                builder => builder.setFloat32Array([-1e300])
            )
        ).toStrictEqual([1, 0, 0, 0, ...negativeInfinity]);
    });

    it('should write a lone surrogate as U+FFFD', () => {
        expect(
            written(
                sizer => sizer.addString('\ud800'),
                writer => writer.setString(0, '\ud800'),
                builder => builder.setString('\ud800')
            )
        ).toStrictEqual([3, 0, 0, 0, 0xef, 0xbf, 0xbd]);
    });

    it('should throw TypeError for a number for a 64-bit integer, or a bigint for another type', () => {
        const writer = (add: (sizer: TableSizer) => void): TableWriter => {
            const sizer = new TableSizer(1);
            add(sizer);
            return sizer.writer();
        };

        const builder = (set: (builder: TableBuilder) => void): TableBuilder => {
            const built = new TableBuilder(1);
            set(built);
            return built;
        };

        expect(() => builder(built => built.setInt64(1 as unknown as bigint)).bytes()).toThrow(
            TypeError
        );
        expect(() =>
            writer(sizer => sizer.addUint64()).setUint64(0, 1 as unknown as bigint)
        ).toThrow(TypeError);
        expect(() =>
            builder(built => built.setUint8Array([1n] as unknown as number[])).bytes()
        ).toThrow(TypeError);
        expect(() =>
            writer(sizer => sizer.addFloat64()).setFloat64(0, 1n as unknown as number)
        ).toThrow(TypeError);
        expect(() =>
            writer(sizer => sizer.addInt64Array(1)).setInt64Array(0, [1] as unknown as bigint[])
        ).toThrow(TypeError);
    });
});
