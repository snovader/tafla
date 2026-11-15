import { ByteBuffer } from '../source/byteBuffer.ts';
import {
    SIZEOF_8,
    SIZEOF_16,
    SIZEOF_32,
    SIZEOF_64
} from '../source/constants.ts';
import {
    setFloat32Elements,
    setFloat64Elements,
    setInt8Elements,
    setInt16Elements,
    setInt32Elements,
    setInt64Elements,
    setUint8Elements,
    setUint16Elements,
    setUint32Elements,
    setUint64Elements
} from '../source/elements.ts';
import { encodeInto, utf8Length } from '../source/encoding.ts';
import {
    assertAlignment,
    assertCount,
    headerLength,
    maxLength,
    nestedHeaderLength,
    padding,
    TableLayout
} from '../source/layout.ts';
import { structCount } from '../source/tableWriter.ts';
import type { ElementSize, OffsetSize, OffsetType } from '../source/types.ts';

// A prototype, not part of the package: a builder with `TableBuilder`'s API,
// bytes and errors that writes each field into a growing buffer when it's
// set, rather than recording it and writing every field in `bytes()`.
// README.md beside it describes the design and how it's measured.

/** How the buffer grows, which the benchmark compares */
export type Growth =
    /** Into a new `Uint8Array` of at least twice the length, with `set()` */
    | 'copy'
    /** With `ArrayBuffer.prototype.transfer(newLength)` */
    | 'transfer'
    /**
     * With `resize()` of a resizable `ArrayBuffer`, under length-tracking
     * `Uint8Array` and `DataView` views
     */
    | 'resizable';

/** How `bytes()` makes bytes of the table's exact length from the buffer */
export type Result =
    /** `slice()`, a copy */
    | 'slice'
    /**
     * `transfer(length)` of the buffer, or `transferToFixedLength(length)`
     * of a resizable one, which may keep its memory
     */
    | 'transfer';

/** How a string is given room in the buffer before it's encoded */
export type StringRoom =
    /** 3 bytes for each UTF-16 code unit, its longest UTF-8 */
    | 'worst'
    /** Its UTF-8 length, counted first */
    | 'count'
    /**
     * The longest UTF-8 when the buffer has room for it, and otherwise the
     * length counted first, so that the buffer grows only when the string
     * needs it to
     */
    | 'room'
    /**
     * The longest UTF-8 of a string of up to {@link SHORT_STRING_UNITS}
     * code units, growing the buffer for it, and otherwise as `'room'`
     */
    | 'short';

/** How scalars, offsets and lengths are written */
export type Scalars =
    /** Through a scratch `DataView` and byte copies, as `TableWriter` does */
    | 'bytes'
    /**
     * Through a `DataView` of the buffer once it's outside V8's heap: once
     * it grew, or reused with its view. A new buffer of 64 bytes is written
     * as `'bytes'` writes it, since a view would make V8 give it an
     * `ArrayBuffer`.
     */
    | 'view';

export interface Options {
    growth: Growth;
    result: Result;
    strings: StringRoom;
    scalars: Scalars;
    /**
     * The shortest run of bytes, of an array, an array of structs, a struct
     * or a table nested as bytes, that's kept as a segment rather than
     * copied into the buffer: `Infinity` copies every run
     */
    segmentBytes: number;
    /**
     * Whether a builder starts from the buffer that an earlier one left in
     * `bytes()`, up to {@link SPARE_BYTES}, rather than a new one
     */
    reuse: boolean;
    /**
     * The length of the chunks of an arena that builders take their bytes
     * from, one after another, or 0 for a buffer of each builder's own. The
     * last builder to take bytes holds the rest of the chunk, and keeps
     * those it has written or made room for when the next takes bytes. A
     * builder that outgrows its bytes moves to the rest of the chunk, or of
     * a new one, and the last gives its bytes back in `bytes()`. With an
     * arena, `reuse` doesn't apply, the buffer grows by copying and `bytes()`
     * slices it.
     */
    arenaBytes: number;
}

// The variant of the design. The tests set them with configure(), and the
// benchmark copies this module with other initial values, one copy per
// variant, so that V8 doesn't share type feedback between them.
let growth: Growth = 'copy';
let result: Result = 'slice';
let stringRoom: StringRoom = 'short';
let scalars: Scalars = 'view';
let segmentBytes = 1024;
let reuse = true;
let arenaBytes = 64 * 1024;

/** Sets the variant of the design, for the tests */
export function configure(options: Partial<Options>): void {
    growth = options.growth ?? growth;
    result = options.result ?? result;
    stringRoom = options.strings ?? stringRoom;
    scalars = options.scalars ?? scalars;
    segmentBytes = options.segmentBytes ?? segmentBytes;
    reuse = options.reuse ?? reuse;
    arenaBytes = options.arenaBytes ?? arenaBytes;
    spare = undefined;
    spareView = undefined;
    chunk = undefined;
    chunkView = undefined;
    top = 0;
    last = undefined;
}

/** The variant of the design in use */
export function options(): Options {
    return {
        growth,
        result,
        strings: stringRoom,
        scalars,
        segmentBytes,
        reuse,
        arenaBytes
    };
}

/**
 * The buffer's length when a builder is made: V8 keeps a `Uint8Array` of up
 * to 64 bytes on its heap, which is several times as fast to allocate
 */
const INITIAL_BYTES = 64;
/** The buffer's least length once it grows, as C++'s prototype has it */
const FIRST_GROWTH = 512;
/**
 * The longest string, in UTF-16 code units, that the buffer grows to give
 * room for its longest UTF-8 rather than counting it first
 */
const SHORT_STRING_UNITS = 1024;
/** The memory that a resizable buffer reserves when a builder is made */
const RESIZABLE_RESERVE = 64 * 1024;
/** The longest buffer that's kept for the next builder */
const SPARE_BYTES = 64 * 1024;

/**
 * The buffer that a builder left when it wrote its table, which the next
 * builder starts from. `bytes()` copies the table out of it, so no table
 * holds it, and builders write every byte of their tables, so what it held
 * before isn't read.
 */
let spare: Uint8Array | undefined;
/** The view of the spare buffer, for the variants that write through one */
let spareView: DataView | undefined;

/**
 * The arena's chunk that builders take their bytes from, its view for the
 * variants that write through one, and where its bytes that no builder holds
 * start. Earlier chunks stay alive while builders hold bytes of them.
 */
let chunk: Uint8Array | undefined;
let chunkView: DataView | undefined;
let top = 0;
/**
 * The builder that holds the rest of the chunk, as the last to take bytes
 * from it, until another builder takes bytes or it writes its table
 */
let last: DirectTableBuilder | undefined;

const EMPTY = new Uint8Array(0);

