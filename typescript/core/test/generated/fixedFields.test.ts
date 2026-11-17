import { describe, it } from 'node:test';
import * as fc from 'fast-check';
import { SCALAR_VALUES, STRINGS } from '../conformance/arbitraries.ts';
import type { FieldDescription, LayoutDescription } from '../conformance/description.ts';
import { expect } from '../conformance/expect.ts';
import { prop } from '../conformance/property.ts';
import { buildTable } from '../conformance/tables.ts';
import {
    array,
    message,
    scalar,
    string,
    struct,
    table,
    vector,
    DEFAULT_LAYOUT
} from './descriptions.ts';
import * as javascript from './fixed_fields/javascript/fixedFields.js';
import * as typescript from './fixed_fields/typescript/fixedFields.ts';
import type {
    BlockFields,
    FrameFields,
    QuadFields,
    RangeFields,
    TaggedFields,
    Vec2Fields
} from './fixed_fields/typescript/fixedFields.ts';

// The code generated from schemas/fixed_fields.tafla, in TypeScript and in
// JavaScript, writes the tables that TableBuilder writes by hand in every
// offset type and header, and fails to write a message whose fixed fields
// alone, or with a string after them, are too long for its offset type, as
// TableBuilder fails. It writes these messages and nested tables whole, at
// positions that the generator works out: Tiny, Tagged, Pixel, Box and Label
// without a sizer and a writer, and the others' nested tables of fixed size
// in one call each, and the items of a vector of them through a DataView
// from 128 bytes, 8 Ranges.

type FixedFields = typeof typescript | typeof javascript;

const TARGETS: ReadonlyArray<readonly [string, FixedFields]> = [
    ['TypeScript', typescript],
    ['JavaScript', javascript]
];

// The layouts of the messages, from their attributes in the schema

const TINY = layoutOf('uint8', { sized: true });
const TAGGED = layoutOf('uint8', { sized: true, versioned: true });
const HEADER = layoutOf('uint8');
const PIXEL = layoutOf('uint16', { sized: true, versioned: true });
const STRIP = layoutOf('uint16');
const BOX = layoutOf('uint32', { versioned: true });

function layoutOf(
    offsetType: LayoutDescription['offsetType'],
    header: Partial<LayoutDescription> = {}
): LayoutDescription {
    return { sized: false, versioned: false, offsetType, ...header };
}

const VEC2S: fc.Arbitrary<Vec2Fields> = fc.record({
    x: SCALAR_VALUES.float32,
    y: SCALAR_VALUES.float32
});

const RANGES: fc.Arbitrary<RangeFields> = fc.record({
    start: SCALAR_VALUES.uint32,
    end: SCALAR_VALUES.uint32
});

const FRAMES: fc.Arbitrary<FrameFields> = fc.record({
    origin: VEC2S,
    range: RANGES,
    flags: SCALAR_VALUES.uint16
});

/** Labels of up to 100 code points, of which 237 bytes fit in Tagged */
const LABELS = fc.string({ unit: 'binary', maxLength: 100 });

/** Vectors of Ranges on both sides of the 8 that a DataView writes */
const RANGE_VECTORS = fc.array(RANGES, { maxLength: 20 });

function describeVec2({ x, y }: Vec2Fields): FieldDescription {
    return struct([
        ['float32', x],
        ['float32', y]
    ]);
}

function describeRange({ start, end }: RangeFields): FieldDescription[] {
    return [scalar('uint32', start), scalar('uint32', end)];
}

function describeFrame({ origin, range, flags }: FrameFields): FieldDescription[] {
    return [describeVec2(origin), table(describeRange(range)), scalar('uint16', flags)];
}

/** The bytes that `write` returns, or the class and message of what it throws */
function outcomeOf(write: () => Uint8Array): Uint8Array | string {
    try {
        return write();
    } catch (error) {
        return error instanceof Error
            ? `${error.constructor.name}: ${error.message}`
            : String(error);
    }
}

/** The class of what `write` throws, or its bytes */
function classOf(write: () => Uint8Array): Uint8Array | string {
    try {
        return write();
    } catch (error) {
        return error instanceof Error ? error.constructor.name : String(error);
    }
}

const QUAD: QuadFields = { a: 1, b: 2, c: 3, d: 4, e: 5, f: 6, g: 7, h: 8 };
const BLOCK: BlockFields = { first: QUAD, second: QUAD, third: QUAD, fourth: QUAD };
const ORIGIN: Vec2Fields = { x: 1.5, y: -0 };
const FRAME: FrameFields = { origin: ORIGIN, range: { start: 3, end: 7 }, flags: 0xa5a5 };

