using System.IO;
using System.Runtime.CompilerServices;
using Google.FlatBuffers;
using Fbs = Tafla.Benchmarks.FlatBuffers;

namespace Tafla.Benchmarks.Codecs;

/// <summary>
/// Google's FlatBuffers, with the code that flatc generates from <c>schemas/workloads.fbs</c>. Each
/// message is built with a new <see cref="FlatBufferBuilder"/> of 1,024 bytes, as the C# tutorial
/// creates one, which grows by doubling, and copied out of it with
/// <see cref="FlatBufferBuilder.SizedByteArray"/>. Columns are written in bulk with the generated
/// <c>Create…VectorBlock()</c> methods, and read into arrays with <c>Get…Array()</c>. Readers verify
/// each message with the generated verifiers before reading it, as the Rust and TypeScript
/// comparisons do, since the bytes may be untrusted.
/// </summary>
internal readonly struct FlatBuffersCodec : IRecordCodec, IDocumentCodec, IListCodec, IStructsCodec, ISampleCodec
{
    /// <summary>The builder's initial size, the C# tutorial's</summary>
    private const int InitialSize = 1024;

    public string Name => "FlatBuffers";

    [MethodImpl(MethodImplOptions.NoInlining)]
    public byte[] SerializeRecord(Record record)
    {
        var builder = new FlatBufferBuilder(InitialSize);
        var name = builder.CreateString(record.Name);
        var root = Fbs.Record.CreateRecord(builder, record.X, record.Y, record.Id, name);
        builder.Finish(root.Value);

        return builder.SizedByteArray();
    }

    [MethodImpl(MethodImplOptions.NoInlining)]
    public Record DeserializeRecord(byte[] bytes)
    {
        var buffer = Verified(bytes, Fbs.RecordVerify.Verify);
        var root = Fbs.Record.GetRootAsRecord(buffer);

        return new Record(root.X, root.Y, root.Id, root.Name ?? throw Missing("name"));
    }

    [MethodImpl(MethodImplOptions.NoInlining)]
    public byte[] SerializeDocument(Document document)
    {
        var builder = new FlatBufferBuilder(InitialSize);
        var keys = Fbs.Columns.CreateKeysVectorBlock(builder, document.Keys);
        var a = Fbs.Columns.CreateAVectorBlock(builder, document.A);
        var b = Fbs.Columns.CreateBVectorBlock(builder, document.B);
        var columns = Fbs.Columns.CreateColumns(builder, document.Base, keys, a, b);

        var name = builder.CreateString(document.Name);
        var root = Fbs.Document.CreateDocument(builder, document.X, document.Y, name, columns);
        builder.Finish(root.Value);

        return builder.SizedByteArray();
    }

    [MethodImpl(MethodImplOptions.NoInlining)]
    public Document DeserializeDocument(byte[] bytes)
    {
        var buffer = Verified(bytes, Fbs.DocumentVerify.Verify);
        var root = Fbs.Document.GetRootAsDocument(buffer);
        var columns = root.Columns ?? throw Missing("columns");

        return new Document(
            root.X,
            root.Y,
            root.Name ?? throw Missing("name"),
            columns.Base,
            columns.GetKeysArray() ?? throw Missing("keys"),
            columns.GetAArray() ?? throw Missing("a"),
            columns.GetBArray() ?? throw Missing("b")
        );
    }

    /// <summary>Sums column a with the generated accessor of each element, after verifying the message.</summary>
    [MethodImpl(MethodImplOptions.NoInlining)]
    public double SumA(byte[] bytes)
    {
        var buffer = Verified(bytes, Fbs.DocumentVerify.Verify);
        var columns = Fbs.Document.GetRootAsDocument(buffer).Columns ?? throw Missing("columns");

        var sum = 0.0;
        var length = columns.ALength;
        for (var index = 0; index < length; index++)
        {
            sum += columns.A(index);
        }

        return sum;
    }

    [MethodImpl(MethodImplOptions.NoInlining)]
    public byte[] SerializeList(ItemList list)
    {
        var builder = new FlatBufferBuilder(InitialSize);

        // Each item's pair before the item, since the builder writes its bytes from the end
        var items = new Offset<Fbs.Item>[list.Items.Length];
        for (var index = 0; index < items.Length; index++)
        {
            var input = list.Items[index];
            var pair = Fbs.Pair.CreatePair(builder, input.First, input.Second);
            items[index] = Fbs.Item.CreateItem(builder, input.Value, pair);
        }

        var vector = Fbs.List.CreateItemsVector(builder, items);
        var root = Fbs.List.CreateList(builder, vector);
        builder.Finish(root.Value);

        return builder.SizedByteArray();
    }

    [MethodImpl(MethodImplOptions.NoInlining)]
    public ItemList DeserializeList(byte[] bytes)
    {
        var buffer = Verified(bytes, Fbs.ListVerify.Verify);
        var root = Fbs.List.GetRootAsList(buffer);

        var items = new Item[root.ItemsLength];
        for (var index = 0; index < items.Length; index++)
        {
            var item = root.Items(index) ?? throw Missing("an item");
            var pair = item.Pair ?? throw Missing("a pair");
            items[index] = new Item(item.Value, pair.First, pair.Second);
        }

        return new ItemList(items);
    }

    /// <summary>Writes the vector of structs from its last item to its first, as the builder writes vectors.</summary>
    [MethodImpl(MethodImplOptions.NoInlining)]
    public byte[] SerializeStructs(ItemList list)
    {
        var builder = new FlatBufferBuilder(InitialSize);

        Fbs.Triples.StartItemsVector(builder, list.Items.Length);
        for (var index = list.Items.Length - 1; index >= 0; index--)
        {
            var item = list.Items[index];
            Fbs.Triple.CreateTriple(builder, item.Value, item.First, item.Second);
        }

        var vector = builder.EndVector();
        var root = Fbs.Triples.CreateTriples(builder, vector);
        builder.Finish(root.Value);

        return builder.SizedByteArray();
    }

    [MethodImpl(MethodImplOptions.NoInlining)]
    public ItemList DeserializeStructs(byte[] bytes)
    {
        var buffer = Verified(bytes, Fbs.TriplesVerify.Verify);
        var root = Fbs.Triples.GetRootAsTriples(buffer);

        var items = new Item[root.ItemsLength];
        for (var index = 0; index < items.Length; index++)
        {
            var triple = root.Items(index) ?? throw Missing("a struct");
            items[index] = new Item(triple.Value, triple.First, triple.Second);
        }

        return new ItemList(items);
    }

    [MethodImpl(MethodImplOptions.NoInlining)]
    public byte[] SerializeSample(Sample sample)
    {
        var builder = new FlatBufferBuilder(InitialSize);
        var root = Fbs.Sample.CreateSample(builder, sample.Key, sample.A, sample.B);
        builder.Finish(root.Value);

        return builder.SizedByteArray();
    }

    [MethodImpl(MethodImplOptions.NoInlining)]
    public Sample DeserializeSample(byte[] bytes)
    {
        var buffer = Verified(bytes, Fbs.SampleVerify.Verify);
        var root = Fbs.Sample.GetRootAsSample(buffer);

        return new Sample(root.Key, root.A, root.B);
    }

    /// <summary>Returns a buffer of the bytes, after verifying the message with its root table's verifier.</summary>
    private static ByteBuffer Verified(byte[] bytes, VerifyTableAction verify)
    {
        var buffer = new ByteBuffer(bytes);
        if (!new Verifier(buffer).VerifyBuffer(null, false, verify))
        {
            throw new InvalidDataException("The FlatBuffers message doesn't verify");
        }

        return buffer;
    }

    /// <summary>A field that the message leaves out, which every workload has</summary>
    private static InvalidDataException Missing(string field) => new($"The message has no {field}");
}
