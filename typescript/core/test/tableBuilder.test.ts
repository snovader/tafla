import { describe, expect, vi } from 'vitest';
import { fc, it } from '@fast-check/vitest';
import { ByteBuffer, TableBuilder, type TableItem, TableLayout } from 'tafla';
import {
    FIELDS,
    fitsItsLayout,
    MAX_VALUES,
    OFFSET_TYPES,
    tables
} from './conformance/arbitraries.ts';
import type {
    FieldDescription,
    LayoutDescription,
    OffsetType,
    ScalarType,
    TableDescription
} from './conformance/description.ts';
import { OFFSET_SIZES } from './conformance/format.ts';
import {
    alignmentOf,
    buildTable,
    buildTableInPlace,
    createBuilder,
    nestedInPlace,
    setField,
    setFieldInPlace,
    writeTable
} from './conformance/tables.ts';

/**
 * Fields of about 1 KiB, around the length from which the builder keeps an
 * array or an array of structs until the table is written, rather than
 * copying it when it's set, and strings around the length from which it
 * counts their UTF-8 rather than giving them room for their longest
 */
const KIB_FIELDS: fc.Arbitrary<FieldDescription> = fc.oneof(
    fc.integer({ min: 1000, max: 1100 }).map(length => arrayField('uint8', length)),
    fc.integer({ min: 120, max: 140 }).map(length => arrayField('float64', length)),
    fc.integer({ min: 100, max: 130 }).map(
        (count): FieldDescription => ({
            type: 'struct[]',
            fields: ['float64', 'uint8'],
            items: Array.from({ length: count }, () => [1, 2])
        })
    ),
    fc
        .integer({ min: 1000, max: 1100 })
        .map((length): FieldDescription => ({ type: 'string', value: 'é'.repeat(length) }))
);

/** Tables, nested from builders or bytes, with fields of about 1 KiB */
const tablesWithKibFields = fc
    .tuple(
        tables({ offsetTypes: ['uint16', 'uint32'] }),
        fc.array(KIB_FIELDS, { minLength: 1, maxLength: 2 })
    )
    .map(([table, long]) => ({ ...table, fields: [...table.fields, ...long] }))
    .filter(fitsItsLayout);

it('should reject a field beyond the number the table holds', () => {
    const builder = new TableBuilder(1);
    builder.setUint8(1);

    expect(() => builder.setUint8(2)).toThrow(new RangeError("Can't add field 2 to a table of 1"));
});

it.prop([fc.nat({ max: 4 })])('should reject finalizing before every field is set', set => {
    const builder = new TableBuilder(set + 1);
    for (let i = 0; i < set; i++) {
        builder.setFloat64(i);
    }

    expect(() => builder.finalize()).toThrow(
        `Only ${set} of the table's ${set + 1} fields were set`
    );
});

it('should reject setting a field after finalizing', () => {
    const builder = new TableBuilder(0);
    builder.finalize();

    expect(() => builder.setInt8(1)).toThrow(TypeError);
});

it('should finalize once and return the same buffer', () => {
    const builder = new TableBuilder(1);
    builder.setString('once');

    expect(builder.finalize()).toBe(builder.finalize());
    expect(builder.bytes()).toBe(builder.finalize().bytes());
});

it('should throw TypeError from the setter of a number for an int64, and leave the builder as it was', () => {
    const builder = new TableBuilder(1);

    expect(() => builder.setInt64(1 as unknown as bigint)).toThrow(TypeError);
    builder.setInt64(1n);
    expect(builder.bytes()).toStrictEqual(writeTable(tableOf([{ type: 'int64', value: '1' }])));
});

