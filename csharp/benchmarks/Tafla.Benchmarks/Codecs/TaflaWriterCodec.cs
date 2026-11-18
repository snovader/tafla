using System.Runtime.CompilerServices;

namespace Tafla.Benchmarks.Codecs;

/// <summary>
/// tafla's tables, sized with <see cref="TableSizer"/> and written with <see cref="TableWriter"/>, as
/// the code that tafla-gen generates writes them in the other languages: into new bytes of each
/// table's length, or into bytes that the caller reuses, with nested tables, vector items and structs
/// written in place. They're the bytes that <see cref="TaflaCodec"/>'s builders write, which it reads
/// back with <see cref="TaflaCodec"/>'s readers, so the comparison times only its writes.
/// </summary>
internal readonly struct TaflaWriterCodec : IRecordCodec, IDocumentCodec, IDocumentIntoCodec, IListCodec, IStructsCodec
{
    /// <summary>The alignment of the document's columns: the largest of their arrays'</summary>
    private const int ColumnsAlignment = sizeof(uint);

    /// <summary>The alignment of an item's pair, a table of scalars only</summary>
    private const int PairAlignment = 1;

    public string Name => "tafla TableWriter";

    [MethodImpl(MethodImplOptions.NoInlining)]
    public byte[] SerializeRecord(Record record)
    {
        var sizer = new TableSizer(4);
        sizer.AddFloat64();
        sizer.AddFloat64();
        sizer.AddInt64();
        sizer.AddString(record.Name);

        var writer = sizer.CreateWriter();
        writer.SetFloat64(0, record.X);
        writer.SetFloat64(1, record.Y);
        writer.SetInt64(2, record.Id);
        writer.SetString(3, record.Name);

        return writer.Bytes;
    }

    [MethodImpl(MethodImplOptions.NoInlining)]
    public Record DeserializeRecord(byte[] bytes) => default(TaflaCodec).DeserializeRecord(bytes);

    [MethodImpl(MethodImplOptions.NoInlining)]
    public byte[] SerializeDocument(Document document)
    {
        var writer = SizeDocument(document).CreateWriter();
        SetDocument(ref writer, document);

        return writer.Bytes;
    }

    /// <summary>Writes the document into the start of <paramref name="bytes"/>, and returns its length.</summary>
    [MethodImpl(MethodImplOptions.NoInlining)]
    public int SerializeDocumentInto(Document document, byte[] bytes)
    {
        var writer = SizeDocument(document).CreateWriter(bytes);
        SetDocument(ref writer, document);

        return writer.ByteLength;
    }

    [MethodImpl(MethodImplOptions.NoInlining)]
    public Document DeserializeDocument(byte[] bytes) => default(TaflaCodec).DeserializeDocument(bytes);

    [MethodImpl(MethodImplOptions.NoInlining)]
    public double SumA(byte[] bytes) => default(TaflaCodec).SumA(bytes);

    [MethodImpl(MethodImplOptions.NoInlining)]
    public byte[] SerializeList(ItemList list)
    {
        var items = list.Items;

        var sizer = new TableSizer(1);
        sizer.AddVector(items.Length);
        for (var index = 0; index < items.Length; index++)
        {
            var item = sizer.StartTable(2);
            sizer.AddFloat32();

            sizer.AddTable(PairAlignment);
            var pair = sizer.StartTable(2);
            sizer.AddFloat32();
            sizer.AddFloat32();
            sizer.EndTable(pair);
            sizer.EndTable(item);
        }

        var writer = sizer.CreateWriter();
        var vector = writer.SetVector(0, items.Length);
        for (var index = 0; index < items.Length; index++)
        {
            var input = items[index];
            var item = writer.StartTable(2);
            writer.SetFloat32(0, input.Value);

            writer.SetTable(1, PairAlignment);
            var pair = writer.StartTable(2);
            writer.SetFloat32(0, input.First);
            writer.SetFloat32(1, input.Second);
            writer.EndTable(pair);
            writer.EndTable(item);
            writer.SetItemEnd(vector, index);
        }

        return writer.Bytes;
    }

    [MethodImpl(MethodImplOptions.NoInlining)]
    public ItemList DeserializeList(byte[] bytes) => default(TaflaCodec).DeserializeList(bytes);

    /// <summary>Writes each struct straight into the table, rather than into bytes that are then copied.</summary>
    [MethodImpl(MethodImplOptions.NoInlining)]
    public byte[] SerializeStructs(ItemList list)
    {
        var items = list.Items;

        var sizer = new TableSizer(1);
        sizer.AddStructArray(items.Length, TaflaCodec.TripleSize, TaflaCodec.TripleAlignment);

        var writer = sizer.CreateWriter();
        var structs = writer.ReserveStructArray(0, items.Length, TaflaCodec.TripleSize, TaflaCodec.TripleAlignment);
        for (var index = 0; index < items.Length; index++)
        {
            TaflaCodec.WriteTriple(structs.Slice(index * TaflaCodec.TripleSize, TaflaCodec.TripleSize), items[index]);
        }

        return writer.Bytes;
    }

    [MethodImpl(MethodImplOptions.NoInlining)]
    public ItemList DeserializeStructs(byte[] bytes) => default(TaflaCodec).DeserializeStructs(bytes);

    private static TableSizer SizeDocument(Document document)
    {
        var sizer = new TableSizer(4, TaflaCodec.DocumentLayout, TaflaCodec.DocumentVersion);
        sizer.AddFloat64();
        sizer.AddFloat64();
        sizer.AddString(document.Name);

        sizer.AddTable(ColumnsAlignment);
        var columns = sizer.StartTable(4);
        sizer.AddInt64();
        sizer.AddUInt32Array(document.Keys.Length);
        sizer.AddFloat32Array(document.A.Length);
        sizer.AddFloat32Array(document.B.Length);
        sizer.EndTable(columns);

        return sizer;
    }

    private static void SetDocument(ref TableWriter writer, Document document)
    {
        writer.SetFloat64(0, document.X);
        writer.SetFloat64(1, document.Y);
        writer.SetString(2, document.Name);

        writer.SetTable(3, ColumnsAlignment);
        var columns = writer.StartTable(4);
        writer.SetInt64(0, document.Base);
        writer.SetUInt32Array(1, document.Keys);
        writer.SetFloat32Array(2, document.A);
        writer.SetFloat32Array(3, document.B);
        writer.EndTable(columns);
    }
}
