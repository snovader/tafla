import { describe, expect } from 'vitest';
import { fc, it } from '@fast-check/vitest';
import { ByteBuffer, DecodeError } from 'tafla';
import {
    BYTE_OFFSETS,
    CORRUPTIONS,
    corruptionOf,
    FIELDS,
    fitsItsLayout,
    LAYOUTS,
    MAX_VALUES,
    SCALAR_TYPES,
    SLICES,
    sliceOf,
    tables
} from './conformance/arbitraries.ts';
import {
    corrupt,
    elementTypeOf,
    type FieldDescription,
    isArrayField,
    type LayoutDescription,
    type OffsetSize,
    type OffsetType,
    type ScalarType,
    type TableDescription
} from './conformance/description.ts';
import {
    headerSizeOf,
    OFFSET_SIZES,
    readScalar,
    readUnsigned,
    tablesIn
} from './conformance/format.ts';
import {
    alignmentOf,
    buildTable,
    createBuilder,
    DescriptionMismatch,
    ELEMENT_SIZES,
    layoutOf,
    readTable,
    setField,
    structAlignment,
    structSize,
    TableReader
} from './conformance/tables.ts';

// The properties that every implementation of tafla shares, of tables built
// from arbitrary descriptions in the conformance suite's model. `tablesIn()`
// reads the bytes that the format rules check with DataView, independently
// of ByteBuffer.

it.prop([tables(), SLICES])(
    'should read back the values of a table built from any description, wherever its bytes lie',
    (table, slice) => {
        const bytes = sliceOf(buildTable(table).bytes(), slice);

        // Descriptions hold -0, NaN and the infinities by name and 64-bit
        // integers as strings, so that they compare exactly
        expect(readTable(bytes, table)).toStrictEqual(table);
    }
);

describe('format rules', () => {
    it.prop([tables()])("should hold a sized table's byte length in its size slot", table => {
        for (const {
            table: { layout },
            bytes
        } of tablesIn(buildTable(table).bytes(), table)) {
            const { sized } = layout;
            const offsetSize = OFFSET_SIZES[layout.offsetType];

            if (sized) {
                expect(readUnsigned(bytes, 0, offsetSize)).toBe(bytes.length);
            }
        }
    });

    it.prop([tables()])("should hold a versioned table's version in its version slot", table => {
        for (const { table: inner, bytes } of tablesIn(buildTable(table).bytes(), table)) {
            const { sized, versioned } = inner.layout;
            const offsetSize = OFFSET_SIZES[inner.layout.offsetType];

            if (versioned) {
                const slot = sized ? offsetSize : 0;
                expect(readUnsigned(bytes, slot, offsetSize)).toBe(inner.version);
            }
        }
    });

    it.prop([tables()])(
        'should write offsets within the table, after its header and in the order of the fields',
        table => {
            for (const { table: inner, bytes, offsets } of tablesIn(
                buildTable(table).bytes(),
                table
            )) {
                const { layout, fields } = inner;
                let previous =
                    headerSizeOf(layout) + fields.length * OFFSET_SIZES[layout.offsetType];

                for (const offset of offsets) {
                    expect(offset).toBeGreaterThanOrEqual(previous);
                    expect(offset).toBeLessThanOrEqual(bytes.length);
                    previous = offset;
                }
            }
        }
    );

    it.prop([tables()])(
        "should start every array at a multiple of its alignment from the start of its table: its element size, or its structs' largest scalar size",
        table => {
            for (const { table: inner, offsets } of tablesIn(buildTable(table).bytes(), table)) {
                const offsetSize = OFFSET_SIZES[inner.layout.offsetType];

                for (const [index, field] of inner.fields.entries()) {
                    if (isArrayField(field)) {
                        const elementSize = ELEMENT_SIZES[elementTypeOf(field.type)];
                        expect((offsets[index] + offsetSize) % elementSize).toBe(0);
                    } else if (field.type === 'struct[]') {
                        const alignment = structAlignment(field.fields);
                        expect((offsets[index] + offsetSize) % alignment).toBe(0);
                    }
                }
            }
        }
    );

    it.prop([tables()])(
        "should start a table nested with setTable() at a multiple of its alignment from its parent's start",
        table => {
            for (const { table: parent, offsets } of tablesIn(buildTable(table).bytes(), table)) {
                for (const [index, field] of parent.fields.entries()) {
                    if (field.type === 'table') {
                        expect(offsets[index] % alignmentOf(field)).toBe(0);
                    }
                }
            }
        }
    );

    it.prop([tables()])(
        'should write multi-byte values, lengths and counts little-endian',
        table => {
            for (const { table: inner, bytes, offsets } of tablesIn(
                buildTable(table).bytes(),
                table
            )) {
                for (const [index, field] of inner.fields.entries()) {
                    expectLittleEndian(
                        bytes,
                        offsets[index],
                        OFFSET_SIZES[inner.layout.offsetType],
                        field
                    );
                }
            }
        }
    );
});