describe('strings', () => {
    // A string with lone surrogates that TextEncoder.encodeInto() stops a
    // code unit short of when it's given exactly its UTF-8 length, in Node 26
    // and 27, as about 1% of such strings of 300 units or more. Its symbols
    // index SYMBOLS.
    const SYMBOLS = ['a', 'é', '€', '😀', '\udc00', '\ud800'];
    const SHORT_BY_A_UNIT = [
        '4225552353214043133003054111452042500042325143235501302505252510' +
            '3240511413522215255212200412001010550144140132555220235131333504' +
            '0244550301340250221304232042134034043504402211334424233214055030' +
            '245230523325155542425055111152130'
    ]
        .join('')
        .split('')
        .map(symbol => SYMBOLS[Number(symbol)])
        .join('');

    it.each([
        ['on its own', ''],
        // Over 128 KiB of UTF-8, twice the longest buffer that a builder
        // leaves for the next, so that the buffer grows to the string's end
        ['after 45,000 three-byte characters', '€'.repeat(45000)]
    ])(
        'should write a string with lone surrogates that a TextEncoder takes a byte more than its UTF-8 for, %s',
        (_, before) => {
            const string = before + SHORT_BY_A_UNIT;
            const builder = new TableBuilder(1);
            builder.setString(string);

            const buffer = builder.finalize();
            expect(buffer.getString(buffer.rootOffset(0))).toBe(string.toWellFormed());
        }
    );
});

it('should write zeros as padding, rather than the bytes of a table built before it', () => {
    // Leaves a buffer of 64 KiB of 0xff for the next builder
    const filled = new TableBuilder(60);
    for (let i = 0; i < 60; i++) {
        filled.setUint8Array(new Uint8Array(1000).fill(0xff));
    }
    filled.bytes();

    // Padding before each array, the array of structs and the nested table
    const table = tableOf([
        { type: 'uint8', value: 1 },
        { type: 'float64[]', value: [2] },
        { type: 'uint8', value: 3 },
        { type: 'struct[]', fields: ['float64'], items: [[4]] },
        { type: 'uint8', value: 5 },
        {
            type: 'table',
            from: 'builder',
            table: tableOf([
                { type: 'uint8', value: 6 },
                { type: 'float64[]', value: [7] }
            ])
        }
    ]);
    expect(buildTable(table).bytes()).toStrictEqual(writeTable(table));
    expect(buildTableInPlace(table).bytes()).toStrictEqual(writeTable(table));
});

// Arrays, arrays of structs, structs and tables given as bytes of 1 KiB or
// more are kept until the table is written, and shorter ones copied when
// they're set
describe('values of 1 KiB or more', () => {
    it('should copy a shorter array when it is set, and write a longer one as it is when the table is written', () => {
        const short = new Float32Array([1, 2]);
        const long = new Float32Array(256).fill(1);
        const builder = new TableBuilder(2);
        builder.setFloat32Array(short);
        builder.setFloat32Array(long);
        short[0] = 3;
        long[0] = 3;

        const buffer = builder.finalize();
        expect(buffer.getFloat32Array(buffer.rootOffset(0))).toStrictEqual(
            new Float32Array([1, 2])
        );
        expect(buffer.getFloat32Array(buffer.rootOffset(1))[0]).toBe(3);
    });

    it('should write a struct as it is when the table is written', () => {
        const struct = Uint8Array.from({ length: 1500 }, (_, i) => i);
        const builder = new TableBuilder(2);
        builder.setStruct(struct);
        builder.setUint8(7);
        struct[0] = 99;

        const buffer = builder.finalize();
        expect(buffer.getStruct(buffer.rootOffset(0), struct.length)).toStrictEqual(struct);
        expect(buffer.getUint8(buffer.rootOffset(1))).toBe(7);
    });

    it.each([
        ['grew', 257],
        ['shrank', 255]
    ])(
        'should throw RangeError for an array that %s, and write it once it is restored',
        (_, length) => {
            const values = new Array<number>(256).fill(1);
            const builder = new TableBuilder(2);
            builder.setFloat64Array(values);
            builder.setUint8(7);

            values.length = length;
            values.fill(1);
            const message = `An array of 256 elements held ${length} when the table was written`;
            expect(() => builder.bytes()).toThrow(new RangeError(message));
            expect(() => builder.finalize()).toThrow(new RangeError(message));

            values.length = 256;
            values.fill(1);
            expect(builder.bytes()).toStrictEqual(
                writeTable(
                    tableOf([
                        { type: 'float64[]', value: values },
                        { type: 'uint8', value: 7 }
                    ])
                )
            );
        }
    );

    it('should throw RangeError for a typed array whose buffer was transferred, and stay unfinalized', () => {
        const values = new Float64Array(256);
        const builder = new TableBuilder(2);
        builder.setFloat64Array(values);
        builder.setUint8(7);

        values.buffer.transfer();
        expect(() => builder.finalize()).toThrow(
            new RangeError('An array of 256 elements held 0 when the table was written')
        );
        // It's still unfinalized, so a field past its last is a RangeError
        // rather than a finalized table's TypeError
        expect(() => builder.setUint8(1)).toThrow(
            new RangeError("Can't add field 3 to a table of 2")
        );
    });

    it("should copy a nested builder's table when it is nested, with its arrays of 1 KiB or more as they are when its parent is written", () => {
        const short = new Float32Array([1, 2]);
        const long = new Float32Array(256).fill(1);
        const nested = new TableBuilder(2);
        nested.setFloat32Array(short);
        nested.setFloat32Array(long);

        const parent = new TableBuilder(1);
        parent.setTable(nested);
        short[0] = 3;
        long[0] = 3;

        const buffer = parent.finalize();
        const table = buffer.rootOffset(0);
        expect(buffer.getFloat32Array(buffer.fieldOffset(table, 0))).toStrictEqual(
            new Float32Array([1, 2])
        );
        expect(buffer.getFloat32Array(buffer.fieldOffset(table, 1))[0]).toBe(3);
    });
});

