import { getFloat32Columns } from '@tafla/native';
import { TableReader } from 'tafla';

/**
 * The paths of an item's three float32s, in the vector of nested tables:
 * its value, and the two fields of its pair, a nested table
 */
const VALUE = Uint32Array.of(0);
const FIRST = Uint32Array.of(1, 0);
const SECOND = Uint32Array.of(1, 1);

/** A column for each of the items' float32s */
export interface ItemColumns {
    value: Float32Array;
    first: Float32Array;
    second: Float32Array;
}

export function createColumns(items: number): ItemColumns {
    return {
        value: new Float32Array(items),
        first: new Float32Array(items),
        second: new Float32Array(items)
    };
}

/**
 * Sums every item of the vector of nested tables, as `ItemAccess`'s
 * `everyTable()` does, from columns that the addon reads in one call,
 * into columns created in each call, as a caller that keeps them would
 */
export function sumNewColumns(bytes: Uint8Array): number {
    const reader = new TableReader(bytes);
    const vector = reader.rootOffset(0);

    return sumColumns(
        bytes,
        vector,
        createColumns(reader.getVectorLength(vector))
    );
}

/**
 * Returns a function that sums every item as `sumNewColumns()` does, from
 * columns for `capacity` items that it keeps between calls
 */
export function createReusedColumnsSum(
    capacity: number
): (bytes: Uint8Array) => number {
    const columns = createColumns(capacity);

    return bytes => {
        const vector = new TableReader(bytes).rootOffset(0);

        return sumColumns(bytes, vector, columns);
    };
}

function sumColumns(
    bytes: Uint8Array,
    vector: number,
    { value, first, second }: ItemColumns
): number {
    const items = getFloat32Columns(
        bytes,
        vector,
        VALUE,
        value,
        FIRST,
        first,
        SECOND,
        second
    );

    let sum = 0;
    for (let i = 0; i < items; i++) {
        sum += value[i] + first[i] + second[i];
    }

    return sum;
}