/** The bytes that a builder takes from the arena, besides its header */
const ARENA_ROOM = 16;

// The kinds of segment, which bytes() switches on to write each
const INT8_ARRAY = 0;
const UINT8_ARRAY = 1;
const INT16_ARRAY = 2;
const UINT16_ARRAY = 3;
const INT32_ARRAY = 4;
const UINT32_ARRAY = 5;
const INT64_ARRAY = 6;
const UINT64_ARRAY = 7;
const FLOAT32_ARRAY = 8;
const FLOAT64_ARRAY = 9;
/** Bytes copied as they are: a struct, an array of structs or a table */
const BYTES = 10;

/**
 * The slots of a segment in `#segments`: the position in the buffer that
 * its bytes go before, their length, its kind, its values and their count
 */
const SEGMENT_SLOTS = 5;

/**
 * The fields of a table that a {@link DirectTableBuilder} writes in place,
 * which its `setTableInPlace()` and `setVectorInPlace()` give to `fill`
 */
export type DirectTableItem = Omit<DirectTableBuilder, 'bytes' | 'finalize'>;

/**
 * `TableBuilder`, writing each field when it's set.
 *
 * The builder writes forward into a buffer that grows. Each field's offset
 * is written when the field is set, from the position that it starts at.
 * Runs of `segmentBytes` or more, of arrays, arrays of structs, structs and
 * tables given as bytes, are kept as segments that `bytes()` writes between
 * the buffer's bytes, so that a large array is written once, into the
 * result. A table nested in place is written at its parent's position, and
 * moved forward by its padding once its fields are set and its alignment is
 * known; its offsets count from its own start, so none changes. A nested
 * builder is copied in as its bytes and segments.
 *
 * Every setter checks its field before it writes, so that one that throws
 * leaves the builder as it was. A `fill` that throws is undone by
 * truncating the buffer and the segments to where they were.
 */
export class DirectTableBuilder {
    /**
     * The table once written: its bytes, and then the buffer of them that
     * `finalize()` returns
     */
    #written: Uint8Array | ByteBuffer | undefined;

    readonly #layout: TableLayout;
    /** The offset size, the width of offsets, lengths and header slots */
    readonly #size: OffsetSize;
    /** The longest table that the layout's offsets and lengths address */
    readonly #maxLength: number;

    /** The buffer, or the arena's chunk that the builder's bytes are in */
    #bytes: Uint8Array = EMPTY;
    /**
     * Where the builder's bytes start in the buffer, which every position
     * below counts from, so that they stay valid when the bytes move
     */
    #base = 0;
    /** The builder's bytes in the buffer, written or not */
    #capacity = 0;
    /**
     * The end of the bytes that the builder last made room for, which it
     * keeps, with those written, when another builder takes the rest of the
     * arena's chunk: a setter may call code that makes a builder, such as a
     * value's `valueOf()`, before it writes them
     */
    #claim = 0;
    /** A view of the buffer, for the variants that write through one */
    #view: DataView | undefined;
    /** The bytes of the buffer written so far */
    #end: number;
    /**
     * The bytes of the segments so far, by which the table's positions after
     * them are ahead of the buffer's
     */
    #shift = 0;
    /** The segments, {@link SEGMENT_SLOTS} slots each, once there are any */
    #segments: unknown[] | undefined;

    // What the builder counts of the table whose fields it's writing: its
    // own, or, while a `fill` runs, those of the table that it writes in
    // place, until it restores its own
    #fields: number;
    /** The number of fields set so far */
    #count = 0;
    /** The table's start, in the table, which its offsets count from */
    #table = 0;
    /** The position of the table's first offset in the buffer */
    #slots: number;
    /**
     * The largest alignment that a field needs from the table's start, which
     * a parent pads this table to when it nests it
     */
    #alignment: ElementSize = SIZEOF_8;
    /** The number of tables being written in place, one in another */
    #nesting = 0;

