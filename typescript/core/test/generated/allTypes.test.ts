import { describe, expect } from 'vitest';
import { fc, it } from '@fast-check/vitest';
import { ByteBuffer, DecodeError } from 'tafla';
import { CORRUPTIONS, corruptionOf, SLICES, sliceOf } from '../conformance/arbitraries.ts';
import { corrupt } from '../conformance/description.ts';
import { buildTable } from '../conformance/tables.ts';
import {
    ALL_TYPES,
    ARRAYS_FIELDS,
    DOCUMENTED_FIELDS,
    describeArrays,
    describeNested,
    describeReserved,
    describeScalars,
    describeStructs,
    documentedBytes,
    NESTED_FIELDS,
    packedBytes,
    RESERVED_FIELDS,
    readArrays,
    readDocumented,
    readNested,
    readNestedFromLast,
    readReserved,
    readScalars,
    readStructs,
    readStructsFromLast,
    SCALARS_FIELDS,
    STRUCTS_FIELDS
} from './allTypesTables.ts';
import { expectValuesOrDecodeError } from './expectations.ts';

/** An index outside a view of `length` items, from the view's length */
const OUTSIDE_INDICES: fc.Arbitrary<(length: number) => number> = fc.oneof(
    fc.integer({ max: -1 }).map(index => () => index),
    fc.nat().map(past => (length: number) => length + past),
    fc.constant(() => 0.5),
    fc.constant(() => Number.NaN)
);

const EMPTY_SEGMENT = { start: { x: 0, y: 0 }, end: { x: 0, y: 0 } };

const EMPTY_PACKED = {
    int8: 0,
    uint8: 0,
    int16: 0,
    uint16: 0,
    int32: 0,
    uint32: 0,
    int64: 0n,
    uint64: 0n,
    float32: 0,
    float64: 0,
    bool: false
};

/** A byte of a bool other than the 0 and 1 that writers write */
const TRUE_BYTES = fc.integer({ min: 2, max: 0xff });

// The code generated from schemas/all_types.tafla, in TypeScript and in
// JavaScript, writes and reads tables of every type of field with tafla

