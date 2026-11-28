/*
 * Written by hand in the shape that flatc 25.9.23 generates with `--ts`, with
 * only the accessors and builders the benchmarks use, from this schema:
 *
 *     table Strings { items: [string]; }
 */

import type { Builder, ByteBuffer, Offset } from 'flatbuffers';

export class Strings {
    bb: ByteBuffer | null = null;
    bb_pos = 0;

    __init(i: number, bb: ByteBuffer): Strings {
        this.bb_pos = i;
        this.bb = bb;
        return this;
    }

    static getRootAsStrings(bb: ByteBuffer, obj?: Strings): Strings {
        return (obj || new Strings()).__init(
            bb.readInt32(bb.position()) + bb.position(),
            bb
        );
    }

    items(index: number): string | null {
        const offset = this.bb!.__offset(this.bb_pos, 4);
        return offset
            ? (this.bb!.__string(
                  this.bb!.__vector(this.bb_pos + offset) + index * 4
              ) as string)
            : null;
    }

    itemsLength(): number {
        const offset = this.bb!.__offset(this.bb_pos, 4);
        return offset ? this.bb!.__vector_len(this.bb_pos + offset) : 0;
    }

    static createItemsVector(builder: Builder, data: Offset[]): Offset {
        builder.startVector(4, data.length, 4);
        for (let i = data.length - 1; i >= 0; i--) {
            builder.addOffset(data[i]!);
        }
        return builder.endVector();
    }

    static createStrings(builder: Builder, itemsOffset: Offset): Offset {
        builder.startObject(1);
        builder.addFieldOffset(0, itemsOffset, 0);
        return builder.endObject();
    }
}
