const encoder = new TextEncoder();
// Fatal, so that invalid UTF-8 throws instead of becoming U+FFFD, and keeping
// a leading U+FEFF, which is part of the string rather than a byte order mark
const decoder = new TextDecoder('utf-8', { fatal: true, ignoreBOM: true });

/**
 * The most UTF-16 code units that `encodeInto()` writes after a string's
 * ASCII, and that `utf8Length()` counts, and the most bytes that
 * `decodeShortAscii()` and `decodeShortUtf8()` read, a unit or a byte at a
 * time
 */
const SHORT_STRING = 32;

/**
 * The longest string whose ASCII `encodeInto()` writes a byte at a time, in
 * UTF-16 code units
 */
const ASCII_STRING = 48;

/**
 * The longest ASCII string that `decodeShortUtf8()` reads from its bytes as
 * the arguments of one `String.fromCharCode()`, rather than
 * `decodeShortAscii()` from an array of them
 */
const TINY_STRING = 8;

/** The bytes that `utf8LengthByEncoding()` encodes a string into */
const scratch = new Uint8Array(4096);

export function encode(string: string): Uint8Array {
    return encoder.encode(string);
}

/** @throws {TypeError} If `bytes` isn't valid UTF-8. */
export function decode(bytes: Uint8Array): string {
    return decoder.decode(bytes);
}

/**
 * Reads the `length` bytes from `start` in `bytes`, which must lie within
 * them, as a string when they're a short string of valid UTF-8, of up to
 * {@link SHORT_STRING} bytes. Otherwise it returns `undefined`, for
 * `decode()` to read them, or to throw for them.
 *
 * Through `TableReader.getString()`, which calls it for a string that isn't
 * ASCII of 9 to {@link SHORT_STRING} bytes, tables of 16 strings of 8 to 16
 * characters with characters of 2 to 4 bytes took 0.32–0.48 times as long
 * to read as with `decode()`, in Node 24, 26 and 27.
 */
export function decodeShortUtf8(
    bytes: Uint8Array,
    start: number,
    length: number
): string | undefined {
    if (length > SHORT_STRING) {
        return undefined;
    }

    if (length <= TINY_STRING) {
        const ascii = decodeTinyAscii(bytes, start, length);
        if (ascii !== undefined) {
            return ascii;
        }
    }

    return decodeUtf8(bytes, start, length);
}

/**
 * Reads the `length` bytes from `start` in `bytes`, up to
 * {@link TINY_STRING}, as a string when they're ASCII, from the arguments of
 * one `String.fromCharCode()`, which takes less time than an array of them:
 * `TableReader` read tables of 16 empty strings in 0.75–0.79 times the time
 * of `decodeShortAscii()`'s array, and of 16 strings of 8 characters in
 * 0.74–0.86, in Node 24, 26 and 27. It's more than the 460 bytes of bytecode
 * that V8 inlines, so it stays a call from `decodeShortUtf8()`.
 */
function decodeTinyAscii(
    bytes: Uint8Array,
    start: number,
    length: number
): string | undefined {
    let bits = 0;
    for (let i = 0; i < length; i++) {
        bits |= bytes[start + i];
    }

    if (bits >= 0x80) {
        return undefined;
    }

    // Loaded once, rather than at each call, which made this function 535
    // bytes of bytecode rather than 470 in Node 26, and 530 rather than 465
    // in Node 24 and 27
    const fromCharCode = String.fromCharCode;
    const b = bytes;
    const s = start;
    switch (length) {
        case 0:
            return '';
        case 1:
            return fromCharCode(b[s]);
        case 2:
            return fromCharCode(b[s], b[s + 1]);
        case 3:
            return fromCharCode(b[s], b[s + 1], b[s + 2]);
        case 4:
            return fromCharCode(b[s], b[s + 1], b[s + 2], b[s + 3]);
        case 5:
            return fromCharCode(b[s], b[s + 1], b[s + 2], b[s + 3], b[s + 4]);
        case 6:
            return fromCharCode(
                b[s],
                b[s + 1],
                b[s + 2],
                b[s + 3],
                b[s + 4],
                b[s + 5]
            );
        case 7:
            return fromCharCode(
                b[s],
                b[s + 1],
                b[s + 2],
                b[s + 3],
                b[s + 4],
                b[s + 5],
                b[s + 6]
            );
        default:
            return fromCharCode(
                b[s],
                b[s + 1],
                b[s + 2],
                b[s + 3],
                b[s + 4],
                b[s + 5],
                b[s + 6],
                b[s + 7]
            );
    }
}

