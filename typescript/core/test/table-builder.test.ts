import { describe, expect, vi } from 'hypotes';
import { fc, it } from 'hypotes/fast-check';
import { TableBuilder, TableLayout, TableReader, type TableItem } from 'tafla';
import {
    fitsItsLayout,
    tables,
    FIELDS,
    MAX_VALUES,
    OFFSET_TYPES
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
import { SHORTFALL_STRINGS } from './long-strings.ts';

/**
 * Fields of about 1 KiB, around the length from which the builder keeps an
 * array or an array of structs until the table is written, rather than
 * copying it when it's set, and strings of about 1,000 code units, whose
 * room for their longest UTF-8 grows the buffer
 */
const KIB_FIELDS: fc.Arbitrary<FieldDescription> = fc.oneof(
    fc.integer({ min: 1000, max: 1100 }).map(length => arrayField('uint8', length)),
    fc.integer({ min: 120, max: 140 }).map(length => arrayField('float64', length)),
    fc.integer({ min: 100, max: 130 }).map((count): FieldDescription => ({
        type: 'struct[]',
        fields: ['float64', 'uint8'],
        items: Array.from({ length: count }, () => [1, 2])
    })),
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
    .map(withFields)
    .filter(fitsItsLayout);

/** A copy of the table with the fields after its own */
function withFields([table, fields]: [TableDescription, FieldDescription[]]): TableDescription {
    return { ...table, fields: [...table.fields, ...fields] };
}

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

it('should finalize once and return the same reader', () => {
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
    it.each(Object.entries(SHORTFALL_STRINGS))(
        'should write a %s string that encodeInto() stops short of in bytes of its length',
        (_, string) => {
            const builder = new TableBuilder(1);
            builder.setString(string);

            const buffer = builder.finalize();
            expect(buffer.getString(buffer.rootOffset(0))).toBe(string.toWellFormed());
        }
    );

    it("should write such a string when it's counted, into a buffer grown to its end", () => {
        // Its longest UTF-8, 3 bytes for each code unit, is too long for
        // uint16 offsets, so it's counted, and the builder's bytes grow to
        // the string's end, after those that another builder holds.
        // encodeInto() stops a unit short of it.
        const taken = new TableBuilder(0);
        const string = 'a'.repeat(22_104) + SHORTFALL_STRINGS['well-formed'];
        const layout = TableLayout.create({ offsetType: 'uint16' });
        const builder = new TableBuilder(1, layout);
        builder.setString(string);
        taken.bytes();

        const buffer = new TableReader(builder.bytes(), layout);
        expect(buffer.getString(buffer.rootOffset(0))).toBe(string);
    });
});

it('should write zeros as padding, rather than the bytes of a table built before it', () => {
    leaveFilledBuffer();

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
        const reader = builder.finalize();
        expect(reader.rootOffset(1)).toBe(12);
        expect(reader.getStructArray(12, 8)).toHaveLength(16);
        expect(reader.getUint32(12)).toBe(2);
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
        const reader = builder.finalize();
        expect(reader.getInt8Array(reader.rootOffset(0))).toStrictEqual(new Int8Array([1]));
    });
});

