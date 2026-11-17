import { describe, it } from 'node:test';
import { TableLayout, type OffsetType } from 'tafla';
import { LAYOUTS } from './conformance/arbitraries.ts';
import { expect } from './conformance/expect.ts';
import { headerSizeOf, OFFSET_SIZES } from './conformance/format.ts';
import { prop } from './conformance/property.ts';

describe('TableLayout', () => {
    it('should default to no header slots and uint32 offsets', () => {
        for (const layout of [TableLayout.DEFAULT, TableLayout.create()]) {
            expect(layout.sized()).toBe(false);
            expect(layout.versioned()).toBe(false);
            expect(layout.offsetType()).toBe('uint32');
            expect(layout.offsetSize()).toBe(4);
            expect(layout.headerSize()).toBe(0);
        }
    });

    prop([LAYOUTS])(
        'should have a header slot of the offset size for the length and for the version',
        ({ sized, versioned, offsetType }) => {
            const layout = TableLayout.create({ sized, versioned, offsetType });

            expect(layout.sized()).toBe(sized);
            expect(layout.versioned()).toBe(versioned);
            expect(layout.offsetType()).toBe(offsetType);
            expect(layout.offsetSize()).toBe(OFFSET_SIZES[offsetType]);
            expect(layout.headerSize()).toBe(headerSizeOf({ sized, versioned, offsetType }));
        }
    );

    for (const offsetType of ['int16', 'uint64', 'UINT8', '', 'toString']) {
        it(`should reject '${offsetType}' as an offset type`, () => {
            const create = (): TableLayout =>
                TableLayout.create({ offsetType: offsetType as OffsetType });

            expect(create).toThrow(RangeError);
            expect(create).toThrow(new RangeError(`${offsetType} isn't an offset type`));
        });
    }

    // Each converts to an offset type's name, as a property key or a string
    for (const [description, offsetType, type] of [
        ['an offset size', 2, 'number'],
        ['an array of a name', ['uint8'], 'object'],
        // oxlint-disable-next-line eslint/no-new-wrappers, unicorn/new-for-builtins -- The object is the input
        ['a String object', new String('uint16'), 'object'],
        ['an object that converts to a name', { toString: () => 'uint32' }, 'object'],
        ['null', null, 'object']
    ] as const) {
        it(`should reject ${description} as an offset type`, () => {
            const create = (): TableLayout =>
                TableLayout.create({ offsetType: offsetType as OffsetType });

            expect(create).toThrow(RangeError);
            expect(create).toThrow(
                new RangeError(
                    `An offset type must be a string such as 'uint16', not a value of type ${type}`
                )
            );
        });
    }

    // An option that isn't read would leave its default in its place
    for (const [description, options, name] of [
        ['the offset size', { offsetSize: 2 }, 'offsetSize'],
        ['the offset size beside the type', { offsetType: 'uint16', offsetSize: 2 }, 'offsetSize'],
        ['a misspelled option', { versioned: true, Sized: true }, 'Sized'],
        ['an option set to undefined', { size: undefined }, 'size']
    ] as Array<[description: string, options: object, name: string]>) {
        it(`should reject ${description} as an option`, () => {
            const create = (): TableLayout => TableLayout.create(options);

            expect(create).toThrow(TypeError);
            expect(create).toThrow(
                new TypeError(
                    `TableLayout.create() takes sized, versioned and offsetType, not ${name}`
                )
            );
        });
    }
});