/**
 * Reads the `length` bytes from `start` in `bytes` as a string, a byte at a
 * time, when they're what a fatal decoder accepts. It returns `undefined`
 * for anything else, so that the decoder decides, and throws: a sequence
 * cut short by the end of the bytes, a byte that isn't the continuation
 * byte it should be, an overlong sequence, a surrogate, and a code point
 * past U+10FFFF.
 *
 * It's more bytecode than V8 inlines, so `decodeShortUtf8()` checks the
 * length for it, where a long string then costs no call.
 */
function decodeUtf8(
    bytes: Uint8Array,
    start: number,
    length: number
): string | undefined {
    // Appended, rather than stored into an array of a unit per byte that's
    // shortened after, with which strings of 8 to 16 characters outside
    // ASCII took 1.3–1.45 times as long to read in Node 24, 26 and 27
    const codes: number[] = [];
    const end = start + length;
    let position = start;

    while (position < end) {
        const lead = bytes[position];

        if (lead < 0x80) {
            codes.push(lead);
            position++;
        } else if (lead < 0xe0) {
            // 2 bytes hold U+0080 to U+07FF, so 0xC0 and 0xC1 are
            // overlong, and below them are the continuation bytes
            if (lead < 0xc2 || position + 1 >= end) {
                return undefined;
            }

            const second = bytes[position + 1];
            if ((second & 0xc0) !== 0x80) {
                return undefined;
            }

            codes.push(((lead & 0x1f) << 6) | (second & 0x3f));
            position += 2;
        } else if (lead < 0xf0) {
            if (position + 2 >= end) {
                return undefined;
            }

            const second = bytes[position + 1];
            const third = bytes[position + 2];
            if ((second & 0xc0) !== 0x80 || (third & 0xc0) !== 0x80) {
                return undefined;
            }

            // 3 bytes hold U+0800 to U+FFFF, but for the surrogates
            const unit =
                ((lead & 0x0f) << 12) | ((second & 0x3f) << 6) | (third & 0x3f);
            if (unit < 0x800 || (unit & 0xf800) === 0xd800) {
                return undefined;
            }

            codes.push(unit);
            position += 3;
        } else {
            if (lead > 0xf4 || position + 3 >= end) {
                return undefined;
            }

            const second = bytes[position + 1];
            const third = bytes[position + 2];
            const fourth = bytes[position + 3];
            if (
                (second & 0xc0) !== 0x80 ||
                (third & 0xc0) !== 0x80 ||
                (fourth & 0xc0) !== 0x80
            ) {
                return undefined;
            }

            // 4 bytes hold U+10000 to U+10FFFF, as a surrogate pair
            const point =
                ((lead & 0x07) << 18) |
                ((second & 0x3f) << 12) |
                ((third & 0x3f) << 6) |
                (fourth & 0x3f);
            if (point < 0x10000 || point > 0x10ffff) {
                return undefined;
            }

            codes.push(
                0xd800 | ((point - 0x10000) >> 10),
                0xdc00 | (point & 0x3ff)
            );
            position += 4;
        }
    }

    return String.fromCharCode.apply(null, codes);
}

