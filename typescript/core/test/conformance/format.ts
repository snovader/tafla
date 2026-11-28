import {
    toJsonNumber,
    type JsonNumber,
    type LayoutDescription,
    type NestedTable,
    type OffsetSize,
    type OffsetType,
    type ScalarType,
    type TableDescription
} from './description.ts';

// Reads the bytes of tables with DataView, as the README sets out the format,
// rather than with TableReader, so that tests can check the bytes that tafla
// writes without relying on its reader.

/** The offset size of each offset type */
export const OFFSET_SIZES: Readonly<Record<OffsetType, OffsetSize>> = {
    uint8: 1,
    uint16: 2,
    uint32: 4
};

/** The byte length of a layout's header slots */
export function headerSizeOf({
    sized,
    versioned,
    offsetType
}: LayoutDescription): number {
    return (Number(sized) + Number(versioned)) * OFFSET_SIZES[offsetType];
}

/**
 * Reads a header slot, offset, length or count, which are little-endian
 *
 * @throws {RangeError} If it doesn't lie within `bytes`.
 */
export function readUnsigned(
    bytes: Uint8Array,
    position: number,
    size: OffsetSize
): number {
    const view = viewOf(bytes);

    switch (size) {
        case 1:
            return view.getUint8(position);

        case 2:
            return view.getUint16(position, true);

        case 4:
            return view.getUint32(position, true);
    }
}

/**
 * Reads a scalar as descriptions hold it, little-endian unless
 * `littleEndian` is false
 *
 * @throws {RangeError} If it doesn't lie within `bytes`.
 */
export function readScalar(
    bytes: Uint8Array,
    type: ScalarType,
    position: number,
    littleEndian = true
): JsonNumber {
    const view = viewOf(bytes);

    switch (type) {
        case 'int8':
            return view.getInt8(position);

        case 'uint8':
            return view.getUint8(position);

        case 'int16':
            return view.getInt16(position, littleEndian);

        case 'uint16':
            return view.getUint16(position, littleEndian);

        case 'int32':
            return view.getInt32(position, littleEndian);

        case 'uint32':
            return view.getUint32(position, littleEndian);

        case 'int64':
            return String(view.getBigInt64(position, littleEndian));

        case 'uint64':
            return String(view.getBigUint64(position, littleEndian));

        case 'float32':
            return toJsonNumber(view.getFloat32(position, littleEndian));

        case 'float64':
            return toJsonNumber(view.getFloat64(position, littleEndian));
    }
}

function viewOf(bytes: Uint8Array): DataView {
    return new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
}

/** A table in the bytes of another, or the root itself */
export interface TableAt {
    table: TableDescription;
    /**
     * The bytes from the table's start. They end with the table when it's
     * the root or a vector item, and otherwise with its parent, since a
     * table nested with `setTable()` has no length.
     */
    bytes: Uint8Array;
    /** The offset of each field from the table's start */
    offsets: number[];
}

/**
 * The tables in the bytes of a table built from its description: the root,
 * and every table nested in it, depth first
 *
 * @throws {RangeError} If an offset, length or count lies outside them.
 */
export function* tablesIn(
    bytes: Uint8Array,
    table: TableDescription
): Generator<TableAt> {
    const { layout, fields } = table;
    const headerSize = headerSizeOf(layout);
    const offsetSize = OFFSET_SIZES[layout.offsetType];
    const offsets = fields.map((_, index) =>
        readUnsigned(bytes, headerSize + index * offsetSize, offsetSize)
    );

    yield { table, bytes, offsets };

    for (const [index, field] of fields.entries()) {
        if (field.type === 'table') {
            yield* tablesIn(bytes.subarray(offsets[index]), field.table);
        } else if (field.type === 'vector') {
            yield* itemsIn(bytes, offsets[index], offsetSize, field.items);
        }
    }
}

/**
 * The tables in a vector's items, each from the end of the item before it, or
 * of the ends, to its own. The vector's count isn't checked.
 *
 * @throws {RangeError} If an item ends before it starts or past the end of
 * `bytes`.
 */
function* itemsIn(
    bytes: Uint8Array,
    position: number,
    offsetSize: OffsetSize,
    items: NestedTable[]
): Generator<TableAt> {
    let start = (items.length + 1) * offsetSize;

    for (const [index, { table }] of items.entries()) {
        const end = readUnsigned(
            bytes,
            position + (index + 1) * offsetSize,
            offsetSize
        );
        if (start > end || position + end > bytes.length) {
            throw new RangeError(
                `A vector item from ${position + start} to ${position + end} doesn't lie within ${bytes.length} bytes`
            );
        }

        yield* tablesIn(
            bytes.subarray(position + start, position + end),
            table
        );
        start = end;
    }
}