describe('arrays of structs', () => {
    it.each([
        [new Uint8Array(5), 2, "5 bytes aren't a whole number of 2-byte structs"],
        [new Uint8Array(4), 0, "A struct's size must be a positive integer, not 0"],
        [new Uint8Array(4), 1.5, "A struct's size must be a positive integer, not 1.5"]
    ])(
        'should reject %s bytes of %s-byte structs, and leave the builder as it was',
        (bytes, size, message) => {
            const builder = new TableBuilder(1);

            expect(() => builder.setStructArray(bytes, size, 1)).toThrow(new RangeError(message));
            builder.setStructArray(new Uint8Array([1, 2, 3, 4]), 2, 2);
            expect(builder.bytes()).toStrictEqual(
                new Uint8Array([4, 0, 0, 0, 2, 0, 0, 0, 1, 2, 3, 4])
            );
        }
    );

    it('should count structs, and pad so that they start at a multiple of their alignment', () => {
        const builder = new TableBuilder(2);
        builder.setUint8(7);
        builder.setStructArray(new Uint8Array(16), 8, 8);

        // The count goes at 12, after the uint8 at 8 and 3 bytes of padding,
        // so that the structs start at 16
        const buffer = builder.finalize();
        expect(buffer.rootOffset(1)).toBe(12);
        expect(buffer.getStructArray(12, 8)).toHaveLength(16);
        expect(buffer.getUint32(12)).toBe(2);
    });
});

describe('tables longer than the offsets address', () => {
    it.each(['uint8', 'uint16'] as const)(
        'should reject a header longer than %s offsets address',
        offsetType => {
            const max = MAX_VALUES[offsetType];
            const offsetSize = OFFSET_SIZES[offsetType];

            for (const sized of [false, true]) {
                const layout = TableLayout.create({ sized, offsetType });
                const fields = Math.floor(max / offsetSize) - (sized ? 1 : 0);

                expect(() => new TableBuilder(fields, layout)).not.toThrow();
                expect(() => new TableBuilder(fields + 1, layout)).toThrow(
                    `A header of ${fields + 1} fields doesn't fit in ${offsetType} offsets`
                );
            }
        }
    );

    it('should reject a table of uint32 offsets past 4 GiB before allocating it', () => {
        // An array-like stands in for a 4 GiB array, whose elements aren't read.
        // The table would hold its offset, its length and 2^32 elements.
        const builder = new TableBuilder(1);
        expect(() => builder.setInt8Array({ length: MAX_VALUES.uint32 + 1 })).toThrow(
            new RangeError('Field 1 makes the table 4294967304 bytes, too long for uint32 offsets')
        );

        builder.setInt8Array([1]);
        const buffer = builder.finalize();
        expect(buffer.getInt8Array(buffer.rootOffset(0))).toStrictEqual(new Int8Array([1]));
    });
});