/**
 * Reads the `length` bytes from `start` in `bytes`, which must lie within
 * them, as a string when they're a short ASCII string, of more than
 * {@link TINY_STRING} and up to {@link SHORT_STRING} bytes below 0x80, which
 * UTF-8 decodes to the same code points. Otherwise it returns `undefined`,
 * for `decodeShortUtf8()` or `decode()` to read them.
 *
 * Through `TableReader.getString()`, the loop read 8, 16 and 32 bytes of
 * ASCII in 30, 45 and 78 ns in Node 26, and in 28, 43 and 72 ns in Node 27,
 * where `decode()` took 84–89 ns for any of them, and broke even at about 37
 * bytes in Node 26 and 41 in Node 27. A string with a byte of 0x80 or more
 * pays for the loop until that byte, and then for `decode()`: 8 ns more when
 * it's the first of 32 bytes, and 37 ns when it's the last. Adding each
 * character to a string took longer than `decode()` from 16 bytes, adding 4
 * at a time longer than this loop from 16, and `fromCharCode()` of a
 * `subarray()` longer than this loop at every length.
 */
export function decodeShortAscii(
    bytes: Uint8Array,
    start: number,
    length: number
): string | undefined {
    // Above TINY_STRING and up to SHORT_STRING, in one unsigned comparison:
    // with two, Node 26 read the comparison suite's record, whose string has
    // 16 bytes, in 113–118 ns rather than 100–104 in 6 of 20 processes
    if ((length - (TINY_STRING + 1)) >>> 0 > SHORT_STRING - (TINY_STRING + 1)) {
        return undefined;
    }

    const codes = new Array<number>(length);
    for (let i = 0; i < length; i++) {
        const byte = bytes[start + i];
        if (byte >= 0x80) {
            return undefined;
        }

        codes[i] = byte;
    }

    return String.fromCharCode.apply(null, codes);
}

/**
 * Writes `string` as UTF-8 at `position` in `bytes`, which must have room for
 * it, with lone surrogates replaced by U+FFFD, as `encode()` writes them.
 *
 * The ASCII of a string of up to {@link ASCII_STRING} UTF-16 code units is
 * written a byte at a time, which took 6–60 ns for up to 32 characters in
 * Node 26, against 53–67 ns for `encodeInto()`, whose `subarray()` also
 * gives a small `Uint8Array` the `ArrayBuffer` that V8 otherwise leaves out.
 * The loop took about 51 ns for 40 characters and 80 ns for 64, where
 * `encodeInto()` took 58–71 ns for either in Node 24, 26 and 27: with a limit
 * of 48 rather than 32, tables of 16 strings of 40 characters of ASCII were
 * written in 0.91–0.96 times the time, and with 64, tables of 16 strings of
 * 64 characters in 1.16–1.33.
 *
 * @returns The number of bytes written.
 * @throws {RangeError} If `bytes` are too short for the string.
 */
export function encodeInto(
    string: string,
    bytes: Uint8Array,
    position: number
): number {
    const length = string.length;

    let i = 0;
    if (length <= ASCII_STRING && position + length <= bytes.length) {
        while (i < length) {
            const unit = string.charCodeAt(i);
            if (unit >= 0x80) {
                break;
            }

            bytes[position + i] = unit;
            i++;
        }

        if (i === length) {
            return length;
        }
    }

    return encodeIntoOutOfLine(string, bytes, position, i);
}

