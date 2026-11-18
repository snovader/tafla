import { readItemListItems, writeItemListItems } from '@tafla/native/workloads';
import { TableReader } from 'tafla';
import type { ItemInput, ListInput } from '../comparison/workloads.ts';
import { createColumns, type ItemColumns } from './itemColumns.ts';

/**
 * Reads and writes the vector of nested tables with the addon's functions of
 * the schema, which take a column of each of an item's fields. It keeps its
 * columns and the bytes that it writes into between calls, for lists of up
 * to `capacity` items, and returns objects and bytes of the caller's own.
 */
export class GeneratedItems {
    readonly #columns: ItemColumns;

    /** Bytes that are long enough for the longest table written so far */
    #target = new Uint8Array(0);

    constructor(capacity: number) {
        this.#columns = createColumns(capacity);
    }

    /** Sums every item, as `ItemAccess`'s `everyTable()` does */
    sum(bytes: Uint8Array): number {
        return sumColumns(bytes, this.#columns);
    }

    /** Reads the items as the workload's objects */
    deserialize(bytes: Uint8Array): ListInput {
        const { value, first, second } = this.#columns;
        const count = readItemListItems(bytes, value, first, second);

        const items = new Array<ItemInput>(count);
        for (let i = 0; i < count; i++) {
            items[i] = { value: value[i], first: first[i], second: second[i] };
        }

        return { items };
    }

    /** Writes the workload's objects, through the columns */
    serialize({ items }: ListInput): Uint8Array {
        const { value, first, second } = this.#columns;

        for (let i = 0; i < items.length; i++) {
            const item = items[i];
            value[i] = item.value;
            first[i] = item.first;
            second[i] = item.second;
        }

        return this.serializeColumns(items.length, this.#columns);
    }

    /** Writes the first `count` items of columns that the caller holds */
    serializeColumns(
        count: number,
        { value, first, second }: ItemColumns
    ): Uint8Array {
        let length = writeItemListItems(
            count,
            value,
            first,
            second,
            this.#target
        );

        if (length > this.#target.length) {
            this.#target = new Uint8Array(length);
            length = writeItemListItems(
                count,
                value,
                first,
                second,
                this.#target
            );
        }

        return this.#target.slice(0, length);
    }
}

/**
 * Sums every item from columns created in the call, as a caller that keeps
 * them would create them
 */
export function sumNewGeneratedColumns(bytes: Uint8Array): number {
    const reader = new TableReader(bytes);
    const items = reader.getVectorLength(reader.rootOffset(0));

    return sumColumns(bytes, createColumns(items));
}

function sumColumns(
    bytes: Uint8Array,
    { value, first, second }: ItemColumns
): number {
    const items = readItemListItems(bytes, value, first, second);

    let sum = 0;
    for (let i = 0; i < items; i++) {
        sum += value[i] + first[i] + second[i];
    }

    return sum;
}
