import { describe, expect } from 'hypotes';
import { fc, it } from 'hypotes/fast-check';
import { TableArena, TableBuilder } from 'tafla';
import { tables } from './conformance/arbitraries.ts';
import { buildTable } from './conformance/tables.ts';

/** Lengths of buffers around those that tables of the suite's shapes need */
const BYTES = fc.oneof(fc.nat({ max: 64 }), fc.nat({ max: 8192 }));

/**
 * A table of one string of `length` ASCII characters, which the builder
 * writes in its buffer, as it does every value shorter than 1 KiB and every
 * string
 */
function stringTable(arena: TableArena, length: number): Uint8Array {
    const builder = new TableBuilder(arena, 1);
    builder.setString('a'.repeat(length));

    return builder.bytes();
}

it.prop([tables(), BYTES, BYTES])(
    "should write the same table in an arena of any lengths as in the default arena's",
    (table, initialBytes, more) => {
        const arena = new TableArena({ initialBytes, maxRetainedBytes: initialBytes + more });
        const expected = buildTable(table).bytes();

        // Twice, so that the second is written in the buffer that's kept
        expect(buildTable(table, arena).bytes()).toStrictEqual(expected);
        expect(buildTable(table, arena).bytes()).toStrictEqual(expected);
        expect(arena.retainedBytes()).toBeLessThanOrEqual(initialBytes + more);
    }
);

it.prop([fc.array(tables(), { minLength: 2, maxLength: 4 }), BYTES])(
    'should write the tables of builders that hold bytes of one arena at once',
    (descriptions, maxRetainedBytes) => {
        const arena = new TableArena({ maxRetainedBytes });
        const builders = descriptions.map(table => buildTable(table, arena));

        for (const [i, builder] of builders.entries()) {
            expect(builder.bytes()).toStrictEqual(buildTable(descriptions[i]).bytes());
        }
    }
);

it('should nest builders of different arenas in each other', () => {
    const build = (outer: TableArena, inner: TableArena): Uint8Array => {
        const short = new TableBuilder(inner, 1);
        short.setUint32(7);
        // Long enough for its parent to keep it, rather than copy it in
        const long = new TableBuilder(inner, 1);
        long.setUint8Array(new Uint8Array(500).fill(3));

        const parent = new TableBuilder(outer, 2);
        parent.setTable(short);
        parent.setVector([long, short]);

        return parent.bytes();
    };
    const arena = new TableArena();

    expect(build(arena, new TableArena({ initialBytes: 64 }))).toStrictEqual(
        build(TableArena.DEFAULT, TableArena.DEFAULT)
    );
});

describe('retainedBytes()', () => {
    it("should be 0 until a builder takes bytes, and then the arena's initial bytes", () => {
        const arena = new TableArena({ initialBytes: 16_384 });
        expect(arena.retainedBytes()).toBe(0);

        const builder = new TableBuilder(arena, 1);
        expect(arena.retainedBytes()).toBe(16_384);

        builder.setUint8(1);
        builder.bytes();
        expect(arena.retainedBytes()).toBe(16_384);
    });

    it('should be 4 KiB at first by default', () => {
        const arena = new TableArena();
        stringTable(arena, 10);

        expect(arena.retainedBytes()).toBe(4096);
    });

    it('should round initial bytes up to a multiple of 8, and write a table that fills them', () => {
        // An array of less than 1 KiB, which the builder copies into its buffer
        const build = (arena: TableArena): Uint8Array => {
            const builder = new TableBuilder(arena, 1);
            builder.setUint8Array(new Uint8Array(996).fill(5));

            return builder.bytes();
        };
        const arena = new TableArena({ initialBytes: 1001 });

        expect(build(arena)).toStrictEqual(build(TableArena.DEFAULT));
        expect(arena.retainedBytes()).toBe(1008);
    });

    it('should grow for a longer table, and keep that buffer for the next', () => {
        const arena = new TableArena({ initialBytes: 64 });
        stringTable(arena, 100_000);
        const grown = arena.retainedBytes();
        expect(grown).toBeGreaterThanOrEqual(100_000);

        stringTable(arena, 100_000);
        expect(arena.retainedBytes()).toBe(grown);
    });

    it('should keep no more than the max retained bytes, and still write a longer table', () => {
        const arena = new TableArena({ maxRetainedBytes: 1001 });
        const expected = stringTable(TableArena.DEFAULT, 50_000);

        expect(stringTable(arena, 50_000)).toStrictEqual(expected);
        expect(arena.retainedBytes()).toBeLessThanOrEqual(1000);
    });

    it('should keep initial bytes of more than the default max retained bytes', () => {
        const arena = new TableArena({ initialBytes: 2 * 1024 * 1024 });
        stringTable(arena, 10);

        expect(arena.retainedBytes()).toBe(2 * 1024 * 1024);
    });

    it('should keep nothing with max retained bytes of 0', () => {
        const arena = new TableArena({ maxRetainedBytes: 0 });

        expect(stringTable(arena, 100)).toStrictEqual(stringTable(TableArena.DEFAULT, 100));
        expect(arena.retainedBytes()).toBe(0);
    });
});

describe('release()', () => {
    it("should drop the arena's buffer, which the next builder allocates again", () => {
        const arena = new TableArena({ initialBytes: 64 });
        stringTable(arena, 10_000);

        arena.release();
        expect(arena.retainedBytes()).toBe(0);

        expect(stringTable(arena, 10)).toStrictEqual(stringTable(TableArena.DEFAULT, 10));
        expect(arena.retainedBytes()).toBe(64);
    });

    it('should leave the tables of builders that hold bytes of the buffer as they were', () => {
        const arena = new TableArena();
        const before = new TableBuilder(arena, 2);
        before.setString('set before the release');

        arena.release();
        const after = new TableBuilder(arena, 1);
        after.setString('a builder of the next buffer');
        before.setString('set after it');
        after.bytes();

        const expected = new TableBuilder(TableArena.DEFAULT, 2);
        expected.setString('set before the release');
        expected.setString('set after it');
        expect(before.bytes()).toStrictEqual(expected.bytes());
    });
});

describe('new TableArena()', () => {
    it.each([-1, 1.5, Number.NaN, Infinity, 2 ** 32])('should reject %d bytes', bytes => {
        expect(() => new TableArena({ initialBytes: bytes })).toThrow(
            new RangeError(`initialBytes must be a non-negative integer, got ${bytes}`)
        );
        expect(() => new TableArena({ maxRetainedBytes: bytes })).toThrow(
            new RangeError(`maxRetainedBytes must be a non-negative integer, got ${bytes}`)
        );
    });

    it('should reject initial bytes of more than the max retained bytes', () => {
        expect(() => new TableArena({ initialBytes: 4096, maxRetainedBytes: 1024 })).toThrow(
            new RangeError('initialBytes 4096 is more than maxRetainedBytes 1024')
        );
    });

    it('should start at no more than max retained bytes of less than the default 4 KiB', () => {
        const arena = new TableArena({ maxRetainedBytes: 1024 });
        stringTable(arena, 10);

        expect(arena.retainedBytes()).toBe(1024);
    });
});

it('should reject a builder without an arena', () => {
    // @ts-expect-error The call that the builder had before it took an arena
    expect(() => new TableBuilder(3)).toThrow(RangeError);
    /* oxlint-disable typescript/no-unsafe-class-instance-assignment -- an object that isn't an arena, which is what's tested */
    // @ts-expect-error An object that isn't an arena
    expect(() => new TableBuilder({}, 3)).toThrow(TypeError);
    /* oxlint-enable typescript/no-unsafe-class-instance-assignment */
});