    /**
     * @param fields - The number of fields that the table will hold.
     * @param layout - The table's header and offset type, which its readers
     * must use too.
     * @param version - The version to write to the header, required when the
     * layout is versioned.
     * @throws {TypeError} If `version` is given for a layout without a version
     * slot, or missing for one with it.
     * @throws {RangeError} If `fields` isn't a non-negative integer, `version`
     * isn't an integer that fits in the layout's offset type, or the header
     * alone is longer than the layout addresses.
     */
    constructor(fields: number, layout?: TableLayout<false>);
    constructor(fields: number, layout: TableLayout<true>, version: number);
    constructor(
        fields: number,
        layout: TableLayout = TableLayout.DEFAULT,
        version?: number
    ) {
        const size = layout.offsetSize();
        const header = headerLength(fields, layout, version);

        this.#layout = layout;
        this.#size = size;
        this.#maxLength = maxLength(size);
        this.#fields = fields;

        const length = Math.max(INITIAL_BYTES, header);
        if (arenaBytes !== 0) {
            this.#take(header + ARENA_ROOM);
        } else if (growth === 'resizable') {
            const buffer = new ArrayBuffer(length, {
                maxByteLength: Math.max(RESIZABLE_RESERVE, length)
            });
            this.#bytes = new Uint8Array(buffer);
            this.#view = new DataView(buffer);
            this.#capacity = length;
        } else {
            const buffer = spare;
            if (reuse && buffer !== undefined && buffer.length >= length) {
                this.#bytes = buffer;
                this.#view = spareView;
                this.#capacity = buffer.length;
                spare = undefined;
                spareView = undefined;
            } else {
                this.#bytes = new Uint8Array(length);
                this.#view =
                    scalars === 'view' && length > INITIAL_BYTES
                        ? new DataView(this.#bytes.buffer)
                        : undefined;
                this.#capacity = length;
            }
        }

        // The length, when the layout is sized, is written by bytes(), and
        // the offsets as the fields are set
        const slots = layout.headerSize();
        if (version !== undefined) {
            this.#setUnsigned(slots - size, version);
        }

        this.#slots = slots;
        this.#end = header;
    }

    /**
     * Writes the table, once, and returns a `ByteBuffer` that reads it; later
     * calls return the same buffer.
     *
     * @throws {RangeError} If fewer fields were set than the table holds, or
     * an array kept until now has changed its length.
     */
    finalize(): ByteBuffer {
        const written = this.#written ?? this.#write();
        if (written instanceof ByteBuffer) {
            return written;
        }

        const buffer = new ByteBuffer(written, this.#layout);
        this.#written = buffer;

        return buffer;
    }

    /**
     * Writes the table, once, as `finalize()` does, and returns its bytes,
     * which `finalize()` then returns a buffer of
     *
     * @throws {RangeError} If fewer fields were set than the table holds, or
     * an array kept until now has changed its length.
     */
    bytes(): Uint8Array {
        const written = this.#written ?? this.#write();
        return written instanceof ByteBuffer ? written.bytes() : written;
    }

    setInt8(value: number): void {
        const index = this.#check(0, SIZEOF_8);
        const at = this.#reserve(SIZEOF_8);

        if (this.#view !== undefined) {
            this.#view.setInt8(this.#base + at, value);
        } else {
            SCRATCH.setInt8(0, value);
            this.#bytes[this.#base + at] = SCRATCH_BYTES[0];
        }

        this.#endScalar(index, at, SIZEOF_8);
    }

    setUint8(value: number): void {
        const index = this.#check(0, SIZEOF_8);
        const at = this.#reserve(SIZEOF_8);

        if (this.#view !== undefined) {
            this.#view.setUint8(this.#base + at, value);
        } else {
            SCRATCH.setUint8(0, value);
            this.#bytes[this.#base + at] = SCRATCH_BYTES[0];
        }

        this.#endScalar(index, at, SIZEOF_8);
    }

    setInt16(value: number): void {
        const index = this.#check(0, SIZEOF_16);
        const at = this.#reserve(SIZEOF_16);

        if (this.#view !== undefined) {
            this.#view.setInt16(this.#base + at, value, true);
        } else {
            SCRATCH.setInt16(0, value, true);
            copy2(this.#bytes, this.#base + at);
        }

        this.#endScalar(index, at, SIZEOF_16);
    }

    setUint16(value: number): void {
        const index = this.#check(0, SIZEOF_16);
        const at = this.#reserve(SIZEOF_16);

        if (this.#view !== undefined) {
            this.#view.setUint16(this.#base + at, value, true);
        } else {
            SCRATCH.setUint16(0, value, true);
            copy2(this.#bytes, this.#base + at);
        }

        this.#endScalar(index, at, SIZEOF_16);
    }

    setInt32(value: number): void {
        const index = this.#check(0, SIZEOF_32);
        const at = this.#reserve(SIZEOF_32);

        if (this.#view !== undefined) {
            this.#view.setInt32(this.#base + at, value, true);
        } else {
            SCRATCH.setInt32(0, value, true);
            copy4(this.#bytes, this.#base + at);
        }

        this.#endScalar(index, at, SIZEOF_32);
    }

    setUint32(value: number): void {
        const index = this.#check(0, SIZEOF_32);
        const at = this.#reserve(SIZEOF_32);

        if (this.#view !== undefined) {
            this.#view.setUint32(this.#base + at, value, true);
        } else {
            SCRATCH.setUint32(0, value, true);
            copy4(this.#bytes, this.#base + at);
        }

        this.#endScalar(index, at, SIZEOF_32);
    }

    setInt64(value: bigint): void {
        const index = this.#check(0, SIZEOF_64);
        const at = this.#reserve(SIZEOF_64);

        if (this.#view !== undefined) {
            this.#view.setBigInt64(this.#base + at, value, true);
        } else {
            SCRATCH.setBigInt64(0, value, true);
            copy8(this.#bytes, this.#base + at);
        }

        this.#endScalar(index, at, SIZEOF_64);
    }

    setUint64(value: bigint): void {
        const index = this.#check(0, SIZEOF_64);
        const at = this.#reserve(SIZEOF_64);

        if (this.#view !== undefined) {
            this.#view.setBigUint64(this.#base + at, value, true);
        } else {
            SCRATCH.setBigUint64(0, value, true);
            copy8(this.#bytes, this.#base + at);
        }

        this.#endScalar(index, at, SIZEOF_64);
    }

    setFloat32(value: number): void {
        const index = this.#check(0, SIZEOF_32);
        const at = this.#reserve(SIZEOF_32);

        if (this.#view !== undefined) {
            this.#view.setFloat32(this.#base + at, value, true);
        } else {
            SCRATCH.setFloat32(0, value, true);
            copy4(this.#bytes, this.#base + at);
        }

        this.#endScalar(index, at, SIZEOF_32);
    }

    setFloat64(value: number): void {
        const index = this.#check(0, SIZEOF_64);
        const at = this.#reserve(SIZEOF_64);

        if (this.#view !== undefined) {
            this.#view.setFloat64(this.#base + at, value, true);
        } else {
            SCRATCH.setFloat64(0, value, true);
            copy8(this.#bytes, this.#base + at);
        }

        this.#endScalar(index, at, SIZEOF_64);
    }

    // Each array setter writes its elements with its own type's function,
    // so that each conversion sees one type, as TableWriter's do

    setInt8Array(values: ArrayLike<number> | Int8Array): void {
        const count = values.length;
        if (count * SIZEOF_8 >= segmentBytes) {
            this.#addSegment(
                INT8_ARRAY,
                values,
                count,
                count * SIZEOF_8,
                SIZEOF_8
            );
            return;
        }

        const elements = this.#startArray(count * SIZEOF_8, SIZEOF_8);
        setInt8Elements(this.#bytes, this.#base + elements, values, count);
        this.#endArray(count, elements, count * SIZEOF_8, SIZEOF_8);
    }

    setUint8Array(values: ArrayLike<number> | Uint8Array): void {
        const count = values.length;
        if (count * SIZEOF_8 >= segmentBytes) {
            this.#addSegment(
                UINT8_ARRAY,
                values,
                count,
                count * SIZEOF_8,
                SIZEOF_8
            );
            return;
        }

        const elements = this.#startArray(count * SIZEOF_8, SIZEOF_8);
        setUint8Elements(this.#bytes, this.#base + elements, values, count);
        this.#endArray(count, elements, count * SIZEOF_8, SIZEOF_8);
    }

    setInt16Array(values: ArrayLike<number> | Int16Array): void {
        const count = values.length;
        if (count * SIZEOF_16 >= segmentBytes) {
            this.#addSegment(
                INT16_ARRAY,
                values,
                count,
                count * SIZEOF_16,
                SIZEOF_16
            );
            return;
        }

        const elements = this.#startArray(count * SIZEOF_16, SIZEOF_16);
        setInt16Elements(this.#bytes, this.#base + elements, values, count);
        this.#endArray(count, elements, count * SIZEOF_16, SIZEOF_16);
    }

    setUint16Array(values: ArrayLike<number> | Uint16Array): void {
        const count = values.length;
        if (count * SIZEOF_16 >= segmentBytes) {
            this.#addSegment(
                UINT16_ARRAY,
                values,
                count,
                count * SIZEOF_16,
                SIZEOF_16
            );
            return;
        }

        const elements = this.#startArray(count * SIZEOF_16, SIZEOF_16);
        setUint16Elements(this.#bytes, this.#base + elements, values, count);
        this.#endArray(count, elements, count * SIZEOF_16, SIZEOF_16);
    }

    setInt32Array(values: ArrayLike<number> | Int32Array): void {
        const count = values.length;
        if (count * SIZEOF_32 >= segmentBytes) {
            this.#addSegment(
                INT32_ARRAY,
                values,
                count,
                count * SIZEOF_32,
                SIZEOF_32
            );
            return;
        }

        const elements = this.#startArray(count * SIZEOF_32, SIZEOF_32);
        setInt32Elements(this.#bytes, this.#base + elements, values, count);
        this.#endArray(count, elements, count * SIZEOF_32, SIZEOF_32);
    }

    setUint32Array(values: ArrayLike<number> | Uint32Array): void {
        const count = values.length;
        if (count * SIZEOF_32 >= segmentBytes) {
            this.#addSegment(
                UINT32_ARRAY,
                values,
                count,
                count * SIZEOF_32,
                SIZEOF_32
            );
            return;
        }

        const elements = this.#startArray(count * SIZEOF_32, SIZEOF_32);
        setUint32Elements(this.#bytes, this.#base + elements, values, count);
        this.#endArray(count, elements, count * SIZEOF_32, SIZEOF_32);
    }

    setInt64Array(values: ArrayLike<bigint> | BigInt64Array): void {
        const count = values.length;
        if (count * SIZEOF_64 >= segmentBytes) {
            this.#addSegment(
                INT64_ARRAY,
                values,
                count,
                count * SIZEOF_64,
                SIZEOF_64
            );
            return;
        }

        const elements = this.#startArray(count * SIZEOF_64, SIZEOF_64);
        setInt64Elements(this.#bytes, this.#base + elements, values, count);
        this.#endArray(count, elements, count * SIZEOF_64, SIZEOF_64);
    }

    setUint64Array(values: ArrayLike<bigint> | BigUint64Array): void {
        const count = values.length;
        if (count * SIZEOF_64 >= segmentBytes) {
            this.#addSegment(
                UINT64_ARRAY,
                values,
                count,
                count * SIZEOF_64,
                SIZEOF_64
            );
            return;
        }

        const elements = this.#startArray(count * SIZEOF_64, SIZEOF_64);
        setUint64Elements(this.#bytes, this.#base + elements, values, count);
        this.#endArray(count, elements, count * SIZEOF_64, SIZEOF_64);
    }

    setFloat32Array(values: ArrayLike<number> | Float32Array): void {
        const count = values.length;
        if (count * SIZEOF_32 >= segmentBytes) {
            this.#addSegment(
                FLOAT32_ARRAY,
                values,
                count,
                count * SIZEOF_32,
                SIZEOF_32
            );
            return;
        }

        const elements = this.#startArray(count * SIZEOF_32, SIZEOF_32);
        setFloat32Elements(this.#bytes, this.#base + elements, values, count);
        this.#endArray(count, elements, count * SIZEOF_32, SIZEOF_32);
    }

    setFloat64Array(values: ArrayLike<number> | Float64Array): void {
        const count = values.length;
        if (count * SIZEOF_64 >= segmentBytes) {
            this.#addSegment(
                FLOAT64_ARRAY,
                values,
                count,
                count * SIZEOF_64,
                SIZEOF_64
            );
            return;
        }

        const elements = this.#startArray(count * SIZEOF_64, SIZEOF_64);
        setFloat64Elements(this.#bytes, this.#base + elements, values, count);
        this.#endArray(count, elements, count * SIZEOF_64, SIZEOF_64);
    }

    /** Writes `string` as UTF-8, with lone surrogates replaced by U+FFFD */
    setString(string: string): void {
        const index = this.#checkRoom();
        const units = string.length;
        const at = this.#end;
        const start = at + this.#size;
        const before = at + this.#shift - this.#table + this.#size;

        // With room for its longest UTF-8, a string that fits the table needs
        // no count, and only one too long for it is counted, for its error
        let room = 3 * units;
        if (
            stringRoom === 'count' ||
            !(before + room <= this.#maxLength) ||
            (start + room > this.#capacity &&
                (stringRoom === 'room' ||
                    (stringRoom === 'short' && units > SHORT_STRING_UNITS)))
        ) {
            room = utf8Length(string);
            this.#checkLength(index, before + room);
        }

        if (start + room > this.#capacity) {
            this.#grow(start + room);
        }
        this.#claim = start + room;

        const length = encodeInto(string, this.#bytes, this.#base + start);
        this.#setOffset(index, at);
        this.#setUnsigned(at, length);
        this.#end = start + length;
        this.#count = index + 1;
    }

    /**
     * Sets a struct field from its bytes: its scalars, little-endian, one
     * after another, which are written as they are, without padding.
     */
    setStruct(bytes: Uint8Array): void {
        const byteLength = bytes.length;
        const index = this.#check(0, byteLength);
        const at = this.#end;

        this.#setOffset(index, at);
        this.#appendBytes(bytes, byteLength);
        this.#count = index + 1;
    }

    /**
     * Sets an array of structs of `size` bytes each, from their bytes one
     * after another, padded so that the structs start at a multiple of
     * `alignment` from the start of the table.
     *
     * @throws {RangeError} If `size` isn't a positive integer, `bytes` aren't
     * a whole number of structs, or `alignment` isn't 1, 2, 4 or 8.
     */
    setStructArray(
        bytes: Uint8Array,
        size: number,
        alignment: ElementSize
    ): void {
        const count = structCount(bytes.length, size);
        assertAlignment(alignment);

        const byteLength = bytes.length;
        if (byteLength >= segmentBytes) {
            this.#addSegment(BYTES, bytes, count, byteLength, alignment);
            return;
        }

        const elements = this.#startArray(byteLength, alignment);
        copyBytes(bytes, 0, byteLength, this.#bytes, this.#base + elements);
        this.#endArray(count, elements, byteLength, alignment);
    }

    /**
     * Embeds a table, from its builder or its bytes, without a length
     * prefix, as `TableBuilder.setTable()` does. A builder is copied as its
     * bytes and segments, whatever its layout, so it must have all its fields
     * set, and its values must not change until this table is written.
     *
     * @throws {RangeError} If the builder has fields left to set, or
     * `alignment` isn't 1, 2, 4 or 8.
     */
    setTable(
        table: DirectTableBuilder | Uint8Array,
        alignment: ElementSize = table instanceof DirectTableBuilder
            ? table.#alignment
            : SIZEOF_32
    ): void {
        assertAlignment(alignment);

        const byteLength = DirectTableBuilder.#lengthOf(table);
        const end = this.#end;
        const pad = padding(end + this.#shift - this.#table, alignment);
        const index = this.#check(pad, byteLength);

        const at = end + pad;
        this.#pad(at);
        this.#setOffset(index, at);
        this.#append(table);
        this.#count = index + 1;
        this.#alignTo(alignment);
    }

    /**
     * Writes a vector of tables, from their builders or their bytes, after
     * the end of each, as `TableBuilder.setVector()` does
     *
     * @throws {RangeError} If a builder has fields left to set.
     */
    setVector(vector: ReadonlyArray<DirectTableBuilder | Uint8Array>): void {
        const size = this.#size;
        const count = vector.length;

        let byteLength = size + count * size;
        for (let i = 0; i < count; i++) {
            byteLength += DirectTableBuilder.#lengthOf(vector[i]);
        }

        const index = this.#check(0, byteLength);
        const at = this.#reserve(size + count * size);
        const position = at + this.#shift;

        this.#setOffset(index, at);
        this.#setUnsigned(at, count);
        this.#end = at + size + count * size;

        for (let i = 0; i < count; i++) {
            this.#append(vector[i]);
            this.#setUnsigned(
                at + (i + 1) * size,
                this.#end + this.#shift - position
            );
        }

        this.#count = index + 1;
    }

    /**
     * Writes a table of `fields` fields nested in this one, in place,
     * without a builder of its own, as `TableBuilder.setTableInPlace()`
     * records one: `fill` sets its fields, in order, on the
     * {@link DirectTableItem} that it's given.
     *
     * @throws {RangeError} If `fields` isn't a non-negative integer, `fill`
     * leaves fields of the table unset, or as the other setters throw.
     * @throws What `fill` throws. Either way, the builder is left as it was.
     */
    setTableInPlace(
        fields: number,
        fill: (table: DirectTableItem) => void
    ): void {
        this.#checkRoom();
        const header = nestedHeaderLength(fields, this.#layout);

        // Where the table starts, unpadded, and what to restore after it
        const parentFields = this.#fields;
        const index = this.#count;
        const parentTable = this.#table;
        const slots = this.#slots;
        const alignment = this.#alignment;
        const nesting = this.#nesting;
        const at = this.#end;
        const shift = this.#shift;
        const segments = this.#segments?.length ?? 0;
        const start = at + shift;

        this.#startTable(fields, header);
        try {
            fill(this);
            this.#assertSet();
        } catch (error) {
            this.#restore(parentFields, index, parentTable, slots, alignment);
            this.#nesting = nesting;
            this.#truncate(at, shift, segments);
            throw error;
        }

        const length = this.#end + this.#shift - start;
        const tableAlignment = this.#alignment;
        this.#restore(parentFields, index, parentTable, slots, alignment);
        this.#nesting = nesting;

        // Padded from the parent's start, as a nested builder is
        const pad = padding(start - parentTable, tableAlignment);
        const total = start - parentTable + pad + length;
        if (!(total <= this.#maxLength)) {
            this.#truncate(at, shift, segments);
            throw tooLong(index, total, this.#layout.offsetType());
        }

        if (pad !== 0) {
            this.#moveTable(at, segments, pad);
        }

        this.#setUnsigned(
            slots + index * this.#size,
            start + pad - parentTable
        );
        this.#count = index + 1;
        this.#alignTo(tableAlignment);
    }

    /**
     * Writes a vector of `count` tables of `fields` fields each, in place,
     * as `setTableInPlace()` writes one: `fill(item, index)` sets the fields
     * of item `index`, for each in order. Items aren't padded.
     *
     * @throws {RangeError} If `count` or `fields` isn't a non-negative
     * integer, the vector's count and item ends and its items' offsets make
     * the table too long, before any item is written, `fill` leaves fields
     * of an item unset, or as the other setters throw.
     * @throws What `fill` throws. Either way, the builder is left as it was.
     */
    setVectorInPlace(
        count: number,
        fields: number,
        fill: (item: DirectTableItem, index: number) => void
    ): void {
        assertCount(count);
        this.#checkRoom();

        // The count, the items' ends and their offsets, which fail before
        // any item is written, rather than after a `fill` for each
        const size = this.#size;
        const header = nestedHeaderLength(fields, this.#layout);
        const index = this.#count;
        const at = this.#end;
        const shift = this.#shift;
        const parentTable = this.#table;
        const position = at + shift;
        const headers =
            position - parentTable + size + count * size + count * header;
        if (!(headers <= this.#maxLength)) {
            throw tooLong(index, headers, this.#layout.offsetType());
        }

        const parentFields = this.#fields;
        const slots = this.#slots;
        const alignment = this.#alignment;
        const nesting = this.#nesting;
        const segments = this.#segments?.length ?? 0;

        const ends = at + size;
        this.#reserve(size + count * size);
        this.#setUnsigned(at, count);
        this.#end = ends + count * size;

        try {
            for (let item = 0; item < count; item++) {
                const itemStart = this.#end;
                this.#startTable(fields, header);
                fill(this, item);
                this.#assertSet();
                this.#nesting = nesting;

                const end = this.#end;
                this.#setUnsigned(
                    ends + item * size,
                    end + this.#shift - position
                );

                // Room for the other items, as long as the first in the
                // buffer, up to what the offset type addresses
                if (item === 0 && count > 1) {
                    const room = Math.min(
                        (count - 1) * (end - itemStart),
                        this.#maxLength
                    );
                    this.#reserve(room);
                }
            }
        } catch (error) {
            this.#restore(parentFields, index, parentTable, slots, alignment);
            this.#nesting = nesting;
            this.#truncate(at, shift, segments);
            throw error;
        }

        this.#restore(parentFields, index, parentTable, slots, alignment);

        const length = this.#end + this.#shift - parentTable;
        if (!(length <= this.#maxLength)) {
            this.#truncate(at, shift, segments);
            throw tooLong(index, length, this.#layout.offsetType());
        }

        this.#setUnsigned(slots + index * size, position - parentTable);
        this.#count = index + 1;
    }

    /**
     * Starts writing a table of `fields` fields in place at the end of the
     * buffer, whose header of `header` bytes holds only their offsets, and
     * counts its fields rather than its parent's
     */
    #startTable(fields: number, header: number): void {
        const at = this.#reserve(header);

        this.#fields = fields;
        this.#count = 0;
        this.#table = at + this.#shift;
        this.#slots = at;
        this.#alignment = SIZEOF_8;
        this.#end = at + header;
        this.#nesting++;
    }

    /**
     * Counts the parent's fields again after a table written in place: those
     * set before it, its start, its offsets' position and its alignment. The
     * caller restores the nesting.
     */
    #restore(
        fields: number,
        count: number,
        table: number,
        slots: number,
        alignment: ElementSize
    ): void {
        this.#fields = fields;
        this.#count = count;
        this.#table = table;
        this.#slots = slots;
        this.#alignment = alignment;
    }

    /**
     * Removes what was written from `end` in the buffer, with `shift` bytes
     * of segments before it, and the segments from slot `segments` on,
     * which releases their values
     */
    #truncate(end: number, shift: number, segments: number): void {
        this.#end = end;
        this.#shift = shift;

        if (this.#segments !== undefined) {
            this.#segments.length = segments;
        }
    }

    /**
     * Moves the table written in place from `at` forward by `pad` bytes of
     * padding, with its segments from slot `segments` on
     */
    #moveTable(at: number, segments: number, pad: number): void {
        const end = this.#reserve(pad);
        const bytes = this.#bytes;
        const base = this.#base;

        bytes.copyWithin(base + at + pad, base + at, base + end);
        bytes.fill(0, base + at, base + at + pad);
        this.#end = end + pad;

        const list = this.#segments;
        if (list !== undefined) {
            for (
                let slot = segments;
                slot < list.length;
                slot += SEGMENT_SLOTS
            ) {
                (list[slot] as number) += pad;
            }
        }
    }

    /**
     * Where an array's elements go in the buffer, after the padding that
     * aligns them and its count, once the array fits the table and the
     * buffer has room for it. It writes nothing.
     */
    #startArray(byteLength: number, alignment: ElementSize): number {
        const start = this.#end + this.#size;
        const pad = padding(start + this.#shift - this.#table, alignment);
        this.#check(pad, this.#size + byteLength);

        const elements = start + pad;
        const claim = elements + byteLength;
        if (claim > this.#capacity) {
            this.#grow(claim);
        }

        this.#claim = claim;
        return elements;
    }

    /**
     * Writes the padding, offset and count of an array whose elements were
     * written at `elements`, aligned to `alignment`, and moves past them
     */
    #endArray(
        count: number,
        elements: number,
        byteLength: number,
        alignment: ElementSize
    ): void {
        const at = elements - this.#size;

        this.#pad(at);
        this.#setOffset(this.#count, at);
        this.#setUnsigned(at, count);
        this.#end = elements + byteLength;
        this.#count++;
        this.#alignTo(alignment);
    }

    /**
     * Writes an array's padding, offset and count, and keeps its elements as
     * a segment that `bytes()` writes after the count
     */
    #addSegment(
        kind: number,
        values: unknown,
        count: number,
        byteLength: number,
        alignment: ElementSize
    ): void {
        const end = this.#end;
        const size = this.#size;
        const pad = padding(end + size + this.#shift - this.#table, alignment);
        const index = this.#check(pad, size + byteLength);

        const at = end + pad;
        this.#reserve(pad + size);
        this.#pad(at);
        this.#setOffset(index, at);
        this.#setUnsigned(at, count);
        this.#end = at + size;
        this.#pushSegment(kind, values, count, byteLength);
        this.#count = index + 1;
        this.#alignTo(alignment);
    }

    /** Keeps a segment of `byteLength` bytes at the end of the buffer */
    #pushSegment(
        kind: number,
        values: unknown,
        count: number,
        byteLength: number
    ): void {
        if (this.#segments === undefined) {
            this.#segments = [];
        }

        this.#segments.push(this.#end, byteLength, kind, values, count);
        this.#shift += byteLength;
    }

    /**
     * Appends bytes, as a segment when they're long, or else copied into the
     * buffer
     */
    #appendBytes(bytes: Uint8Array, byteLength: number): void {
        if (byteLength >= segmentBytes) {
            this.#pushSegment(BYTES, bytes, byteLength, byteLength);
            return;
        }

        const at = this.#reserve(byteLength);
        copyBytes(bytes, 0, byteLength, this.#bytes, this.#base + at);
        this.#end = at + byteLength;
    }

    /**
     * Appends a table: its bytes, or a builder's bytes and segments, with
     * its length written into a sized header
     */
    #append(table: DirectTableBuilder | Uint8Array): void {
        if (!(table instanceof DirectTableBuilder)) {
            this.#appendBytes(table, table.length);
            return;
        }

        if (table.#written !== undefined) {
            const bytes = table.bytes();
            this.#appendBytes(bytes, bytes.length);
            return;
        }

        const length = table.#end;
        const at = this.#reserve(length);
        copyBytes(
            table.#bytes,
            table.#base,
            table.#base + length,
            this.#bytes,
            this.#base + at
        );
        if (table.#layout.sized()) {
            writeUnsigned(
                this.#bytes,
                this.#base + at,
                table.#size,
                length + table.#shift
            );
        }

        const segments = table.#segments;
        if (segments !== undefined && segments.length !== 0) {
            if (this.#segments === undefined) {
                this.#segments = [];
            }

            const own = this.#segments;
            for (let slot = 0; slot < segments.length; slot += SEGMENT_SLOTS) {
                own.push(
                    at + (segments[slot] as number),
                    segments[slot + 1],
                    segments[slot + 2],
                    segments[slot + 3],
                    segments[slot + 4]
                );
            }
        }

        this.#end = at + length;
        this.#shift += table.#shift;
    }

    /** Writes zero padding from the end of the buffer to `at` */
    #pad(at: number): void {
        const end = this.#end;
        if (at === end) {
            return;
        }

        if (at > this.#capacity) {
            this.#grow(at);
        }

        // The buffer's bytes may have been written before a truncation, as
        // room for a string, or by an earlier builder
        const bytes = this.#bytes;
        const base = this.#base;
        for (let i = end; i < at; i++) {
            bytes[base + i] = 0;
        }
        this.#end = at;
    }

    #endScalar(index: number, at: number, byteLength: number): void {
        this.#setOffset(index, at);
        this.#end = at + byteLength;
        this.#count = index + 1;
    }

    /** Writes the offset of field `index`, which starts at `at` in the buffer */
    #setOffset(index: number, at: number): void {
        this.#setUnsigned(
            this.#slots + index * this.#size,
            at + this.#shift - this.#table
        );
    }

    /**
     * Writes an offset, length, count, item end or header slot at `at` in
     * the buffer, little-endian. A value too large for the width is wrapped,
     * as DataView's setters wrap it.
     */
    #setUnsigned(at: number, value: number): void {
        const view = this.#view;
        const position = this.#base + at;
        if (view === undefined) {
            writeUnsigned(this.#bytes, position, this.#size, value);
        } else if (this.#size === SIZEOF_32) {
            view.setUint32(position, value, true);
        } else if (this.#size === SIZEOF_16) {
            view.setUint16(position, value, true);
        } else {
            view.setUint8(position, value);
        }
    }

    /**
     * Makes room for `byteLength` bytes at the end of the buffer, and
     * returns where they go
     */
    #reserve(byteLength: number): number {
        const end = this.#end;
        const claim = end + byteLength;
        if (claim > this.#capacity) {
            this.#grow(claim);
        }

        this.#claim = claim;
        return end;
    }

    /** Grows the buffer to at least `length` bytes, at least doubling it */
    #grow(length: number): void {
        if (arenaBytes !== 0) {
            this.#growInArena(length);
            return;
        }

        const bytes = this.#bytes;
        const capacity = Math.max(length, 2 * bytes.length, FIRST_GROWTH);
        this.#capacity = capacity;

        if (growth === 'resizable') {
            const buffer = bytes.buffer as ArrayBuffer;
            if (capacity <= buffer.maxByteLength) {
                buffer.resize(capacity);
                return;
            }

            const next = new ArrayBuffer(capacity, {
                maxByteLength: 2 * capacity
            });
            new Uint8Array(next).set(bytes);
            this.#bytes = new Uint8Array(next);
            this.#view = new DataView(next);
            return;
        }

        if (growth === 'transfer') {
            this.#bytes = new Uint8Array(
                (bytes.buffer as ArrayBuffer).transfer(capacity)
            );
        } else {
            const next = new Uint8Array(capacity);
            next.set(bytes);
            this.#bytes = next;
        }

        if (scalars === 'view') {
            this.#view = new DataView(this.#bytes.buffer);
        }
    }

    /**
     * Moves the builder's bytes in the arena to bytes with room for at least
     * `length`: the rest of the chunk, or of a new chunk
     */
    #growInArena(length: number): void {
        const bytes = this.#bytes;
        const base = this.#base;

        this.#take(Math.max(length, 2 * this.#end));
        copyBytes(bytes, base, base + this.#end, this.#bytes, this.#base);
    }

    /**
     * Gives the builder the rest of the arena's chunk, of at least `length`
     * bytes, or of a new chunk when it hasn't room, and makes it the last
     * builder. The builder that was the last keeps the bytes that it has
     * written or claimed. Bytes longer than half a chunk are a buffer of the
     * builder's own instead.
     */
    #take(length: number): void {
        const previous = last;
        if (previous !== undefined) {
            const end =
                previous.#base + Math.max(previous.#end, previous.#claim);
            previous.#capacity = end - previous.#base;
            top = end;
            last = undefined;
        }

        if (length > arenaBytes / 2) {
            const bytes = new Uint8Array(length);
            this.#bytes = bytes;
            this.#view =
                scalars === 'view' ? new DataView(bytes.buffer) : undefined;
            this.#base = 0;
            this.#capacity = length;
            return;
        }

        let bytes = chunk;
        if (bytes === undefined || top + length > bytes.length) {
            bytes = new Uint8Array(arenaBytes);
            chunk = bytes;
            chunkView =
                scalars === 'view' ? new DataView(bytes.buffer) : undefined;
            top = 0;
        }

        this.#bytes = bytes;
        this.#view = chunkView;
        this.#base = top;
        this.#capacity = bytes.length - top;
        top = bytes.length;
        last = this;
    }

    #alignTo(alignment: ElementSize): void {
        if (alignment > this.#alignment) {
            this.#alignment = alignment;
        }
    }

    /**
     * The byte length of a table to nest, which a builder knows once all its
     * fields are set
     *
     * @throws {RangeError} If the builder has fields left to set.
     */
    static #lengthOf(table: DirectTableBuilder | Uint8Array): number {
        if (!(table instanceof DirectTableBuilder)) {
            return table.length;
        }

        table.#assertComplete();
        const written = table.#written;
        if (written === undefined) {
            return table.#end + table.#shift;
        }

        return written instanceof ByteBuffer
            ? written.bytes().length
            : written.length;
    }

    /**
     * @throws {TypeError} If a table is being written in place, whose fields
     * the builder is counting rather than its own.
     * @throws {RangeError} If the table has fields left to set.
     */
    #assertComplete(): void {
        if (this.#nesting !== 0) {
            throw new TypeError(
                "A table can't be finalized or nested while a table is recorded in place in it"
            );
        }

        this.#assertSet();
    }

    /** @throws {RangeError} If the table being counted has fields left to set. */
    #assertSet(): void {
        if (this.#count !== this.#fields) {
            throw new RangeError(
                `Only ${this.#count} of the table's ${this.#fields} fields were set`
            );
        }
    }

