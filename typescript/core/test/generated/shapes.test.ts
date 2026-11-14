import { describe, expect } from 'vitest';
import { fc, it } from '@fast-check/vitest';
import { DecodeError } from 'tafla';
import {
    BYTE_OFFSETS,
    CORRUPTIONS,
    corruptionOf,
    SLICES,
    sliceOf
} from '../conformance/arbitraries.ts';
import { corrupt } from '../conformance/description.ts';
import { buildTable } from '../conformance/tables.ts';
import { expectValuesOrDecodeError, withPlainArrays } from './expectations.ts';
import {
    DOCUMENT_FIELDS,
    DOCUMENT_LAYOUT,
    describeDocument,
    describeItemList,
    describePoint,
    ITEM_LIST_FIELDS,
    type ItemList,
    POINT_FIELDS,
    readDocument,
    readItem,
    readItemList,
    readPoint,
    readSample,
    SAMPLE_FIELDS,
    SHAPES,
    sampleBytes
} from './shapesTables.ts';

// The code generated from schemas/shapes.tafla, in TypeScript and in
// JavaScript, writes and reads tables with tafla

const VERSIONS = fc.integer({ min: 0, max: 0xffff_ffff });
const OTHER_VERSIONS = VERSIONS.filter(version => version !== DOCUMENT_LAYOUT.version);