describe('tables nested from their builders', () => {
    // The nested table follows a uint8[] of any length up to 7, which
    // starts it at any offset from an alignment of 8
    it.prop([tables({ headerless: true }), fc.nat({ max: 7 })])(
        'should write the same bytes as nesting the bytes that the builder writes, at the same alignment',
        (table, length) => {
            const parent: TableDescription = {
                layout: { sized: false, versioned: false, offsetType: 'uint32' },
                fields: [
                    { type: 'uint8[]', value: new Array<number>(length).fill(0) },
                    { type: 'table', from: 'builder', table }
                ]
            };

            expect(buildTable(nestedTheOtherWay(parent)).bytes()).toStrictEqual(
                buildTable(parent).bytes()
            );
        }
    );

    // A builder is copied as the bytes that it writes, whatever its layout,
    // and whether or not it was finalized
    it.prop([tables(), fc.nat({ max: 7 })])(
        'should write a builder that was finalized, or of any layout, as the bytes that it writes',
        expectNestedAsBytes
    );

    it.prop([tablesWithKibFields, fc.nat({ max: 7 })])(
        'should write a builder with values of 1 KiB or more as the bytes that it writes',
        expectNestedAsBytes
    );

    it('should reject a builder with fields left to set, and leave the table as it was', () => {
        const nested = new TableBuilder(2);
        nested.setUint8(1);

        const parent = new TableBuilder(2);
        expect(() => parent.setTable(nested)).toThrow("Only 1 of the table's 2 fields were set");
        expect(() => parent.setVector([new Uint8Array(1), nested])).toThrow(RangeError);

        nested.setUint8(2);
        parent.setTable(nested);
        parent.setVector([nested]);

        const buffer = parent.finalize();
        const table = buffer.rootOffset(0);
        expect(buffer.getUint8(buffer.fieldOffset(table, 1))).toBe(2);
        expect(buffer.getVector(buffer.rootOffset(1))).toStrictEqual([nested.bytes()]);
    });

    it('should write a builder nested several times, or finalized itself, the same each time', () => {
        const nested = new TableBuilder(2);
        nested.setFloat64Array([1, 2]);
        nested.setString('nested');

        // Written straight into the parent, and then copied once finalized
        const first = new TableBuilder(2);
        first.setTable(nested);
        first.setVector([nested, nested]);
        const written = first.bytes();

        const own = nested.bytes();
        const second = new TableBuilder(2);
        second.setTable(nested);
        second.setVector([nested, nested]);

        expect(second.bytes()).toStrictEqual(written);

        const buffer = second.finalize();
        expect(buffer.getVector(buffer.rootOffset(1))).toStrictEqual([own, own]);
    });
});

