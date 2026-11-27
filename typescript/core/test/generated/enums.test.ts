import { describe, expect } from 'hypotes';
import { it } from 'hypotes/fast-check';
import { TableReader } from 'tafla';
import {
    corruptionOf,
    sliceOf,
    CORRUPTIONS,
    SCALAR_VALUES,
    SLICES
} from '../conformance/arbitraries.ts';
import { corrupt } from '../conformance/description.ts';
import { buildTable } from '../conformance/tables.ts';
import { ALL_TYPES } from './all-types-tables.ts';
import {
    describeEnums,
    readEnums,
    readMark,
    ENUM_VALUES,
    ENUMS_FIELDS,
    MARK_FIELDS,
    MARK_SIZE
} from './enums-tables.ts';
import { expectValuesOrDecodeError } from './expectations.ts';

// The enums of schemas/all_types.tafla: names for some of the integers of a
// type, which the generated code stores as that type, so a field of an enum
// reads any integer of it, named or not

describe.each(ALL_TYPES)('%s', (_, allTypes) => {
    describe('values', () => {
        it.each(ENUM_VALUES)(
            'should hold the values of %s, of %s, that the schema names, as constants',
            (name, _type, values) => {
                expect(allTypes[name]).toStrictEqual(values);
                expect(Object.isFrozen(allTypes[name])).toBe(true);
            }
        );
    });

    describe('round trip', () => {
        it.prop([ENUMS_FIELDS, SLICES])(
            'should read back any integer of an enum, named or not, wherever its bytes lie',
            (fields, slice) => {
                const bytes = sliceOf(allTypes.Enums.serialize(fields), slice);

                expect(readEnums(new allTypes.Enums(bytes))).toEqual(fields);
            }
        );

        it.prop([MARK_FIELDS])('should read back a struct of enums', fields => {
            const bytes = allTypes.Mark.serialize(fields);

            expect(bytes).toHaveLength(MARK_SIZE);
            expect(readMark(new allTypes.Mark(bytes))).toEqual(fields);
        });

        it.prop([ENUMS_FIELDS])(
            'should read an array of enums in place, as an array of their integer type',
            fields => {
                const bytes = allTypes.Enums.serialize(fields);
                const enums = new allTypes.Enums(bytes);

                expect(enums.units()).toBeInstanceOf(Uint16Array);
                expect(enums.units().buffer).toBe(bytes.buffer);
                expect(enums.masks()).toBeInstanceOf(BigUint64Array);
                expect(enums.masks().buffer).toBe(bytes.buffer);
            }
        );

        it.prop([ENUMS_FIELDS, SCALAR_VALUES.uint8])(
            'should read any byte of an enum of uint8 as its value',
            (fields, byte) => {
                const bytes = allTypes.Enums.serialize(fields);
                bytes[new TableReader(bytes).rootOffset(0)] = byte;

                expect(new allTypes.Enums(bytes).level()).toBe(byte);
            }
        );

        it('should write the values that an enum names', () => {
            const { Level, Direction, Mask, Extent } = allTypes;
            const mark = {
                level: Level.High,
                change: allTypes.Change.Least,
                weight: 0.5,
                extent: Extent.Greatest,
                turn: { direction: Direction.Backward, mask: Mask.BeyondFloat64 }
            };

            const read = readMark(new allTypes.Mark(allTypes.Mark.serialize(mark)));

            expect(read).toStrictEqual({
                level: 2,
                change: -32_768,
                weight: 0.5,
                extent: 2n ** 63n - 1n,
                turn: { direction: -1, mask: 9_007_199_254_740_993n }
            });
        });
    });

    describe('tables built by hand', () => {
        it.prop([ENUMS_FIELDS])(
            'should write the bytes of Enums that TableBuilder writes of their integer types',
            fields => {
                const byHand = buildTable(describeEnums(fields)).bytes();

                expect(allTypes.Enums.serialize(fields)).toStrictEqual(byHand);
            }
        );
    });

    describe('decode errors', () => {
        it.prop([ENUMS_FIELDS, CORRUPTIONS])(
            'should read corrupted Enums as values or throw DecodeError',
            (fields, corruption) => {
                const bytes = allTypes.Enums.serialize(fields);
                const corrupted = corrupt(bytes, corruptionOf(corruption, bytes.length));

                expectValuesOrDecodeError(() => readEnums(new allTypes.Enums(corrupted)));
            }
        );
    });
});
