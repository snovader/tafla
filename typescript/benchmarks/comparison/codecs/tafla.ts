import {
    readFloat32,
    readUint32,
    SIZEOF_32,
    StructWriter,
    TableArena,
    TableBuilder,
    TableLayout,
    TableReader,
    type TableItem
} from 'tafla';
import type { DocumentOutput, ItemInput } from '../workloads.ts';
import type { Codec } from './codec.ts';

const DOCUMENT_VERSION = 1;
const DOCUMENT_LAYOUT = TableLayout.create({
    sized: true,
    version: DOCUMENT_VERSION
});

const SAMPLE_SIZE = 3 * SIZEOF_32;
const sampleWriter = new StructWriter(new Uint8Array(SAMPLE_SIZE));

/** An item as a struct: its three float32s, aligned to 4 in an array */
const ITEM_SIZE = 3 * SIZEOF_32;
const ITEM_ALIGNMENT = SIZEOF_32;

export const tafla: Codec = {
    name: 'tafla',
    record: {
        serialize(input) {
            const builder = new TableBuilder(TableArena.DEFAULT, 4);
            builder.setFloat64(input.x);
            builder.setFloat64(input.y);
            builder.setInt64(input.id);
            builder.setString(input.name);

            return builder.bytes();
        },
        deserialize(bytes) {
            const reader = new TableReader(bytes);

            return {
                x: reader.getFloat64(reader.rootOffset(0)),
                y: reader.getFloat64(reader.rootOffset(1)),
                id: reader.getInt64(reader.rootOffset(2)),
                name: reader.getString(reader.rootOffset(3))
            };
        }
    },
    document: {
        serialize(input) {
            const document = new TableBuilder(
                TableArena.DEFAULT,
                4,
                DOCUMENT_LAYOUT
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
            const reader = openDocument(bytes);
            const columns = reader.rootOffset(3);

            return {
                x: reader.getFloat64(reader.rootOffset(0)),
                y: reader.getFloat64(reader.rootOffset(1)),
                name: reader.getString(reader.rootOffset(2)),
                base: reader.getInt64(reader.fieldOffset(columns, 0)),
                keys: reader.getUint32Array(reader.fieldOffset(columns, 1)),
                a: reader.getFloat32Array(reader.fieldOffset(columns, 2)),
                b: reader.getFloat32Array(reader.fieldOffset(columns, 3))
            };
        },
        sumA(bytes) {
            const reader = openDocument(bytes);
            const a = reader.getFloat32Array(
                reader.fieldOffset(reader.rootOffset(3), 2)
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
            const builder = new TableBuilder(TableArena.DEFAULT, 1);
            builder.setVectorInPlace(items.length, 2, (item, index) => {
                setItem(item, items[index]);
            });

            return builder.bytes();
        },
        deserialize(bytes) {
            const reader = new TableReader(bytes);
            const offsets = reader.getVectorOffsets(reader.rootOffset(0));

            const items = new Array<ItemInput>(offsets.length);
            for (let i = 0; i < offsets.length; i++) {
                const item = offsets[i];
                const pair = reader.fieldOffset(item, 1);

                items[i] = {
                    value: reader.getFloat32(reader.fieldOffset(item, 0)),
                    first: reader.getFloat32(reader.fieldOffset(pair, 0)),
                    second: reader.getFloat32(reader.fieldOffset(pair, 1))
                };
            }

            return { items };
        }
    },
    structs: {
        serialize({ items }) {
            // tafla-gen's code writes the structs straight into the table,
            // with TableWriter.reserveStructArray(). TableBuilder takes their
            // bytes, so StructWriter writes them first.
            const structs = new Uint8Array(items.length * ITEM_SIZE);
            const writer = new StructWriter(structs);
            for (const { value, first, second } of items) {
                writer.setFloat32(value);
                writer.setFloat32(first);
                writer.setFloat32(second);
            }

            const builder = new TableBuilder(TableArena.DEFAULT, 1);
            builder.setStructArray(structs, ITEM_SIZE, ITEM_ALIGNMENT);

            return builder.bytes();
        },
        deserialize(bytes) {
            const reader = new TableReader(bytes);
            const structs = reader.getStructArray(
                reader.rootOffset(0),
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
function openDocument(bytes: Uint8Array): TableReader {
    const reader = new TableReader(bytes, DOCUMENT_LAYOUT);
    if (reader.getVersion() !== DOCUMENT_VERSION) {
        throw new Error(`Unsupported document version ${reader.getVersion()}`);
    }

    return reader;
}

function setItem(item: TableItem, { value, first, second }: ItemInput): void {
    item.setFloat32(value);
    item.setTableInPlace(2, pair => {
        pair.setFloat32(first);
        pair.setFloat32(second);
    });
}
