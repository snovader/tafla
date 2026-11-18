using Tafla.Benchmarks.Codecs;

namespace Tafla.Benchmarks.Tests;

// Every codec reads back exactly what it wrote, with floats compared by their bits, on every
// workload, and sums column a to the same bits, as the benchmarks check before they time a codec.
public class RoundTripTests
{
    public static TheoryData<string> CodecNames() => [.. EncodingReport.Codecs.Select(codec => codec.Name)];

    [Theory]
    [MemberData(nameof(CodecNames))]
    public void ReadsBackTheRecord(string name)
    {
        if (Codec(name) is not IRecordCodec codec)
        {
            return;
        }

        var record = Workloads.TheRecord;
        Assert.True(codec.DeserializeRecord(codec.SerializeRecord(record)).Same(record));
    }

    [Theory]
    [MemberData(nameof(CodecNames))]
    public void ReadsBackTheDocuments(string name)
    {
        if (Codec(name) is not IDocumentCodec codec)
        {
            return;
        }

        foreach (var rows in Workloads.DocumentRows)
        {
            var document = Workloads.NewDocument(rows);
            var bytes = codec.SerializeDocument(document);

            Assert.True(codec.DeserializeDocument(bytes).Same(document));
            Assert.Equal(Bits(Workloads.SumColumn(document.A)), Bits(codec.SumA(bytes)));
        }
    }

    [Theory]
    [MemberData(nameof(CodecNames))]
    public void ReadsBackTheVectorOfNestedTables(string name)
    {
        if (Codec(name) is not IListCodec codec)
        {
            return;
        }

        var list = Workloads.NewList(Workloads.ListItems);
        Assert.True(codec.DeserializeList(codec.SerializeList(list)).Same(list));
    }

    [Theory]
    [MemberData(nameof(CodecNames))]
    public void ReadsBackTheArrayOfStructs(string name)
    {
        if (Codec(name) is not IStructsCodec codec)
        {
            return;
        }

        var list = Workloads.NewList(Workloads.ListItems);
        Assert.True(codec.DeserializeStructs(codec.SerializeStructs(list)).Same(list));
    }

    [Theory]
    [MemberData(nameof(CodecNames))]
    public void ReadsBackTheFixedSizeRecord(string name)
    {
        if (Codec(name) is not ISampleCodec codec)
        {
            return;
        }

        var sample = Workloads.TheSample;
        Assert.True(codec.DeserializeSample(codec.SerializeSample(sample)).Same(sample));
    }

    [Fact]
    public void TableWriterWritesTheBuildersBytes()
    {
        var record = Workloads.TheRecord;
        var list = Workloads.NewList(Workloads.ListItems);
        TaflaCodec builder = default;
        TaflaWriterCodec writer = default;

        Assert.Equal(builder.SerializeRecord(record), writer.SerializeRecord(record));
        Assert.Equal(builder.SerializeList(list), writer.SerializeList(list));
        Assert.Equal(builder.SerializeStructs(list), writer.SerializeStructs(list));
        foreach (var rows in Workloads.DocumentRows)
        {
            var document = Workloads.NewDocument(rows);
            Assert.Equal(builder.SerializeDocument(document), writer.SerializeDocument(document));
        }
    }

    [Fact]
    public void TableWriterWritesIntoReusedBytes()
    {
        var document = Workloads.NewDocument(100);
        var expected = default(TaflaCodec).SerializeDocument(document);

        // Longer than the document, and not zeroed, so that the write must cover every byte it owns
        var reused = new byte[expected.Length + 8];
        reused.AsSpan().Fill(0xff);
        var length = default(TaflaWriterCodec).SerializeDocumentInto(document, reused);

        Assert.Equal(expected.Length, length);
        Assert.Equal(expected, reused.AsSpan(0, length).ToArray());
    }

    [Fact]
    public void EveryBenchmarkChecksWhatItsVariantsStore()
    {
        // Each variant's call once, and its check of what the call stored, as the program runs them
        // before timing
        foreach (var benchmark in Comparison.All())
        {
            Comparison.Check(benchmark);
        }
    }

    [Fact]
    public void ChecksFailForWrongResults()
    {
        var benchmark = Comparison.All().First(benchmark => benchmark.Name == "record: deserialize");
        var variant = benchmark.Variants[0];

        Sink.Clear();
        variant.Loop(1);
        Sink.Record = Sink.Record! with { X = -Sink.Record.X };

        Assert.False(variant.Check());
    }

    private static ICodec Codec(string name) => EncodingReport.Codecs.Single(codec => codec.Name == name);

    private static ulong Bits(double value) => BitConverter.DoubleToUInt64Bits(value);
}