    /**
     * Checks that a field can be added, and returns its index
     *
     * @throws {TypeError} If the table is already written.
     * @throws {RangeError} If the table already holds all its fields.
     */
    #checkRoom(): number {
        if (this.#written !== undefined) {
            throw new TypeError('The table was already finalized');
        }

        const index = this.#count;
        if (index === this.#fields) {
            throw tooManyFields(this.#fields);
        }

        return index;
    }

    /**
     * Checks that field `index` leaves the table being written `length`
     * bytes long, which the layout addresses
     *
     * @throws {RangeError} If it doesn't.
     */
    #checkLength(index: number, length: number): void {
        if (!(length <= this.#maxLength)) {
            throw tooLong(index, length, this.#layout.offsetType());
        }
    }

    /**
     * Checks that a field of `byteLength` bytes after `padding` bytes can be
     * added, and returns its index. Every offset and length is shorter than
     * the table, so the table's length is the one value to check.
     *
     * @throws {TypeError} If the table is already written.
     * @throws {RangeError} If the table already holds all its fields, or the
     * field would make it longer than the layout addresses.
     */
    #check(padding: number, byteLength: number): number {
        const index = this.#checkRoom();
        this.#checkLength(
            index,
            this.#end + this.#shift - this.#table + padding + byteLength
        );

