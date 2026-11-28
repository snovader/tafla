// Written by hand in the shape that flatc 25.2.10 generates with `--java` from
// src/main/flatbuffers/workloads.fbs, with only the accessors and builders that the benchmarks use

package io.github.snovader.tafla.benchmarks.generated.flatbuffers;

import com.google.flatbuffers.FlatBufferBuilder;
import com.google.flatbuffers.Table;
import java.nio.ByteBuffer;
import java.nio.ByteOrder;

@SuppressWarnings("unused")
public final class Strings extends Table {
  public static Strings getRootAsStrings(ByteBuffer _bb) { return getRootAsStrings(_bb, new Strings()); }
  public static Strings getRootAsStrings(ByteBuffer _bb, Strings obj) { _bb.order(ByteOrder.LITTLE_ENDIAN); return (obj.__assign(_bb.getInt(_bb.position()) + _bb.position(), _bb)); }
  public void __init(int _i, ByteBuffer _bb) { __reset(_i, _bb); }
  public Strings __assign(int _i, ByteBuffer _bb) { __init(_i, _bb); return this; }

  public String items(int j) { int o = __offset(4); return o != 0 ? __string(__vector(o) + j * 4) : null; }
  public int itemsLength() { int o = __offset(4); return o != 0 ? __vector_len(o) : 0; }

  public static int createStrings(FlatBufferBuilder builder,
      int itemsOffset) {
    builder.startTable(1);
    Strings.addItems(builder, itemsOffset);
    return Strings.endStrings(builder);
  }

  public static void addItems(FlatBufferBuilder builder, int itemsOffset) { builder.addOffset(0, itemsOffset, 0); }
  public static int createItemsVector(FlatBufferBuilder builder, int[] data) { builder.startVector(4, data.length, 4); for (int i = data.length - 1; i >= 0; i--) builder.addOffset(data[i]); return builder.endVector(); }
  public static int endStrings(FlatBufferBuilder builder) {
    int o = builder.endTable();
    return o;
  }
}
