using System.IO;
using System.Runtime.CompilerServices;
using Google.Protobuf;
using Google.Protobuf.Collections;
using Proto = Tafla.Benchmarks.Protobuf;

namespace Tafla.Benchmarks.Codecs;

/// <summary>
/// Google's protobuf, with the messages that protoc generates from <c>schemas/messages.proto</c>,
/// built from the input and written with <see cref="MessageExtensions.ToByteArray"/>, which sizes the
/// bytes first, and read with each message's parser. Building a message copies the input's columns
/// into its repeated fields, and reading copies them out into the document's arrays. protobuf has no
/// structs, so the array of them is a repeated message of the three floats, which it writes as
/// fixed32s.
/// </summary>
internal readonly struct ProtobufCodec : IRecordCodec, IDocumentCodec, IListCodec, IStructsCodec, ISampleCodec
{
    public string Name => "protobuf";

    [MethodImpl(MethodImplOptions.NoInlining)]
    public byte[] SerializeRecord(Record record) =>
        new Proto.Record
        {
            X = record.X,
            Y = record.Y,
            Id = record.Id,
            Name = record.Name,
        }.ToByteArray();

    [MethodImpl(MethodImplOptions.NoInlining)]
    public Record DeserializeRecord(byte[] bytes)
    {
        var message = Proto.Record.Parser.ParseFrom(bytes);

        return new Record(message.X, message.Y, message.Id, message.Name);
    }

    [MethodImpl(MethodImplOptions.NoInlining)]
    public byte[] SerializeDocument(Document document)
    {
        var columns = new Proto.Columns { Base = document.Base };
        columns.Keys.AddRange(document.Keys);
        columns.A.AddRange(document.A);
        columns.B.AddRange(document.B);

        var message = new Proto.Document
        {
            X = document.X,
            Y = document.Y,
            Name = document.Name,
            Columns = columns,
        };

        return message.ToByteArray();
    }

    [MethodImpl(MethodImplOptions.NoInlining)]
    public Document DeserializeDocument(byte[] bytes)
    {
        var message = Proto.Document.Parser.ParseFrom(bytes);
        var columns = message.Columns ?? throw Missing("columns");

        return new Document(
            message.X,
            message.Y,
            message.Name,
            columns.Base,
            ToArray(columns.Keys),
            ToArray(columns.A),
            ToArray(columns.B)
        );
    }

    /// <summary>Parses the document, and sums column a with its repeated field's indexer.</summary>
    [MethodImpl(MethodImplOptions.NoInlining)]
    public double SumA(byte[] bytes)
    {
        var columns = Proto.Document.Parser.ParseFrom(bytes).Columns ?? throw Missing("columns");
        var a = columns.A;

        var sum = 0.0;
        for (var index = 0; index < a.Count; index++)
        {
            sum += a[index];
        }

        return sum;
    }

    [MethodImpl(MethodImplOptions.NoInlining)]
    public byte[] SerializeList(ItemList list)
    {
        var message = new Proto.List();
        foreach (var item in list.Items)
        {
            message.Items.Add(
                new Proto.Item
                {
                    Value = item.Value,
                    Pair = new Proto.Pair { First = item.First, Second = item.Second },
                }
            );
        }

        return message.ToByteArray();
    }

    [MethodImpl(MethodImplOptions.NoInlining)]
    public ItemList DeserializeList(byte[] bytes)
    {
        var message = Proto.List.Parser.ParseFrom(bytes);

        var items = new Item[message.Items.Count];
        for (var index = 0; index < items.Length; index++)
        {
            var item = message.Items[index];
            var pair = item.Pair ?? throw Missing("a pair");
            items[index] = new Item(item.Value, pair.First, pair.Second);
        }

        return new ItemList(items);
    }

    [MethodImpl(MethodImplOptions.NoInlining)]
    public byte[] SerializeStructs(ItemList list)
    {
        var message = new Proto.Triples();
        foreach (var item in list.Items)
        {
            message.Items.Add(
                new Proto.Triple
                {
                    Value = item.Value,
                    First = item.First,
                    Second = item.Second,
                }
            );
        }

        return message.ToByteArray();
    }

    [MethodImpl(MethodImplOptions.NoInlining)]
    public ItemList DeserializeStructs(byte[] bytes)
    {
        var message = Proto.Triples.Parser.ParseFrom(bytes);

        var items = new Item[message.Items.Count];
        for (var index = 0; index < items.Length; index++)
        {
            var triple = message.Items[index];
            items[index] = new Item(triple.Value, triple.First, triple.Second);
        }

        return new ItemList(items);
    }

    [MethodImpl(MethodImplOptions.NoInlining)]
    public byte[] SerializeSample(Sample sample) =>
        new Proto.Sample
        {
            Key = sample.Key,
            A = sample.A,
            B = sample.B,
        }.ToByteArray();

    [MethodImpl(MethodImplOptions.NoInlining)]
    public Sample DeserializeSample(byte[] bytes)
    {
        var message = Proto.Sample.Parser.ParseFrom(bytes);

        return new Sample(message.Key, message.A, message.B);
    }

    /// <summary>Copies a repeated field into an array of its length.</summary>
    private static T[] ToArray<T>(RepeatedField<T> field)
    {
        var array = new T[field.Count];
        field.CopyTo(array, 0);

        return array;
    }

    /// <summary>A field that the message leaves out, which every workload has</summary>
    private static InvalidDataException Missing(string field) => new($"The message has no {field}");
}
