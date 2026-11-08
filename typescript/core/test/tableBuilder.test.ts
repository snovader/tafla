import { describe, expect } from 'vitest';
import { fc, it } from '@fast-check/vitest';
import { ByteBuffer, TableBuilder, TableLayout } from 'tafla';
import { MAX_VALUES, tables } from './conformance/arbitraries.ts';
import type { FieldDescription, TableDescription } from './conformance/description.ts';
import { OFFSET_SIZES } from './conformance/format.ts';
import { alignmentOf, buildTable } from './conformance/tables.ts';

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

it('should write values as they are when finalizing', () => {
    const values = new Float32Array([1, 2]);
    const builder = new TableBuilder(1);
    builder.setFloat32Array(values);
    values[0] = 3;

    const buffer = builder.finalize();
    expect(buffer.getFloat32Array(buffer.rootOffset(0))).toStrictEqual(new Float32Array([3, 2]));
});

describe('arrays whose length changes before finalizing', () => {
    // A float64[] of 3 after the header at 8: its count at 8, 4 bytes of
    // padding and its elements at 16, and then a uint8 at 40
    function builderOf(values: ArrayLike<number>): TableBuilder {
        const builder = new TableBuilder(2);
        builder.setFloat64Array(values);
        builder.setUint8(7);
        return builder;
    }

    const EXPECTED = builderOf([1, 2, 3]).bytes();

    it.each([
        ['grew', [1, 2, 3, 4], "The fields need 48 bytes, more than the table's 41"],
        ['shrank', [1, 2], "The fields wrote 33 of the table's 41 bytes"]
    ])(
        'should throw RangeError for an array that %s, and finalize once it is restored',
        (_, changed, message) => {
            const values = [1, 2, 3];
            const builder = builderOf(values);

            values.splice(0, values.length, ...changed);
            expect(() => builder.bytes()).toThrow(new RangeError(message));
            expect(() => builder.finalize()).toThrow(new RangeError(message));

            values.splice(0, values.length, 1, 2, 3);
            expect(builder.bytes()).toStrictEqual(EXPECTED);
        }
    );

    it('should throw the TypeError of copying a typed array whose buffer was transferred', () => {
        const values = new Float64Array([1, 2, 3]);
        const builder = builderOf(values);

        values.buffer.transfer();
        expect(() => builder.finalize()).toThrow(TypeError);
        // It's still unfinalized, so a field past its last is a RangeError
        // rather than a finalized table's TypeError
        expect(() => builder.setUint8(1)).toThrow(RangeError);
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

    // A builder that was finalized, or whose layout has a header or another
    // offset type than its parent's, is copied as a table nested as bytes is
    it.prop([tables(), fc.nat({ max: 7 })])(
        'should write a builder that was finalized, or of any layout, as the bytes that it writes',
        (table, length) => {
            const bytes = buildTable(table).bytes();
            const finalized = buildTable(table);
            finalized.finalize();

            const expected = parentOf(() => bytes, length);
            expect(parentOf(() => buildTable(table), length)).toStrictEqual(expected);
            expect(parentOf(() => finalized, length)).toStrictEqual(expected);

            const buffer = new ByteBuffer(expected);
            expect(buffer.getVector(buffer.rootOffset(2))).toStrictEqual([bytes, bytes]);
        }
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

    it("should write a nested builder's values as they are when its parent is finalized", () => {
        const values = new Float32Array([1, 2]);
        const nested = new TableBuilder(1);
        nested.setFloat32Array(values);

        const parent = new TableBuilder(1);
        parent.setTable(nested);
        values[0] = 3;

        const buffer = parent.finalize();
        const read = buffer.getFloat32Array(buffer.fieldOffset(buffer.rootOffset(0), 0));
        expect(read).toStrictEqual(new Float32Array([3, 2]));
    });
});

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
