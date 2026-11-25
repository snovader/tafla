import { describe, expect, it } from 'hypotes';
import { fc, it as property } from 'hypotes/fast-check';
import {
    DecodeError,
    StringVector,
    TableBuilder,
    TableLayout,
    TableReader,
    TableSizer
} from 'tafla';
import { MAX_VALUES, STRINGS } from './conformance/arbitraries.ts';

const ENCODER = new TextEncoder();

const VECTORS = fc.array(STRINGS, { maxLength: 8 });

/** A table whose only field is a vector of `strings` */
function tableOf(strings: readonly string[], layout?: TableLayout): Uint8Array {
    const builder = new TableBuilder(1, layout);
    builder.setStringVector(strings);

    return builder.bytes();
}

function viewOf(bytes: Uint8Array, layout?: TableLayout): StringVector {
    const reader = new TableReader(bytes, layout);

    return new StringVector(reader, reader.rootOffset(0));
}

describe('a vector of strings', () => {
    property.prop([VECTORS])(
        'should read back every string, at once and each from its ends alone',
        strings => {
            const view = viewOf(tableOf(strings));
            const each = Array.from({ length: view.length() }, (_, index) => view.get(index));

            expect(view.length()).toBe(strings.length);
            expect(view.toArray()).toStrictEqual(strings);
            expect(each).toStrictEqual(strings);
        }
    );

    property.prop([VECTORS])(
        'should take the bytes of a vector whose items are the strings’ UTF-8',
        strings => {
            const builder = new TableBuilder(1);
            builder.setVector(strings.map(string => ENCODER.encode(string)));

            expect(tableOf(strings)).toStrictEqual(builder.bytes());
        }
    );

    property.prop([VECTORS])(
        'should take the end of each string and a count beside the strings’ UTF-8',
        strings => {
            const utf8 = strings.reduce(
                (length, string) => length + ENCODER.encode(string).length,
                0
            );

            // The table's one offset, the count and the ends, of 4 bytes each
            expect(tableOf(strings)).toHaveLength(4 + 4 + 4 * strings.length + utf8);
        }
    );

    property.prop([VECTORS])(
        'should be written by a sizer and its writer as by a builder',
        strings => {
            const sizer = new TableSizer(1);
            sizer.addStringVector(strings);
            const writer = sizer.writer();
            writer.setStringVector(0, strings);

            expect(writer.bytes()).toStrictEqual(tableOf(strings));
        }
    );

    it('should write a lone surrogate as U+FFFD, as a string field does', () => {
        const view = viewOf(tableOf(['a\uD800', '\uDC00b', '🌍']));

        expect(view.toArray()).toStrictEqual(['a�', '�b', '🌍']);
    });

    it('should read strings longer than the short strings that are read a byte at a time', () => {
        const strings = ['a'.repeat(33), 'é'.repeat(40), '東'.repeat(300), 'b'.repeat(4096)];

        expect(viewOf(tableOf(strings)).toArray()).toStrictEqual(strings);
        expect(viewOf(tableOf(strings)).get(2)).toBe(strings[2]);
    });
});

describe('a vector of strings that readers reject', () => {
    it.each([-1, 2, 0.5, Number.NaN])(
        'should throw RangeError for string %s of a vector of 2',
        index => {
            const view = viewOf(tableOf(['a', 'b']));

            expect(() => view.get(index)).toThrow(
                new RangeError(`Item ${index} is outside the vector of 2 at 4`)
            );
        }
    );

    it('should reject an impossible count before anything allocates for it', () => {
        const reader = new TableReader(new Uint8Array([255, 255, 255, 255]));

        expect(() => reader.getStringVector(0)).toThrow(DecodeError);
        expect(() => new StringVector(reader, 0)).toThrow(DecodeError);
    });

    it('should throw DecodeError for the strings on either side of an end that splits a character, and read the others', () => {
        const bytes = tableOf(['a', '東', '京', 'b']);
        // The end of the second string, after the table's offset, the count
        // and the first end
        bytes[4 + 4 + 4]--;
        const view = viewOf(bytes);

        expect(view.get(0)).toBe('a');
        expect(() => view.get(1)).toThrow(DecodeError);
        expect(() => view.get(2)).toThrow(DecodeError);
        expect(view.get(3)).toBe('b');
        expect(() => view.toArray()).toThrow(DecodeError);
    });

    it.each([
        ['a short string', 3],
        ['a string too long to read a byte at a time', 40]
    ])('should name the position of %s that isn’t valid UTF-8', (_, length) => {
        const bytes = tableOf(['a', 'b'.repeat(length)]);
        // The second string starts after the offset, the count, two ends and
        // the first string
        bytes[17] = 0xff;

        expect(() => viewOf(bytes).get(1)).toThrow(
            new DecodeError("The string at 17 isn't valid UTF-8")
        );
    });
});

describe('a vector of strings that writers reject', () => {
    it.each(['uint8', 'uint16'] as const)(
        'should fill a table of %s offsets, and throw RangeError for a byte more, leaving the builder as it was',
        offsetType => {
            const layout = TableLayout.create({ offsetType });
            const size = layout.offsetSize();
            // Two offsets, a uint8, and the vector's count and its one end
            const room = MAX_VALUES[offsetType] - 4 * size - 1;
            const builder = new TableBuilder(2, layout);
            builder.setUint8(7);

            expect(() => builder.setStringVector(['a'.repeat(room + 1)])).toThrow(
                new RangeError(
                    `Field 2 makes the table ${MAX_VALUES[offsetType] + 1} bytes, too long for ${offsetType} offsets`
                )
            );
            builder.setStringVector(['a'.repeat(room)]);

            const bytes = builder.bytes();
            const reader = new TableReader(bytes, layout);
            expect(bytes).toHaveLength(MAX_VALUES[offsetType]);
            expect(reader.getUint8(reader.rootOffset(0))).toBe(7);
            expect(reader.getStringVector(reader.rootOffset(1))).toStrictEqual(['a'.repeat(room)]);
        }
    );

    it('should throw RangeError for strings longer than those sized, without moving the writer', () => {
        const sizer = new TableSizer(1);
        sizer.addStringVector(['a', 'b']);
        const writer = sizer.writer();

        expect(() => writer.setStringVector(0, ['a', 'bc'])).toThrow(RangeError);
        expect(() => writer.setStringVector(0, ['a', 'b', ''])).toThrow(RangeError);
        writer.setStringVector(0, ['a', 'b']);

        expect(writer.bytes()).toStrictEqual(tableOf(['a', 'b']));
    });

    it('should throw RangeError from bytes() for strings shorter than those sized', () => {
        const sizer = new TableSizer(1);
        sizer.addStringVector(['ab']);
        const writer = sizer.writer();
        writer.setStringVector(0, ['a']);

        expect(() => writer.bytes()).toThrow(
            new RangeError("The fields wrote 13 of the table's 14 bytes")
        );
    });
});