        return index;
    }

    /**
     * Writes the table into bytes of its exact length: the buffer, with the
     * segments between its bytes, and the length in a sized header
     *
     * @throws {RangeError} If fewer fields were set than the table holds, or
     * an array kept as a segment has changed its length.
     */
    #write(): Uint8Array {
        this.#assertComplete();

        const length = this.#end + this.#shift;
        const segments = this.#segments;
        const table =
            segments === undefined || segments.length === 0
                ? this.#resultOf(length)
                : this.#writeSegments(segments, length);

        if (this.#layout.sized()) {
            writeUnsigned(table, 0, this.#size, length);
        }

        // Release the buffer and the values, which may be large arrays, and
        // leave the buffer, or the bytes taken from the arena when they're
        // the last, for the next builder
        if (arenaBytes !== 0) {
            if (this === last) {
                top = this.#base;
                last = undefined;
            } else if (
                this.#bytes === chunk &&
                this.#base + this.#capacity === top
            ) {
                top = this.#base;
            }
        } else if (
            reuse &&
            result === 'slice' &&
            growth !== 'resizable' &&
            spare === undefined &&
            this.#bytes.length <= SPARE_BYTES
        ) {
            spare = this.#bytes;
            spareView = this.#view;
        }
        this.#bytes = EMPTY;
        this.#view = undefined;
        this.#segments = undefined;
        this.#written = table;

        return table;
    }

    /** The buffer's first `length` bytes, as bytes of their own */
    #resultOf(length: number): Uint8Array {
        if (result === 'slice' || arenaBytes !== 0) {
            return this.#bytes.slice(this.#base, this.#base + length);
        }

        const buffer = this.#bytes.buffer as ArrayBuffer;
        return new Uint8Array(
            growth === 'resizable'
                ? buffer.transferToFixedLength(length)
                : buffer.transfer(length)
        );
    }

    /**
     * Writes the buffer's bytes and the segments between them into new
     * bytes of `length`
     *
     * @throws {RangeError} If an array has changed its length, before the
     * bytes are returned.
     */
    #writeSegments(segments: unknown[], length: number): Uint8Array {
        const bytes = this.#bytes;
        const base = this.#base;
        const table = new Uint8Array(length);

        let from = 0;
        let position = 0;
        for (let slot = 0; slot < segments.length; slot += SEGMENT_SLOTS) {
            const at = segments[slot] as number;
            const byteLength = segments[slot + 1] as number;

            copyBytes(bytes, base + from, base + at, table, position);
            position += at - from;
            from = at;

            writeSegment(
                table,
                position,
                segments[slot + 2] as number,
                segments[slot + 3],
                segments[slot + 4] as number,
                byteLength
            );
            position += byteLength;
        }

        copyBytes(bytes, base + from, base + this.#end, table, position);

        return table;
    }
}

