using System.Runtime.CompilerServices;
using Gen = global::Workloads;

namespace Tafla.Benchmarks.Codecs;

/// <summary>
/// tafla's tables, written and read by the code that tafla-gen generates from <c>workloads.tafla</c>, in
/// <c>Generated/TaflaGen/</c>. A message's <c>…Fields</c> writes it with <c>Serialize()</c>, which counts
/// its length and writes it into bytes of that length, laying out itself the tables whose sizes the
/// schema fixes: the record whole, and each of the vector's items. Its readers read each field with a
/// getter, columns as spans in place, and the vector and the array of structs through views, which read
/// each item when it's asked for. It takes the workloads' types, as the other codecs do, so it creates the
/// generated fields from them in each call, the vector's items and the structs as arrays, as the protobuf
/// codec builds its messages, and creates the workloads' types from the readers. The bytes are those that
/// <see cref="TaflaCodec"/>'s builders write.
/// </summary>
internal readonly struct TaflaGenCodec
    : IRecordCodec,
        IDocumentCodec,
        IDocumentIntoCodec,
        IListCodec,
        IStructsCodec,
        ISampleCodec
{
    public string Name => "tafla-gen";

    [MethodImpl(MethodImplOptions.NoInlining)]
    public byte[] SerializeRecord(Record record) =>
        new Gen.RecordFields
        {
            X = record.X,
            Y = record.Y,
            Id = record.Id,
            Name = record.Name,
        }.Serialize();

    [MethodImpl(MethodImplOptions.NoInlining)]
    public Record DeserializeRecord(byte[] bytes)
    {
        var record = new Gen.Record(bytes);

        return new Record(record.X, record.Y, record.Id, record.Name);
    }

    [MethodImpl(MethodImplOptions.NoInlining)]
    public byte[] SerializeDocument(Document document) => DocumentFields(document).Serialize();

    /// <summary>Writes the document into the start of <paramref name="bytes"/>, and returns its length.</summary>
    [MethodImpl(MethodImplOptions.NoInlining)]
    public int SerializeDocumentInto(Document document, byte[] bytes) =>
        DocumentFields(document).SerializeInto(bytes);

    [MethodImpl(MethodImplOptions.NoInlining)]
    public Document DeserializeDocument(byte[] bytes)
    {
        var document = new Gen.Document(bytes);
        var columns = document.Columns;

        return new Document(
            document.X,
            document.Y,
            document.Name,
            columns.Base,
            columns.Keys.ToArray(),
            columns.A.ToArray(),
            columns.B.ToArray()
        );
    }

    /// <summary>Sums column a from its span, in place.</summary>
    [MethodImpl(MethodImplOptions.NoInlining)]
    public double SumA(byte[] bytes) => Workloads.SumColumn(new Gen.Document(bytes).Columns.A);

    [MethodImpl(MethodImplOptions.NoInlining)]
    public byte[] SerializeList(ItemList list)
    {
        var items = new Gen.ItemFields[list.Items.Length];
        for (var index = 0; index < items.Length; index++)
        {
            var item = list.Items[index];
            items[index] = new Gen.ItemFields
            {
                Value = item.Value,
                Pair = new Gen.PairFields { First = item.First, Second = item.Second },
            };
        }

        return new Gen.ItemListFields { Items = items }.Serialize();
    }

    /// <summary>Reads every item through the vector's view, each in place.</summary>
    [MethodImpl(MethodImplOptions.NoInlining)]
    public ItemList DeserializeList(byte[] bytes)
    {
        var view = new Gen.ItemList(bytes).Items;

        var items = new Item[view.Count];
        var index = 0;
        foreach (var item in view)
        {
            var pair = item.Pair;
            items[index++] = new Item(item.Value, pair.First, pair.Second);
        }

        return new ItemList(items);
    }

    /// <summary>Writes the structs straight into the table, from an array of the generated struct.</summary>
    [MethodImpl(MethodImplOptions.NoInlining)]
    public byte[] SerializeStructs(ItemList list)
    {
        var triples = new Gen.Triple[list.Items.Length];
        for (var index = 0; index < triples.Length; index++)
        {
            var item = list.Items[index];
            triples[index] = new Gen.Triple
            {
                Value = item.Value,
                First = item.First,
                Second = item.Second,
            };
        }

        return new Gen.TriplesFields { Items = triples }.Serialize();
    }

    /// <summary>Reads every struct through the array's view, from the bytes in place.</summary>
    [MethodImpl(MethodImplOptions.NoInlining)]
    public ItemList DeserializeStructs(byte[] bytes)
    {
        var view = new Gen.Triples(bytes).Items;

        var items = new Item[view.Count];
        var index = 0;
        foreach (var triple in view)
        {
            items[index++] = new Item(triple.Value, triple.First, triple.Second);
        }

        return new ItemList(items);
    }

    [MethodImpl(MethodImplOptions.NoInlining)]
    public byte[] SerializeSample(Sample sample) =>
        new Gen.Sample
        {
            Key = sample.Key,
            A = sample.A,
            B = sample.B,
        }.Serialize();

    [MethodImpl(MethodImplOptions.NoInlining)]
    public Sample DeserializeSample(byte[] bytes)
    {
        var sample = new Gen.Sample(bytes);

        return new Sample(sample.Key, sample.A, sample.B);
    }

    /// <summary>The document's generated fields, which share its columns.</summary>
    private static Gen.DocumentFields DocumentFields(Document document) =>
        new()
        {
            X = document.X,
            Y = document.Y,
            Name = document.Name,
            Columns = new Gen.ColumnsFields
            {
                Base = document.Base,
                Keys = document.Keys,
                A = document.A,
                B = document.B,
            },
        };
}
