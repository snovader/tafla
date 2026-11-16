import { SIZEOF_8, SIZEOF_16, SIZEOF_32, SIZEOF_64 } from './constants.ts';
import { toUint8Array } from './converters.ts';
import type { TypedArrayConstructor } from './types.ts';

/**
 * Copies `length` elements from `byteOffset` in `buffer` into a new array,
 * reversing the bytes of each element. Unlike a view, the elements don't need
 * to be aligned.
 */
export function byteOrderSwapped<T extends TypedArrayConstructor>(
    TypedArray: T,
    buffer: ArrayBufferLike,
    byteOffset: number,
    length: number
): InstanceType<T> {
    const elementSize = TypedArray.BYTES_PER_ELEMENT;
    const source = new Uint8Array(buffer, byteOffset, length * elementSize);
    const swapped = new TypedArray(length);

    byteOrderSwap(source, toUint8Array(swapped), elementSize);

    return swapped;
}

/**
 * Copies `source` into `destination`, reversing the bytes of each element of
 * `elementSize` bytes.
 *
 * @throws {Error} If `elementSize` isn't 1, 2, 4 or 8.
 */
export function byteOrderSwap(
    source: Uint8Array,
    destination: Uint8Array,
    elementSize: number
): void {
    switch (elementSize) {
        case SIZEOF_8:
            destination.set(source);
            break;

        case SIZEOF_16:
            swap2(source, destination);
            break;

        case SIZEOF_32:
            swap4(source, destination);
            break;

        case SIZEOF_64:
            swap8(source, destination);
            break;

        default:
            throw new Error(`Unsupported "BYTES_PER_ELEMENT" ${elementSize}`);
    }
}

function swap2(input: Uint8Array, output: Uint8Array): void {
    const swap = crateSwapper(input, output);

    for (let i = 0; i < input.length; i += SIZEOF_16) {
        swap(i, i + 1);
    }
}

function swap4(input: Uint8Array, output: Uint8Array): void {
    const swap = crateSwapper(input, output);

    for (let i = 0; i < input.length; i += SIZEOF_32) {
        swap(i, i + 3);
        swap(i + 1, i + 2);
    }
}

function swap8(input: Uint8Array, output: Uint8Array): void {
    const swap = crateSwapper(input, output);

    for (let i = 0; i < input.length; i += SIZEOF_64) {
        swap(i, i + 7);
        swap(i + 1, i + 6);
        swap(i + 2, i + 5);
        swap(i + 3, i + 4);
    }
}

function crateSwapper(
    input: Uint8Array,
    output: Uint8Array
): (i: number, j: number) => void {
    return (i, j) => {
        const temp = input[i];
        output[i] = input[j];
        output[j] = temp;
    };
}
