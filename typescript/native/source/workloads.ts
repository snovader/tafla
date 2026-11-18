import { addon } from './addon.ts';

/**
 * The functions of the benchmarks' schema, `workloads.tafla`, which the addon
 * has written by hand, as tafla-gen might generate them, to time them before
 * it does
 */
interface WorkloadsAddon {
    /**
     * Reads the fields of every item of the `ItemList` in `bytes` into the
     * columns, and returns the number of items.
     *
     * Throws an `Error` with the code `TAFLA_DECODE` if an item doesn't lie
     * within the bytes, and a `RangeError` if a column is shorter than the
     * list.
     */
    readItemListItems: (
        bytes: Uint8Array,
        value: Float32Array,
        first: Float32Array,
        second: Float32Array
    ) => number;

    /**
     * Writes an `ItemList` of `count` items, from the first `count` elements
     * of the columns, to the start of `target`, and returns the table's
     * length. Writes nothing if `target` is shorter than that.
     *
     * Throws a `RangeError` if a column holds fewer than `count` elements,
     * and an `Error` with the code `TAFLA_BUILD` if the table is too long
     * for its offset type.
     */
    writeItemListItems: (
        count: number,
        value: Float32Array,
        first: Float32Array,
        second: Float32Array,
        target: Uint8Array
    ) => number;
}

export const { readItemListItems, writeItemListItems } =
    addon as WorkloadsAddon;