describe.each(SHAPES)('%s', (_, shapes) => {
    describe('round trip', () => {
        it.prop([POINT_FIELDS, SLICES])(
            'should read back the fields of a Point, wherever its bytes lie',
            (fields, slice) => {
                const bytes = sliceOf(shapes.Point.serialize(fields), slice);

                expect(readPoint(new shapes.Point(bytes))).toEqual(fields);
            }
        );

        it.prop([DOCUMENT_FIELDS, SLICES])(
            'should read back the fields of a Document and its nested Columns, wherever its bytes lie',
            (fields, slice) => {
                const bytes = sliceOf(shapes.Document.serialize(fields), slice);

                expect(readDocument(new shapes.Document(bytes))).toEqual(fields);
            }
        );

        it.prop([ITEM_LIST_FIELDS, SLICES])(
            'should read back the Items of an ItemList and their Pairs, wherever its bytes lie',
            (fields, slice) => {
                const bytes = sliceOf(shapes.ItemList.serialize(fields), slice);

                expect(readItemList(new shapes.ItemList(bytes))).toEqual(fields);
            }
        );
    });

    describe('tables built by hand', () => {
        it.prop([POINT_FIELDS])(
            'should write the bytes of a Point that TableBuilder writes',
            fields => {
                const byHand = buildTable(describePoint(fields)).bytes();

                expect(shapes.Point.serialize(fields)).toStrictEqual(byHand);
            }
        );

        it.prop([DOCUMENT_FIELDS])(
            'should write the bytes of a Document, sized and versioned, that TableBuilder writes',
            fields => {
                const byHand = buildTable(describeDocument(fields)).bytes();

                expect(shapes.Document.serialize(fields)).toStrictEqual(byHand);
            }
        );

        it.prop([ITEM_LIST_FIELDS])(
            'should write the bytes of an ItemList that TableBuilder writes',
            fields => {
                const byHand = buildTable(describeItemList(fields)).bytes();

                expect(shapes.ItemList.serialize(fields)).toStrictEqual(byHand);
            }
        );
    });

    describe('bytes of the caller', () => {
        it.prop([DOCUMENT_FIELDS, fc.nat({ max: 64 }), fc.nat({ max: 0xff })])(
            'should write a Document into the start of longer bytes of any content, as serialize() writes it, and return the view of them that holds it',
            (fields, extra, fill) => {
                const expected = shapes.Document.serialize(fields);
                const bytes = new Uint8Array(expected.length + extra).fill(fill);
                const written = shapes.Document.serializeInto(fields, bytes);

                expect(written).toStrictEqual(expected);
                expect(written.buffer).toBe(bytes.buffer);
                expect(written.byteOffset).toBe(bytes.byteOffset);
                expect(bytes.subarray(expected.length)).toStrictEqual(
                    new Uint8Array(extra).fill(fill)
                );
            }
        );

        it.prop([ITEM_LIST_FIELDS, fc.nat({ max: 64 }), fc.nat({ max: 0xff })])(
            'should write an ItemList into the start of longer bytes of any content, as serialize() writes it',
            (fields, extra, fill) => {
                const expected = shapes.ItemList.serialize(fields);
                const bytes = new Uint8Array(expected.length + extra).fill(fill);

                expect(shapes.ItemList.serializeInto(fields, bytes)).toStrictEqual(expected);
            }
        );

        it.prop([DOCUMENT_FIELDS, fc.nat(), fc.nat({ max: 0xff })])(
            'should throw RangeError for bytes shorter than a Document, and write none of them',
            (fields, cut, fill) => {
                const length = shapes.Document.serialize(fields).length;
                const bytes = new Uint8Array(length - 1 - (cut % length)).fill(fill);

                expect(() => shapes.Document.serializeInto(fields, bytes)).toThrow(
                    new RangeError(
                        `The table is ${length} bytes, longer than the ${bytes.length} bytes given`
                    )
                );
                expect(bytes).toStrictEqual(new Uint8Array(bytes.length).fill(fill));
            }
        );

        it('should return a view that the next serializeInto() into the same bytes overwrites', () => {
            const first = { x: 1.5, y: 2.5, id: 1n, name: 'a' };
            const second = { x: 3.5, y: 4.5, id: 2n, name: 'b' };
            const bytes = new Uint8Array(64);

            const written = shapes.Point.serializeInto(first, bytes);
            expect(readPoint(new shapes.Point(written))).toEqual(first);

            shapes.Point.serializeInto(second, bytes);
            expect(readPoint(new shapes.Point(written))).toEqual(second);
        });
    });

    describe('versions', () => {
        it.prop([DOCUMENT_FIELDS, VERSIONS])(
            "should read a Document's version from its bytes, of any version, to choose its reader",
            (fields, version) => {
                const bytes = buildTable(describeDocument(fields, version)).bytes();

                expect(shapes.Document.versionOf(bytes)).toBe(version);
            }
        );

        it.prop([DOCUMENT_FIELDS, fc.integer({ min: 0, max: 7 })])(
            "should throw DecodeError for the version of bytes that don't hold a Document's header",
            (fields, length) => {
                const bytes = shapes.Document.serialize(fields).slice(0, length);

                expect(() => shapes.Document.versionOf(bytes)).toThrow(DecodeError);
            }
        );
    });

    describe('decode errors', () => {
        it.prop([DOCUMENT_FIELDS, OTHER_VERSIONS])(
            'should reject a Document of another version with DecodeError',
            (fields, version) => {
                const bytes = buildTable(describeDocument(fields, version)).bytes();

                expect(() => new shapes.Document(bytes)).toThrow(DecodeError);
            }
        );

        it.prop([POINT_FIELDS, CORRUPTIONS])(
            'should read a corrupted Point as values or throw DecodeError',
            (fields, corruption) => {
                const bytes = shapes.Point.serialize(fields);
                const corrupted = corrupt(bytes, corruptionOf(corruption, bytes.length));

                expectValuesOrDecodeError(() => readPoint(new shapes.Point(corrupted)));
            }
        );

        it.prop([DOCUMENT_FIELDS, CORRUPTIONS])(
            'should read a corrupted Document as values or throw DecodeError',
            (fields, corruption) => {
                const bytes = shapes.Document.serialize(fields);
                const corrupted = corrupt(bytes, corruptionOf(corruption, bytes.length));

                expectValuesOrDecodeError(() => readDocument(new shapes.Document(corrupted)));
            }
        );

        it.prop([ITEM_LIST_FIELDS, CORRUPTIONS])(
            'should read a corrupted ItemList as values or throw DecodeError',
            (fields, corruption) => {
                const bytes = shapes.ItemList.serialize(fields);
                const corrupted = corrupt(bytes, corruptionOf(corruption, bytes.length));

                expectValuesOrDecodeError(() => readItemList(new shapes.ItemList(corrupted)));
            }
        );

        it.prop([fc.uint8Array({ maxLength: 64 })])(
            'should read arbitrary bytes as any message or throw DecodeError',
            bytes => {
                expectValuesOrDecodeError(() => readPoint(new shapes.Point(bytes)));
                expectValuesOrDecodeError(() => readDocument(new shapes.Document(bytes)));
                expectValuesOrDecodeError(() => readItemList(new shapes.ItemList(bytes)));
            }
        );

        it.prop([ITEM_LIST_FIELDS, CORRUPTIONS, BYTE_OFFSETS])(
            'should read each Item of a corrupted ItemList through its view as values or throw DecodeError, within its bytes',
            (fields, corruption, byteOffset) => {
                const bytes = shapes.ItemList.serialize(fields);
                const corrupted = corrupt(bytes, corruptionOf(corruption, bytes.length));

                expectViewReadsWithin(bytes => new shapes.ItemList(bytes), corrupted, byteOffset);
            }
        );

        it.prop([fc.uint8Array({ maxLength: 64 }), BYTE_OFFSETS])(
            'should read each Item of arbitrary bytes through an ItemList view as values or throw DecodeError, within the bytes',
            (bytes, byteOffset) => {
                expectViewReadsWithin(bytes => new shapes.ItemList(bytes), bytes, byteOffset);
            }
        );

        it('should view a vector with a corrupt item, and throw DecodeError when that item is read', () => {
            const items = [
                { value: 1.5, pair: { first: 2.5, second: 3.5 } },
                { value: 4.5, pair: { first: 5.5, second: 6.5 } }
            ];
            const bytes = shapes.ItemList.serialize({ items });
            // The vector at 4 holds its count and its items' ends, the second's
            // at 12, which then ends past the bytes
            expect(bytes.subarray(4, 8)).toStrictEqual(new Uint8Array([2, 0, 0, 0]));
            bytes.set([0xff, 0xff, 0, 0], 12);
            const view = new shapes.ItemList(bytes).items();

            expect(view.length()).toBe(2);
            expect(readItem(view.get(0))).toEqual(items[0]);
            expect(() => view.get(1)).toThrow(DecodeError);
        });
    });

    describe('whole reads', () => {
        it.prop([POINT_FIELDS, DOCUMENT_FIELDS, ITEM_LIST_FIELDS, SLICES])(
            'should read every field of a message in one call, as its getters read them, wherever its bytes lie',
            (pointFields, documentFields, itemListFields, slice) => {
                const point = new shapes.Point(sliceOf(shapes.Point.serialize(pointFields), slice));
                const document = new shapes.Document(
                    sliceOf(shapes.Document.serialize(documentFields), slice)
                );
                const itemList = new shapes.ItemList(
                    sliceOf(shapes.ItemList.serialize(itemListFields), slice)
                );

                expect(point.fields()).toEqual(readPoint(point));
                expect(withPlainArrays(document.fields())).toEqual(readDocument(document));
                expect(itemList.fields()).toEqual(readItemList(itemList));
            }
        );

        it.prop([ITEM_LIST_FIELDS])(
            "should read every Item's fields through the view, as each Item's reader reads them",
            fields => {
                const itemList = new shapes.ItemList(shapes.ItemList.serialize(fields));
                const items = itemList.items();

                expect(items.fields()).toEqual(fields.items);
                for (let i = 0; i < items.length(); i++) {
                    expect(items.get(i).fields()).toEqual(readItem(items.get(i)));
                }
            }
        );

        it.prop([POINT_FIELDS, DOCUMENT_FIELDS, ITEM_LIST_FIELDS, CORRUPTIONS])(
            'should read every field of a corrupted message in one call as values or throw DecodeError',
            (pointFields, documentFields, itemListFields, corruption) => {
                const corrupted = (bytes: Uint8Array): Uint8Array =>
                    corrupt(bytes, corruptionOf(corruption, bytes.length));

                expectValuesOrDecodeError(() =>
                    new shapes.Point(corrupted(shapes.Point.serialize(pointFields))).fields()
                );
                expectValuesOrDecodeError(() =>
                    new shapes.Document(
                        corrupted(shapes.Document.serialize(documentFields))
                    ).fields()
                );
                expectValuesOrDecodeError(() =>
                    new shapes.ItemList(corrupted(shapes.ItemList.serialize(itemListFields)))
                        .items()
                        .fields()
                );
            }
        );

        it.prop([fc.uint8Array({ maxLength: 64 })])(
            'should read every field of arbitrary bytes as any message in one call or throw DecodeError',
            bytes => {
                expectValuesOrDecodeError(() => new shapes.Point(bytes).fields());
                expectValuesOrDecodeError(() => new shapes.Document(bytes).fields());
                expectValuesOrDecodeError(() => new shapes.ItemList(bytes).fields());
            }
        );

        it.prop([SAMPLE_FIELDS, fc.nat({ max: 16 }), fc.nat({ max: 8 }), fc.nat({ max: 0xff })])(
            'should read the fields of a Sample at any offset in longer bytes, without creating it',
            (fields, offset, after, fill) => {
                const bytes = new Uint8Array(offset + 12 + after).fill(fill);
                bytes.set(sampleBytes(fields), offset);

                expect(shapes.Sample.read(bytes, offset)).toEqual(fields);
                expect(shapes.Sample.read(bytes.subarray(offset))).toEqual(fields);
            }
        );

        it.prop([fc.uint8Array({ maxLength: 24 }), fc.integer({ min: -16, max: 16 })])(
            "should throw DecodeError reading the fields of a Sample that the bytes don't hold",
            (bytes, offset) => {
                fc.pre(offset < 0 || offset + 12 > bytes.length);

                expect(() => shapes.Sample.read(bytes, offset)).toThrow(DecodeError);
            }
        );
    });

    describe('reading in place', () => {
        it('should read an Item in place, past its end, within the bytes of its ItemList', () => {
            const item = { value: 1.5, pair: { first: 2.5, second: 3.5 } };
            const bytes = shapes.ItemList.serialize({ items: [item] });
            // The vector at 4 holds its count and the item's end, 36, and then
            // the item at 12: its two offsets, its value at 20 and its Pair at
            // 24. An end of 16 leaves the item only its offsets, which point
            // past it.
            expect(bytes.subarray(8, 12)).toStrictEqual(new Uint8Array([36, 0, 0, 0]));
            bytes.set([16, 0, 0, 0], 8);
            const itemList = new shapes.ItemList(bytes);

            expect(readItemList(itemList)).toEqual({ items: [item] });
        });
    });

    describe('struct', () => {
        it.prop([SAMPLE_FIELDS, SLICES])(
            'should write a Sample in its 12 bytes, little-endian at fixed offsets, and read it back wherever they lie',
            (fields, slice) => {
                const bytes = shapes.Sample.serialize(fields);

                expect(shapes.Sample.SIZE).toBe(12);
                expect(bytes).toStrictEqual(sampleBytes(fields));
                expect(readSample(new shapes.Sample(sliceOf(bytes, slice)))).toEqual(fields);
            }
        );

        it.prop([SAMPLE_FIELDS, fc.uint8Array({ minLength: 1, maxLength: 8 })])(
            'should read a Sample from the start of longer bytes',
            (fields, after) => {
                const bytes = new Uint8Array([...shapes.Sample.serialize(fields), ...after]);

                expect(readSample(new shapes.Sample(bytes))).toEqual(fields);
            }
        );

        it.prop([fc.uint8Array({ maxLength: 11 })])(
            'should throw DecodeError reading a Sample from fewer than 12 bytes',
            bytes => {
                expect(() => new shapes.Sample(bytes)).toThrow(DecodeError);
            }
        );

        it.prop([SAMPLE_FIELDS, fc.nat({ max: 16 }), fc.nat({ max: 8 }), fc.nat({ max: 0xff })])(
            'should write a Sample at any offset in longer bytes, and leave the others as they were',
            (fields, offset, after, fill) => {
                const bytes = new Uint8Array(offset + 12 + after).fill(fill);
                shapes.Sample.writeAt(bytes, offset, fields);

                const expected = new Uint8Array(bytes.length).fill(fill);
                expected.set(sampleBytes(fields), offset);
                expect(bytes).toStrictEqual(expected);
            }
        );

        it.prop([SAMPLE_FIELDS, fc.nat({ max: 11 }), fc.nat({ max: 16 })])(
            'should throw RangeError writing a Sample past the end of the bytes',
            (fields, short, offset) => {
                const bytes = new Uint8Array(offset + short);

                expect(() => shapes.Sample.writeAt(bytes, offset, fields)).toThrow(RangeError);
            }
        );

        it.prop([SAMPLE_FIELDS, SAMPLE_FIELDS])(
            'should return bytes of its own from each serialize()',
            (first, second) => {
                const bytes = shapes.Sample.serialize(first);
                shapes.Sample.serialize(second);

                expect(bytes).toStrictEqual(sampleBytes(first));
            }
        );

        it.prop([SAMPLE_FIELDS, SAMPLE_FIELDS])(
            'should write the next Sample after a serialize() that threw',
            (first, second) => {
                // Throws once `key` and `a` are written
                const unreadable = {
                    ...first,
                    get b(): number {
                        throw new Error('An unreadable field');
                    }
                };

                expect(() => shapes.Sample.serialize(unreadable)).toThrow('An unreadable field');
                expect(shapes.Sample.serialize(second)).toStrictEqual(sampleBytes(second));
            }
        );

        it.prop([SAMPLE_FIELDS, SAMPLE_FIELDS])(
            'should write a Sample whose field getter serializes another Sample',
            (outer, inner) => {
                let innerBytes: Uint8Array | undefined;
                // Reads `b` once `key` and `a` are written
                const reentrant = {
                    ...outer,
                    get b(): number {
                        innerBytes = shapes.Sample.serialize(inner);
                        return outer.b;
                    }
                };

                expect(shapes.Sample.serialize(reentrant)).toStrictEqual(sampleBytes(outer));
                expect(innerBytes).toStrictEqual(sampleBytes(inner));
            }
        );

        it.prop([SAMPLE_FIELDS, SAMPLE_FIELDS])(
            "should write a Sample whose field's valueOf() serializes another Sample",
            (outer, inner) => {
                let innerBytes: Uint8Array | undefined;
                // The setter converts `a` once `key` is written
                const a = {
                    valueOf(): number {
                        innerBytes = shapes.Sample.serialize(inner);
                        return outer.a;
                    }
                };
                const reentrant = { ...outer, a: a as unknown as number };

                expect(shapes.Sample.serialize(reentrant)).toStrictEqual(sampleBytes(outer));
                expect(innerBytes).toStrictEqual(sampleBytes(inner));
            }
        );

        it.prop([SAMPLE_FIELDS, SAMPLE_FIELDS])(
            'should write a Sample whose field getter catches a serialize() of another that threw',
            (outer, inner) => {
                const unreadable = {
                    ...inner,
                    get b(): number {
                        throw new Error('An unreadable field');
                    }
                };
                const reentrant = {
                    ...outer,
                    get b(): number {
                        expect(() => shapes.Sample.serialize(unreadable)).toThrow(
                            'An unreadable field'
                        );
                        return outer.b;
                    }
                };

                expect(shapes.Sample.serialize(reentrant)).toStrictEqual(sampleBytes(outer));
                expect(shapes.Sample.serialize(inner)).toStrictEqual(sampleBytes(inner));
            }
        );
    });
});

