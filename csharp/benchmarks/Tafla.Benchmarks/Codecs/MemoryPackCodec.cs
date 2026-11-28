using System.IO;
using System.Runtime.CompilerServices;
using MemoryPack;

namespace Tafla.Benchmarks.Codecs;

/// <summary>
/// Cysharp's MemoryPack, which writes the workloads' own types with the code that its source
/// generator writes from their attributes: their members in order, without names or tags, in .NET's
/// little-endian layout, and each array of unmanaged values, such as a column or the items, with one
/// copy of its memory. It has no tables or structs of its own, so it writes the vector of nested
/// tables and the array of structs alike, as an array of the items' three float32s.
/// </summary>
internal readonly struct MemoryPackCodec : IRecordCodec, IDocumentCodec, IListCodec, IStructsCodec, ISampleCodec
{
    public string Name => "MemoryPack";

    [MethodImpl(MethodImplOptions.NoInlining)]
    public byte[] SerializeRecord(Record record) => MemoryPackSerializer.Serialize(record);

    [MethodImpl(MethodImplOptions.NoInlining)]
    public Record DeserializeRecord(byte[] bytes) =>
        MemoryPackSerializer.Deserialize<Record>(bytes) ?? throw Missing();

    [MethodImpl(MethodImplOptions.NoInlining)]
    public byte[] SerializeDocument(Document document) => MemoryPackSerializer.Serialize(document);

    [MethodImpl(MethodImplOptions.NoInlining)]
    public Document DeserializeDocument(byte[] bytes) =>
        MemoryPackSerializer.Deserialize<Document>(bytes) ?? throw Missing();

    /// <summary>Reads the document, and sums its column, since MemoryPack reads no element in place.</summary>
    [MethodImpl(MethodImplOptions.NoInlining)]
    public double SumA(byte[] bytes) =>
        Workloads.SumColumn((MemoryPackSerializer.Deserialize<Document>(bytes) ?? throw Missing()).A);

    [MethodImpl(MethodImplOptions.NoInlining)]
    public byte[] SerializeList(ItemList list) => MemoryPackSerializer.Serialize(list);

    [MethodImpl(MethodImplOptions.NoInlining)]
    public ItemList DeserializeList(byte[] bytes) =>
        MemoryPackSerializer.Deserialize<ItemList>(bytes) ?? throw Missing();

    [MethodImpl(MethodImplOptions.NoInlining)]
    public byte[] SerializeStructs(ItemList list) => MemoryPackSerializer.Serialize(list);

    [MethodImpl(MethodImplOptions.NoInlining)]
    public ItemList DeserializeStructs(byte[] bytes) =>
        MemoryPackSerializer.Deserialize<ItemList>(bytes) ?? throw Missing();

    [MethodImpl(MethodImplOptions.NoInlining)]
    public byte[] SerializeSample(Sample sample) => MemoryPackSerializer.Serialize(sample);

    [MethodImpl(MethodImplOptions.NoInlining)]
    public Sample DeserializeSample(byte[] bytes) => MemoryPackSerializer.Deserialize<Sample>(bytes);

    /// <summary>The bytes held null, which no workload is</summary>
    private static InvalidDataException Missing() => new("The bytes hold null");
}