/**
 * Writes a segment's values, `byteLength` bytes of them, at `position` in the
 * table
 *
 * @throws {RangeError} If an array no longer holds `count` elements, or bytes
 * `byteLength` bytes, as when their buffer was transferred.
 */
function writeSegment(
    table: Uint8Array,
    position: number,
    kind: number,
    values: unknown,
    count: number,
    byteLength: number
): void {
    const length = (values as ArrayLike<unknown>).length;
    if (length !== (kind === BYTES ? byteLength : count)) {
        throw new RangeError(
            `An array of ${kind === BYTES ? byteLength : count} elements held ${length} when the table was written`
        );
    }

    switch (kind) {
        case INT8_ARRAY:
            setInt8Elements(table, position, values as Int8Array, count);
            break;

        case UINT8_ARRAY:
            setUint8Elements(table, position, values as Uint8Array, count);
            break;

        case INT16_ARRAY:
            setInt16Elements(table, position, values as Int16Array, count);
            break;

        case UINT16_ARRAY:
            setUint16Elements(table, position, values as Uint16Array, count);
            break;

        case INT32_ARRAY:
            setInt32Elements(table, position, values as Int32Array, count);
            break;

        case UINT32_ARRAY:
            setUint32Elements(table, position, values as Uint32Array, count);
            break;

        case INT64_ARRAY:
            setInt64Elements(table, position, values as BigInt64Array, count);
            break;

        case UINT64_ARRAY:
            setUint64Elements(table, position, values as BigUint64Array, count);
            break;

        case FLOAT32_ARRAY:
            setFloat32Elements(table, position, values as Float32Array, count);
            break;

        case FLOAT64_ARRAY:
            setFloat64Elements(table, position, values as Float64Array, count);
            break;

        default: {
            // Bytes: count holds their length, or their structs' count
            const bytes = values as Uint8Array;
            table.set(bytes, position);
        }
    }
}