describe('tables written in place', () => {
    // Every nested table is made one that a builder writes in place, so
    // that it's written in place too, except those with an alignment of
    // their own and vectors of items of different numbers of fields
    it.prop([tables()])(
        'should write the bytes of the builders that they replace, and fail as they fail',
        table => {
            const nested = nestedInPlace(table);

            expect(outcomeOf(() => buildTableInPlace(nested).bytes())).toStrictEqual(
                outcomeOf(() => buildTable(nested).bytes())
            );
        }
    );

    // The vector of nested tables that the benchmarks write: items of a
    // float32 and a nested pair of float32s
    const VALUES = [1, 2, 3];

    function setItems(builder: TableBuilder): void {
        builder.setVectorInPlace(VALUES.length, 2, (item, index) => {
            item.setFloat32(VALUES[index]);
            item.setTableInPlace(2, pair => {
                pair.setFloat32(VALUES[index] + 1);
                pair.setFloat32(VALUES[index] + 2);
            });
        });
    }

    function itemsFromBuilders(): Uint8Array {
        const items = VALUES.map(value => {
            const pair = new TableBuilder(2);
            pair.setFloat32(value + 1);
            pair.setFloat32(value + 2);

            const item = new TableBuilder(2);
            item.setFloat32(value);
            item.setTable(pair);

            return item;
        });

        const builder = new TableBuilder(1);
        builder.setVector(items);

        return builder.bytes();
    }

    const EXPECTED = itemsFromBuilders();

    it("should pass on what fill throws, and leave the builder without the vector's items", () => {
        const builder = new TableBuilder(1);
        const thrown = new Error("The caller's");

        expect(() =>
            builder.setVectorInPlace(3, 2, (item, index) => {
                item.setFloat32(1);
                if (index === 2) {
                    throw thrown;
                }
                item.setFloat32(2);
            })
        ).toThrow(thrown);

        setItems(builder);
        expect(builder.bytes()).toStrictEqual(EXPECTED);
    });

    it('should reject an item with fields left to set, and leave the builder as it was', () => {
        const builder = new TableBuilder(1);

        expect(() =>
            builder.setVectorInPlace(3, 2, (item, index) => {
                item.setFloat32(1);
                if (index !== 1) {
                    item.setFloat32(2);
                }
            })
        ).toThrow(new RangeError("Only 1 of the table's 2 fields were set"));

        setItems(builder);
        expect(builder.bytes()).toStrictEqual(EXPECTED);
    });

    it('should reject a table with fields left to set within an item, and leave the builder as it was', () => {
        const builder = new TableBuilder(1);

        expect(() =>
            builder.setVectorInPlace(3, 2, item => {
                item.setFloat32(1);
                item.setTableInPlace(2, pair => pair.setFloat32(2));
            })
        ).toThrow(new RangeError("Only 1 of the table's 2 fields were set"));

        setItems(builder);
        expect(builder.bytes()).toStrictEqual(EXPECTED);
    });

    it('should reject a field beyond the number a table written in place holds', () => {
        const builder = new TableBuilder(1);

        expect(() =>
            builder.setTableInPlace(1, table => {
                table.setUint8(1);
                table.setUint8(2);
            })
        ).toThrow(new RangeError("Can't add field 2 to a table of 1"));

        // The root's offset, and then the nested table's and its uint8
        builder.setTableInPlace(1, table => table.setUint8(1));
        expect(builder.bytes()).toStrictEqual(new Uint8Array([4, 0, 0, 0, 4, 0, 0, 0, 1]));
    });

    it("should reject a vector whose count, item ends and items' offsets make the table too long before recording any item", () => {
        const layout = TableLayout.create({ offsetType: 'uint8' });
        const fill = vi.fn();
        const builder = new TableBuilder(1, layout);

        // The table's offset and the vector's count and 254 ends
        expect(() => builder.setVectorInPlace(254, 0, fill)).toThrow(
            new RangeError('Field 1 makes the table 256 bytes, too long for uint8 offsets')
        );
        // The vector's count and 100 ends, and two offsets in each item
        expect(() => builder.setVectorInPlace(100, 2, fill)).toThrow(
            new RangeError('Field 1 makes the table 302 bytes, too long for uint8 offsets')
        );
        expect(fill).not.toHaveBeenCalled();

        builder.setVectorInPlace(253, 0, fill);
        expect(fill).toHaveBeenCalledTimes(253);
        expect(builder.bytes()).toHaveLength(MAX_VALUES.uint8);
    });

    it("should zero the padding of a field written over a failed fill's bytes", () => {
        const set = (builder: TableItem, fail: boolean): void => {
            builder.setUint8(1);
            if (fail) {
                expect(() =>
                    builder.setTableInPlace(1, table => {
                        table.setFloat64(Number.NaN);
                        throw new Error("The caller's");
                    })
                ).toThrow("The caller's");
            }
            // Padded to 8 over the failed table's offset and float64
            builder.setFloat64Array([1]);
        };

        const builder = new TableBuilder(2);
        set(builder, true);
        const expected = new TableBuilder(2);
        set(expected, false);
        expect(builder.bytes()).toStrictEqual(expected.bytes());
    });

    it.each(['uint8', 'uint16'] as const)(
        'should reject a %s table made too long by a table or vector that fits itself, and leave the builder as it was',
        offsetType => {
            const layout = TableLayout.create({ offsetType });
            const room = MAX_VALUES[offsetType] - 2 * OFFSET_SIZES[offsetType];
            const long = new Uint8Array(room - 2 * OFFSET_SIZES[offsetType]);
            const fill = (table: TableItem): void => table.setUint8Array(long);

            const builder = new TableBuilder(2, layout);
            builder.setUint8(1);
            expect(() => builder.setTableInPlace(1, fill)).toThrow(RangeError);
            expect(() => builder.setVectorInPlace(1, 1, fill)).toThrow(RangeError);
            builder.setUint8(2);

            const expected = new TableBuilder(2, layout);
            expected.setUint8(1);
            expected.setUint8(2);
            expect(builder.bytes()).toStrictEqual(expected.bytes());
        }
    );

    it('should reject finalizing or nesting a builder while it writes a table in place', () => {
        const builder = new TableBuilder(1);
        const message =
            "A table can't be finalized or nested while a table is recorded in place in it";

        expect(() => builder.setTableInPlace(0, () => builder.finalize())).toThrow(
            new TypeError(message)
        );
        expect(() =>
            builder.setTableInPlace(0, () => new TableBuilder(1).setTable(builder))
        ).toThrow(new TypeError(message));

        builder.setTableInPlace(0, () => {});
        expect(builder.bytes()).toStrictEqual(new Uint8Array([4, 0, 0, 0]));
    });
});