for (const [language, fixed] of TARGETS) {
    describe(language, () => {
        describe('tables built by hand', () => {
            prop([SCALAR_VALUES.uint16, SCALAR_VALUES.uint8])(
                'should write Tiny whole as TableBuilder writes it',
                (id, kind) => {
                    const byHand = buildTable(
                        message(TINY, [scalar('uint16', id), scalar('uint8', kind)])
                    ).bytes();

                    expect(fixed.Tiny.serialize({ id, kind })).toStrictEqual(byHand);
                }
            );

            prop([SCALAR_VALUES.uint32, VEC2S, LABELS])(
                'should write Tagged whole as TableBuilder writes it, or throw as it does for a label too long for uint8 offsets',
                (id, position, label) => {
                    const described = [scalar('uint32', id), describeVec2(position), string(label)];
                    const byHand = classOf(() => buildTable(message(TAGGED, described, 3)).bytes());

                    expect(
                        classOf(() => fixed.Tagged.serialize({ id, position, label }))
                    ).toStrictEqual(byHand);
                }
            );

            prop([VEC2S, SCALAR_VALUES.uint32, VEC2S, RANGES])(
                'should write Pixel and Box whole as TableBuilder writes them',
                (position, color, max, range) => {
                    const pixel = buildTable(
                        message(PIXEL, [describeVec2(position), scalar('uint32', color)], 2)
                    ).bytes();
                    expect(fixed.Pixel.serialize({ position, color })).toStrictEqual(pixel);

                    const described = [
                        describeVec2(position),
                        describeVec2(max),
                        table(describeRange(range))
                    ];
                    const box = buildTable(message(BOX, described, 1)).bytes();
                    expect(fixed.Box.serialize({ min: position, max, range })).toStrictEqual(box);
                }
            );

            prop([
                SCALAR_VALUES.uint8,
                SCALAR_VALUES.uint16,
                fc.array(SCALAR_VALUES.uint8, { maxLength: 8 }),
                VEC2S,
                fc.array(SCALAR_VALUES.uint16, { maxLength: 8 })
            ])(
                'should write Header and Strip, whose arrays follow their fixed fields, as TableBuilder writes them',
                (kind, length, bytes, origin, values) => {
                    const header = buildTable(
                        message(HEADER, [
                            scalar('uint8', kind),
                            scalar('uint16', length),
                            array('uint8', bytes)
                        ])
                    ).bytes();
                    expect(fixed.Header.serialize({ kind, length, values: bytes })).toStrictEqual(
                        header
                    );

                    const described = [
                        scalar('uint32', length),
                        describeVec2(origin),
                        array('uint16', values)
                    ];
                    const strip = buildTable(message(STRIP, described)).bytes();
                    expect(fixed.Strip.serialize({ id: length, origin, values })).toStrictEqual(
                        strip
                    );
                }
            );

            prop([
                SCALAR_VALUES.uint32,
                VEC2S,
                FRAMES,
                fc.array(SCALAR_VALUES.float32, { maxLength: 8 }),
                STRINGS,
                RANGES,
                RANGE_VECTORS
            ])(
                'should nest the tables that it writes whole, alone and in a vector, as TableBuilder nests them',
                (id, origin, frame, points, name, range, ranges) => {
                    const track = buildTable(
                        message(DEFAULT_LAYOUT, [
                            scalar('uint32', id),
                            describeVec2(origin),
                            table(describeFrame(frame)),
                            array('float32', points),
                            string(name)
                        ])
                    ).bytes();
                    expect(
                        fixed.Track.serialize({ id, origin, frame, points, name })
                    ).toStrictEqual(track);

                    const span = buildTable(
                        message(DEFAULT_LAYOUT, [
                            string(name),
                            table(describeRange(range)),
                            table(describeFrame(frame)),
                            vector(ranges.map(describeRange))
                        ])
                    ).bytes();
                    expect(fixed.Span.serialize({ name, range, frame, ranges })).toStrictEqual(
                        span
                    );
                }
            );

            prop([STRINGS])(
                'should write Label, only a string, whole as TableBuilder writes it',
                text => {
                    const byHand = buildTable(message(DEFAULT_LAYOUT, [string(text)])).bytes();

                    expect(fixed.Label.serialize({ text })).toStrictEqual(byHand);
                }
            );
        });

        describe('bytes of the caller', () => {
            prop([SCALAR_VALUES.uint32, VEC2S, STRINGS.filter(label => label.length <= 50)])(
                'should write Tagged whole into the start of longer bytes of any content, as serialize() writes it',
                (id, position, label) => {
                    const fields: TaggedFields = { id, position, label };
                    const expected = fixed.Tagged.serialize(fields);
                    const bytes = new Uint8Array(expected.length + 5).fill(0xa5);
                    const written = fixed.Tagged.serializeInto(fields, bytes);

                    expect(written).toStrictEqual(expected);
                    expect(written.buffer).toBe(bytes.buffer);
                    expect(bytes.subarray(expected.length)).toStrictEqual(
                        new Uint8Array(5).fill(0xa5)
                    );
                }
            );

            prop([FRAMES, RANGE_VECTORS])(
                'should write Span, with its vector of Ranges, into the start of longer bytes of any content',
                (frame, ranges) => {
                    const fields = { name: 'span', range: { start: 1, end: 2 }, frame, ranges };
                    const expected = fixed.Span.serialize(fields);
                    const bytes = new Uint8Array(expected.length + 3).fill(0xff);

                    expect(fixed.Span.serializeInto(fields, bytes)).toStrictEqual(expected);
                }
            );

            it('should throw RangeError for bytes shorter than a whole message, and write none of them', () => {
                const fields = { min: ORIGIN, max: ORIGIN, range: { start: 1, end: 2 } };
                const length = fixed.Box.serialize(fields).length;
                const bytes = new Uint8Array(length - 1).fill(0xa5);

                expect(() => fixed.Box.serializeInto(fields, bytes)).toThrow(
                    new RangeError(
                        `The table is ${length} bytes, longer than the ${length - 1} bytes given`
                    )
                );
                expect(bytes).toStrictEqual(new Uint8Array(length - 1).fill(0xa5));
            });
        });

        describe('round trip', () => {
            it('should read back the fields of the messages that it writes whole', () => {
                const tiny = fixed.Tiny.serialize({ id: 513, kind: 9 });
                expect(tiny).toHaveLength(6);
                expect([new fixed.Tiny(tiny).id(), new fixed.Tiny(tiny).kind()]).toStrictEqual([
                    513, 9
                ]);

                const tagged = fixed.Tagged.serialize({
                    id: 0xffff_ffff,
                    position: ORIGIN,
                    label: 'a 😀'
                });
                const read = new fixed.Tagged(tagged);
                expect(fixed.Tagged.versionOf(tagged)).toBe(3);
                expect(read.id()).toBe(0xffff_ffff);
                expect(Object.is(read.position().y(), -0)).toBe(true);
                expect(read.label()).toBe('a 😀');

                const box = new fixed.Box(
                    fixed.Box.serialize({
                        min: ORIGIN,
                        max: { x: 2, y: 3 },
                        range: { start: 1, end: 2 }
                    })
                );
                expect([box.max().x(), box.range().start(), box.range().end()]).toStrictEqual([
                    2, 1, 2
                ]);
            });

            it('should read back a vector of Ranges written through a DataView', () => {
                const ranges = Array.from({ length: 20 }, (_, i) => ({ start: i, end: 2 * i }));
                const span = new fixed.Span(
                    fixed.Span.serialize({
                        name: 'span',
                        range: { start: 5, end: 6 },
                        frame: FRAME,
                        ranges
                    })
                );
                const view = span.ranges();

                expect(view.fields()).toStrictEqual(ranges);
                expect([
                    span.range().end(),
                    span.frame().flags(),
                    span.frame().range().end()
                ]).toStrictEqual([6, 0xa5a5, 7]);
            });
        });

        describe('tables too long for their offsets', () => {
            it('should write a Tagged of 255 bytes, and throw the sizer’s RangeError for one more byte of label', () => {
                // The header, offsets, fields and length take 18 of the 255 bytes
                const fits = fixed.Tagged.serialize({
                    id: 1,
                    position: ORIGIN,
                    label: 'a'.repeat(237)
                });
                expect(fits).toHaveLength(255);
                expect(new fixed.Tagged(fits).label()).toBe('a'.repeat(237));

                expect(
                    outcomeOf(() =>
                        fixed.Tagged.serialize({ id: 1, position: ORIGIN, label: 'a'.repeat(238) })
                    )
                ).toBe(
                    "RangeError: The table's fields make it 256 bytes, too long for uint8 offsets"
                );
            });

            it('should write Overflow, whose fixed fields alone are too long for uint8 offsets, field by field and throw as TableBuilder does', () => {
                const byHand = classOf(() =>
                    buildTable(
                        message(HEADER, [
                            scalar('uint8', 1),
                            struct(
                                Array.from(
                                    { length: 32 },
                                    (_, i) => ['float64', (i % 8) + 1] as const
                                )
                            )
                        ])
                    ).bytes()
                );

                // The offsets, the id and then the block's 256 bytes
                expect(outcomeOf(() => fixed.Overflow.serialize({ id: 1, block: BLOCK }))).toBe(
                    "RangeError: The table's fields make it 259 bytes, too long for uint8 offsets"
                );
                expect(byHand).toBe('RangeError');
            });
        });
    });
}
