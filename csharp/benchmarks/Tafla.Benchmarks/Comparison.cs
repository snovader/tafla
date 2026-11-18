using System;
using System.Collections.Generic;
using Tafla.Benchmarks.Codecs;

namespace Tafla.Benchmarks;

/// <summary>
/// A row of the report: a workload's operation, and each codec's variant of it, the first of which the
/// others' times are compared with. <paramref name="Group"/> is the report's table that it belongs to.
/// </summary>
internal sealed record Benchmark(string Group, string Name, IReadOnlyList<Variant> Variants);

/// <summary>
/// The benchmarks: every codec's operations on every workload, and tafla's reads of only some fields.
/// tafla's variant comes first in each, and again as an A/A pair, which shows the harness's noise:
/// the copy's loop is its own, but it calls the same compiled codec method.
/// </summary>
internal static class Comparison
{
    /// <summary>The report's table of the codecs' operations on every workload</summary>
    public const string Codecs = "comparison";

    /// <summary>The report's table of tafla's reads of only some fields</summary>
    public const string Reads = "reads";

    /// <summary>The rows of the document that's also written into reused bytes: the large one.</summary>
    private const int ReusedRows = 100_000;

    public static List<Benchmark> All()
    {
        var record = Workloads.TheRecord;
        var list = Workloads.NewList(Workloads.ListItems);
        var sample = Workloads.TheSample;
        var documents = Array.ConvertAll(Workloads.DocumentRows, Workloads.NewDocument);

        var all = new List<Benchmark>();
        all.AddRange(Records(record));
        foreach (var document in documents)
        {
            all.AddRange(Documents(document));
        }

        all.AddRange(Lists(list));
        all.AddRange(Structs(list));
        all.AddRange(Samples(sample));

        all.Add(RecordScalars(record));
        foreach (var document in documents)
        {
            all.Add(ColumnByGetter(document));
        }

        all.AddRange(ListItems(list));
        all.AddRange(StructItems(list));

        return all;
    }

    /// <summary>
    /// Checks that every variant of the benchmark stores what it should after a call: what was written,
    /// read back by its codec.
    /// </summary>
    /// <exception cref="InvalidOperationException">A variant doesn't.</exception>
    public static void Check(Benchmark benchmark)
    {
        foreach (var variant in benchmark.Variants)
        {
            Sink.Clear();
            variant.Loop(1);
            if (!variant.Check())
            {
                throw new InvalidOperationException($"{benchmark.Name}: {variant.Name} doesn't read back what was written");
            }
        }
    }

    private static Benchmark[] Records(Record record)
    {
        var tafla = default(TaflaCodec).SerializeRecord(record);
        Variant[] serialize =
        [
            .. Loops.WithCopy(new SerializeRecord<TaflaCodec>(record)),
            Loops.Of<SerializeRecord<TaflaWriterCodec>, Original>(new(record)),
            Loops.Of<SerializeRecord<FlatBuffersCodec>, Original>(new(record)),
            Loops.Of<SerializeRecord<ProtobufCodec>, Original>(new(record)),
            Loops.Of<SerializeRecord<MessagePackCodec>, Original>(new(record)),
            Loops.Of<SerializeRecord<MemoryPackCodec>, Original>(new(record)),
        ];

        Variant[] deserialize =
        [
            .. Loops.WithCopy(new DeserializeRecord<TaflaCodec>(tafla, record)),
            Loops.Of<DeserializeRecord<FlatBuffersCodec>, Original>(new(default(FlatBuffersCodec).SerializeRecord(record), record)),
            Loops.Of<DeserializeRecord<ProtobufCodec>, Original>(new(default(ProtobufCodec).SerializeRecord(record), record)),
            Loops.Of<DeserializeRecord<MessagePackCodec>, Original>(new(default(MessagePackCodec).SerializeRecord(record), record)),
            Loops.Of<DeserializeRecord<MemoryPackCodec>, Original>(new(default(MemoryPackCodec).SerializeRecord(record), record)),
        ];

        return [new(Codecs, "record: serialize", serialize), new(Codecs, "record: deserialize", deserialize)];
    }

