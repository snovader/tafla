import { describe, expect } from 'hypotes';
import { it } from 'hypotes/fast-check';
import { TableBuilder, TableLayout, type OffsetType, type TableLayoutOptions } from 'tafla';
import { LAYOUTS } from './conformance/arbitraries.ts';
import { headerSizeOf, OFFSET_SIZES } from './conformance/format.ts';

describe('TableLayout', () => {
    it('should default to no header slots and uint32 offsets', () => {
        for (const layout of [TableLayout.DEFAULT, TableLayout.create()]) {
            expect(layout.sized()).toBe(false);
            expect(layout.versioned()).toBe(false);
            expect(layout.version()).toBeUndefined();
            expect(layout.offsetType()).toBe('uint32');
            expect(layout.offsetSize()).toBe(4);
            expect(layout.headerSize()).toBe(0);
        }
    });

    it.prop([LAYOUTS])(
        'should have a header slot of the offset size for the length and for the version',
        ({ sized, versioned, offsetType }) => {
            const version = versioned ? 1 : undefined;
            const layout = TableLayout.create({ sized, version, offsetType });

            expect(layout.sized()).toBe(sized);
            expect(layout.versioned()).toBe(versioned);
            expect(layout.version()).toBe(version);
            expect(layout.offsetType()).toBe(offsetType);
            expect(layout.offsetSize()).toBe(OFFSET_SIZES[offsetType]);
            expect(layout.headerSize()).toBe(headerSizeOf({ sized, versioned, offsetType }));
        }
    );

    it.each(['int16', 'uint64', 'UINT8', '', 'toString'])(
        "should reject '%s' as an offset type",
        offsetType => {
            const create = (): TableLayout =>
                TableLayout.create({ offsetType: offsetType as OffsetType });

            expect(create).toThrow(RangeError);
            expect(create).toThrow(new RangeError(`${offsetType} isn't an offset type`));
        }
    );

    // Each converts to an offset type's name, as a property key or a string
    it.each([
        ['an offset size', 2, 'number'],
        ['an array of a name', ['uint8'], 'object'],
        // oxlint-disable-next-line eslint/no-new-wrappers, unicorn/new-for-builtins -- The object is the input
        ['a String object', new String('uint16'), 'object'],
        ['an object that converts to a name', { toString: () => 'uint32' }, 'object'],
        ['null', null, 'object']
    ])('should reject %s as an offset type', (_, offsetType, type) => {
        const create = (): TableLayout =>
            TableLayout.create({ offsetType: offsetType as OffsetType });

        expect(create).toThrow(RangeError);
        expect(create).toThrow(
            new RangeError(
                `An offset type must be a string such as 'uint16', not a value of type ${type}`
            )
        );
    });

    it('should take a null version, as JavaScript may give, as a missing one', () => {
        const layout = TableLayout.create({ sized: true, version: null as unknown as undefined });

        expect(layout.versioned()).toBe(false);
        expect(layout.version()).toBeUndefined();
        expect(layout.headerSize()).toBe(4);
    });

    it('should give a layout without a version the slot for one, from withVersion()', () => {
        const layout = TableLayout.create({ sized: true, offsetType: 'uint16' });
        const versioned = layout.withVersion(1);

        expect(layout.versioned()).toBe(false);
        expect(versioned.version()).toBe(1);
        expect(versioned.headerSize()).toBe(layout.headerSize() + 2);
        // The length, and then the version
        expect(new TableBuilder(0, versioned).bytes()).toStrictEqual(new Uint8Array([4, 0, 1, 0]));
    });

    // An option that isn't read would leave its default in its place
    it.each([
        ['the offset size', { offsetSize: 2 }, 'offsetSize'],
        ['the offset size beside the type', { offsetType: 'uint16', offsetSize: 2 }, 'offsetSize'],
        ['a misspelled option', { version: 1, Sized: true }, 'Sized'],
        ['the option that version replaced', { versioned: true }, 'versioned'],
        ['an option set to undefined', { size: undefined }, 'size']
    ])('should reject %s as an option', (_, options, name) => {
        const create = (): TableLayout => TableLayout.create(options as TableLayoutOptions);

        expect(create).toThrow(TypeError);
        expect(create).toThrow(
            new TypeError(`TableLayout.create() takes sized, version and offsetType, not ${name}`)
        );
    });
});