const ENCODER = new TextEncoder();

/**
 * Expects a field's bytes, read with DataView, to hold its description's
 * values little-endian. A nested table's fields are checked as their own.
 */
function expectLittleEndian(
    bytes: Uint8Array,
    position: number,
    offsetSize: OffsetSize,
    field: FieldDescription
): void {
    if (field.type === 'table') {
        return;
    }

    if (field.type === 'vector') {
        // The count, then the ends, which only grow
        const count = field.items.length;
        const ends = field.items.map((_, index) =>
            readUnsigned(bytes, position + (index + 1) * offsetSize, offsetSize)
        );

        expect(readUnsigned(bytes, position, offsetSize)).toBe(count);
        expect(ends).toStrictEqual(ends.toSorted((a, b) => a - b));
        expect(ends.every(end => end >= (count + 1) * offsetSize)).toBe(true);
        return;
    }

    if (field.type === 'struct') {
        let at = position;
        for (const { type, value } of field.fields) {
            expect(readScalar(bytes, type, at)).toBe(value);
            at += ELEMENT_SIZES[type];
        }
        return;
    }

    if (field.type === 'struct[]') {
        const size = structSize(field.fields);
        const start = position + offsetSize;
        const items = field.items.map((_, item) => {
            let at = start + item * size;
            return field.fields.map(type => {
                const value = readScalar(bytes, type, at);
                at += ELEMENT_SIZES[type];
                return value;
            });
        });

        expect(readUnsigned(bytes, position, offsetSize)).toBe(field.items.length);
        expect(items).toStrictEqual(field.items);
        return;
    }

    if (field.type === 'string') {
        const utf8 = ENCODER.encode(field.value);
        const start = position + offsetSize;

        expect(readUnsigned(bytes, position, offsetSize)).toBe(utf8.length);
        expect(bytes.subarray(start, start + utf8.length)).toStrictEqual(utf8);
        return;
    }

    if (isArrayField(field)) {
        const type = elementTypeOf(field.type);
        const start = position + offsetSize;
        const values = field.value.map((_, index) =>
            readScalar(bytes, type, start + index * ELEMENT_SIZES[type])
        );

        expect(readUnsigned(bytes, position, offsetSize)).toBe(field.value.length);
        expect(values).toStrictEqual(field.value);
        return;
    }

    if (field.type === 'bool') {
        expect(bytes[position]).toBe(field.value ? 1 : 0);
        return;
    }

    expect(readScalar(bytes, field.type, position)).toBe(field.value);
}