/** The longest run of bytes that copyBytes() copies a byte at a time */
const SHORT_COPY = 32;

/**
 * Copies `source`'s bytes from `from` to `to` to `at` in `target`. Short runs
 * are copied a byte at a time, since `subarray()` allocates a view, and
 * gives a `Uint8Array` that V8 keeps on its heap an `ArrayBuffer`.
 */
function copyBytes(
    source: Uint8Array,
    from: number,
    to: number,
    target: Uint8Array,
    at: number
): void {
    const length = to - from;
    if (length <= SHORT_COPY) {
        for (let i = 0; i < length; i++) {
            target[at + i] = source[from + i];
        }
    } else if (source === target) {
        target.copyWithin(at, from, to);
    } else {
        target.set(source.subarray(from, to), at);
    }
}

/**
 * Writes `value` as an unsigned integer of `size` bytes at `at`,
 * little-endian, wrapped as DataView's setters wrap it
 */
function writeUnsigned(
    bytes: Uint8Array,
    at: number,
    size: OffsetSize,
    value: number
): void {
    bytes[at] = value;

    if (size !== SIZEOF_8) {
        bytes[at + 1] = value >>> 8;

        if (size === SIZEOF_32) {
            bytes[at + 2] = value >>> 16;
            bytes[at + 3] = value >>> 24;
        }
    }
}

