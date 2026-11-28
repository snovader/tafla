import { SIZEOF_8, SIZEOF_16, SIZEOF_32, SIZEOF_64 } from './constants.ts';

/**
 * Writes scalars one after another into the supplied bytes, without a header,
 * offsets, length prefixes or padding, so a struct is read back at fixed
 * offsets. `bytes()` rewinds and returns a copy, so one writer can be reused for
 * every struct with the same layout. A struct whose write threw never reaches
 * `bytes()`, so a shared writer calls `reset()` before each struct. A struct
 * written while another is, such as from a getter of one of its values,
 * mustn't share its writer, whose `reset()` would write over the other.
 *
 * A scalar that doesn't fit in the bytes throws a `RangeError` and writes
 * nothing, as `DataView`'s setters throw.
 */
export class StructWriter {
    declare private readonly _bytes: Uint8Array;
    declare private readonly _view: DataView;

    private _position = 0;

    constructor(bytes: Uint8Array) {
        this._bytes = bytes;
        this._view = new DataView(
            bytes.buffer,
            bytes.byteOffset,
            bytes.byteLength
        );
    }

    /** Rewinds to the start, discarding a struct that wasn't finished */
    reset(): void {
        this._position = 0;
    }

    bytes(): Uint8Array {
        this._position = 0;
        return this._bytes.slice();
    }

    setInt8(value: number): void {
        this._view.setInt8(this._position, value);
        this._position += SIZEOF_8;
    }

    setUint8(value: number): void {
        this._view.setUint8(this._position, value);
        this._position += SIZEOF_8;
    }

    setInt16(value: number): void {
        this._view.setInt16(this._position, value, true);
        this._position += SIZEOF_16;
    }

    setUint16(value: number): void {
        this._view.setUint16(this._position, value, true);
        this._position += SIZEOF_16;
    }

    setInt32(value: number): void {
        this._view.setInt32(this._position, value, true);
        this._position += SIZEOF_32;
    }

    setUint32(value: number): void {
        this._view.setUint32(this._position, value, true);
        this._position += SIZEOF_32;
    }

    setInt64(value: bigint): void {
        this._view.setBigInt64(this._position, value, true);
        this._position += SIZEOF_64;
    }

    setUint64(value: bigint): void {
        this._view.setBigUint64(this._position, value, true);
        this._position += SIZEOF_64;
    }

    setFloat32(value: number): void {
        this._view.setFloat32(this._position, value, true);
        this._position += SIZEOF_32;
    }

    setFloat64(value: number): void {
        this._view.setFloat64(this._position, value, true);
        this._position += SIZEOF_64;
    }
}