describe.each(ALL_TYPES)('%s', (_, allTypes) => {
    describe('round trip', () => {
        it.prop([SCALARS_FIELDS, SLICES])(
            'should read back a field of every scalar type, wherever its bytes lie',
            (fields, slice) => {
                const bytes = sliceOf(allTypes.Scalars.serialize(fields), slice);

                expect(readScalars(new allTypes.Scalars(bytes))).toEqual(fields);
            }
        );

        it.prop([SCALARS_FIELDS, TRUE_BYTES])(
            'should read any byte of a bool but 0 as true',
            (fields, byte) => {
                const bytes = allTypes.Scalars.serialize({ ...fields, bool: false });
                bytes[new ByteBuffer(bytes).rootOffset(10)] = byte;

                expect(readScalars(new allTypes.Scalars(bytes))).toEqual({ ...fields, bool: true });
            }
        );

        it.prop([ARRAYS_FIELDS, SLICES])(
            'should read back an array of every scalar type, wherever its bytes lie',
            (fields, slice) => {
                const bytes = sliceOf(allTypes.Arrays.serialize(fields), slice);

                expect(readArrays(new allTypes.Arrays(bytes))).toEqual(fields);
            }
        );

        it.prop([NESTED_FIELDS, SLICES])(
            'should read back tables nested three deep and vectors of trees, wherever their bytes lie',
            (fields, slice) => {
                const bytes = sliceOf(allTypes.Nested.serialize(fields), slice);

                expect(readNested(new allTypes.Nested(bytes))).toEqual(fields);
            }
        );

        it.prop([RESERVED_FIELDS, SLICES])(
            'should read back the fields named after keywords, wherever their bytes lie',
            (fields, slice) => {
                const bytes = sliceOf(allTypes.Reserved.serialize(fields), slice);

                expect(readReserved(new allTypes.Reserved(bytes))).toEqual(fields);
            }
        );

        it.prop([STRUCTS_FIELDS, SLICES])(
            'should read back structs as fields, in arrays and within structs, wherever their bytes lie',
            (fields, slice) => {
                const bytes = sliceOf(allTypes.Structs.serialize(fields), slice);

                expect(readStructs(new allTypes.Structs(bytes))).toEqual(fields);
            }
        );
    });

    describe('tables built by hand', () => {
        it.prop([SCALARS_FIELDS])(
            'should write the bytes of Scalars that TableBuilder writes',
            fields => {
                const byHand = buildTable(describeScalars(fields)).bytes();

                expect(allTypes.Scalars.serialize(fields)).toStrictEqual(byHand);
            }
        );

        it.prop([ARRAYS_FIELDS])(
            'should write the bytes of Arrays, aligned to their element sizes, that TableBuilder writes',
            fields => {
                const byHand = buildTable(describeArrays(fields)).bytes();

                expect(allTypes.Arrays.serialize(fields)).toStrictEqual(byHand);
            }
        );

        it.prop([NESTED_FIELDS])(
            'should write the bytes of Nested that TableBuilder writes',
            fields => {
                const byHand = buildTable(describeNested(fields)).bytes();

                expect(allTypes.Nested.serialize(fields)).toStrictEqual(byHand);
            }
        );

        it.prop([RESERVED_FIELDS])(
            'should write the bytes of Reserved that TableBuilder writes',
            fields => {
                const byHand = buildTable(describeReserved(fields)).bytes();

                expect(allTypes.Reserved.serialize(fields)).toStrictEqual(byHand);
            }
        );

        it.prop([STRUCTS_FIELDS])(
            'should write the bytes of Structs, with its arrays aligned to their structs, that TableBuilder writes',
            fields => {
                const byHand = buildTable(describeStructs(fields)).bytes();

                expect(allTypes.Structs.serialize(fields)).toStrictEqual(byHand);
            }
        );
    });

    describe('decode errors', () => {
        it.prop([SCALARS_FIELDS, CORRUPTIONS])(
            'should read corrupted Scalars as values or throw DecodeError',
            (fields, corruption) => {
                const bytes = allTypes.Scalars.serialize(fields);
                const corrupted = corrupt(bytes, corruptionOf(corruption, bytes.length));

                expectValuesOrDecodeError(() => readScalars(new allTypes.Scalars(corrupted)));
            }
        );

        it.prop([ARRAYS_FIELDS, CORRUPTIONS])(
            'should read corrupted Arrays as values or throw DecodeError',
            (fields, corruption) => {
                const bytes = allTypes.Arrays.serialize(fields);
                const corrupted = corrupt(bytes, corruptionOf(corruption, bytes.length));

                expectValuesOrDecodeError(() => readArrays(new allTypes.Arrays(corrupted)));
            }
        );

        it.prop([NESTED_FIELDS, CORRUPTIONS])(
            'should read corrupted Nested as values or throw DecodeError',
            (fields, corruption) => {
                const bytes = allTypes.Nested.serialize(fields);
                const corrupted = corrupt(bytes, corruptionOf(corruption, bytes.length));

                expectValuesOrDecodeError(() => readNested(new allTypes.Nested(corrupted)));
            }
        );

        it.prop([RESERVED_FIELDS, CORRUPTIONS])(
            'should read corrupted Reserved as values or throw DecodeError',
            (fields, corruption) => {
                const bytes = allTypes.Reserved.serialize(fields);
                const corrupted = corrupt(bytes, corruptionOf(corruption, bytes.length));

                expectValuesOrDecodeError(() => readReserved(new allTypes.Reserved(corrupted)));
            }
        );

        it.prop([STRUCTS_FIELDS, CORRUPTIONS])(
            'should read corrupted Structs as values or throw DecodeError',
            (fields, corruption) => {
                const bytes = allTypes.Structs.serialize(fields);
                const corrupted = corrupt(bytes, corruptionOf(corruption, bytes.length));

                expectValuesOrDecodeError(() => readStructs(new allTypes.Structs(corrupted)));
            }
        );

        it.prop([fc.uint8Array({ maxLength: 64 })])(
            'should read arbitrary bytes as any message or throw DecodeError',
            bytes => {
                expectValuesOrDecodeError(() => readScalars(new allTypes.Scalars(bytes)));
                expectValuesOrDecodeError(() => readArrays(new allTypes.Arrays(bytes)));
                expectValuesOrDecodeError(() => readNested(new allTypes.Nested(bytes)));
                expectValuesOrDecodeError(() => readReserved(new allTypes.Reserved(bytes)));
                expectValuesOrDecodeError(() => readStructs(new allTypes.Structs(bytes)));
            }
        );
    });

    describe('views', () => {
        it.prop([NESTED_FIELDS, SLICES])(
            'should read back vectors of trees through their views from the last item to the first, wherever their bytes lie',
            (fields, slice) => {
                const bytes = sliceOf(allTypes.Nested.serialize(fields), slice);

                expect(readNestedFromLast(new allTypes.Nested(bytes), bytes.length)).toEqual(
                    fields
                );
            }
        );

        it.prop([STRUCTS_FIELDS, SLICES])(
            'should read back arrays of structs through their views from the last to the first, wherever their bytes lie',
            (fields, slice) => {
                const bytes = sliceOf(allTypes.Structs.serialize(fields), slice);

                expect(readStructsFromLast(new allTypes.Structs(bytes))).toEqual(fields);
            }
        );

        it.prop([NESTED_FIELDS, CORRUPTIONS])(
            'should read corrupted Nested from the last item of each view as values or throw DecodeError',
            (fields, corruption) => {
                const bytes = allTypes.Nested.serialize(fields);
                const corrupted = corrupt(bytes, corruptionOf(corruption, bytes.length));

                expectValuesOrDecodeError(() =>
                    readNestedFromLast(new allTypes.Nested(corrupted), corrupted.length)
                );
            }
        );

        it.prop([STRUCTS_FIELDS, CORRUPTIONS])(
            'should read corrupted Structs from the last item of each view as values or throw DecodeError',
            (fields, corruption) => {
                const bytes = allTypes.Structs.serialize(fields);
                const corrupted = corrupt(bytes, corruptionOf(corruption, bytes.length));

                expectValuesOrDecodeError(() =>
                    readStructsFromLast(new allTypes.Structs(corrupted))
                );
            }
        );

        it.prop([fc.uint8Array({ maxLength: 64 })])(
            'should read arbitrary bytes from the last item of the views of any message or throw DecodeError',
            bytes => {
                expectValuesOrDecodeError(() =>
                    readNestedFromLast(new allTypes.Nested(bytes), bytes.length)
                );
                expectValuesOrDecodeError(() => readStructsFromLast(new allTypes.Structs(bytes)));
            }
        );

        it.prop([NESTED_FIELDS, STRUCTS_FIELDS, OUTSIDE_INDICES])(
            'should throw RangeError for an item outside a view',
            (nestedFields, structsFields, outside) => {
                const nested = new allTypes.Nested(allTypes.Nested.serialize(nestedFields));
                const structs = new allTypes.Structs(allTypes.Structs.serialize(structsFields));

                for (const view of [nested.trees(), structs.documented()]) {
                    expect(() => view.get(outside(view.length()))).toThrow(RangeError);
                }
            }
        );

        it("should throw DecodeError for a vector whose count the bytes can't hold", () => {
            const bytes = allTypes.Nested.serialize({
                label: '',
                outer: { count: 0, inner: { name: '', values: [] } },
                trees: [{ value: 0, children: [] }]
            });
            // A count of 2^31 in the tree's empty vector of children, its count
            // alone, which ends the bytes
            bytes.set([0, 0, 0, 0x80], bytes.length - 4);
            const tree = new allTypes.Nested(bytes).trees().get(0);

            expect(() => tree.children()).toThrow(DecodeError);
        });

        it('should read a tree that holds itself from the last item, where reading the trees from the first throws DecodeError', () => {
            const bytes = allTypes.Nested.serialize({
                label: '',
                outer: { count: 0, inner: { name: '', values: [] } },
                trees: [
                    { value: 0, children: [] },
                    { value: 0, children: [] }
                ]
            });
            // End 0 for the first tree, which then ends before it starts. The
            // second starts at the trees' count, and reads the count and this
            // end as its offsets, so its children are the trees, and it's the
            // second of them.
            const trees = new ByteBuffer(bytes).rootOffset(2);
            bytes.set([0, 0, 0, 0], trees + 4);
            const nested = new allTypes.Nested(bytes);

            const top = nested.trees().get(1);
            let tree = top;
            for (let level = 0; level < bytes.length; level++) {
                tree = tree.children().get(1);
            }

            expect(tree.value()).toBe(top.value());
            expect(tree.children().length()).toBe(2);
            expect(() => nested.trees().get(0)).toThrow(DecodeError);
            expect(() => readNested(nested)).toThrow(DecodeError);
            expect(() => readNestedFromLast(nested, bytes.length)).toThrow(DecodeError);
        });

        it('should view empty vectors and arrays of structs', () => {
            const nested = new allTypes.Nested(
                allTypes.Nested.serialize({
                    label: '',
                    outer: { count: 0, inner: { name: '', values: [] } },
                    trees: []
                })
            );
            const structs = new allTypes.Structs(
                allTypes.Structs.serialize({
                    packed: EMPTY_PACKED,
                    documented: [],
                    segment: EMPTY_SEGMENT,
                    segments: [],
                    path: { start: EMPTY_SEGMENT.start, points: [] }
                })
            );

            for (const view of [
                nested.trees(),
                structs.documented(),
                structs.segments(),
                structs.path().points()
            ]) {
                expect(view.length()).toBe(0);
                expect(() => view.get(0)).toThrow(RangeError);
            }
        });
    });

    describe('structs', () => {
        it.prop([SCALARS_FIELDS, SLICES])(
            'should write Packed in its 43 bytes, little-endian at fixed offsets, and read it back wherever they lie',
            (fields, slice) => {
                const bytes = allTypes.Packed.serialize(fields);

                expect(allTypes.Packed.SIZE).toBe(43);
                expect(bytes).toStrictEqual(packedBytes(fields));
                expect(readScalars(new allTypes.Packed(sliceOf(bytes, slice)))).toEqual(fields);
            }
        );

        it.prop([DOCUMENTED_FIELDS, SLICES])(
            'should write Documented in its 7 bytes, little-endian at fixed offsets, and read it back wherever they lie',
            (fields, slice) => {
                const bytes = allTypes.Documented.serialize(fields);

                expect(allTypes.Documented.SIZE).toBe(7);
                expect(bytes).toStrictEqual(documentedBytes(fields));
                expect(readDocumented(new allTypes.Documented(sliceOf(bytes, slice)))).toEqual(
                    fields
                );
            }
        );

        it.prop([SCALARS_FIELDS, fc.uint8Array({ minLength: 1, maxLength: 8 })])(
            'should read Packed from the start of longer bytes',
            (fields, after) => {
                const bytes = new Uint8Array([...allTypes.Packed.serialize(fields), ...after]);

                expect(readScalars(new allTypes.Packed(bytes))).toEqual(fields);
            }
        );

        it.prop([SCALARS_FIELDS, TRUE_BYTES])(
            'should read any byte of the bool of Packed but 0 as true',
            (fields, byte) => {
                const bytes = packedBytes({ ...fields, bool: false });
                bytes[42] = byte;

                expect(readScalars(new allTypes.Packed(bytes))).toEqual({ ...fields, bool: true });
            }
        );

        it.prop([fc.uint8Array({ maxLength: 42 })])(
            'should throw DecodeError reading Packed from fewer than 43 bytes',
            bytes => {
                expect(() => new allTypes.Packed(bytes)).toThrow(DecodeError);
            }
        );

        it.prop([fc.uint8Array({ maxLength: 6 })])(
            'should throw DecodeError reading Documented from fewer than 7 bytes',
            bytes => {
                expect(() => new allTypes.Documented(bytes)).toThrow(DecodeError);
            }
        );

        it.prop([SCALARS_FIELDS, SCALARS_FIELDS])(
            'should return bytes of its own from each serialize()',
            (first, second) => {
                const bytes = allTypes.Packed.serialize(first);
                allTypes.Packed.serialize(second);

                expect(bytes).toStrictEqual(packedBytes(first));
            }
        );
    });
});
