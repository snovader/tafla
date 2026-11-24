import { Builder, ByteBuffer } from 'flatbuffers';
import type { Packr as TypedPackr } from 'msgpackr';
// The plain JavaScript entry point, as the comparison suite uses it
import { Packr as UntypedPackr } from 'msgpackr/pack';
import protobuf from 'protobufjs';
import { TableBuilder, TableReader } from 'tafla';
import { Strings as FlatStrings } from './generated/flatbuffers.ts';
import { Strings } from './generated/tafla/strings.ts';
import { COUNT } from './shapes.ts';

/** One library's encoding of 16 strings */
export interface Codec {
    name: string;
    serialize(strings: string[]): Uint8Array;
    /** Reads every string, in order */
    deserialize(bytes: Uint8Array): string[];
}

/**
 * A table of 16 string fields, since tafla has no vector of strings, written
 * with a `TableBuilder` and read with `TableReader.getString()`
 */
export const tafla: Codec = {
    name: 'tafla',
    serialize(strings) {
        const builder = new TableBuilder(COUNT);
        for (let i = 0; i < COUNT; i++) {
            builder.setString(strings[i]);
        }

        return builder.bytes();
    },
    deserialize(bytes) {
        const reader = new TableReader(bytes);
        const strings = new Array<string>(COUNT);
        for (let i = 0; i < COUNT; i++) {
            strings[i] = reader.getString(reader.rootOffset(i));
        }

        return strings;
    }
};

/**
 * The same table, written by the code that tafla-gen generates from
 * `strings.tafla`, which counts it with a `TableSizer` and writes it with a
 * `TableWriter`. It creates the generated fields in each call, as the
 * protobufjs codec creates its message, and reads each with its getter.
 */
export const taflaGen: Codec = {
    name: 'tafla-gen',
    serialize(s) {
        return Strings.serialize({
            s0: s[0],
            s1: s[1],
            s2: s[2],
            s3: s[3],
            s4: s[4],
            s5: s[5],
            s6: s[6],
            s7: s[7],
            s8: s[8],
            s9: s[9],
            s10: s[10],
            s11: s[11],
            s12: s[12],
            s13: s[13],
            s14: s[14],
            s15: s[15]
        });
    },
    deserialize(bytes) {
        const table = new Strings(bytes);

        return [
            table.s0(),
            table.s1(),
            table.s2(),
            table.s3(),
            table.s4(),
            table.s5(),
            table.s6(),
            table.s7(),
            table.s8(),
            table.s9(),
            table.s10(),
            table.s11(),
            table.s12(),
            table.s13(),
            table.s14(),
            table.s15()
        ];
    }
};

/** A vector of strings, with a builder of the default size */
const flatbuffers: Codec = {
    name: 'flatbuffers',
    serialize(strings) {
        const builder = new Builder();
        const offsets = new Array<number>(COUNT);
        for (let i = 0; i < COUNT; i++) {
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
    }
};

const StringsMessage = protobuf
    .parse('syntax = "proto3"; message Strings { repeated string items = 1; }')
    .root.lookupType('Strings');

/** A repeated string, with protobufjs's reflection-generated code */
const protobufjs: Codec = {
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
const msgpackr: Codec = {
    name: 'msgpackr',
    serialize(strings) {
        return packr.pack(strings);
    },
    deserialize(bytes) {
        return packr.unpack(bytes) as string[];
    }
};

export const CODECS: Codec[] = [
    tafla,
    taflaGen,
    flatbuffers,
    protobufjs,
    msgpackr
];
