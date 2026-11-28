import type { TableReader } from './table-reader.ts';

/**
 * A vector of strings in a table, which reads each string when it's asked
 * for, from its end and the end of the string before it. The code that
 * tafla-gen generates returns one from the getter of a `[string]` field.
 */
export class StringVector {
    declare private readonly _reader: TableReader;
    declare private readonly _position: number;
    declare private readonly _length: number;

    /**
     * Views the vector of strings at `position`, as `rootOffset()` or
     * `fieldOffset()` finds it.
     *
     * @throws {DecodeError} If the vector's count or the ends of its strings
     * lie outside the bytes.
     */
    constructor(reader: TableReader, position: number) {
        this._reader = reader;
        this._position = position;
        this._length = reader.getVectorLength(position);
    }

    /** The number of strings */
    length(): number {
        return this._length;
    }

    /**
     * Reads string `index`, without reading the others.
     *
     * @throws {RangeError} If the vector has no string `index`.
     * @throws {DecodeError} If the string lies outside the bytes, or isn't
     * valid UTF-8.
     */
    get(index: number): string {
        // The count that the constructor checked, rather than reading and
        // checking it for each string
        return this._reader.getStringVectorItem(
            this._position,
            this._length,
            index
        );
    }

    /**
     * Reads every string, from the first to the last.
     *
     * @throws {DecodeError} If a string lies outside the bytes, or isn't
     * valid UTF-8.
     */
    toArray(): string[] {
        return this._reader.getStringVector(this._position);
    }
}
