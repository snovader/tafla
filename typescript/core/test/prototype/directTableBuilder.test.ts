import { readFile } from 'node:fs/promises';
import { afterAll, beforeAll, describe, expect } from 'vitest';
import { fc, it } from '@fast-check/vitest';
import { TableBuilder, type TableItem, TableLayout } from 'tafla';
import { configure, DirectTableBuilder, type Options } from '../../prototype/directTableBuilder.ts';
import { FIELDS, MAX_VALUES, OFFSET_TYPES } from '../conformance/arbitraries.ts';
import type {
    FieldDescription,
    LayoutDescription,
    OffsetType,
    Suite,
    TableDescription
} from '../conformance/description.ts';
import { SUITE_DIRECTORY } from '../conformance/files.ts';
import { OFFSET_SIZES } from '../conformance/format.ts';
import {
    type Builder,
    buildTable,
    buildTableInPlace,
    createBuilder,
    nestedInPlace,
    setField,
    setFieldInPlace
} from '../conformance/tables.ts';

// The prototype has TableBuilder's API, so the conformance helpers build
// with it in TableBuilder's place
const Direct = DirectTableBuilder as unknown as Builder;

const json = await readFile(new URL('cases.json', SUITE_DIRECTORY), 'utf8');
const suite: Suite = JSON.parse(json);
const golden = await Promise.all(
    suite.valid.map(
        async ({ file }) => new Uint8Array(await readFile(new URL(file, SUITE_DIRECTORY)))
    )
);

const DEFAULTS: Options = {
    growth: 'copy',
    result: 'slice',
    strings: 'short',
    scalars: 'view',
    segmentBytes: 1024,
    reuse: true,
    arenaBytes: 64 * 1024
};

/** The variants of the design, each tested as the proposed one is */
const VARIANTS: Array<[string, Partial<Options>]> = [
    ['proposed: an arena of 64 KiB, runs of 1 KiB as segments', {}],
    ['an arena of 256 bytes', { arenaBytes: 256 }],
    ['an arena of 256 bytes, scalars written as bytes', { arenaBytes: 256, scalars: 'bytes' }],
    ['every run of a byte or more a segment', { segmentBytes: 1 }],
    ['runs of 8 bytes or more segments', { segmentBytes: 8 }],
    ['runs of 256 bytes or more segments', { segmentBytes: 256 }],
    ['no segments', { segmentBytes: Number.POSITIVE_INFINITY }],
    ['strings counted before they are written', { strings: 'count' }],
    ['strings counted when the buffer has no room for them', { strings: 'room' }],
    ['strings given room for their longest UTF-8', { strings: 'worst' }],
    ['scalars written as bytes', { scalars: 'bytes' }],
    ['no arena, buffers reused', { arenaBytes: 0 }],
    ['no arena, buffers not reused', { arenaBytes: 0, reuse: false }],
    ['no arena, every run of a byte or more a segment', { arenaBytes: 0, segmentBytes: 1 }],
    [
        'no arena, scalars written as bytes, buffers not reused',
        { arenaBytes: 0, scalars: 'bytes', reuse: false }
    ],
    [
        'no arena, grown and returned by transfer()',
        { arenaBytes: 0, growth: 'transfer', result: 'transfer' }
    ],
    ['no arena, a resizable buffer, sliced', { arenaBytes: 0, growth: 'resizable' }],
    [
        'no arena, a resizable buffer, transferred',
        { arenaBytes: 0, growth: 'resizable', result: 'transfer' }
    ]
];

describe.each(VARIANTS)('%s', (_, variant) => {
    beforeAll(() => configure({ ...DEFAULTS, ...variant }));
    afterAll(() => configure(DEFAULTS));

    describe('the conformance suite', () => {
        const cases = suite.valid.map((validCase, i) => ({
            ...validCase,
            bytes: golden[i]
        }));

        it.each(cases)('should rebuild $name byte for byte', ({ table, bytes }) => {
            expect(buildTable(table, Direct).bytes()).toStrictEqual(bytes);
        });

        it.each(cases)(
            'should rebuild $name with its nested tables in place as TableBuilder does',
            ({ table }) => {
                const nested = nestedInPlace(table);
                expect(buildTableInPlace(nested, Direct).bytes()).toStrictEqual(
                    buildTableInPlace(nested).bytes()
                );
            }
        );
    });

    it.prop([scripts()], { numRuns: 300 })(
        'should write the bytes, and throw the errors, that TableBuilder writes and throws',
        script => {
            expect(outcomesOf(Direct, script)).toStrictEqual(outcomesOf(TableBuilder, script));
        }
    );
});