/** Every getter of a value at a position, with sizes of structs that need one */
const GETTERS = {
    getInt8: (buffer, position) => buffer.getInt8(position),
    getUint8: (buffer, position) => buffer.getUint8(position),
    getInt16: (buffer, position) => buffer.getInt16(position),
    getUint16: (buffer, position) => buffer.getUint16(position),
    getInt32: (buffer, position) => buffer.getInt32(position),
    getUint32: (buffer, position) => buffer.getUint32(position),
    getInt64: (buffer, position) => buffer.getInt64(position),
    getUint64: (buffer, position) => buffer.getUint64(position),
    getFloat32: (buffer, position) => buffer.getFloat32(position),
    getFloat64: (buffer, position) => buffer.getFloat64(position),
    getInt8Array: (buffer, position) => buffer.getInt8Array(position),
    getUint8Array: (buffer, position) => buffer.getUint8Array(position),
    getInt16Array: (buffer, position) => buffer.getInt16Array(position),
    getUint16Array: (buffer, position) => buffer.getUint16Array(position),
    getInt32Array: (buffer, position) => buffer.getInt32Array(position),
    getUint32Array: (buffer, position) => buffer.getUint32Array(position),
    getInt64Array: (buffer, position) => buffer.getInt64Array(position),
    getUint64Array: (buffer, position) => buffer.getUint64Array(position),
    getFloat32Array: (buffer, position) => buffer.getFloat32Array(position),
    getFloat64Array: (buffer, position) => buffer.getFloat64Array(position),
    getString: (buffer, position) => buffer.getString(position),
    getStruct: (buffer, position) => buffer.getStruct(position, 9),
    getStructArray: (buffer, position) => buffer.getStructArray(position, 3),
    getVector: (buffer, position) => buffer.getVector(position),
    getVectorOffsets: (buffer, position) => buffer.getVectorOffsets(position),
    // The last item, which reads the ends before and after it
    getVectorOffset: (buffer, position) => {
        const length = buffer.getVectorLength(position);
        return length === 0 ? null : buffer.getVectorOffset(position, length - 1);
    },
    getVectorItemOffset: (buffer, position) => {
        const length = buffer.getVectorLength(position);
        return length === 0 ? null : buffer.getVectorItemOffset(position, length, length - 1);
    }
} satisfies Record<string, (buffer: ByteBuffer, position: number) => unknown>;

type Getter = keyof typeof GETTERS;

/** The index of a root field, and then of a field of each nested table */
const PATHS = fc.array(fc.nat({ max: 20 }), { minLength: 1, maxLength: 3 });

// Each property reads the same bytes from two buffers that hold different
// bytes around them, which a read that strayed outside them would see
describe('untrusted bytes', () => {
    // Each run reads a table cut at each of its lengths, often a hundred or
    // more, so fewer runs suffice
    it.prop([tables(), BYTE_OFFSETS], { numRuns: 20 })(
        'should read the fields of a table cut at any length as values or throw DecodeError, within its bytes',
        (table, byteOffset) => {
            const bytes = buildTable(table).bytes();

            for (let length = 0; length < bytes.length; length++) {
                expectReadsWithin(bytes.subarray(0, length), table, byteOffset);
            }
        }
    );

    it.prop([tables(), fc.array(CORRUPTIONS, { minLength: 1, maxLength: 3 }), BYTE_OFFSETS])(
        'should read the fields of a table with overwritten or cut bytes as values or throw DecodeError, within its bytes',
        (table, corruptions, byteOffset) => {
            let bytes = buildTable(table).bytes();
            for (const corruption of corruptions) {
                bytes = corrupt(bytes, corruptionOf(corruption, bytes.length));
            }

            expectReadsWithin(bytes, table, byteOffset);
        }
    );

    it.prop([
        fc.uint8Array({ maxLength: 64 }),
        LAYOUTS,
        fc.constantFrom(...(Object.keys(GETTERS) as Getter[])),
        PATHS,
        BYTE_OFFSETS
    ])(
        'should read any bytes with any getter at any field as a value or throw DecodeError, within the bytes',
        (bytes, layout, getter, path, byteOffset) => {
            expect(
                readPath(sliceOf(bytes, { byteOffset, fill: 0x00 }), layout, getter, path)
            ).toStrictEqual(
                readPath(sliceOf(bytes, { byteOffset, fill: 0xff }), layout, getter, path)
            );
        }
    );
});

/**
 * Expects the fields of a table, read as its description sets them out from
 * slices of two buffers that hold different bytes around them, to be the same
 * values or DecodeErrors
 */