    private static List<Benchmark> Documents(Document document)
    {
        var rows = $"document of {document.A.Length:N0} rows";
        var sum = Workloads.SumColumn(document.A);

        Variant[] serialize =
        [
            .. Loops.WithCopy(new SerializeDocument<TaflaCodec>(document)),
            Loops.Of<SerializeDocument<TaflaWriterCodec>, Original>(new(document)),
            Loops.Of<SerializeDocument<FlatBuffersCodec>, Original>(new(document)),
            Loops.Of<SerializeDocument<ProtobufCodec>, Original>(new(document)),
            Loops.Of<SerializeDocument<MessagePackCodec>, Original>(new(document)),
            Loops.Of<SerializeDocument<MemoryPackCodec>, Original>(new(document)),
        ];

        var tafla = default(TaflaCodec).SerializeDocument(document);
        var flatBuffers = default(FlatBuffersCodec).SerializeDocument(document);
        var protobuf = default(ProtobufCodec).SerializeDocument(document);
        var messagePack = default(MessagePackCodec).SerializeDocument(document);
        var memoryPack = default(MemoryPackCodec).SerializeDocument(document);

        Variant[] deserialize =
        [
            .. Loops.WithCopy(new DeserializeDocument<TaflaCodec>(tafla, document)),
            Loops.Of<DeserializeDocument<FlatBuffersCodec>, Original>(new(flatBuffers, document)),
            Loops.Of<DeserializeDocument<ProtobufCodec>, Original>(new(protobuf, document)),
            Loops.Of<DeserializeDocument<MessagePackCodec>, Original>(new(messagePack, document)),
            Loops.Of<DeserializeDocument<MemoryPackCodec>, Original>(new(memoryPack, document)),
        ];

        Variant[] sumA =
        [
            .. Loops.WithCopy(new SumA<TaflaCodec>(tafla, sum)),
            Loops.Of<SumA<FlatBuffersCodec>, Original>(new(flatBuffers, sum)),
            Loops.Of<SumA<ProtobufCodec>, Original>(new(protobuf, sum)),
            Loops.Of<SumA<MessagePackCodec>, Original>(new(messagePack, sum)),
            Loops.Of<SumA<MemoryPackCodec>, Original>(new(memoryPack, sum)),
        ];

        List<Benchmark> benchmarks =
        [
            new(Codecs, $"{rows}: serialize", serialize),
            new(Codecs, $"{rows}: deserialize", deserialize),
            new(Codecs, $"{rows}: sum a column per element", sumA),
        ];
        if (document.A.Length == ReusedRows)
        {
            benchmarks.Add(SerializeInto(rows, document, tafla));
        }

        return benchmarks;
    }

    /// <summary>
    /// Writes the document into bytes that each variant keeps between calls, as long as the document,
    /// by the codec that can: TableWriter, which takes the bytes to write into.
    /// </summary>
    private static Benchmark SerializeInto(string rows, Document document, byte[] expected)
    {
        Variant[] serialize =
        [
            Loops.Of<SerializeDocumentInto<TaflaWriterCodec>, Original>(new(document, Filled(expected.Length), expected)),
            Loops.Of<SerializeDocumentInto<TaflaWriterCodec>, Again>(new(document, Filled(expected.Length), expected)),
        ];

        return new(Codecs, $"{rows}: serialize into reused bytes", serialize);
    }

    private static Benchmark[] Lists(ItemList list)
    {
        var tafla = default(TaflaCodec).SerializeList(list);
        Variant[] serialize =
        [
            .. Loops.WithCopy(new SerializeList<TaflaCodec>(list)),
            Loops.Of<SerializeList<TaflaWriterCodec>, Original>(new(list)),
            Loops.Of<SerializeList<FlatBuffersCodec>, Original>(new(list)),
            Loops.Of<SerializeList<ProtobufCodec>, Original>(new(list)),
            Loops.Of<SerializeList<MessagePackCodec>, Original>(new(list)),
            Loops.Of<SerializeList<MemoryPackCodec>, Original>(new(list)),
        ];

        Variant[] deserialize =
        [
            .. Loops.WithCopy(new DeserializeList<TaflaCodec>(tafla, list)),
            Loops.Of<DeserializeList<FlatBuffersCodec>, Original>(new(default(FlatBuffersCodec).SerializeList(list), list)),
            Loops.Of<DeserializeList<ProtobufCodec>, Original>(new(default(ProtobufCodec).SerializeList(list), list)),
            Loops.Of<DeserializeList<MessagePackCodec>, Original>(new(default(MessagePackCodec).SerializeList(list), list)),
            Loops.Of<DeserializeList<MemoryPackCodec>, Original>(new(default(MemoryPackCodec).SerializeList(list), list)),
        ];

        return
        [
            new(Codecs, "vector of 100 nested tables: serialize", serialize),
            new(Codecs, "vector of 100 nested tables: deserialize", deserialize),
        ];
    }