describe('tables written in place', () => {
    beforeAll(() => configure(DEFAULTS));

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

        const builder = new Direct(2);
        set(builder, true);
        expect(builder.bytes()).toStrictEqual(expectedBytes(2, table => set(table, false)));
    });

    it.each(['uint8', 'uint16'] as const)(
        'should reject a %s table made too long by a table or vector written in place, and leave it as it was',
        offsetType => {
            const layout = TableLayout.create({ offsetType });
            const room = MAX_VALUES[offsetType] - 2 * OFFSET_SIZES[offsetType];
            const long = new Uint8Array(room - 2 * OFFSET_SIZES[offsetType]);
            const fill = (table: TableItem): void => table.setUint8Array(long);

            const builder = new Direct(2, layout);
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
});

describe.each([64 * 1024, 256])('an arena of %i bytes', arenaBytes => {
    beforeAll(() => configure({ ...DEFAULTS, arenaBytes }));
    afterAll(() => configure(DEFAULTS));

    function setFields(builder: TableItem, value: number): void {
        builder.setUint32(value);
        builder.setString('x'.repeat(value));
    }

    it("should keep a builder's bytes when an earlier builder is written, and give them to no other builder", () => {
        const first = new Direct(2);
        setFields(first, 1);
        const second = new Direct(2);
        setFields(second, 2);

        // The first builder's bytes aren't the arena's last, so the third
        // builder, whose string reaches past them, takes bytes after the
        // second's
        const firstBytes = first.bytes();
        const third = new Direct(2);
        setFields(third, 40);
        const nested = new Direct(1);
        nested.setTable(second);

        expect(firstBytes).toStrictEqual(expectedBytes(2, table => setFields(table, 1)));
        expect(third.bytes()).toStrictEqual(expectedBytes(2, table => setFields(table, 40)));
        expect(second.bytes()).toStrictEqual(expectedBytes(2, table => setFields(table, 2)));
        expect(nested.bytes()).toStrictEqual(
            expectedBytes(1, table => table.setTable(second.bytes(), 1))
        );
    });

    it('should give back the bytes of a builder written after the last only when no builder holds bytes after them', () => {
        const builders = [1, 2, 3].map(value => {
            const builder = new Direct(2);
            setFields(builder, value);
            return builder;
        });
        const [first, second, third] = builders;

        // The third gives its bytes back, but the first's are below the
        // second's, which a fourth builder mustn't take
        third.bytes();
        first.bytes();
        const fourth = new Direct(2);
        setFields(fourth, 40);

        expect(second.bytes()).toStrictEqual(expectedBytes(2, table => setFields(table, 2)));
        expect(fourth.bytes()).toStrictEqual(expectedBytes(2, table => setFields(table, 40)));
    });

    it("should keep the bytes that a setter made room for when a value's valueOf() makes a builder", () => {
        const inner: TableBuilder[] = [];
        const value = {
            valueOf(): number {
                const builder = new Direct(2);
                setFields(builder, 5);
                inner.push(builder);
                return 2.5;
            }
        };

        const builder = new Direct(3);
        builder.setUint32(1);
        builder.setFloat64(value as unknown as number);
        builder.setFloat64Array([1, value as unknown as number, 3]);

        expect(builder.bytes()).toStrictEqual(
            expectedBytes(3, table => {
                table.setUint32(1);
                table.setFloat64(2.5);
                table.setFloat64Array([1, 2.5, 3]);
            })
        );
        expect(inner).toHaveLength(2);
        for (const made of inner) {
            expect(made.bytes()).toStrictEqual(expectedBytes(2, table => setFields(table, 5)));
        }
    });

    it('should move a builder that grows after a later one took bytes, and keep both', () => {
        const growing = new Direct(2);
        growing.setUint32(1);
        const later = new Direct(2);
        setFields(later, 4);
        growing.setString('y'.repeat(300));

        expect(growing.bytes()).toStrictEqual(
            expectedBytes(2, table => {
                table.setUint32(1);
                table.setString('y'.repeat(300));
            })
        );
        expect(later.bytes()).toStrictEqual(expectedBytes(2, table => setFields(table, 4)));
    });
});