function expectReadsWithin(bytes: Uint8Array, table: TableDescription, byteOffset: number): void {
    expect(readFields(sliceOf(bytes, { byteOffset, fill: 0x00 }), table)).toStrictEqual(
        readFields(sliceOf(bytes, { byteOffset, fill: 0xff }), table)
    );
}

/**
 * Reads the header and every root field of a table as its description sets
 * them out, and returns the values or DecodeError of each
 */
function readFields(bytes: Uint8Array, table: TableDescription): unknown[] {
    const { layout, fields } = table;
    const buffer = new ByteBuffer(bytes, layoutOf(layout));
    const reader = new TableReader(bytes, layout);

    return [
        valueOrDecodeError(() => buffer.getSize()),
        valueOrDecodeError(() => (layout.versioned ? buffer.getVersion() : null)),
        ...fields.map((field, index) =>
            valueOrDecodeError(() => readRootField(reader, index, field))
        )
    ];
}

/**
 * Reads a root field as its description sets it out. Corrupted bytes may
 * hold values that don't match the description, which this returns as the
 * mismatch.
 */
function readRootField(reader: TableReader, index: number, field: FieldDescription): unknown {
    try {
        return reader.readRootField(index, field);
    } catch (error) {
        if (error instanceof DescriptionMismatch) {
            return error;
        }

        throw error;
    }
}

/**
 * Reads the header of bytes in a layout, and then a value with the getter at
 * the end of a path of field offsets, and returns the values or DecodeError
 * of each
 */
function readPath(
    bytes: Uint8Array,
    layout: LayoutDescription,
    getter: Getter,
    [root, ...path]: number[]
): unknown[] {
    const buffer = new ByteBuffer(bytes, layoutOf(layout));

    return [
        valueOrDecodeError(() => buffer.getSize()),
        valueOrDecodeError(() => (layout.versioned ? buffer.getVersion() : null)),
        valueOrDecodeError(() => {
            let position = buffer.rootOffset(root);
            for (const index of path) {
                position = buffer.fieldOffset(position, index);
            }

            return GETTERS[getter](buffer, position);
        })
    ];
}

/** Returns what a read returns, or the DecodeError that it throws */
function valueOrDecodeError(read: () => unknown): unknown {
    try {
        return read();
    } catch (error) {
        // Neither a RangeError, which DecodeError extends, nor any other error
        expect(error).toBeInstanceOf(DecodeError);
        return error;
    }
}

