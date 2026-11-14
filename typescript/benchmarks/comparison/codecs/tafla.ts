import {
    ByteBuffer,
    readFloat32,
    readUint32,
    SIZEOF_32,
    StructWriter,
    TableBuilder,
    type TableItem,
    TableLayout
} from 'tafla';
import type { DocumentOutput, ItemInput } from '../workloads.ts';
import type { Codec } from './codec.ts';

const DOCUMENT_LAYOUT = TableLayout.create({ sized: true, versioned: true });
const DOCUMENT_VERSION = 1;

const SAMPLE_SIZE = 3 * SIZEOF_32;
const sampleWriter = new StructWriter(new Uint8Array(SAMPLE_SIZE));

/** An item as a struct: its three float32s, aligned to 4 in an array */
const ITEM_SIZE = 3 * SIZEOF_32;
const ITEM_ALIGNMENT = SIZEOF_32;

export const tafla: Codec = {
    name: 'tafla',
    record: {
        serialize(input) {
            const builder = new TableBuilder(4);
            builder.setFloat64(input.x);
            builder.setFloat64(input.y);
            builder.setInt64(input.id);
            builder.setString(input.name);

            return builder.bytes();
        },
        deserialize(bytes) {
            const buffer = new ByteBuffer(bytes);

            return {
                x: buffer.getFloat64(buffer.rootOffset(0)),
                y: buffer.getFloat64(buffer.rootOffset(1)),
                id: buffer.getInt64(buffer.rootOffset(2)),
                name: buffer.getString(buffer.rootOffset(3))
            };
        }
    },
    document: {
        serialize(input) {
            const document = new TableBuilder(
                4,
                DOCUMENT_LAYOUT,
                DOCUMENT_VERSION
            );
            document.setFloat64(input.x);
            document.setFloat64(input.y);
            document.setString(input.name);
            document.setTableInPlace(4, columns => {
                columns.setInt64(input.base);
                columns.setUint32Array(input.keys);
                columns.setFloat32Array(input.a);
                columns.setFloat32Array(input.b);
            });

            return document.bytes();
        },
        deserialize(bytes): DocumentOutput {
            const buffer = openDocument(bytes);
            const columns = buffer.rootOffset(3);

            return {
                x: buffer.getFloat64(buffer.rootOffset(0)),
                y: buffer.getFloat64(buffer.rootOffset(1)),
                name: buffer.getString(buffer.rootOffset(2)),
                base: buffer.getInt64(buffer.fieldOffset(columns, 0)),
                keys: buffer.getUint32Array(buffer.fieldOffset(columns, 1)),
                a: buffer.getFloat32Array(buffer.fieldOffset(columns, 2)),
                b: buffer.getFloat32Array(buffer.fieldOffset(columns, 3))
            };
        },
        sumA(bytes) {
            const buffer = openDocument(bytes);
            const a = buffer.getFloat32Array(
                buffer.fieldOffset(buffer.rootOffset(3), 2)
            );

            let sum = 0;
            for (let i = 0; i < a.length; i++) {
                sum += a[i];
            }

            return sum;
        }
    },
    list: {
        serialize({ items }) {
            const builder = new TableBuilder(1);
            builder.setVectorInPlace(items.length, 2, (item, index) => {
                setItem(item, items[index]);
            });

            return builder.bytes();
        },
        deserialize(bytes) {
            const buffer = new ByteBuffer(bytes);
            const offsets = buffer.getVectorOffsets(buffer.rootOffset(0));

            const items = new Array<ItemInput>(offsets.length);
            for (let i = 0; i < offsets.length; i++) {
                const item = offsets[i];
                const pair = buffer.fieldOffset(item, 1);

                items[i] = {
                    value: buffer.getFloat32(buffer.fieldOffset(item, 0)),
                    first: buffer.getFloat32(buffer.fieldOffset(pair, 0)),
                    second: buffer.getFloat32(buffer.fieldOffset(pair, 1))
                };
            }

            return { items };
        }
    },
    structs: {
        serialize({ items }) {
            // As tafla-gen's code writes an array of structs
            const structs = new Uint8Array(items.length * ITEM_SIZE);
            const writer = new StructWriter(structs);
            for (const { value, first, second } of items) {
                writer.setFloat32(value);
                writer.setFloat32(first);
                writer.setFloat32(second);
            }

            const builder = new TableBuilder(1);
            builder.setStructArray(structs, ITEM_SIZE, ITEM_ALIGNMENT);

            return builder.bytes();
        },
        deserialize(bytes) {
            const buffer = new ByteBuffer(bytes);
            const structs = buffer.getStructArray(
                buffer.rootOffset(0),
                ITEM_SIZE
            );

            const items = new Array<ItemInput>(structs.length / ITEM_SIZE);
            for (let i = 0; i < items.length; i++) {
                const offset = i * ITEM_SIZE;

                items[i] = {
                    value: readFloat32(structs, offset),
                    first: readFloat32(structs, offset + SIZEOF_32),
                    second: readFloat32(structs, offset + 2 * SIZEOF_32)
                };
            }

            return { items };
        }
    },
    sample: {
        serialize({ key, a, b }) {
            // As the generated code does, since the writer is shared
            sampleWriter.reset();
            sampleWriter.setUint32(key);
            sampleWriter.setFloat32(a);
            sampleWriter.setFloat32(b);

            return sampleWriter.bytes();
        },
        deserialize(bytes) {
            return {
                key: readUint32(bytes, 0),
                a: readFloat32(bytes, SIZEOF_32),
                b: readFloat32(bytes, 2 * SIZEOF_32)
            };
        }
    }
};

/** @throws {Error} If the document is in another version */
function openDocument(bytes: Uint8Array): ByteBuffer {
    const buffer = new ByteBuffer(bytes, DOCUMENT_LAYOUT);
    if (buffer.getVersion() !== DOCUMENT_VERSION) {
        throw new Error(`Unsupported document version ${buffer.getVersion()}`);
    }

    return buffer;
}

function setItem(item: TableItem, { value, first, second }: ItemInput): void {
    item.setFloat32(value);
    item.setTableInPlace(2, pair => {
        pair.setFloat32(first);
        pair.setFloat32(second);
    });
}
