using System;
using System.Globalization;
using Tafla.Benchmarks.Codecs;

namespace Tafla.Benchmarks;

/// <summary>
/// Prints each codec's encoded size of every workload, and what it does with a truncated document of
/// 100,000 rows, as the other languages' comparisons do.
/// </summary>
internal static class EncodingReport
{
    /// <summary>Every codec, in the order that reports list them.</summary>
    public static readonly ICodec[] Codecs =
    [
        default(TaflaCodec),
        default(TaflaWriterCodec),
        default(FlatBuffersCodec),
        default(ProtobufCodec),
        default(MessagePackCodec),
        default(MemoryPackCodec),
        default(BinaryPrimitivesCodec),
    ];

    public static void Print()
    {
        var record = Workloads.TheRecord;
        var small = Workloads.NewDocument(Workloads.DocumentRows[0]);
        var large = Workloads.NewDocument(Workloads.DocumentRows[1]);
        var list = Workloads.NewList(Workloads.ListItems);
        var sample = Workloads.TheSample;

        Console.WriteLine("Encoded size (bytes)");
        Console.WriteLine($"  {"",-20}{"record",12}{"doc 100",12}{"doc 100000",12}{"vector",12}{"structs",12}{"fixed",12}");
        foreach (var codec in Codecs)
        {
            var sizes = new string[6];
            sizes.AsSpan().Fill("");
            if (codec is IRecordCodec records)
            {
                sizes[0] = Size(records.SerializeRecord(record));
            }

            if (codec is IDocumentCodec documents)
            {
                sizes[1] = Size(documents.SerializeDocument(small));
                sizes[2] = Size(documents.SerializeDocument(large));
            }

            if (codec is IListCodec lists)
            {
                sizes[3] = Size(lists.SerializeList(list));
            }

            if (codec is IStructsCodec structs)
            {
                sizes[4] = Size(structs.SerializeStructs(list));
            }

            if (codec is ISampleCodec samples)
            {
                sizes[5] = Size(samples.SerializeSample(sample));
            }

            Console.WriteLine(
                $"  {codec.Name,-20}{sizes[0],12}{sizes[1],12}{sizes[2],12}{sizes[3],12}{sizes[4],12}{sizes[5],12}".TrimEnd()
            );
        }

        Console.WriteLine();
        Console.WriteLine("Truncated document of 100000 rows: deserialize");
        foreach (var codec in Codecs)
        {
            // The writer's bytes are the builder's, which the same reader reads
            if (codec is not IDocumentCodec documents || codec is TaflaWriterCodec)
            {
                continue;
            }

            var bytes = documents.SerializeDocument(large);
            foreach (var length in new[] { bytes.Length / 2, 24 })
            {
                Console.WriteLine($"  {codec.Name,-20}{length,8} B  {Outcome(documents, bytes.AsSpan(0, length).ToArray())}");
            }
        }
    }

    private static string Size(byte[] bytes) => bytes.Length.ToString(CultureInfo.InvariantCulture);

    /// <summary>Reads truncated bytes, and describes what happened.</summary>
    private static string Outcome(IDocumentCodec codec, byte[] bytes)
    {
        try
        {
            var document = codec.DeserializeDocument(bytes);
            return $"returned a document of {document.A.Length} rows";
        }
#pragma warning disable CA1031 // Reports whatever the library throws, which is what this shows
        catch (Exception exception)
#pragma warning restore CA1031
        {
            var description = $"threw {exception.GetType().Name}: {exception.Message}";
            return description.Length > 160 ? description[..157] + "..." : description;
        }
    }
}
