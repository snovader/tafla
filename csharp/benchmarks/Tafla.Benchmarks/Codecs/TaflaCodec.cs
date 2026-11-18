using System;
using System.Buffers.Binary;
using System.Runtime.CompilerServices;

namespace Tafla.Benchmarks.Codecs;

/// <summary>
/// tafla's tables, written with <see cref="TableBuilder"/>s and read with <see cref="TableReader"/>'s
/// getters, as the runtime's README shows. Nested tables and vector items are builders of their own,
/// which their parent copies. Columns are read in place and copied into the document's arrays, and
/// vector items are read in place, each with a reader of its bytes. The structs and the fixed-size
/// record are written with <see cref="BinaryPrimitives"/>, and read from
/// <see cref="TableReader.GetStructArray"/>'s bytes and with the getters.
/// </summary>
internal readonly struct TaflaCodec : IRecordCodec, IDocumentCodec, IListCodec, IStructsCodec, ISampleCodec
{
    /// <summary>The document's header: its length and version</summary>
    public static readonly TableLayout DocumentLayout = new() { Sized = true, Versioned = true };

    public const uint DocumentVersion = 1;

    /// <summary>An item as a struct: its three float32s, aligned to 4 in an array</summary>
    public const int TripleSize = 3 * sizeof(float);

    public const int TripleAlignment = sizeof(float);

    /// <summary>The fixed-size record: a uint32 and two float32s</summary>
    public const int SampleSize = sizeof(uint) + 2 * sizeof(float);

    public string Name => "tafla";

    [MethodImpl(MethodImplOptions.NoInlining)]
    public byte[] SerializeRecord(Record record)
    {
        var builder = new TableBuilder(4);
        builder.SetFloat64(record.X);
        builder.SetFloat64(record.Y);
        builder.SetInt64(record.Id);
        builder.SetString(record.Name);

        return builder.ToArray();
    }

    [MethodImpl(MethodImplOptions.NoInlining)]
    public Record DeserializeRecord(byte[] bytes)
    {
        var reader = new TableReader(bytes);

        return new Record(
            reader.GetFloat64(reader.RootOffset(0)),
            reader.GetFloat64(reader.RootOffset(1)),
            reader.GetInt64(reader.RootOffset(2)),
            reader.GetString(reader.RootOffset(3))
        );
    }

    [MethodImpl(MethodImplOptions.NoInlining)]
    public byte[] SerializeDocument(Document document)
    {
        var columns = new TableBuilder(4);
        columns.SetInt64(document.Base);
        columns.SetUInt32Array(document.Keys);
        columns.SetFloat32Array(document.A);
        columns.SetFloat32Array(document.B);

        var builder = new TableBuilder(4, DocumentLayout, DocumentVersion);
        builder.SetFloat64(document.X);
        builder.SetFloat64(document.Y);
        builder.SetString(document.Name);
        builder.SetTable(columns);

        return builder.ToArray();
    }

    [MethodImpl(MethodImplOptions.NoInlining)]
    public Document DeserializeDocument(byte[] bytes)
    {
        var reader = OpenDocument(bytes);
        var columns = reader.RootOffset(3);

        return new Document(
            reader.GetFloat64(reader.RootOffset(0)),
            reader.GetFloat64(reader.RootOffset(1)),
            reader.GetString(reader.RootOffset(2)),
            reader.GetInt64(reader.FieldOffset(columns, 0)),
            reader.GetUInt32Array(reader.FieldOffset(columns, 1)).ToArray(),
            reader.GetFloat32Array(reader.FieldOffset(columns, 2)).ToArray(),
            reader.GetFloat32Array(reader.FieldOffset(columns, 3)).ToArray()
        );
    }

    /// <summary>Sums column a from its span, in place.</summary>
    [MethodImpl(MethodImplOptions.NoInlining)]
    public double SumA(byte[] bytes)
    {
        var reader = OpenDocument(bytes);
        var a = reader.GetFloat32Array(reader.FieldOffset(reader.RootOffset(3), 2));

        return Workloads.SumColumn(a);
    }

    [MethodImpl(MethodImplOptions.NoInlining)]
    public byte[] SerializeList(ItemList list)
    {
        var items = new NestedTable[list.Items.Length];
        for (var index = 0; index < items.Length; index++)
        {
            var input = list.Items[index];

            var pair = new TableBuilder(2);
            pair.SetFloat32(input.First);
            pair.SetFloat32(input.Second);

            var item = new TableBuilder(2);
            item.SetFloat32(input.Value);
            item.SetTable(pair);
            items[index] = item;
        }

        var builder = new TableBuilder(1);
        builder.SetVector(items);

        return builder.ToArray();
    }

    [MethodImpl(MethodImplOptions.NoInlining)]
    public ItemList DeserializeList(byte[] bytes)
    {
        var reader = new TableReader(bytes);
        var vector = reader.GetVector(reader.RootOffset(0));

        var items = new Item[vector.Count];
        var index = 0;
        foreach (var inPlace in vector.ItemsInPlace())
        {
            items[index++] = ReadItem(inPlace);
        }

        return new ItemList(items);
    }

    /// <summary>Writes the items' structs with <see cref="BinaryPrimitives"/>, into bytes that the builder copies.</summary>
    [MethodImpl(MethodImplOptions.NoInlining)]
    public byte[] SerializeStructs(ItemList list)
    {
        var structs = new byte[list.Items.Length * TripleSize];
        for (var index = 0; index < list.Items.Length; index++)
        {
            WriteTriple(structs.AsSpan(index * TripleSize, TripleSize), list.Items[index]);
        }

        var builder = new TableBuilder(1);
        builder.SetStructArray(structs, TripleSize, TripleAlignment);

        return builder.ToArray();
    }

    [MethodImpl(MethodImplOptions.NoInlining)]
    public ItemList DeserializeStructs(byte[] bytes)
    {
        var structs = GetStructs(bytes);

        var items = new Item[structs.Length / TripleSize];
        for (var index = 0; index < items.Length; index++)
        {
            items[index] = ReadTriple(structs.Slice(index * TripleSize, TripleSize));
        }

        return new ItemList(items);
    }

    /// <summary>
    /// Writes the fixed-size record with <see cref="BinaryPrimitives"/>, since it has no header or
    /// offsets, as Go's README recommends for such records.
    /// </summary>
    [MethodImpl(MethodImplOptions.NoInlining)]
    public byte[] SerializeSample(Sample sample) => BinaryPrimitivesCodec.Write(sample);

    /// <summary>Reads the fixed-size record with the getters, which check each read's range.</summary>
    [MethodImpl(MethodImplOptions.NoInlining)]
    public Sample DeserializeSample(byte[] bytes)
    {
        var reader = new TableReader(bytes);

        return new Sample(reader.GetUInt32(0), reader.GetFloat32(4), reader.GetFloat32(8));
    }

    /// <summary>Reads a document's reader, after checking its version.</summary>
    public static TableReader OpenDocument(byte[] bytes)
    {
        var reader = new TableReader(bytes, DocumentLayout);
        var version = reader.GetVersion();
        if (version != DocumentVersion)
        {
            throw DecodeException.UnsupportedVersion(version, DocumentVersion);
        }

        return reader;
    }

    /// <summary>
    /// Reads an item of the vector, in place: its table starts at 0, and its reader has its parent's
    /// offset type.
    /// </summary>
    public static Item ReadItem(ReadOnlyMemory<byte> inPlace)
    {
        var reader = new TableReader(inPlace);
        var pair = reader.FieldOffset(0, 1);

        return new Item(
            reader.GetFloat32(reader.FieldOffset(0, 0)),
            reader.GetFloat32(reader.FieldOffset(pair, 0)),
            reader.GetFloat32(reader.FieldOffset(pair, 1))
        );
    }

    /// <summary>Returns the bytes of the structs, in place.</summary>
    public static ReadOnlySpan<byte> GetStructs(byte[] bytes)
    {
        var reader = new TableReader(bytes);

        return reader.GetStructArray(reader.RootOffset(0), TripleSize);
    }

    /// <summary>Writes an item's three float32s into a struct's bytes, little-endian.</summary>
    public static void WriteTriple(Span<byte> triple, Item item)
    {
        BinaryPrimitives.WriteSingleLittleEndian(triple, item.Value);
        BinaryPrimitives.WriteSingleLittleEndian(triple[4..], item.First);
        BinaryPrimitives.WriteSingleLittleEndian(triple[8..], item.Second);
    }

    /// <summary>Reads an item from a struct's bytes.</summary>
    public static Item ReadTriple(ReadOnlySpan<byte> triple) =>
        new(
            BinaryPrimitives.ReadSingleLittleEndian(triple),
            BinaryPrimitives.ReadSingleLittleEndian(triple[4..]),
            BinaryPrimitives.ReadSingleLittleEndian(triple[8..])
        );
}
