/**
 * Layouts that a list of strings could have in a table, each written by hand
 * with the same sizing, the same UTF-8 writes and the same decoder, so that
 * their times differ only by the layout. `layouts-main.ts` times them, and
 * `benchmarks/README.md` describes what each is.
 *
 * Each writes the list alone: its count, 4 bytes, and what the layout puts
 * after it. Offsets, counts, lengths and ends are uint32s, as in a table of
 * the default layout.
 */
import { utf8Length, writeUtf8 } from 'tafla';

// The functions are properties, rather than methods, since layouts that
// share another's bytes take its functions as they are
export interface Layout {
    name: string;
    write: (strings: readonly string[]) => Uint8Array;
    /** Reads every string, in order */
    readAll: (bytes: Uint8Array) => string[];
    /** Reads string `index`, without decoding any other */
    readOne: (bytes: Uint8Array, index: number) => string;
}

const decoder = new TextDecoder('utf-8', { fatal: true, ignoreBOM: true });

/** The longest string that `decodeAt()` reads a byte at a time */
const SHORT = 32;

const codesOfLength = Array.from(
    { length: SHORT + 1 },
    (_, length) => new Array<number>(length)
);

/**
 * Reads `length` bytes of UTF-8 at `start`: ASCII of up to 32 bytes a byte
 * at a time, as the runtime's `getString()` reads it, and anything else with
 * the fatal decoder
 */
function decodeAt(bytes: Uint8Array, start: number, length: number): string {
    if (length <= SHORT) {
        const codes = codesOfLength[length];
        let i = 0;
        for (; i < length; i++) {
            const byte = bytes[start + i];
            if (byte >= 0x80) {
                break;
            }

            codes[i] = byte;
        }

        if (i === length) {
            return String.fromCharCode.apply(null, codes);
        }
    }

    return decoder.decode(bytes.subarray(start, start + length));
}

function viewOf(bytes: Uint8Array): DataView {
    return new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
}

function utf8LengthOf(strings: readonly string[]): number {
    let length = 0;
    for (let i = 0; i < strings.length; i++) {
        length += utf8Length(strings[i]);
    }

    return length;
}

function checkIndex(index: number, count: number): void {
    if (!(index >>> 0 === index && index < count)) {
        throw new RangeError(`No string ${index} of ${count}`);
    }
}

function outOfRange(): RangeError {
    return new RangeError('A string lies outside the bytes');
}

/**
 * The proposed vector of strings: the count, the end of each string from the
 * list's start, and then the strings' UTF-8, one after another, as a vector
 * of tables holds its items
 */
export const ends: Layout = {
    name: 'ends',
    write(strings) {
        const count = strings.length;
        const start = 4 + 4 * count;
        const bytes = new Uint8Array(start + utf8LengthOf(strings));
        const view = viewOf(bytes);

        view.setUint32(0, count, true);
        let position = start;
        for (let i = 0; i < count; i++) {
            position += writeUtf8(bytes, position, strings[i]);
            view.setUint32(4 + 4 * i, position, true);
        }

        return bytes;
    },
    readAll(bytes) {
        const view = viewOf(bytes);
        const count = view.getUint32(0, true);
        if (!(4 + 4 * count <= bytes.length)) {
            throw outOfRange();
        }

        const strings = new Array<string>(count);
        let start = 4 + 4 * count;
        for (let i = 0; i < count; i++) {
            const end = view.getUint32(4 + 4 * i, true);
            if (start > end || end > bytes.length) {
                throw outOfRange();
            }

            strings[i] = decodeAt(bytes, start, end - start);
            start = end;
        }

        return strings;
    },
    readOne(bytes, index) {
        const view = viewOf(bytes);
        const count = view.getUint32(0, true);
        checkIndex(index, count);
        if (!(4 + 4 * count <= bytes.length)) {
            throw outOfRange();
        }

        const start =
            index === 0 ? 4 + 4 * count : view.getUint32(4 * index, true);
        const end = view.getUint32(4 + 4 * index, true);
        if (start > end || end > bytes.length) {
            throw outOfRange();
        }

        return decodeAt(bytes, start, end - start);
    }
};