/**
 * `encodeInto()` of a string that isn't short ASCII with room for it, after
 * the `ascii` units of ASCII at its start that `encodeInto()` wrote.
 *
 * The rest of a string after them, of up to {@link SHORT_STRING} UTF-16 code
 * units, is written a unit or a surrogate pair at a time, as
 * `TextEncoder.encodeInto()` encodes them, and any other string with
 * `TextEncoder.encodeInto()`, from its start. The loop doesn't check that
 * each byte lies within `bytes`, since a typed array ignores a write past its
 * end, and throws afterwards for a string that ended past them. The bytes
 * then hold the start of the string, which may end with the first bytes of a
 * character.
 *
 * Continuing after the ASCII, rather than writing the string again from its
 * start, made tables of 16 strings of 24 characters, the last of 2 bytes,
 * take 0.70–0.73 times as long to write in Node 24, 26 and 27, and of 8 with
 * one of 2 bytes 0.91–0.94. The rest of a string of 33 to 48 units is written
 * here only when it's no longer than {@link SHORT_STRING}: writing all of it
 * made 16 strings of 48 characters of 3 bytes take 1.15–1.25 times as long in
 * Node 26 and 27, whose `TextEncoder` writes them faster than the loop.
 *
 * It's one function, of more bytecode than V8 inlines, so that the bytecode
 * that V8 inlines with `encodeInto()` is its loop over ASCII alone. With
 * the loop in a function of 415 bytes, which V8 can inline, beside the
 * encoder's, `writeUtf8()` took 1.21 times as long to write 8 characters of
 * ASCII in Node 27.
 *
 * Through `writeUtf8()` in Node 27, strings of 8 characters took 28 ns
 * rather than 66 with a character of 2 bytes among them, and 26 ns rather
 * than 75 with 3 bytes each; `TableBuilder` wrote strings of up to 32 such
 * characters in 0.48–0.82 times the time in Node 24, 26 and 27. A string of
 * 33 to 64 characters of ASCII takes 3–6 ns longer than the 60–70 ns that
 * it took with `encodeInto()` in a function small enough to inline.
 */
function encodeIntoOutOfLine(
    string: string,
    bytes: Uint8Array,
    position: number,
    ascii: number
): number {
    const length = string.length;

    if (length - ascii > SHORT_STRING) {
        const { read, written } = encoder.encodeInto(
            string,
            bytes.subarray(position)
        );
        if (read !== length) {
            return written + encodeRest(string, read, bytes, position, written);
        }

        return written;
    }

    let end = position + ascii;
    for (let i = ascii; i < length; i++) {
        let unit = string.charCodeAt(i);

        if (unit < 0x80) {
            bytes[end++] = unit;
            continue;
        }

        if (unit < 0x800) {
            bytes[end] = 0xc0 | (unit >> 6);
            bytes[end + 1] = 0x80 | (unit & 0x3f);
            end += 2;
            continue;
        }

        if ((unit & 0xf800) === 0xd800) {
            // NaN past the string's end, which is no low surrogate
            const low = string.charCodeAt(i + 1);

            if (unit < 0xdc00 && (low & 0xfc00) === 0xdc00) {
                const point = 0x10000 + ((unit & 0x3ff) << 10) + (low & 0x3ff);
                bytes[end] = 0xf0 | (point >> 18);
                bytes[end + 1] = 0x80 | ((point >> 12) & 0x3f);
                bytes[end + 2] = 0x80 | ((point >> 6) & 0x3f);
                bytes[end + 3] = 0x80 | (point & 0x3f);
                end += 4;
                i++;
                continue;
            }

            unit = 0xfffd;
        }

        bytes[end] = 0xe0 | (unit >> 12);
        bytes[end + 1] = 0x80 | ((unit >> 6) & 0x3f);
        bytes[end + 2] = 0x80 | (unit & 0x3f);
        end += 3;
    }

    if (end > bytes.length) {
        throw doesNotFit(string, bytes, position);
    }

    return end - position;
}

function doesNotFit(
    string: string,
    bytes: Uint8Array,
    position: number
): RangeError {
    return new RangeError(
        `A string of ${string.length} UTF-16 code units doesn't fit in ${bytes.length - position} bytes`
    );
}

/**
 * Writes the rest of `string`, from its `read`th UTF-16 code unit, after the
 * `written` bytes at `position` in `bytes` at which `TextEncoder.encodeInto()`
 * stopped, and returns the number of bytes in the rest.
 *
 * From Node 25.4, `encodeInto()` can stop before characters that fit, as
 * https://github.com/nodejs/node/issues/65994 reports. Near the end of the
 * bytes, it counts 3 bytes for characters that take 2, U+0400 to U+07FF and,
 * in a string that V8 holds as Latin-1 on a host whose `char` is signed,
 * U+0080 to U+00FF, and 6 for a surrogate pair, which takes 4. In bytes of
 * exactly a string's UTF-8 length, it then leaves out the last unit of some
 * strings of more than 256 units with such characters, such as
 * `'aé'.repeat(133) + 'a'`. It never stops within a surrogate pair, so the
 * rest starts at a code point.
 *
 * @throws {RangeError} If the rest doesn't fit in `bytes`.
 */
