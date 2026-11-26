/**
 * How tafla and other libraries write and read a list of strings: the
 * proposed vector of strings, today's vector of tables of one string each,
 * and the libraries' own lists. `main.ts` times them, and
 * `benchmarks/README.md` describes the suite.
 */
import { Builder, ByteBuffer } from 'flatbuffers';
import type { Packr as TypedPackr } from 'msgpackr';
// The plain JavaScript entry point, as the comparison suite uses it
import { Packr as UntypedPackr } from 'msgpackr/pack';
import protobuf from 'protobufjs';
import { StringVector, TableBuilder, TableReader } from 'tafla';
import { Strings as FlatStrings } from '../generated/flatbuffers.ts';
import { NameList, StringList } from './generated/lists.ts';

/** One library's encoding of a list of strings */
export interface ListCodec {
    name: string;
    serialize: (strings: readonly string[]) => Uint8Array;
    /** Reads every string, in order */
    deserialize: (bytes: Uint8Array) => string[];
    /**
     * Reads the last string, without reading the others where the library
     * can find it so; absent where it can't
     */
    readLast?: (bytes: Uint8Array) => string;
}

/**
 * The proposed vector of strings, written with `TableBuilder` and read with
 * `TableReader.getStringVector()`, or a string from its ends with a
 * `StringVector`
 */
export const taflaStrings: ListCodec = {
    name: 'tafla [string]',
    serialize(strings) {
        const builder = new TableBuilder(1);
        builder.setStringVector(strings);

        return builder.bytes();
    },
    deserialize(bytes) {
        const reader = new TableReader(bytes);

        return reader.getStringVector(reader.rootOffset(0));
    },
    readLast(bytes) {
        const reader = new TableReader(bytes);
        const view = new StringVector(reader, reader.rootOffset(0));

        return view.get(view.length() - 1);
    }
};

/**
 * The same table, read a string at a time from its ends with a
 * `StringVector`, as reading every string reads strings outside ASCII,
 * rather than decoding a run of ASCII once
 */
export const taflaStringsEach: ListCodec = {
    name: 'tafla [string] each',
    serialize: taflaStrings.serialize,
    deserialize(bytes) {
        const reader = new TableReader(bytes);
        const view = new StringVector(reader, reader.rootOffset(0));
        const strings = new Array<string>(view.length());
        for (let i = 0; i < strings.length; i++) {
            strings[i] = view.get(i);
        }

        return strings;
    }
};

/**
 * The same table, written by the code that tafla-gen generates from
 * `lists.tafla`, which sizes it with a `TableSizer` and writes it with a
 * `TableWriter`, and read with its getter's `StringVector`
 */
export const taflaGenStrings: ListCodec = {
    name: 'tafla-gen [string]',
    serialize(items) {
        return StringList.serialize({ items });
    },
    deserialize(bytes) {
        return new StringList(bytes).items().toArray();
    },
    readLast(bytes) {
        const items = new StringList(bytes).items();

        return items.get(items.length() - 1);
    }
};

/**
 * Today's list of strings: a vector of tables of one string each, written
 * with `TableBuilder`, which records each table in place, and read with
 * `TableReader`'s getters
 */
export const taflaTables: ListCodec = {
    name: 'tafla [Name]',
    serialize(strings) {
        const builder = new TableBuilder(1);
        builder.setVectorInPlace(strings.length, 1, (item, index) => {
            item.setString(strings[index]);
        });

        return builder.bytes();
    },
    deserialize(bytes) {
        const reader = new TableReader(bytes);
        const offsets = reader.getVectorOffsets(reader.rootOffset(0));
        const strings = new Array<string>(offsets.length);
        for (let i = 0; i < offsets.length; i++) {
            strings[i] = reader.getString(reader.fieldOffset(offsets[i], 0));
        }

        return strings;
    },
    readLast(bytes) {
        const reader = new TableReader(bytes);
        const vector = reader.rootOffset(0);
        const last = reader.getVectorLength(vector) - 1;

        return reader.getString(
            reader.fieldOffset(reader.getVectorOffset(vector, last), 0)
        );
    }
};

/**
 * The same tables, written by the code that tafla-gen generates, from the
 * generated fields of each, which it creates in each call, and read with the
 * generated getters, which create a reader of each table
 */
export const taflaGenTables: ListCodec = {
    name: 'tafla-gen [Name]',
    serialize(strings) {
        const items = new Array<{ value: string }>(strings.length);
        for (let i = 0; i < strings.length; i++) {
            items[i] = { value: strings[i] };
        }

        return NameList.serialize({ items });
    },
    deserialize(bytes) {
        const items = new NameList(bytes).items();
        const strings = new Array<string>(items.length());
        for (let i = 0; i < strings.length; i++) {
            strings[i] = items.get(i).value();
        }

        return strings;
    },
    readLast(bytes) {
        const items = new NameList(bytes).items();

        return items.get(items.length() - 1).value();
    }
};

/** A vector of strings, with a builder of the default size */
const flatbuffers: ListCodec = {
    name: 'flatbuffers',
    serialize(strings) {
        const builder = new Builder();
        const offsets = new Array<number>(strings.length);
        for (let i = 0; i < strings.length; i++) {
            offsets[i] = builder.createString(strings[i]);
        }

        const items = FlatStrings.createItemsVector(builder, offsets);
        builder.finish(FlatStrings.createStrings(builder, items));

        return builder.asUint8Array();
    },
    deserialize(bytes) {
        const table = FlatStrings.getRootAsStrings(new ByteBuffer(bytes));
        const strings = new Array<string>(table.itemsLength());
        for (let i = 0; i < strings.length; i++) {
            strings[i] = table.items(i)!;
        }

        return strings;
    },
    readLast(bytes) {
        const table = FlatStrings.getRootAsStrings(new ByteBuffer(bytes));

        return table.items(table.itemsLength() - 1)!;
    }
};

const StringsMessage = protobuf
    .parse('syntax = "proto3"; message Strings { repeated string items = 1; }')
    .root.lookupType('Strings');

/** A repeated string, with protobufjs's reflection-generated code */
const protobufjs: ListCodec = {
    name: 'protobufjs',
    serialize(items) {
        return StringsMessage.encode({ items }).finish();
    },
    deserialize(bytes) {
        const { items } = StringsMessage.decode(bytes) as unknown as {
            items: string[];
        };

        return items;
    }
};

// The entry point's types re-export the root's from '.', which NodeNext doesn't
// resolve, so they type it as any
const Packr = UntypedPackr as typeof TypedPackr;
const packr = new Packr();

/**
 * An array of strings, packed into the buffer that the `Packr` reuses, and
 * returned as a view of it
 */
const msgpackr: ListCodec = {
    name: 'msgpackr',
    serialize(strings) {
        return packr.pack(strings);
    },
    deserialize(bytes) {
        return packr.unpack(bytes) as string[];
    }
};

export const LIST_CODECS: ListCodec[] = [
    taflaStrings,
    taflaStringsEach,
    taflaGenStrings,
    taflaTables,
    taflaGenTables,
    flatbuffers,
    protobufjs,
    msgpackr
];
