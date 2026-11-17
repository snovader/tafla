import { mock } from 'node:test';
import * as fc from 'fast-check';
import type { TableDescription } from './conformance/description.ts';
import { expect } from './conformance/expect.ts';
import { prop } from './conformance/property.ts';

// Runs tafla as if on a big-endian host. This host is little-endian, so the
// arrays it writes must come out in reverse byte order per element, which is
// little-endian on a big-endian host. The mock replaces the module for the
// imports after it, so tafla and the helpers that import it are imported
// below it.
mock.module(import.meta.resolve('../source/constants.ts'), {
    exports: {
        SIZEOF_8: 1,
        SIZEOF_16: 2,
        SIZEOF_32: 4,
        SIZEOF_64: 8,
        IS_LITTLE_ENDIAN: false
    }
});

const { TableWriter } = await import('tafla');
const { growing } = await import('./arrayLikes.ts');
const { fitsItsLayout, sliceOf, tables, ARRAY_FIELDS, SLICES } =
    await import('./conformance/arbitraries.ts');
const { elementTypeOf } = await import('./conformance/description.ts');
const { readScalar, readUnsigned } = await import('./conformance/format.ts');
const { buildTable, nestedInPlace, readTable, writeTable, ELEMENT_SIZES } =
    await import('./conformance/tables.ts');

// A table of one float array holds its offset, its count at 4, and its
// elements from 8
const ELEMENTS = 8;

prop([fc.array(fc.double()), fc.boolean()])(
    'should byte-swap each element, copied or converted, on a big-endian host',
    (values, typed) => {
        const writer = new TableWriter(new Uint8Array(ELEMENTS + values.length * 4), 1);
        writer.setFloat32Array(0, typed ? Float32Array.from(values) : values);
        const bytes = writer.bytes();

        const native = new Uint8Array(Float32Array.from(values).buffer);
        const swapped = native.map((_, i) => native[i - (i % 4) + 3 - (i % 4)]);

        expect(new DataView(bytes.buffer).getUint32(4, true)).toBe(values.length);
        expect(bytes.subarray(ELEMENTS)).toStrictEqual(swapped);
    }
);

prop([fc.array(fc.double(), { minLength: 1, maxLength: 8 }), fc.nat(), SLICES])(
    'should throw a RangeError for an array that grows after it is counted on a big-endian host, writing nothing',
    (values, cut, slice) => {
        const counted = cut % values.length;
        // Room for the grown array, so that the count must bound the write
        const bytes = sliceOf(new Uint8Array(ELEMENTS + values.length * 8), slice);
        const writer = new TableWriter(bytes, 1);
        const before = new Uint8Array(bytes.buffer.slice(0));

        expect(() => writer.setFloat64Array(0, growing(values, counted))).toThrow(RangeError);
        expect(new Uint8Array(bytes.buffer)).toStrictEqual(before);
    }
);

prop([ARRAY_FIELDS])(
    'should byte-swap the elements of every array type on a big-endian host',
    field => {
        const table: TableDescription = {
            layout: { sized: false, versioned: false, offsetType: 'uint32' },
            fields: [field]
        };
        const bytes = buildTable(table).bytes();

        // Its lengths are written with DataView, little-endian on any host
        const position = readUnsigned(bytes, 0, 4);
        const type = elementTypeOf(field.type);
        const values = field.value.map((_, index) =>
            readScalar(bytes, type, position + 4 + index * ELEMENT_SIZES[type], false)
        );

        expect(values).toStrictEqual(field.value);
    }
);

prop([tables(), SLICES])(
    'should read back the values of a table built from any description on a big-endian host',
    (table, slice) => {
        const bytes = sliceOf(buildTable(table).bytes(), slice);

        expect(readTable(bytes, table)).toStrictEqual(table);
    }
);

// TableBuilder writes through a TableWriter too, so this compares the sizer's
// count of each table with the builder's, and their calls of the writer. The
// tests above check the bytes that the builder writes through it.
prop([tables().map(nestedInPlace).filter(fitsItsLayout)])(
    'should count each table as long as TableBuilder does on a big-endian host, and write the same bytes',
    table => {
        expect(writeTable(table)).toStrictEqual(buildTable(table).bytes());
    }
);
