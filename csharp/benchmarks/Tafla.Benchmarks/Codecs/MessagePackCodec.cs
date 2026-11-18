using System.Runtime.CompilerServices;
using MessagePack;

namespace Tafla.Benchmarks.Codecs;

/// <summary>
/// MessagePack-CSharp, which packs the workloads' own types with the formatters that its source
/// generator writes from their attributes, with <see cref="MessagePackSerializer"/>'s default options.
/// String keys pack a type as a map with its field names, as msgpackr packs an object, and a column
/// element by element. The array of structs packs each struct, and the table that holds them, as an
/// array of its fields in order, without their names, as every language's MessagePack codec packs that
/// fixed shape, so this codec copies the items into such types and back in each call, as Go's does.
/// </summary>
internal readonly struct MessagePackCodec : IRecordCodec, IDocumentCodec, IListCodec, IStructsCodec, ISampleCodec
{
    public string Name => "MessagePack";

    [MethodImpl(MethodImplOptions.NoInlining)]
    public byte[] SerializeRecord(Record record) => MessagePackSerializer.Serialize(record);

    [MethodImpl(MethodImplOptions.NoInlining)]
    public Record DeserializeRecord(byte[] bytes) => MessagePackSerializer.Deserialize<Record>(bytes);

    [MethodImpl(MethodImplOptions.NoInlining)]
    public byte[] SerializeDocument(Document document) => MessagePackSerializer.Serialize(document);

    [MethodImpl(MethodImplOptions.NoInlining)]
    public Document DeserializeDocument(byte[] bytes) => MessagePackSerializer.Deserialize<Document>(bytes);

    /// <summary>Unpacks the document, and sums its column, since MessagePack reads no element in place.</summary>
    [MethodImpl(MethodImplOptions.NoInlining)]
    public double SumA(byte[] bytes) => Workloads.SumColumn(MessagePackSerializer.Deserialize<Document>(bytes).A);

    [MethodImpl(MethodImplOptions.NoInlining)]
    public byte[] SerializeList(ItemList list) => MessagePackSerializer.Serialize(list);

    [MethodImpl(MethodImplOptions.NoInlining)]
    public ItemList DeserializeList(byte[] bytes) => MessagePackSerializer.Deserialize<ItemList>(bytes);

    [MethodImpl(MethodImplOptions.NoInlining)]
    public byte[] SerializeStructs(ItemList list)
    {
        var triples = new Triple[list.Items.Length];
        for (var index = 0; index < triples.Length; index++)
        {
            var item = list.Items[index];
            triples[index] = new Triple(item.Value, item.First, item.Second);
        }

        return MessagePackSerializer.Serialize(new Triples(triples));
    }

    [MethodImpl(MethodImplOptions.NoInlining)]
    public ItemList DeserializeStructs(byte[] bytes)
    {
        var triples = MessagePackSerializer.Deserialize<Triples>(bytes).Items;

        var items = new Item[triples.Length];
        for (var index = 0; index < items.Length; index++)
        {
            var triple = triples[index];
            items[index] = new Item(triple.Value, triple.First, triple.Second);
        }

        return new ItemList(items);
    }

    [MethodImpl(MethodImplOptions.NoInlining)]
    public byte[] SerializeSample(Sample sample) => MessagePackSerializer.Serialize(sample);

    [MethodImpl(MethodImplOptions.NoInlining)]
    public Sample DeserializeSample(byte[] bytes) => MessagePackSerializer.Deserialize<Sample>(bytes);
}

/// <summary>An item as a struct: an array of its three float32s</summary>
[MessagePackObject(AllowPrivate = true)]
internal readonly partial record struct Triple([property: Key(0)] float Value, [property: Key(1)] float First, [property: Key(2)] float Second);

/// <summary>The table that holds the structs: an array of their array</summary>
[MessagePackObject(AllowPrivate = true)]
internal sealed partial record Triples([property: Key(0)] Triple[] Items);