/**
 * The same bytes as {@link ends}, read all at once by decoding every
 * string's bytes as one run, and then slicing the run at the ends when it's
 * ASCII, whose characters are a byte each. A run with other characters is
 * read a string at a time.
 */
export const endsRun: Layout = {
    name: 'ends, run decoded once',
    write: ends.write,
    readAll(bytes) {
        const view = viewOf(bytes);
        const count = view.getUint32(0, true);
        const first = 4 + 4 * count;
        if (!(first <= bytes.length)) {
            throw outOfRange();
        }
        if (count === 0) {
            return [];
        }

        const last = view.getUint32(4 * count, true);
        if (first > last || last > bytes.length) {
            throw outOfRange();
        }

        const run = decoder.decode(bytes.subarray(first, last));
        if (run.length !== last - first) {
            return ends.readAll(bytes);
        }

        const strings = new Array<string>(count);
        let start = first;
        for (let i = 0; i < count; i++) {
            const end = view.getUint32(4 + 4 * i, true);
            if (start > end || end > last) {
                throw outOfRange();
            }

            strings[i] = run.slice(start - first, end - first);
            start = end;
        }

        return strings;
    },
    readOne: ends.readOne
};

/**
 * The same bytes as {@link ends}, read all at once as {@link endsRun} reads
 * them when the first string is ASCII, and otherwise a string at a time, so
 * that a list of strings outside ASCII isn't decoded twice
 */
export const endsRunIfAscii: Layout = {
    name: 'ends, run if first is ASCII',
    write: ends.write,
    readAll(bytes) {
        const view = viewOf(bytes);
        const count = view.getUint32(0, true);
        const first = 4 + 4 * count;
        if (!(first <= bytes.length)) {
            throw outOfRange();
        }
        if (count === 0) {
            return [];
        }

        const firstEnd = view.getUint32(4, true);
        if (first > firstEnd || firstEnd > bytes.length) {
            throw outOfRange();
        }

        // Fewer characters than bytes, when any character isn't ASCII
        const head = decodeAt(bytes, first, firstEnd - first);
        if (head.length !== firstEnd - first) {
            return ends.readAll(bytes);
        }

        return endsRun.readAll(bytes);
    },
    readOne: ends.readOne
};

/**
 * The count, the ends as an array of uint32s, which is aligned to 4 bytes
 * here as a table would pad it, and then the strings' UTF-8. The ends are
 * written and read through a `Uint32Array` over the bytes, as a typed array
 * field is, which a little-endian host can do when the bytes are aligned.
 */
export const alignedEnds: Layout = {
    name: 'ends as a Uint32Array',
    write(strings) {
        const count = strings.length;
        const start = 4 + 4 * count;
        const bytes = new Uint8Array(start + utf8LengthOf(strings));
        const slots = new Uint32Array(bytes.buffer, 0, count + 1);

        slots[0] = count;
        let position = start;
        for (let i = 0; i < count; i++) {
            position += writeUtf8(bytes, position, strings[i]);
            slots[i + 1] = position;
        }

        return bytes;
    },
    readAll(bytes) {
        const count = viewOf(bytes).getUint32(0, true);
        if (!(4 + 4 * count <= bytes.length)) {
            throw outOfRange();
        }

        const slots = new Uint32Array(
            bytes.buffer,
            bytes.byteOffset + 4,
            count
        );
        const strings = new Array<string>(count);
        let start = 4 + 4 * count;
        for (let i = 0; i < count; i++) {
            const end = slots[i];
            if (start > end || end > bytes.length) {
                throw outOfRange();
            }

            strings[i] = decodeAt(bytes, start, end - start);
            start = end;
        }

        return strings;
    },
    readOne(bytes, index) {
        const count = viewOf(bytes).getUint32(0, true);
        checkIndex(index, count);
        if (!(4 + 4 * count <= bytes.length)) {
            throw outOfRange();
        }

        const slots = new Uint32Array(
            bytes.buffer,
            bytes.byteOffset + 4,
            count
        );
        const start = index === 0 ? 4 + 4 * count : slots[index - 1];
        const end = slots[index];
        if (start > end || end > bytes.length) {
            throw outOfRange();
        }

        return decodeAt(bytes, start, end - start);
    }
};

