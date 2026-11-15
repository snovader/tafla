// The builder calls of the comparison's hand-written codec
// (`../comparison/codecs/tafla.ts`), over a builder class that the
// prototype's benchmark replaces: `builders.ts` copies this module into
// each variant's copy of the runtime, with its imports pointed there.
import { DirectTableBuilder as Builder } from '../../core/prototype/directTableBuilder.ts';
import { SIZEOF_32 } from '../../core/source/constants.ts';
import { TableLayout } from '../../core/source/layout.ts';
import { StructWriter } from '../../core/source/structWriter.ts';
import type {
    DocumentInput,
    ItemInput,
    ListInput,
    RecordInput
} from '../comparison/workloads.ts';

/** The fields of a table that the builder writes in place */
type Item = Parameters<Parameters<Builder['setTableInPlace']>[1]>[0];

export interface BuilderCodec {
    record(input: RecordInput): Uint8Array;
    document(input: DocumentInput): Uint8Array;
    list(input: ListInput): Uint8Array;
    structs(input: ListInput): Uint8Array;
    /** The list's vector, from a builder of each item and its pair */
    listFromBuilders(input: ListInput): Uint8Array;
}

const DOCUMENT_LAYOUT = TableLayout.create({ sized: true, versioned: true });
const DOCUMENT_VERSION = 1;

/** An item as a struct: its three float32s, aligned to 4 in an array */
const ITEM_SIZE = 3 * SIZEOF_32;
const ITEM_ALIGNMENT = SIZEOF_32;

export const codec: BuilderCodec = {
    record(input) {
        const builder = new Builder(4);
        builder.setFloat64(input.x);
        builder.setFloat64(input.y);
        builder.setInt64(input.id);
        builder.setString(input.name);

        return builder.bytes();
    },
    document(input) {
        const document = new Builder(4, DOCUMENT_LAYOUT, DOCUMENT_VERSION);
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
    list({ items }) {
        const builder = new Builder(1);
        builder.setVectorInPlace(items.length, 2, (item, index) => {
            setItem(item, items[index]);
        });

        return builder.bytes();
    },
    structs({ items }) {
        // As tafla-gen's code writes an array of structs
        const structs = new Uint8Array(items.length * ITEM_SIZE);
        const writer = new StructWriter(structs);
        for (const { value, first, second } of items) {
            writer.setFloat32(value);
            writer.setFloat32(first);
            writer.setFloat32(second);
        }

        const builder = new Builder(1);
        builder.setStructArray(structs, ITEM_SIZE, ITEM_ALIGNMENT);

        return builder.bytes();
    },
    listFromBuilders({ items }) {
        const builder = new Builder(1);
        builder.setVector(items.map(itemBuilder));

        return builder.bytes();
    }
};

function itemBuilder({ value, first, second }: ItemInput): Builder {
    const pair = new Builder(2);
    pair.setFloat32(first);
    pair.setFloat32(second);

    const item = new Builder(2);
    item.setFloat32(value);
    item.setTable(pair);

    return item;
}

function setItem(item: Item, { value, first, second }: ItemInput): void {
    item.setFloat32(value);
    item.setTableInPlace(2, pair => {
        pair.setFloat32(first);
        pair.setFloat32(second);
    });
}