    private static Benchmark[] Structs(ItemList list)
    {
        var tafla = default(TaflaCodec).SerializeStructs(list);
        Variant[] serialize =
        [
            .. Loops.WithCopy(new SerializeStructs<TaflaCodec>(list)),
            Loops.Of<SerializeStructs<TaflaWriterCodec>, Original>(new(list)),
            Loops.Of<SerializeStructs<FlatBuffersCodec>, Original>(new(list)),
            Loops.Of<SerializeStructs<ProtobufCodec>, Original>(new(list)),
            Loops.Of<SerializeStructs<MessagePackCodec>, Original>(new(list)),
            Loops.Of<SerializeStructs<MemoryPackCodec>, Original>(new(list)),
        ];

        Variant[] deserialize =
        [
            .. Loops.WithCopy(new DeserializeStructs<TaflaCodec>(tafla, list)),
            Loops.Of<DeserializeStructs<FlatBuffersCodec>, Original>(new(default(FlatBuffersCodec).SerializeStructs(list), list)),
            Loops.Of<DeserializeStructs<ProtobufCodec>, Original>(new(default(ProtobufCodec).SerializeStructs(list), list)),
            Loops.Of<DeserializeStructs<MessagePackCodec>, Original>(new(default(MessagePackCodec).SerializeStructs(list), list)),
            Loops.Of<DeserializeStructs<MemoryPackCodec>, Original>(new(default(MemoryPackCodec).SerializeStructs(list), list)),
        ];

        return
        [
            new(Codecs, "array of 100 structs: serialize", serialize),
            new(Codecs, "array of 100 structs: deserialize", deserialize),
        ];
    }

    private static Benchmark[] Samples(Sample sample)
    {
        var tafla = default(TaflaCodec).SerializeSample(sample);
        Variant[] serialize =
        [
            .. Loops.WithCopy(new SerializeSample<TaflaCodec>(sample)),
            Loops.Of<SerializeSample<FlatBuffersCodec>, Original>(new(sample)),
            Loops.Of<SerializeSample<ProtobufCodec>, Original>(new(sample)),
            Loops.Of<SerializeSample<MessagePackCodec>, Original>(new(sample)),
            Loops.Of<SerializeSample<MemoryPackCodec>, Original>(new(sample)),
            Loops.Of<SerializeSample<BinaryPrimitivesCodec>, Original>(new(sample)),
        ];

        Variant[] deserialize =
        [
            .. Loops.WithCopy(new DeserializeSample<TaflaCodec>(tafla, sample)),
            Loops.Of<DeserializeSample<FlatBuffersCodec>, Original>(new(default(FlatBuffersCodec).SerializeSample(sample), sample)),
            Loops.Of<DeserializeSample<ProtobufCodec>, Original>(new(default(ProtobufCodec).SerializeSample(sample), sample)),
            Loops.Of<DeserializeSample<MessagePackCodec>, Original>(new(default(MessagePackCodec).SerializeSample(sample), sample)),
            Loops.Of<DeserializeSample<MemoryPackCodec>, Original>(new(default(MemoryPackCodec).SerializeSample(sample), sample)),
            Loops.Of<DeserializeSample<BinaryPrimitivesCodec>, Original>(new(tafla, sample)),
        ];

        return [new(Codecs, "fixed-size record: serialize", serialize), new(Codecs, "fixed-size record: deserialize", deserialize)];
    }

    private static Benchmark RecordScalars(Record record)
    {
        var bytes = default(TaflaCodec).SerializeRecord(record);
        var sum = TaflaReads.RecordScalars.Expected(record);

        return new(Reads, "record: read the scalars", Loops.WithCopy(new Read<TaflaReads.RecordScalars>(bytes, sum)));
    }

    private static Benchmark ColumnByGetter(Document document)
    {
        var bytes = default(TaflaCodec).SerializeDocument(document);
        var sum = Workloads.SumColumn(document.A);

        return new(
            Reads,
            $"document of {document.A.Length:N0} rows: sum a column per getter",
            Loops.WithCopy(new Read<TaflaReads.ColumnByGetter>(bytes, sum))
        );
    }

    private static Benchmark[] ListItems(ItemList list)
    {
        var bytes = default(TaflaCodec).SerializeList(list);
        var last = TaflaReads.LastTable.Expected(list);
        var every = TaflaReads.EveryTable.Expected(list);

        return
        [
            new(Reads, "vector of 100 nested tables: read the last item", Loops.WithCopy(new Read<TaflaReads.LastTable>(bytes, last))),
            new(Reads, "vector of 100 nested tables: sum every item", Loops.WithCopy(new Read<TaflaReads.EveryTable>(bytes, every))),
        ];
    }

    private static Benchmark[] StructItems(ItemList list)
    {
        var bytes = default(TaflaCodec).SerializeStructs(list);
        var last = TaflaReads.LastTable.Expected(list);
        var every = TaflaReads.EveryTable.Expected(list);

        return
        [
            new(Reads, "array of 100 structs: read the last item", Loops.WithCopy(new Read<TaflaReads.LastStruct>(bytes, last))),
            new(Reads, "array of 100 structs: sum every item", Loops.WithCopy(new Read<TaflaReads.EveryStruct>(bytes, every))),
        ];
    }

    /// <summary>Returns bytes of <c>0xff</c>, which a write into them overwrites.</summary>
    private static byte[] Filled(int length)
    {
        var bytes = new byte[length];
        bytes.AsSpan().Fill(0xff);

        return bytes;
    }
}