/**
 * What building a table gives: its bytes, or the class and message of its
 * error. A vector written in place checks its count, its item ends and its
 * items' offsets before its items, and fails with the table's length up to
 * them, rather than with the items' whole lengths, so the length is left out
 * of a message of a table too long.
 */
function outcomeOf(build: () => Uint8Array): unknown {
    try {
        return build();
    } catch (error) {
        const thrown = error as Error;
        const message = thrown.message.replace(
            / makes the table \d+ bytes,/,
            ' makes the table too long,'
        );

        return [thrown.constructor, message];
    }
}

/**
 * A table of a uint8[] of `length` zeros, which starts the next field at any
 * offset from an alignment of 8, and then the table that `nested` returns,
 * nested with `setTable()` at an alignment of 8 and twice in a vector
 */
function parentOf(nested: () => TableBuilder | Uint8Array, length: number): Uint8Array {
    const parent = new TableBuilder(3);
    parent.setUint8Array(new Uint8Array(length));
    parent.setTable(nested(), 8);
    parent.setVector([nested(), nested()]);

    return parent.bytes();
}

/**
 * Returns the table with every nested table passed the other way: a builder
 * as its bytes and bytes as their builder, at the alignment that it had
 */
function nestedTheOtherWay(table: TableDescription): TableDescription {
    return { ...table, fields: table.fields.map(fieldNestedTheOtherWay) };
}

function fieldNestedTheOtherWay(field: FieldDescription): FieldDescription {
    switch (field.type) {
        case 'table':
            return {
                type: 'table',
                from: field.from === 'builder' ? 'bytes' : 'builder',
                alignment: alignmentOf(field),
                table: nestedTheOtherWay(field.table)
            };

        case 'vector':
            return {
                type: 'vector',
                items: field.items.map(({ from, table }) => ({
                    from: from === 'builder' ? 'bytes' : 'builder',
                    table: nestedTheOtherWay(table)
                }))
            };

        default:
            return field;
    }
}

/**
 * Nests a table, from its builder, from a builder that was finalized and
 * from its bytes, after a uint8[] of `length` bytes, and expects the same
 * bytes from each
 */
function expectNestedAsBytes(table: TableDescription, length: number): void {
    const bytes = buildTable(table).bytes();
    const finalized = buildTable(table);
    finalized.finalize();

    const expected = parentOf(() => bytes, length);
    expect(parentOf(() => buildTable(table), length)).toStrictEqual(expected);
    expect(parentOf(() => finalized, length)).toStrictEqual(expected);

    const buffer = new ByteBuffer(expected);
    expect(buffer.getVector(buffer.rootOffset(2))).toStrictEqual([bytes, bytes]);
}