// An array-like's length is checked as TableSizer checks a count, before
// any of the array is written, whether it's copied or kept
it.each([1.5, -1, Number.NaN, 2000.5])(
    'should reject an array-like of length %s, and leave the builder as it was',
    length => {
        const builder = new TableBuilder(3);
        builder.setUint8(1);

        expect(() => builder.setInt8Array({ length })).toThrow(
            new RangeError(`A count must be a non-negative integer, not ${length}`)
        );
        builder.setInt8Array([2]);
        builder.setUint8(3);
        expect(builder.bytes()).toStrictEqual(
            writeTable(
                tableOf([
                    { type: 'uint8', value: 1 },
                    { type: 'int8[]', value: [2] },
                    { type: 'uint8', value: 3 }
                ])
            )
        );
    }
);

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

        const reader = parent.finalize();
        const table = reader.rootOffset(0);
        expect(reader.getUint8(reader.fieldOffset(table, 1))).toBe(2);
        expect(reader.getVector(reader.rootOffset(1))).toStrictEqual([nested.bytes()]);
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

        const reader = second.finalize();
        expect(reader.getVector(reader.rootOffset(1))).toStrictEqual([own, own]);
    });

    // A nested builder is copied into its parent while it's short, and kept
    // until the parent is written once it holds 1 KiB, or arrays that it
    // keeps itself
    it.each([2, 128])(
        'should write tables nested from builders 64 deep, each with a float64[] of %i, as the writer does',
        elements => {
            let table = tableOf([{ type: 'uint8', value: 0 }]);
            for (let depth = 1; depth <= 64; depth++) {
                table = tableOf([
                    { type: 'uint8', value: depth },
                    { type: 'float64[]', value: new Array<number>(elements).fill(depth) },
                    { type: 'table', from: 'builder', table }
                ]);
            }

            expect(buildTable(table).bytes()).toStrictEqual(writeTable(table));
        }
    );

    it("should write a nested builder's table as it was set after the builder is finalized and others are built", () => {
        // A string of 1 KiB or more, in a nested builder that the parent
        // keeps rather than copies
        const parent = new TableBuilder(1);
        const nested = new TableBuilder(2);
        nested.setString('n'.repeat(1500));
        nested.setUint8(1);
        leaveFilledBuffer();
        parent.setTable(nested);

        // A builder holds bytes while the nested builder is finalized, and
        // each builder after it writes other bytes into those it takes
        const holder = new TableBuilder(0);
        const own = nested.bytes();
        for (let i = 0; i < 3; i++) {
            const other = new TableBuilder(1);
            other.setString('x'.repeat(1200));
            other.bytes();
        }
        holder.bytes();

        const expected = new TableBuilder(1);
        expected.setTable(own, 1);
        expect(parent.bytes()).toStrictEqual(expected.bytes());
    });

    it('should write a nested builder made before its parent, in bytes that an earlier table was written in, and the builders after it', () => {
        // The nested builder takes bytes that hold 0xff, before its parent
        // is made
        leaveFilledBuffer();

        const nestedTable = tableOf([
            { type: 'float64[]', value: [1, 2] },
            { type: 'string', value: 'nested' }
        ]);
        const nested = buildTable(nestedTable);
        const parent = new TableBuilder(2);
        parent.setTable(nested);

        // A builder made after the nesting, while the parent still writes
        const other = new TableBuilder(1);
        other.setUint8Array(new Uint8Array(1000).fill(0xee));
        parent.setString('parent'.repeat(100));

        expect(other.bytes()).toStrictEqual(
            writeTable(tableOf([{ type: 'uint8[]', value: new Array<number>(1000).fill(0xee) }]))
        );
        expect(parent.bytes()).toStrictEqual(
            writeTable(
                tableOf([
                    { type: 'table', from: 'builder', table: nestedTable },
                    { type: 'string', value: 'parent'.repeat(100) }
                ])
            )
        );
        expect(nested.bytes()).toStrictEqual(writeTable(nestedTable));
    });

    // 70 columns make a table of 70 KB, written in the buffer that builders
    // share, and 1,100 one of more than 1 MiB, in a buffer of its own
    it.each([70, 1100])(
        'should write tables of %i columns of 1,000 bytes one after another, with builders made while they grow',
        count => {
            const written: Array<[Uint8Array, Uint8Array]> = [];
            for (let round = 0; round < 3; round++) {
                const columns: FieldDescription[] = Array.from(
                    { length: count + round },
                    (_, i) => ({
                        type: 'uint8[]',
                        value: new Array<number>(1000).fill((i + round) % 256)
                    })
                );
                // A short nested table is copied in, and a long one kept, so
                // that the table is copied or cut from its buffer, or
                // written from its segments
                const nestedLength = round === 1 ? 1000 : 100;
                const nestedTable = tableOf([
                    { type: 'uint8[]', value: new Array<number>(nestedLength).fill(0xee) }
                ]);

                // The nested builder is made once its parent has grown
                const builder = new TableBuilder(columns.length + 1);
                for (const column of columns) {
                    setField(builder, column);
                }
                builder.setTable(buildTable(nestedTable));

                written.push([
                    builder.bytes(),
                    writeTable(
                        tableOf([
                            ...columns,
                            { type: 'table', from: 'builder', table: nestedTable }
                        ])
                    )
                ]);
            }

            // Each table is still as it was written, after the builders after it
            for (const [table, expected] of written) {
                expect(table).toStrictEqual(expected);
            }
        }
    );

    it.each([70, 1100])(
        'should write a builder of %i columns of 1,000 bytes that a parent nests, both when it is written itself and in its parent',
        count => {
            // Columns shorter than 1 KiB are copied into the builder's bytes
            const nestedTable = tableOf(
                Array.from({ length: count }, (_, i) => ({
                    type: 'uint8[]' as const,
                    value: new Array<number>(1000).fill(i % 256)
                }))
            );
            const nested = buildTable(nestedTable);
            const parent = new TableBuilder(2);
            parent.setTable(nested);
            parent.setString('parent');

            expect(nested.bytes()).toStrictEqual(writeTable(nestedTable));
            expect(parent.bytes()).toStrictEqual(
                writeTable(
                    tableOf([
                        { type: 'table', from: 'builder', table: nestedTable },
                        { type: 'string', value: 'parent' }
                    ])
                )
            );
        }
    );
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
        const fill = vi.fn<(item: TableItem, index: number) => void>();
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

    const buffer = new TableReader(expected);
    expect(buffer.getVector(buffer.rootOffset(2))).toStrictEqual([bytes, bytes]);
}