// A scalar is set little-endian in the scratch, whatever the host's byte
// order, and its bytes copied into the buffer, as TableWriter does
const SCRATCH = new DataView(new ArrayBuffer(SIZEOF_64));
const SCRATCH_BYTES = new Uint8Array(SCRATCH.buffer);

function copy2(bytes: Uint8Array, at: number): void {
    bytes[at] = SCRATCH_BYTES[0];
    bytes[at + 1] = SCRATCH_BYTES[1];
}

function copy4(bytes: Uint8Array, at: number): void {
    bytes[at] = SCRATCH_BYTES[0];
    bytes[at + 1] = SCRATCH_BYTES[1];
    bytes[at + 2] = SCRATCH_BYTES[2];
    bytes[at + 3] = SCRATCH_BYTES[3];
}

function copy8(bytes: Uint8Array, at: number): void {
    bytes[at] = SCRATCH_BYTES[0];
    bytes[at + 1] = SCRATCH_BYTES[1];
    bytes[at + 2] = SCRATCH_BYTES[2];
    bytes[at + 3] = SCRATCH_BYTES[3];
    bytes[at + 4] = SCRATCH_BYTES[4];
    bytes[at + 5] = SCRATCH_BYTES[5];
    bytes[at + 6] = SCRATCH_BYTES[6];
    bytes[at + 7] = SCRATCH_BYTES[7];
}

// The errors are built out of line, as TableBuilder builds them

function tooManyFields(fields: number): RangeError {
    return new RangeError(
        `Can't add field ${fields + 1} to a table of ${fields}`
    );
}

function tooLong(
    index: number,
    length: number,
    offsetType: OffsetType
): RangeError {
    return new RangeError(
        `Field ${index + 1} makes the table ${length} bytes, too long for ${offsetType} offsets`
    );
}