/** A table of the default layout that holds `fields` */
function tableOf(fields: FieldDescription[]): TableDescription {
    return { layout: { sized: false, versioned: false, offsetType: 'uint32' }, fields };
}

/** An array of `length` elements of 1 */
function arrayField(type: ScalarType, length: number): FieldDescription {
    const value = type === 'int64' || type === 'uint64' ? '1' : 1;
    return { type: `${type}[]`, value: new Array(length).fill(value) } as FieldDescription;
}

/**
 * What a builder is asked to do: build `table` with `fields` fields in its
 * header, which may be one more or fewer than it sets, with its tables
 * nested from builders or in place, and with the `throwAt`th call of a
 * setter, counted from 1, throwing before it runs, or none if it's 0
 */
interface Script {
    table: TableDescription;
    fields: number;
    inPlace: boolean;
    throwAt: number;
}

/** Strings with lone surrogates, which are written as U+FFFD */
const LONE_SURROGATES = fc
    .tuple(
        fc.string({ unit: 'binary', maxLength: 4 }),
        fc.constantFrom('\ud800', '\udbff', '\udc00', '\udfff'),
        fc.string({ unit: 'binary', maxLength: 4 })
    )
    .map(
        ([before, surrogate, after]): FieldDescription => ({
            type: 'string',
            value: before + surrogate + after
        })
    );

/**
 * Long fields for a table of `offsetType` offsets: arrays of bytes about as
 * long as it can be, or longer, and fields of about 1 KiB, which the builder
 * may keep until the table is written
 */
function longFields(offsetType: OffsetType): fc.Arbitrary<FieldDescription> {
    const max = MAX_VALUES[offsetType];
    const nearMax = fc
        .integer({ min: Math.max(max - 300, 0), max: max + 8 })
        .map(length => arrayField('uint8', length));

    switch (offsetType) {
        case 'uint8':
            return nearMax;

        case 'uint16':
            return fc.oneof(nearMax, KIB_FIELDS);

        case 'uint32':
            return KIB_FIELDS;
    }
}

/**
 * Tables and vectors whose items hold a long field among others, which may
 * make the table that they're nested in too long, though they fit
 * themselves, and which tables written in place are moved with when they're
 * padded
 */
function longNestedFields(offsetType: OffsetType): fc.Arbitrary<FieldDescription> {
    const layout = { sized: false, versioned: false, offsetType };
    const nested = fc
        .tuple(
            fc.array(FIELDS, { maxLength: 2 }),
            longFields(offsetType),
            fc.array(FIELDS, { maxLength: 1 })
        )
        .map(
            ([before, long, after]): TableDescription => ({
                layout,
                fields: [...before, long, ...after]
            })
        );

    return fc.oneof(
        nested.map((table): FieldDescription => ({ type: 'table', from: 'builder', table })),
        fc.array(nested, { minLength: 1, maxLength: 2 }).map(
            (items): FieldDescription => ({
                type: 'vector',
                items: items.map(table => ({ from: 'builder' as const, table }))
            })
        )
    );
}

function layouts(
    offsetType: OffsetType
): fc.Arbitrary<Pick<TableDescription, 'layout' | 'version'>> {
    return fc
        .record({
            sized: fc.boolean(),
            versioned: fc.boolean(),
            version: fc.integer({ min: 0, max: MAX_VALUES[offsetType] })
        })
        .map(({ sized, versioned, version }) => {
            const layout: LayoutDescription = { sized, versioned, offsetType };
            return versioned ? { layout, version } : { layout };
        });
}

/**
 * Tables of every offset type, which may be too long for it, with any of
 * their fields set, and fills that throw. Their nested tables have no
 * header and their offset type, so that a `TableWriter` writes the same
 * bytes, and they're written in place or from builders.
 */