describe('values kept until the table is written', () => {
    beforeAll(() => configure(DEFAULTS));

    // TableBuilder writes every array when the table is written, the
    // prototype only those it keeps as segments
    it('should copy a short array when it is set, and write a long one as it is when the table is written', () => {
        const short = new Float32Array([1, 2]);
        const long = new Float32Array(512).fill(1);
        const builder = new DirectTableBuilder(2);
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

    it.each([
        ['grew', 257],
        ['shrank', 255]
    ])(
        'should throw RangeError for a long array that %s, and write it once it is restored',
        (_, length) => {
            const values = new Array<number>(256).fill(1);
            const builder = new DirectTableBuilder(2);
            builder.setFloat64Array(values);
            builder.setUint8(7);

            values.length = length;
            values.fill(1);
            expect(() => builder.bytes()).toThrow(
                new RangeError(`An array of 256 elements held ${length} when the table was written`)
            );

            values.length = 256;
            values.fill(1);
            expect(builder.bytes()).toStrictEqual(
                expectedBytes(2, table => {
                    table.setFloat64Array(values);
                    table.setUint8(7);
                })
            );
        }
    );

    it('should throw RangeError for a long typed array whose buffer was transferred, and stay unfinalized', () => {
        const values = new Float64Array(256);
        const builder = new DirectTableBuilder(2);
        builder.setFloat64Array(values);
        builder.setUint8(7);

        values.buffer.transfer();
        expect(() => builder.finalize()).toThrow(RangeError);
        expect(() => builder.setUint8(1)).toThrow(
            new RangeError("Can't add field 3 to a table of 2")
        );
    });

    it('should throw TypeError from the setter of a value of the wrong type, and leave the builder as it was', () => {
        const builder = new DirectTableBuilder(1);

        expect(() => builder.setInt64(1 as unknown as bigint)).toThrow(TypeError);
        builder.setInt64(1n);
        expect(builder.bytes()).toStrictEqual(expectedBytes(1, table => table.setInt64(1n)));
    });
});

/** The bytes of a table of `fields` fields that TableBuilder sets with `set` */
function expectedBytes(fields: number, set: (builder: TableItem) => void): Uint8Array {
    const builder = new TableBuilder(fields);
    set(builder);
    return builder.bytes();
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
 * Long arrays of bytes, which make a table of `offsetType` offsets about as
 * long as it can be, or longer
 */
function longFields(offsetType: OffsetType): fc.Arbitrary<FieldDescription> {
    const max = MAX_VALUES[offsetType];
    if (offsetType === 'uint32') {
        // Too long to write; arrays long enough to be segments instead
        return fc.integer({ min: 200, max: 600 }).map(bytesField);
    }

    return fc.integer({ min: Math.max(max - 300, 0), max: max + 8 }).map(bytesField);
}

/**
 * Tables and vectors whose items hold a long array of bytes, which may make
 * the table that they're nested in too long, though they fit themselves
 */
function longNestedFields(offsetType: OffsetType): fc.Arbitrary<FieldDescription> {
    const layout = { sized: false, versioned: false, offsetType };
    const tables = fc
        .tuple(longFields(offsetType), fc.array(FIELDS, { maxLength: 2 }))
        .map(([long, others]): TableDescription => ({ layout, fields: [...others, long] }));

    return fc.oneof(
        tables.map((table): FieldDescription => ({ type: 'table', from: 'builder', table })),
        fc.array(tables, { minLength: 1, maxLength: 2 }).map(
            (items): FieldDescription => ({
                type: 'vector',
                items: items.map(table => ({ from: 'builder' as const, table }))
            })
        )
    );
}

function bytesField(length: number): FieldDescription {
    return { type: 'uint8[]', value: new Array<number>(length).fill(1) };
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
 * their fields set, and fills that throw
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
            .map(({ header, fields, extra, inPlace, throwAt }): Script => {
                const table = { ...header, fields };

                return {
                    table: inPlace ? nestedInPlace(table) : table,
                    fields: Math.max(fields.length + extra, 0),
                    inPlace,
                    throwAt
                };
            });
    });
}

/** The error that a script's `throwAt` throws */
class ScriptError extends Error {}

/**
 * Runs a script with a builder class, and returns what each step gave: the
 * bytes of the table, or the class and message of each error thrown
 */
function outcomesOf(Builder: Builder, script: Script): unknown[] {
    const description = {
        ...script.table,
        fields: new Array<FieldDescription>(script.fields)
    };

    let builder: TableBuilder;
    try {
        builder = createBuilder(description, Builder);
    } catch (error) {
        return [errorOf(error)];
    }

    const calls = { count: 0, throwAt: script.throwAt };
    const target = throwing(builder, calls);
    const { offsetType } = script.table.layout;

    const outcomes: unknown[] = [];
    for (const field of script.table.fields) {
        outcomes.push(
            outcomeOf(() =>
                script.inPlace
                    ? setFieldInPlace(target, field, offsetType, Builder)
                    : setField(target, field, Builder)
            )
        );
    }

    outcomes.push(outcomeOf(() => builder.bytes()));
    outcomes.push(outcomeOf(() => builder.finalize().bytes()));

    return outcomes;
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

function outcomeOf(action: () => unknown): unknown {
    try {
        const value = action();
        return value instanceof Uint8Array ? value : 'set';
    } catch (error) {
        return errorOf(error);
    }
}

function errorOf(error: unknown): unknown {
    if (!(error instanceof Error)) {
        throw error;
    }

    return [error.constructor.name, error.message];
}