function encodeRest(
    string: string,
    read: number,
    bytes: Uint8Array,
    position: number,
    written: number
): number {
    const start = position + written;
    const available = bytes.length - start;

    // Each unit takes at least a byte, so a rest of more units than there
    // are bytes left doesn't fit, and isn't encoded to find that out
    if (string.length - read <= available) {
        const rest = encoder.encode(string.slice(read));
        if (rest.length <= available) {
            bytes.set(rest, start);
            return rest.length;
        }
    }

    throw doesNotFit(string, bytes, position);
}

/**
 * The byte length of `string` in UTF-8, as `encode()` writes it: a lone
 * surrogate takes the 3 bytes of U+FFFD, and a surrogate pair 4 bytes
 *
 * It counts from the first unit of 0x80 or more out of line, so that the
 * bytecode that V8 inlines into its callers is only the loop over ASCII, and
 * a string of more than {@link SHORT_STRING} units out of line too, by
 * encoding it.
 */
export function utf8Length(string: string): number {
    const length = string.length;

    if (length > SHORT_STRING) {
        return utf8LengthByEncoding(string);
    }

    for (let i = 0; i < length; i++) {
        if (string.charCodeAt(i) >= 0x80) {
            return utf8LengthFrom(string, i);
        }
    }

    return length;
}

/**
 * `utf8Length()` of a long string, as the bytes that
 * `TextEncoder.encodeInto()` writes of it into `scratch`, which it leaves
 * to be overwritten. A string that doesn't fit is encoded part by part, each
 * from where `encodeInto()` stopped, which is never within a surrogate
 * pair, wherever https://github.com/nodejs/node/issues/65994 makes it stop.
 *
 * In Node 27, it counted 65 and 1,024 characters of ASCII in 39 and 54 ns,
 * where the loop took 83 and 1,205 ns, at 1.2 ns a character, and 128
 * characters of 3 bytes in 111 ns rather than 228. Past 32 characters of
 * ASCII it takes less time than the loop: with 32 as the limit rather than
 * 64, tables of 16 strings of 40 and 64 characters took 0.86–0.93 and
 * 0.69–0.77 times as long to write in Node 24, 26 and 27. In Node 24, whose
 * `encodeInto()` takes 2–2.5 ns for each character outside Latin-1, it
 * takes 1.4–1.6 times as long as the loop for strings of those, such as
 * 1.18 times as long to write 16 strings of 48 characters of 3 bytes, and
 * as little as in Node 27 for ASCII.
 */
function utf8LengthByEncoding(string: string): number {
    let { read, written } = encoder.encodeInto(string, scratch);
    let length = written;

    let rest = string;
    while (read < rest.length) {
        rest = rest.slice(read);
        ({ read, written } = encoder.encodeInto(rest, scratch));
        length += written;
    }

    return length;
}

/**
 * `utf8Length()` of a string whose units before `start` are ASCII, and
 * take a byte each
 */
function utf8LengthFrom(string: string, start: number): number {
    let length = string.length;

    for (let i = start; i < string.length; i++) {
        const unit = string.charCodeAt(i);
        if (unit < 0x80) {
            continue;
        }

        if (unit < 0x800) {
            length += 1;
        } else if (
            unit < 0xdc00 &&
            unit >= 0xd800 &&
            (string.charCodeAt(i + 1) & 0xfc00) === 0xdc00
        ) {
            // The pair's two units take 4 bytes
            length += 2;
            i++;
        } else {
            length += 2;
        }
    }

    return length;
}
