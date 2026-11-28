/** An arena's first buffer, unless its `initialBytes` say otherwise */
const INITIAL_BYTES = 4096;
/**
 * The longest buffer that an arena keeps, unless its `maxRetainedBytes` say
 * otherwise. With 64 KiB, tables past it grew a buffer of their own in
 * every build: 130 columns of 500 bytes took 1.35–1.45 times as long as
 * from the builder that recorded its fields, and 0.94–0.95 with 1 MiB.
 */
const MAX_RETAINED_BYTES = 1024 * 1024;
/**
 * The length that an arena's buffer is doubled to when it's replaced because
 * it's full, rather than for a table that needs more. Nested builders keep
 * their bytes, so a program that nests many fills it again and again.
 * Replacing it with one of 4 KiB each time took 1.07–1.46 times as long for
 * vectors and chains of nested builders.
 */
const DOUBLED_BYTES = 64 * 1024;

export interface TableArenaOptions {
    /**
     * The length of the arena's first buffer, which it allocates when a
     * builder first takes bytes of it: 4 KiB by default, and no more than
     * `maxRetainedBytes`. An arena whose tables are known to be longer
     * starts with room for them, rather than growing to it. Lengths are
     * rounded to multiples of 8.
     */
    initialBytes?: number;
    /**
     * The longest buffer that the arena keeps between builders: 1 MiB by
     * default, or `initialBytes` when they're more. A builder that needs more writes in a buffer of its own,
     * which is allocated for that table and not kept.
     */
    maxRetainedBytes?: number;
}

/**
 * An arena's buffer, with its view, and what its builders hold of it. They're
 * fields of an object apart from `TableArena`, which `TableBuilder` reads
 * and writes as it takes and returns bytes, so that the arena's own class
 * has nothing but its public methods.
 *
 * Each builder takes the bytes after the last one's, and copies its table
 * out of them when it's written, so no table holds the buffer. Builders
 * write every byte of their tables, padding included, so what the buffer
 * held before isn't read.
 *
 * A `Uint8Array` of more than 64 bytes is allocated outside V8's heap, in
 * about 150 ns, so a buffer for each builder made a document of 100 rows
 * take 1.2 times as long as recording its fields did. One buffer that only
 * one builder at a time could hold made every other builder that was being
 * written allocate its own: a record took 1.2 times as long from two
 * builders written at once as it did when its fields were recorded, and 32
 * builders, each nested in the next, 3 times as long.
 *
 * A builder's bytes are its own until it's written: it gives them back
 * then, and when it moves to more. A nested builder keeps its bytes, since
 * it can still be written or nested again, so the buffer is replaced when
 * it's full while builders hold bytes of it, and the old one lives as long
 * as they do.
 */
export class ArenaMemory {
    bytes: Uint8Array<ArrayBuffer> = new Uint8Array(0);
    view = new DataView(this.bytes.buffer);
    /** Where the buffer's bytes that no builder holds start */
    top = 0;
    /** The builders that hold bytes of the buffer, and aren't written */
    live = 0;
    /** The first buffer's length, a multiple of 8 */
    declare readonly first: number;
    /** The longest buffer that's kept, a multiple of 8 */
    declare readonly max: number;

    constructor(first: number, max: number) {
        this.first = first;
        this.max = max;
    }

    /** Replaces the buffer with one of at least `length` bytes, a multiple of 8 */
    replace(length: number): void {
        const doubled = Math.min(
            2 * this.bytes.length,
            DOUBLED_BYTES,
            this.max
        );

        this.bytes = new Uint8Array(Math.max(length, doubled, this.first));
        this.view = new DataView(this.bytes.buffer);
        this.top = 0;
        this.live = 0;
    }

    /** Drops the buffer. The builders that hold bytes of it keep it alive. */
    release(): void {
        this.bytes = new Uint8Array(0);
        this.view = new DataView(this.bytes.buffer);
        this.top = 0;
        this.live = 0;
    }
}

/** Reads an arena's memory, which only its class can: it sets this */
let access: (arena: TableArena) => ArenaMemory;

/**
 * The buffer that builders write their tables in. Every `TableBuilder` is
 * given an arena, and takes bytes of its buffer, which it gives back when
 * its table is written, so that building tables allocates little more than
 * their bytes. The arena keeps the buffer between builders.
 *
 * `TableArena.DEFAULT` suits tables of any length. An arena of one's own
 * sets how much memory is taken at first and how much is kept, for a
 * program that knows its tables, and keeps its builders apart from those of
 * other code that uses the default.
 *
 * ```ts
 * const arena = new TableArena({ initialBytes: 64 * 1024 });
 * const builder = new TableBuilder(arena, 3);
 * ```
 *
 * An arena isn't shared between threads: each worker has the arenas that it
 * creates, and a `DEFAULT` of its own.
 */
export class TableArena {
    static {
        // oxlint-disable-next-line eslint/no-underscore-dangle -- an arena's own field, which the rule allows only after `this`
        access = arena => arena._memory;
    }

    /** The arena of builders that aren't given one of their caller's own */
    static readonly DEFAULT: TableArena = new TableArena();

    declare private readonly _memory: ArenaMemory;

    /**
     * @throws {RangeError} If `initialBytes` or `maxRetainedBytes` isn't a
     * non-negative integer that a typed array can be as long as, or
     * `initialBytes` is more than a `maxRetainedBytes` given with it.
     */
    constructor(options: TableArenaOptions = {}) {
        const first = assertBytes(
            'initialBytes',
            options.initialBytes ?? INITIAL_BYTES
        );
        const given = options.maxRetainedBytes;
        // An arena that's only given a first buffer keeps one that long
        const max = assertBytes(
            'maxRetainedBytes',
            given ?? Math.max(MAX_RETAINED_BYTES, first)
        );
        if (options.initialBytes != null && first > max) {
            throw new RangeError(
                `initialBytes ${first} is more than maxRetainedBytes ${max}`
            );
        }

        // Multiples of 8, so that every builder's bytes start at one, and
        // no more than the caller's limit
        const retained = max - (max % 8);
        this._memory = new ArenaMemory(
            Math.min(first + (-first & 7), retained),
            retained
        );
    }

    /**
     * The length of the buffer that the arena holds now: 0 until a builder
     * takes bytes of it, and after `release()`.
     */
    retainedBytes(): number {
        return this._memory.bytes.length;
    }

    /**
     * Drops the arena's buffer, which the next builder allocates again, at
     * `initialBytes`. Builders that hold bytes of it and aren't written yet
     * keep it alive until they are, and write their tables as they would
     * have.
     */
    release(): void {
        this._memory.release();
    }
}

/** The memory of `arena`, for `TableBuilder` */
export const memoryOf: (arena: TableArena) => ArenaMemory = access;

function assertBytes(name: string, value: number): number {
    // 2^32 - 1 bytes, which every supported runtime's typed arrays reach
    if (!Number.isInteger(value) || value < 0 || value > 0xffff_ffff) {
        throw new RangeError(
            `${name} must be a non-negative integer, got ${value}`
        );
    }

    return value;
}