/**
 * Expects each Item of an ItemList's view, read with `get(i)` from slices of
 * two buffers that hold different bytes around the ItemList, to be the same
 * fields or DecodeErrors, which a read outside the bytes would make differ
 */
function expectViewReadsWithin(
    read: (bytes: Uint8Array) => ItemList,
    bytes: Uint8Array,
    byteOffset: number
): void {
    expect(readItemsView(read(sliceOf(bytes, { byteOffset, fill: 0x00 })))).toStrictEqual(
        readItemsView(read(sliceOf(bytes, { byteOffset, fill: 0xff })))
    );
}

/**
 * Reads each Item of an ItemList through its view, one `get(i)` at a time,
 * and returns its fields or the DecodeError that reading it threw, or the
 * DecodeError of the view itself
 */
function readItemsView(itemList: ItemList): unknown[] {
    let view: ReturnType<ItemList['items']>;
    try {
        view = itemList.items();
    } catch (error) {
        expect(error).toBeInstanceOf(DecodeError);
        return [error];
    }

    const items: unknown[] = [];
    for (let i = 0; i < view.length(); i++) {
        items.push(readOrDecodeError(() => readItem(view.get(i))));
    }

    return items;
}

/** What `read` returns, or the DecodeError that it throws */
function readOrDecodeError(read: () => unknown): unknown {
    try {
        return read();
    } catch (error) {
        // Neither a RangeError, which DecodeError extends, nor any other error
        expect(error).toBeInstanceOf(DecodeError);
        return error;
    }
}