/**
 * The count, and then each string as a string field holds it: its length
 * and its UTF-8. A reader finds a string by reading the lengths of those
 * before it.
 */
export const lengths: Layout = {
    name: 'length before each string',
    write(strings) {
        const count = strings.length;
        const bytes = new Uint8Array(4 + 4 * count + utf8LengthOf(strings));
        const view = viewOf(bytes);

        view.setUint32(0, count, true);
        let position = 4;
        for (let i = 0; i < count; i++) {
            const length = writeUtf8(bytes, position + 4, strings[i]);
            view.setUint32(position, length, true);
            position += 4 + length;
        }

        return bytes;
    },
    readAll(bytes) {
        const view = viewOf(bytes);
        const count = view.getUint32(0, true);
        // Each string takes 4 bytes at least, which bounds the count
        if (!(4 + 4 * count <= bytes.length)) {
            throw outOfRange();
        }

        const strings = new Array<string>(count);
        let position = 4;
        for (let i = 0; i < count; i++) {
            const length = view.getUint32(position, true);
            if (position + 4 + length > bytes.length) {
                throw outOfRange();
            }

            strings[i] = decodeAt(bytes, position + 4, length);
            position += 4 + length;
        }

        return strings;
    },
    readOne(bytes, index) {
        const view = viewOf(bytes);
        const count = view.getUint32(0, true);
        checkIndex(index, count);

        let position = 4;
        for (let i = 0; i < index; i++) {
            position += 4 + view.getUint32(position, true);
        }

        const length = view.getUint32(position, true);
        if (position + 4 + length > bytes.length) {
            throw outOfRange();
        }

        return decodeAt(bytes, position + 4, length);
    }
};

/**
 * Today's vector of tables of one string each: the count, the end of each
 * table, and then each table as its one offset, which is 4, the string's
 * length and its UTF-8
 */
export const tables: Layout = {
    name: 'table of one string each',
    write(strings) {
        const count = strings.length;
        const start = 4 + 4 * count;
        const bytes = new Uint8Array(start + 8 * count + utf8LengthOf(strings));
        const view = viewOf(bytes);

        view.setUint32(0, count, true);
        let position = start;
        for (let i = 0; i < count; i++) {
            const length = writeUtf8(bytes, position + 8, strings[i]);
            view.setUint32(position, 4, true);
            view.setUint32(position + 4, length, true);
            position += 8 + length;
            view.setUint32(4 + 4 * i, position, true);
        }

        return bytes;
    },
    readAll(bytes) {
        const view = viewOf(bytes);
        const count = view.getUint32(0, true);
        if (!(4 + 4 * count <= bytes.length)) {
            throw outOfRange();
        }

        const strings = new Array<string>(count);
        let start = 4 + 4 * count;
        for (let i = 0; i < count; i++) {
            const end = view.getUint32(4 + 4 * i, true);
            if (start > end || end > bytes.length) {
                throw outOfRange();
            }

            strings[i] = stringOfTable(bytes, view, start);
            start = end;
        }

        return strings;
    },
    readOne(bytes, index) {
        const view = viewOf(bytes);
        const count = view.getUint32(0, true);
        checkIndex(index, count);
        if (!(4 + 4 * count <= bytes.length)) {
            throw outOfRange();
        }

        const start =
            index === 0 ? 4 + 4 * count : view.getUint32(4 * index, true);
        const end = view.getUint32(4 + 4 * index, true);
        if (start > end || end > bytes.length) {
            throw outOfRange();
        }

        return stringOfTable(bytes, view, start);
    }
};

/** Reads the string that is the one field of the table at `table` */
function stringOfTable(
    bytes: Uint8Array,
    view: DataView,
    table: number
): string {
    const position = table + view.getUint32(table, true);
    const length = view.getUint32(position, true);
    if (position + 4 + length > bytes.length) {
        throw outOfRange();
    }

    return decodeAt(bytes, position + 4, length);
}

export const LAYOUTS: Layout[] = [
    ends,
    endsRun,
    endsRunIfAscii,
    alignedEnds,
    lengths,
    tables
];
