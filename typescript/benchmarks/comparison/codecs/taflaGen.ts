import {
    Document,
    type ItemFields,
    ItemList,
    Record,
    Sample,
    Triples
} from '../generated/tafla/workloads.ts';
import type { DocumentOutput, ItemInput } from '../workloads.ts';
import type { Codec } from './codec.ts';

/**
 * The code that tafla-gen generates from `workloads.tafla`, which writes each
 * message's table in one call with a `TableSizer` and a `TableWriter`, rather
 * than nesting `TableBuilder`s, and reads it with the generated getters, or
 * with `fields()` and a struct's `read()` where the generated fields are the
 * workload's output.
 *
 * The workloads' inputs are flat, so the document and the vector are written
 * from fields in the generated shape, which `serialize()` creates in each
 * call, as the protobufjs codec creates its messages. Reading them creates
 * the workloads' outputs from the generated readers.
 */
export const taflaGen: Codec = {
    name: 'tafla-gen',
    record: {
        serialize(input) {
            return Record.serialize(input);
        },
        deserialize(bytes) {
            // The record's fields are the workload's
            return new Record(bytes).fields();
        }
    },
    document: {
        serialize({ x, y, name, base, keys, a, b }) {
            return Document.serialize({
                x,
                y,
                name,
                columns: { base, keys, a, b }
            });
        },
        serializeInto({ x, y, name, base, keys, a, b }, bytes) {
            return Document.serializeInto(
                { x, y, name, columns: { base, keys, a, b } },
                bytes
            );
        },
        deserialize(bytes): DocumentOutput {
            const document = new Document(bytes);
            const columns = document.columns();

            return {
                x: document.x(),
                y: document.y(),
                name: document.name(),
                base: columns.base(),
                keys: columns.keys(),
                a: columns.a(),
                b: columns.b()
            };
        },
        sumA(bytes) {
            const a = new Document(bytes).columns().a();

            let sum = 0;
            for (let i = 0; i < a.length; i++) {
                sum += a[i];
            }

            return sum;
        }
    },
    list: {
        serialize({ items }) {
            return ItemList.serialize({ items: items.map(toItemFields) });
        },
        deserialize(bytes) {
            const tables = new ItemList(bytes).items();

            const items = new Array<ItemInput>(tables.length());
            for (let i = 0; i < items.length; i++) {
                const item = tables.get(i);
                const pair = item.pair();

                items[i] = {
                    value: item.value(),
                    first: pair.first(),
                    second: pair.second()
                };
            }

            return { items };
        }
    },
    structs: {
        serialize(input) {
            return Triples.serialize(input);
        },
        deserialize(bytes) {
            // The struct's fields are the workload's item's
            return { items: new Triples(bytes).items().fields() };
        }
    },
    sample: {
        serialize(input) {
            return Sample.serialize(input);
        },
        deserialize(bytes) {
            return Sample.read(bytes);
        }
    }
};

function toItemFields({ value, first, second }: ItemInput): ItemFields {
    return { value, pair: { first, second } };
}