/** A table of the default layout that holds `fields` */
/** Writes a table of 60 KB of 0xff, which leaves those bytes in the buffer that the next builders write in */
function leaveFilledBuffer(): void {
    const filled = new TableBuilder(60);
    for (let i = 0; i < 60; i++) {
        filled.setUint8Array(new Uint8Array(1000).fill(0xff));
    }
    filled.bytes();
}

function tableOf(fields: FieldDescription[]): TableDescription {
    return { layout: { sized: false, versioned: false, offsetType: 'uint32' }, fields };
}

/** An array of `length` elements of 1 */
function arrayField(type: ScalarType, length: number): FieldDescription {
    const value = type === 'int64' || type === 'uint64' ? '1' : 1;
    return { type: `${type}[]`, value: new Array(length).fill(value) };
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
    .map(([before, surrogate, after]): FieldDescription => ({
        type: 'string',
        value: before + surrogate + after
    }));

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
        .map(([before, long, after]): TableDescription => ({
            layout,
            fields: [...before, long, ...after]
        }));

    return fc.oneof(
        nested.map((table): FieldDescription => ({ type: 'table', from: 'builder', table })),
        fc.array(nested, { minLength: 1, maxLength: 2 }).map((items): FieldDescription => ({
            type: 'vector',
            items: items.map(table => ({ from: 'builder' as const, table }))
        }))
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
            .map(({ header, fields, extra, inPlace, throwAt }): Script => ({
                table: nestedInPlace({ ...header, fields }),
                fields: Math.max(fields.length + extra, 0),
                inPlace,
                throwAt
            }));
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

/**
 * Tables to build at the same time: `turns` names the table whose builder
 * sets its next field, making it first if it isn't made, and `written` the
 * tables that are then given their other fields and written, in that order.
 * The others' builders are left as they are.
 */
interface Interleaving {
    tables: TableDescription[];
    turns: number[];
    written: number[];
}

/**
 * Tables to build at the same time, all of which are written unless
 * `leavingSome`
 */
function interleavings(leavingSome: boolean): fc.Arbitrary<Interleaving> {
    const table = fc.oneof(
        { arbitrary: tables(), weight: 3 },
        { arbitrary: tablesWithKibFields, weight: 1 }
    );

    return fc.array(table, { minLength: 2, maxLength: 6 }).chain(all =>
        fc.record({
            tables: fc.constant(all),
            turns: fc.array(fc.nat({ max: all.length - 1 }), { maxLength: 40 }),
            written: fc.shuffledSubarray(
                all.map((_, index) => index),
                { minLength: leavingSome ? 0 : all.length }
            )
        })
    );
}

/**
 * Builds the tables at the same time, and expects each one written to be
 * the table that a builder on its own writes
 */
function expectInterleaved({ tables: all, turns, written }: Interleaving): void {
    const expected = all.map(table => buildTable(table).bytes());

    const builders: Array<TableBuilder | undefined> = [];
    const set = all.map(() => 0);
    const builderOf = (index: number): TableBuilder =>
        (builders[index] ??= createBuilder(all[index]));
    const setNext = (index: number): void => {
        const builder = builderOf(index);
        if (set[index] < all[index].fields.length) {
            setField(builder, all[index].fields[set[index]]);
            set[index]++;
        }
    };

    for (const index of turns) {
        setNext(index);
    }

    const tablesWritten = written.map(index => {
        while (set[index] < all[index].fields.length) {
            setNext(index);
        }

        return builderOf(index).bytes();
    });

    expect(tablesWritten).toStrictEqual(written.map(index => expected[index]));
}

// Builders take their bytes from one buffer, each after the last one's, so
// a bug in how they share it would show as one table's bytes in another
describe('builders written at the same time', () => {
    // With every builder written, the next tables start with none that
    // holds bytes, and with some left, among builders that never give
    // theirs back
    it.prop([interleavings(false)], { numRuns: 200 })(
        'should write each table as a builder on its own does, whatever the order of their fields and of their writing',
        expectInterleaved
    );

    it.prop([interleavings(true)], { numRuns: 200 })(
        'should write each table as a builder on its own does, with other builders left unwritten',
        expectInterleaved
    );

    it('should write a table whose nested builder is written itself, and other builders after it, before the table is', () => {
        // The nested builder's table is 128 bytes or longer, so the parent
        // keeps its bytes where they are, the last taken, rather than
        // copying them
        const parent = new TableBuilder(1);
        const nested = new TableBuilder(2);
        nested.setString('n'.repeat(1500));
        nested.setUint8(1);
        parent.setTable(nested);

        const own = nested.bytes();
        for (let i = 0; i < 3; i++) {
            const other = new TableBuilder(1);
            other.setString('x'.repeat(1200));
            other.bytes();
        }

        const expected = new TableBuilder(1);
        expected.setTable(own, 1);
        expect(parent.bytes()).toStrictEqual(expected.bytes());
    });

    it('should write tables whose builders fill the buffer that they share several times over', () => {
        // 60 strings of about 1,500 characters, each given room for three
        // bytes a character: about 270 KB, in builders that each move past
        // the others when their string is set
        const strings = Array.from({ length: 60 }, (_, index) =>
            String.fromCodePoint(97 + (index % 26)).repeat(1500 + index)
        );
        const builders = strings.map(() => new TableBuilder(3));

        for (const [index, builder] of builders.entries()) {
            builder.setUint32(index);
        }
        for (const [index, builder] of builders.entries()) {
            builder.setString(strings[index]);
        }
        for (const [index, builder] of builders.entries()) {
            builder.setFloat64Array([index, -index]);
        }

        // The odd ones first, and then the even ones from the last
        const order = [
            ...builders.keys().filter(index => index % 2 === 1),
            ...builders.keys().filter(index => index % 2 === 0)
        ];
        order.reverse();

        for (const index of order) {
            expect(builders[index].bytes()).toStrictEqual(
                writeTable(
                    tableOf([
                        { type: 'uint32', value: index },
                        { type: 'string', value: strings[index] },
                        { type: 'float64[]', value: [index, -index] }
                    ])
                )
            );
        }
    });

    it('should write a table in place in a builder that moves while the table is filled', () => {
        const parent = new TableBuilder(3);
        parent.setUint8(1);

        // Holds the bytes after the parent's, so that the parent moves once
        // it outgrows its own
        const other = new TableBuilder(1);
        const made: TableBuilder[] = [];

        parent.setTableInPlace(3, table => {
            table.setUint8(2);

            const another = new TableBuilder(1);
            another.setString('x'.repeat(300));
            made.push(another);

            // The table is padded for its float64[] once it's filled
            table.setFloat64Array(new Array<number>(40).fill(1.5));
            table.setString('s'.repeat(200));
        });
        other.setString('other');
        parent.setString('parent');

        expect(parent.bytes()).toStrictEqual(
            writeTable(
                tableOf([
                    { type: 'uint8', value: 1 },
                    {
                        type: 'table',
                        from: 'builder',
                        table: tableOf([
                            { type: 'uint8', value: 2 },
                            { type: 'float64[]', value: new Array<number>(40).fill(1.5) },
                            { type: 'string', value: 's'.repeat(200) }
                        ])
                    },
                    { type: 'string', value: 'parent' }
                ])
            )
        );
        expect(other.bytes()).toStrictEqual(
            writeTable(tableOf([{ type: 'string', value: 'other' }]))
        );
        expect(made[0].bytes()).toStrictEqual(
            writeTable(tableOf([{ type: 'string', value: 'x'.repeat(300) }]))
        );
    });

    it('should write tables whose values make and write other builders while they are set', () => {
        const left: Array<[TableBuilder, number]> = [];
        const written: Array<[Uint8Array, number]> = [];

        // Leaves a builder of a string of `length` characters unwritten, and
        // writes a table of as many bytes
        const makeBuilders = (length: number): void => {
            const builder = new TableBuilder(1);
            builder.setString('o'.repeat(length));
            left.push([builder, length]);

            const other = new TableBuilder(1);
            other.setUint8Array(new Uint8Array(length).fill(length));
            written.push([other.bytes(), length]);
        };
        const numberThatMakes = (value: number, length: number): number =>
            ({
                valueOf(): number {
                    makeBuilders(length);
                    return value;
                }
            }) as unknown as number;

        const builder = new TableBuilder(5);
        builder.setInt32(numberThatMakes(7, 100));
        builder.setFloat64(numberThatMakes(1.5, 3000));
        builder.setFloat64Array({
            length: 2,
            get 0(): number {
                makeBuilders(70_000);
                return 1;
            },
            1: 2
        });
        builder.setUint16Array([numberThatMakes(3, 500), 4]);
        builder.setString('after');

        expect(builder.bytes()).toStrictEqual(
            writeTable(
                tableOf([
                    { type: 'int32', value: 7 },
                    { type: 'float64', value: 1.5 },
                    { type: 'float64[]', value: [1, 2] },
                    { type: 'uint16[]', value: [3, 4] },
                    { type: 'string', value: 'after' }
                ])
            )
        );
        for (const [other, length] of left) {
            expect(other.bytes()).toStrictEqual(
                writeTable(tableOf([{ type: 'string', value: 'o'.repeat(length) }]))
            );
        }
        for (const [table, length] of written) {
            expect(table).toStrictEqual(
                writeTable(
                    tableOf([
                        { type: 'uint8[]', value: new Array<number>(length).fill(length % 256) }
                    ])
                )
            );
        }
    });
});

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
