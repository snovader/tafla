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
    // A DataView of its own rather than a ByteBuffer's setters, which made
    // bundlers keep all of ByteBuffer, its readers included: StructWriter
    // alone took 2.8 KB minified and gzipped with esbuild, and takes 0.3 KB
    readonly #bytes: Uint8Array;
    readonly #view: DataView;

    #position = 0;

    constructor(bytes: Uint8Array) {
        this.#bytes = bytes;
        this.#view = new DataView(
            bytes.buffer,
            bytes.byteOffset,
            bytes.byteLength
        );
    }

    /** Rewinds to the start, discarding a struct that wasn't finished */
    reset(): void {
        this.#position = 0;
    }

    bytes(): Uint8Array {
        this.#position = 0;
        return this.#bytes.slice();
    }

    setInt8(value: number): void {
        this.#view.setInt8(this.#position, value);
        this.#position += SIZEOF_8;
    }

    setUint8(value: number): void {
        this.#view.setUint8(this.#position, value);
        this.#position += SIZEOF_8;
    }

    setInt16(value: number): void {
        this.#view.setInt16(this.#position, value, true);
        this.#position += SIZEOF_16;
    }

    setUint16(value: number): void {
        this.#view.setUint16(this.#position, value, true);
        this.#position += SIZEOF_16;
    }

    setInt32(value: number): void {
        this.#view.setInt32(this.#position, value, true);
        this.#position += SIZEOF_32;
    }

    setUint32(value: number): void {
        this.#view.setUint32(this.#position, value, true);
        this.#position += SIZEOF_32;
    }

    setInt64(value: bigint): void {
        this.#view.setBigInt64(this.#position, value, true);
        this.#position += SIZEOF_64;
    }

    setUint64(value: bigint): void {
        this.#view.setBigUint64(this.#position, value, true);
        this.#position += SIZEOF_64;
    }

    setFloat32(value: number): void {
        this.#view.setFloat32(this.#position, value, true);
        this.#position += SIZEOF_32;
    }

    setFloat64(value: number): void {
        this.#view.setFloat64(this.#position, value, true);
        this.#position += SIZEOF_64;
    }
}
