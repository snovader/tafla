import { ByteBuffer, readFloat32, SIZEOF_32 } from 'tafla';
import { ItemList, Triples } from './generated/tafla/workloads.ts';
import type { ItemInput, ListInput } from './workloads.ts';

/**
 * A way to read the items of tafla's vector of nested tables and of its
 * array of structs, which tafla and tafla-gen write alike: the last item
 * alone, which a reader can find without reading the others, and every item.
 * Each sums the three float32s of the items it reads, so that it returns a
 * number rather than creating objects of its own.
 */
export interface ItemAccess {
    name: string;
    lastTable(bytes: Uint8Array): number;
    everyTable(bytes: Uint8Array): number;
    lastStruct(bytes: Uint8Array): number;
    everyStruct(bytes: Uint8Array): number;
}

/** The bytes of the list as a vector of nested tables and as an array of structs */
export interface ItemBytes {
    tables: Uint8Array;
    structs: Uint8Array;
}

export function writeItems({ items }: ListInput): ItemBytes {
    const tables = ItemList.serialize({
        items: items.map(({ value, first, second }) => ({
            value,
            pair: { first, second }
        }))
    });

    return { tables, structs: Triples.serialize({ items }) };
}

/** The sum of an item's three float32s, in the order that every access adds them */
export function sumItem({ value, first, second }: ItemInput): number {
    return value + first + second;
}

/** An item as a struct: its three float32s */
const ITEM_SIZE = 3 * SIZEOF_32;

/** `ByteBuffer`'s getters by hand, as the tafla codec reads the list */
export const byteBufferAccess: ItemAccess = {
    name: 'tafla',
    lastTable(bytes) {
        const buffer = new ByteBuffer(bytes);
        const vector = buffer.rootOffset(0);
        const item = buffer.getVectorOffset(
            vector,
            buffer.getVectorLength(vector) - 1
        );

        return sumTable(buffer, item);
    },
    everyTable(bytes) {
        const buffer = new ByteBuffer(bytes);
        const offsets = buffer.getVectorOffsets(buffer.rootOffset(0));

        // The reads are in the loop, as the codec's deserialize() has them:
        // V8 didn't inline sumTable() here, which then boxed each item's sum
        let sum = 0;
        for (let i = 0; i < offsets.length; i++) {
            const item = offsets[i];
            const pair = buffer.fieldOffset(item, 1);
            sum +=
                buffer.getFloat32(buffer.fieldOffset(item, 0)) +
                buffer.getFloat32(buffer.fieldOffset(pair, 0)) +
                buffer.getFloat32(buffer.fieldOffset(pair, 1));
        }

        return sum;
    },
    lastStruct(bytes) {
        const buffer = new ByteBuffer(bytes);
        const structs = buffer.getStructArray(buffer.rootOffset(0), ITEM_SIZE);

        return sumStruct(structs, structs.length - ITEM_SIZE);
    },
    everyStruct(bytes) {
        const buffer = new ByteBuffer(bytes);
        const structs = buffer.getStructArray(buffer.rootOffset(0), ITEM_SIZE);

        let sum = 0;
        for (let offset = 0; offset < structs.length; offset += ITEM_SIZE) {
            sum += sumStruct(structs, offset);
        }

        return sum;
    }
};

function sumTable(buffer: ByteBuffer, item: number): number {
    const pair = buffer.fieldOffset(item, 1);

    return (
        buffer.getFloat32(buffer.fieldOffset(item, 0)) +
        buffer.getFloat32(buffer.fieldOffset(pair, 0)) +
        buffer.getFloat32(buffer.fieldOffset(pair, 1))
    );
}

function sumStruct(structs: Uint8Array, offset: number): number {
    return (
        readFloat32(structs, offset) +
        readFloat32(structs, offset + SIZEOF_32) +
        readFloat32(structs, offset + 2 * SIZEOF_32)
    );
}

/**
 * tafla-gen's getters, which return views of the items, whose `get(i)` reads
 * one item, in a loop of `get(i)` for every item
 */
export const getterAccess: ItemAccess = {
    name: 'tafla-gen',
    lastTable(bytes) {
        const items = new ItemList(bytes).items();
        const item = items.get(items.length() - 1);
        const pair = item.pair();

        return item.value() + pair.first() + pair.second();
    },
    everyTable(bytes) {
        const items = new ItemList(bytes).items();

        let sum = 0;
        for (let i = 0; i < items.length(); i++) {
            const item = items.get(i);
            const pair = item.pair();
            sum += item.value() + pair.first() + pair.second();
        }

        return sum;
    },
    // Through the array's getter of each field, which reads it without
    // creating the struct
    lastStruct(bytes) {
        const triples = new Triples(bytes).items();
        const last = triples.length() - 1;

        return triples.value(last) + triples.first(last) + triples.second(last);
    },
    everyStruct(bytes) {
        const triples = new Triples(bytes).items();

        let sum = 0;
        for (let i = 0; i < triples.length(); i++) {
            sum += triples.value(i) + triples.first(i) + triples.second(i);
        }

        return sum;
    }
};