describe('builder limits', () => {
    const NARROW_OFFSET_TYPES = ['uint8', 'uint16'] as const;

    const tablesWithOversizedFields = fc.oneof(
        ...NARROW_OFFSET_TYPES.map(offsetType =>
            fc.record({
                table: tables({ offsetTypes: [offsetType], minFields: 1 }),
                index: fc.nat(),
                field: oversizedFields(offsetType)
            })
        )
    );

    it.prop([tablesWithOversizedFields])(
        'should reject a field too long for the offset type from its setter, and leave the builder as it was',
        ({ table, index, field }) => {
            const { offsetType } = table.layout;
            const at = index % table.fields.length;
            const builder = createBuilder(table);

            for (const before of table.fields.slice(0, at)) {
                setField(builder, before);
            }

            const error = thrownBy(() => setField(builder, field));
            const message = new RegExp(
                `^Field ${at + 1} makes the table (\\d+) bytes, too long for ${offsetType} offsets$`
            );
            expect(error).toBeInstanceOf(RangeError);
            const [, length] = (error as Error).message.match(message) ?? [];
            expect(Number(length)).toBeGreaterThan(MAX_VALUES[offsetType]);

            for (const after of table.fields.slice(at)) {
                setField(builder, after);
            }
            expect(builder.bytes()).toStrictEqual(buildTable(table).bytes());
        }
    );

    // Fields after which a table's length doesn't depend on where they start
    const unpaddedFields = FIELDS.filter(
        field =>
            field.type !== 'table' &&
            !(isArrayField(field) && ELEMENT_SIZES[elementTypeOf(field.type)] > 1) &&
            !(field.type === 'struct[]' && structAlignment(field.fields) > 1)
    );

    const tablesWithLastFields = fc.oneof(
        ...NARROW_OFFSET_TYPES.map(offsetType =>
            fc.record({ table: tables({ offsetTypes: [offsetType] }), last: unpaddedFields })
        )
    );

    it.prop([tablesWithLastFields])(
        'should build a table as long as its offset type addresses, and reject a field one byte longer',
        ({ table, last }) => {
            const { offsetType } = table.layout;
            const max = MAX_VALUES[offsetType];
            const withBytes = (length: number): TableDescription => ({
                ...table,
                fields: [...table.fields, bytesField(length), last]
            });

            const shortest = withBytes(0);
            fc.pre(fitsItsLayout(shortest));
            const room = max - buildTable(shortest).bytes().length;

            const longest = withBytes(room);
            const bytes = buildTable(longest).bytes();
            expect(bytes).toHaveLength(max);

            // Comparing up to 64 KiB of the uint8[] value by value is slow,
            // so it's compared by its length
            const read = readTable(bytes, longest);
            expect(read).toStrictEqual({
                ...longest,
                fields: [...table.fields, expect.anything(), last]
            });
            expect(read.fields.at(-2)).toHaveProperty('value.length', room);

            const builder = createBuilder(longest);
            for (const field of [...table.fields, bytesField(room + 1)]) {
                setField(builder, field);
            }

            const error = thrownBy(() => setField(builder, last));
            expect(error).toBeInstanceOf(RangeError);
            expect(error).toHaveProperty(
                'message',
                `Field ${table.fields.length + 2} makes the table ${max + 1} bytes, too long for ${offsetType} offsets`
            );
        }
    );
});

/** A uint8[] of `length` bytes */
function bytesField(length: number): FieldDescription {
    return { type: 'uint8[]', value: new Array<number>(length).fill(0) };
}

/**
 * Fields that are by themselves longer than a table of `offsetType` offsets
 * addresses: arrays of scalars and of structs, strings, nested tables
 * and vectors with a long item or many items
 */
function oversizedFields(offsetType: OffsetType): fc.Arbitrary<FieldDescription> {
    const max = MAX_VALUES[offsetType];
    const offsetSize = OFFSET_SIZES[offsetType];
    const layout = { sized: false, versioned: false, offsetType: 'uint32' } as const;
    const long: TableDescription = { layout, fields: [arrayField('uint8', max)] };
    const empty: TableDescription = { layout, fields: [] };
    const froms = fc.constantFrom('builder' as const, 'bytes' as const);

    const arrays = SCALAR_TYPES.map(type => arrayField(type, max));
    const string: FieldDescription = { type: 'string', value: 'a'.repeat(max) };
    const structs: FieldDescription = {
        type: 'struct[]',
        fields: ['uint16', 'uint8'],
        items: Array.from({ length: Math.ceil(max / 3) }, () => [1, 2])
    };
    const manyItems: FieldDescription = {
        type: 'vector',
        items: Array.from({ length: Math.ceil(max / offsetSize) }, () => ({
            from: 'bytes' as const,
            table: empty
        }))
    };

    return fc.oneof(
        fc.constantFrom(...arrays, string, structs, manyItems),
        froms.map((from): FieldDescription => ({ type: 'table', from, table: long })),
        froms.map((from): FieldDescription => ({ type: 'vector', items: [{ from, table: long }] }))
    );
}

/** An array of at least `byteLength` bytes of elements */
function arrayField(type: ScalarType, byteLength: number): FieldDescription {
    const length = Math.ceil(byteLength / ELEMENT_SIZES[type]);
    const value = type === 'int64' || type === 'uint64' ? '1' : 1;

    return { type: `${type}[]`, value: new Array(length).fill(value) };
}

function thrownBy(action: () => void): unknown {
    try {
        action();
    } catch (error) {
        return error;
    }

    return undefined;
}