function scripts(): fc.Arbitrary<Script> {
    return fc.constantFrom(...OFFSET_TYPES).chain(offsetType => {
        const fields = fc.oneof(
            { arbitrary: FIELDS, weight: 8 },
            { arbitrary: LONE_SURROGATES, weight: 1 },
            { arbitrary: longFields(offsetType), weight: 1 },
            { arbitrary: longNestedFields(offsetType), weight: 1 }
        );

        return fc
            .record({
                header: layouts(offsetType),
                fields: fc.array(fields, { maxLength: 6 }),
                extra: fc.integer({ min: -1, max: 1 }),
                inPlace: fc.boolean(),
                throwAt: fc.oneof(fc.constant(0), fc.nat({ max: 12 }))
            })
            .map(
                ({ header, fields, extra, inPlace, throwAt }): Script => ({
                    table: nestedInPlace({ ...header, fields }),
                    fields: Math.max(fields.length + extra, 0),
                    inPlace,
                    throwAt
                })
            );
    });
}

/** The error that a script's `throwAt` throws */
class ScriptError extends Error {}

/**
 * Runs a script, and returns the fields whose setters returned, the errors
 * that the others threw, and what `bytes()` and then `finalize()` gave
 */
function run(script: Script): {
    set: FieldDescription[];
    errors: unknown[];
    outcomes: unknown[];
} {
    const builder = createBuilder({
        ...script.table,
        fields: new Array<FieldDescription>(script.fields)
    });
    const target = throwing(builder, { count: 0, throwAt: script.throwAt });
    const { offsetType } = script.table.layout;

    const set: FieldDescription[] = [];
    const errors: unknown[] = [];
    for (const field of script.table.fields) {
        try {
            if (script.inPlace) {
                setFieldInPlace(target, field, offsetType);
            } else {
                setField(target, field);
            }
            set.push(field);
        } catch (error) {
            errors.push(error);
        }
    }

    const outcomes = [resultOf(() => builder.bytes()), resultOf(() => builder.finalize().bytes())];

    return { set, errors, outcomes };
}

/**
 * What `bytes()` and `finalize()` give for a builder of the script's header
 * that was given only the fields in `set`: the bytes that a `TableWriter`
 * writes for them, once they're all the header's fields
 */
function expectedOutcomes(script: Script, set: FieldDescription[]): unknown[] {
    if (set.length !== script.fields) {
        const error = [
            'RangeError',
            `Only ${set.length} of the table's ${script.fields} fields were set`
        ];
        return [error, error];
    }

    const bytes = writeTable({ ...script.table, fields: set });
    return [bytes, bytes];
}

/**
 * A view of a builder, or of a table that it writes in place, whose setters
 * count their calls, and throw a {@link ScriptError} before the call that the
 * script names. The tables that its fills are given are such views too.
 */
function throwing(item: TableItem, calls: { count: number; throwAt: number }): TableItem {
    return new Proxy(item, {
        get(target, key) {
            const value = Reflect.get(target, key) as unknown;
            if (typeof value !== 'function' || typeof key !== 'string' || !key.startsWith('set')) {
                return value;
            }

            return (...args: unknown[]) => {
                calls.count++;
                if (calls.count === calls.throwAt) {
                    throw new ScriptError(`Call ${calls.count}`);
                }

                const last = args.length - 1;
                if (key === 'setTableInPlace' || key === 'setVectorInPlace') {
                    const fill = args[last] as (table: TableItem, index: number) => void;
                    args[last] = (table: TableItem, index: number) =>
                        fill(throwing(table, calls), index);
                }

                return (value as (...args: unknown[]) => unknown).apply(target, args);
            };
        }
    });
}

/** The bytes that `write` returns, or the class and message of its error */
function resultOf(write: () => Uint8Array): unknown {
    try {
        return write();
    } catch (error) {
        const thrown = error as Error;
        return [thrown.constructor.name, thrown.message];
    }
}

describe('setters that throw', () => {
    it.prop([scripts()], { numRuns: 300 })(
        'should leave the builder as it was, so that it writes the table of the fields whose setters returned',
        script => {
            const { set, errors, outcomes } = run(script);

            for (const error of errors) {
                expect(error instanceof ScriptError || error instanceof RangeError).toBe(true);
            }
            expect(outcomes).toStrictEqual(expectedOutcomes(script, set));
        }
    );
});
