using CsCheck;
using Presence;
using Tafla.Tests.Tables;
using static Tafla.Tests.Generated.GeneratedDescriptions;
using static Tafla.Tests.Generated.Values;
using Record = Presence.Record;

namespace Tafla.Tests.Generated;

/// <summary>
/// The code generated from <c>typescript/core/test/presence.tafla</c>: optional fields, which its
/// writers leave out with an offset of 0 when they're null, and fields that a later version of a
/// message or a later revision of a table appends, which its readers find missing from the tables
/// that don't hold them. A missing string or struct reads as null, a missing scalar as its default,
/// and a missing array, vector or table as empty or the default reader, beside a <c>Has</c> property
/// that tells. <see cref="GeneratedMessageTests"/> writes and reads its messages with the others'.
/// </summary>
public class PresenceTests
{
    private static readonly Layout RecordLayout = new(Sized: true, Versioned: true, OffsetType.UInt32);

    // The defaults that the schema gives the scalars that may be missing
    private const bool FlagDefault = true;
    private const long TotalDefault = -1;
    private const double ZDefault = -0.5;
    private const float WeightDefault = 0.1f;
    private const ushort CountDefault = 7;

    /// <summary>The description of a field that the table doesn't hold, or that a reader finds missing</summary>
    private static readonly Absent Absent = new();

    private static readonly Gen<PointFields> Points = Gen.Select(
        Float64s,
        Float64s,
        Float64s,
        Optional(Strings),
        (x, y, z, label) => new PointFields { X = x, Y = y, Z = z, Label = label }
    );

    private static readonly Gen<EntryFields> Entries = Gen.Select(
        UInt16s,
        Optional(Strings),
        Float32s,
        (key, note, weight) => new EntryFields { Key = key, Note = note, Weight = weight }
    );

    private static readonly Gen<Extent> Extents = Gen.Select(
        Float32s,
        Float32s,
        (width, height) => new Extent { Width = width, Height = height }
    );

    /// <summary>A record's fields, each optional one null as often as not</summary>
    public static readonly Gen<RecordFields> Records = Gen.Select(
            UInt32s,
            Optional(Strings),
            Optional(ArraysOf(Float64s, 4)),
            Optional(Points),
            Optional(Gen.Bool),
            (id, name, values, origin, flag) =>
                new RecordFields { Id = id, Name = name, Values = values, Origin = origin, Flag = flag, Label = "" }
        )
        .SelectMany(record => Gen.Select(
            Optional(ArraysOf(Entries, 3)),
            Optional(Int64s),
            Strings,
            Optional(Extents),
            (entries, total, label, extent) => record with { Entries = entries, Total = total, Label = label, Extent = extent }
        ));

    private static readonly Gen<SampleFields> Samples = Gen.Select(
        UInt32s,
        UInt16s,
        (key, count) => new SampleFields { Key = key, Count = count }
    );

    public static readonly GeneratedMessage[] Messages =
    [
        new GeneratedMessage<RecordFields>(
            "presence.Record",
            Records,
            Describe,
            fields => fields.Serialize(),
            (fields, bytes) => fields.SerializeInto(bytes),
            bytes => Describe(new Record(bytes), Record.VersionOf(bytes))
        ),
        new GeneratedMessage<RecordV1Fields>(
            "presence.RecordV1",
            Records.Select(FirstVersionOf),
            Describe,
            fields => fields.Serialize(),
            (fields, bytes) => fields.SerializeInto(bytes),
            bytes => Describe(new RecordV1(bytes))
        ),
        new GeneratedMessage<SampleFields>(
            "presence.Sample",
            Samples,
            fields => Message(DefaultLayout, Of(fields.Key), Of(fields.Count)),
            fields => fields.Serialize(),
            (fields, bytes) => fields.SerializeInto(bytes),
            bytes => Describe(new Sample(bytes))
        ),
        new GeneratedMessage<SampleV1Fields>(
            "presence.SampleV1",
            UInt32s.Select(key => new SampleV1Fields { Key = key }),
            fields => Message(DefaultLayout, Of(fields.Key)),
            fields => fields.Serialize(),
            (fields, bytes) => fields.SerializeInto(bytes),
            bytes => Message(DefaultLayout, Of(new SampleV1(bytes).Key))
        ),
    ];

    // Optional fields

    [Fact]
    public void AnAbsentFieldIsWrittenAsTableBuilderSetAbsentWritesIt()
    {
        var written = new RecordFields { Id = 7, Label = "a" }.Serialize();

        var builder = new TableBuilder(9, RecordLayout.ToTableLayout(Record.FormatVersion));
        builder.SetUInt32(7);
        for (var index = 1; index < 7; index++)
        {
            builder.SetAbsent();
        }

        builder.SetString("a");
        builder.SetAbsent();

        Assert.Equal(builder.ToArray(), written);
    }

    /// <summary>
    /// Null leaves a field out, and an empty array or vector is written empty, which the reader tells
    /// apart from an absent one
    /// </summary>
    [Fact]
    public void AnEmptyArrayOrVectorIsWrittenAsOneRatherThanLeftOut()
    {
        var empty = new Record(new RecordFields { Id = 1, Values = Array.Empty<double>(), Entries = Array.Empty<EntryFields>(), Label = "" }.Serialize());
        var absent = new Record(new RecordFields { Id = 1, Label = "" }.Serialize());

        Assert.True(empty.HasValues && empty.Values.IsEmpty);
        Assert.True(empty.HasEntries && empty.Entries.Count == 0);
        Assert.False(absent.HasValues || absent.HasEntries);
        Assert.True(absent.Values.IsEmpty);
        Assert.Equal(0, absent.Entries.Count);
    }

    /// <summary>
    /// A missing table reads as its default reader, whose reads throw a DecodeException, and a missing
    /// vector as an empty one, which an enumerator reads no items of
    /// </summary>
    [Fact]
    public void AMissingTableIsTheDefaultReaderAndAMissingVectorIsEmpty()
    {
        var record = new Record(new RecordFields { Id = 1, Label = "" }.Serialize());

        Assert.False(record.HasOrigin);
        var origin = record.Origin;
        DecodeException? error = null;
        try
        {
            _ = origin.X;
        }
        catch (DecodeException exception)
        {
            error = exception;
        }

        Assert.Equal(DecodeErrorKind.OutOfRange, error?.Kind);

        foreach (var entry in record.Entries)
        {
            Assert.Fail($"A missing vector holds the item {entry.Key}");
        }
    }

    // Versions and revisions

    [Fact]
    public void ARecordOfTheFirstVersionReadsWithTheFieldsThatTheSecondAddedMissingInNestedTablesAndItemsToo() =>
        Records.Select(FirstVersionOf).Check(fields =>
        {
            var bytes = fields.Serialize();

            Assert.Equal(FirstVersion(fields), Describe(new Record(bytes), Record.VersionOf(bytes)));
        });

    [Fact]
    public void ARecordOfAVersionBeforeTheFirstOrAfterItsOwnThrowsDecodeException() =>
        Gen.Select(Records, Gen.Frequency<uint>((1, Gen.UInt[0, 3]), (1, Gen.UInt))).Check(sample =>
        {
            var (fields, version) = sample;
            var bytes = fields.Serialize();
            System.Buffers.Binary.BinaryPrimitives.WriteUInt32LittleEndian(bytes.AsSpan(sizeof(uint)), version);

            Assert.Equal(version, Record.VersionOf(bytes));
            if (version is >= Record.FirstVersion and <= Record.FormatVersion)
            {
                // A table of the first version lacks the label, whatever its bytes hold
                var record = new Record(bytes);
                Assert.Equal(fields.Id, record.Id);
                Assert.Equal(version == Record.FormatVersion ? fields.Label : null, record.Label);
                return;
            }

            var exception = Assert.Throws<DecodeException>(() => new Record(bytes));
            Assert.Equal((DecodeErrorKind.UnsupportedVersion, version, Record.FormatVersion), (exception.Kind, exception.Version, exception.SupportedVersion));
        });

    [Fact]
    public void AReaderOfOneVersionKeepsRejectingTheOthers()
    {
        var bytes = new RecordFields { Id = 1, Label = "" }.Serialize();

        Assert.Equal(DecodeErrorKind.UnsupportedVersion, Assert.Throws<DecodeException>(() => new RecordV1(bytes)).Kind);
    }

    /// <summary>
    /// A field that a later revision appended is missing from a table whose offsets hold it when its
    /// offset is 0, as it is when it lies in the padding before an aligned first value. The writers
    /// write every field of their revision, so the tables here are built by hand.
    /// </summary>
    [Fact]
    public void AFieldAddedSinceARevisionWhoseOffsetIs0IsMissingAtTheRootAndInAVectorsItems()
    {
        var sample = new TableBuilder(2);
        sample.SetUInt32(5);
        sample.SetAbsent();
        var read = new Sample(sample.ToArray());

        Assert.Equal((5u, CountDefault, false), (read.Key, read.Count, read.HasCount));

        var entry = new TableBuilder(3);
        entry.SetUInt16(2);
        entry.SetAbsent();
        entry.SetAbsent();
        var record = new TableBuilder(9, RecordLayout.ToTableLayout(Record.FormatVersion));
        record.SetUInt32(1);
        for (var index = 1; index < 5; index++)
        {
            record.SetAbsent();
        }

        record.SetVector(entry);
        record.SetAbsent();
        record.SetString("");
        record.SetAbsent();

        var entries = new Record(record.ToArray()).Entries;
        Assert.Equal(1, entries.Count);
        var item = entries[0];
        Assert.Equal(((ushort)2, WeightDefault, false), (item.Key, item.Weight, item.HasWeight));
        Assert.Null(item.Note);
    }

    [Fact]
    public void ASampleOfTheFirstRevisionReadsWithTheDefaultOfTheFieldThatTheSecondAppended() =>
        Samples.Check(fields =>
        {
            var first = new SampleV1Fields { Key = fields.Key }.Serialize();
            var second = fields.Serialize();

            Assert.Equal((fields.Key, CountDefault, false), (new Sample(first).Key, new Sample(first).Count, new Sample(first).HasCount));
            Assert.Equal((fields.Count, true), (new Sample(second).Count, new Sample(second).HasCount));

            // A reader of the first revision reads its fields of a later one
            Assert.Equal(fields.Key, new SampleV1(second).Key);
        });

    // Untrusted bytes

    [Fact]
    public void AnyBytesReadAsASampleWithoutAVersionAsValuesOrThrowDecodeException() =>
        Gen.Byte.Array[0, 48].Check(bytes =>
        {
            GeneratedMessageTests.ReadsAsValuesOrThrowsDecodeException(() => new Sample(bytes).Count);
            GeneratedMessageTests.ReadsAsValuesOrThrowsDecodeException(() => new Sample(bytes).HasCount);
            GeneratedMessageTests.ReadsAsValuesOrThrowsDecodeException(() => new Sample(bytes).Key);
        });

    // Descriptions of the fields that are written, and of those that the generated code reads, with
    // each absent or missing field as absent, once its getter returned the missing value

    public static Table Describe(RecordFields fields) =>
        Message(
            RecordLayout,
            Record.FormatVersion,
            Of(fields.Id),
            Optional(fields.Name),
            Optional(fields.Values, values => Of(values.Span)),
            Optional(fields.Origin, origin => Nested(Of(origin.X), Of(origin.Y), Of(origin.Z), Optional(origin.Label))),
            Optional(fields.Flag, Of),
            Optional(fields.Entries, entries => Vector(entries.ToArray().Select(DescribeEntry))),
            Optional(fields.Total, Of),
            Of(fields.Label),
            Optional(fields.Extent, extent => Struct(Of(extent.Width), Of(extent.Height)))
        );

    public static Table Describe(RecordV1Fields fields) =>
        Message(
            RecordLayout,
            RecordV1.FormatVersion,
            Of(fields.Id),
            Optional(fields.Name),
            Optional(fields.Values, values => Of(values.Span)),
            Optional(fields.Origin, origin => Nested(Of(origin.X), Of(origin.Y))),
            Optional(fields.Flag, Of),
            Optional(fields.Entries, entries => Vector(entries.ToArray().Select(entry => Nested(Of(entry.Key), Optional(entry.Note))))),
            Optional(fields.Total, Of)
        );

    /// <summary>
    /// The fields of a record of the first version as a reader of the second reads them: those that
    /// the second version added missing, in its nested table and the items of its vector too
    /// </summary>
    private static Table FirstVersion(RecordV1Fields fields) =>
        Message(
            RecordLayout,
            RecordV1.FormatVersion,
            Of(fields.Id),
            Optional(fields.Name),
            Optional(fields.Values, values => Of(values.Span)),
            Optional(fields.Origin, origin => Nested(Of(origin.X), Of(origin.Y), Absent, Absent)),
            Optional(fields.Flag, Of),
            Optional(fields.Entries, entries => Vector(entries.ToArray().Select(entry => Nested(Of(entry.Key), Optional(entry.Note), Absent)))),
            Optional(fields.Total, Of),
            Absent,
            Absent
        );

    /// <summary>A record of the first version, of the fields of the second that it has</summary>
    private static RecordV1Fields FirstVersionOf(RecordFields fields) =>
        new()
        {
            Id = fields.Id,
            Name = fields.Name,
            Values = fields.Values,
            Origin = fields.Origin is { } origin ? new PointV1Fields { X = origin.X, Y = origin.Y } : null,
            Flag = fields.Flag,
            // Null rather than a null array, which would convert to an empty vector
            Entries = fields.Entries is { } entries
                ? entries.ToArray().Select(entry => new EntryV1Fields { Key = entry.Key, Note = entry.Note }).ToArray()
                : (ReadOnlyMemory<EntryV1Fields>?)null,
            Total = fields.Total,
        };

    private static TableField DescribeEntry(EntryFields entry) => Nested(Of(entry.Key), Optional(entry.Note), Of(entry.Weight));

    /// <summary>
    /// Reads every field of a record through its getters, of the record's <paramref name="version"/>,
    /// with the items of its vector through their enumerator
    /// </summary>
    public static Table Describe(Record record, uint version) =>
        Message(
            RecordLayout,
            version,
            Of(record.Id),
            Optional(record.Name),
            record.HasValues ? Of(record.Values) : Missing(record.Values.IsEmpty),
            record.HasOrigin ? Describe(record.Origin) : Absent,
            record.HasFlag ? Of(record.Flag) : Missing(Of(record.Flag), Of(FlagDefault)),
            record.HasEntries ? Vector(DescribeEntries(record.Entries)) : Missing(record.Entries.Count == 0),
            record.HasTotal ? Of(record.Total) : Missing(Of(record.Total), Of(TotalDefault)),
            Optional(record.Label),
            Optional(record.Extent, extent => Struct(Of(extent.Width), Of(extent.Height)))
        );

    public static Table Describe(RecordV1 record)
    {
        var entries = new List<TableField>();
        foreach (var entry in record.Entries)
        {
            entries.Add(Nested(Of(entry.Key), Optional(entry.Note)));
        }

        var origin = record.Origin;
        return Message(
            RecordLayout,
            RecordV1.FormatVersion,
            Of(record.Id),
            Optional(record.Name),
            record.HasValues ? Of(record.Values) : Missing(record.Values.IsEmpty),
            record.HasOrigin ? Nested(Of(origin.X), Of(origin.Y)) : Absent,
            record.HasFlag ? Of(record.Flag) : Missing(Of(record.Flag), Of(FlagDefault)),
            record.HasEntries ? Vector(entries) : Missing(entries.Count == 0),
            record.HasTotal ? Of(record.Total) : Missing(Of(record.Total), Of(TotalDefault))
        );
    }

    private static TableField Describe(Point point) =>
        Nested(
            Of(point.X),
            Of(point.Y),
            point.HasZ ? Of(point.Z) : Missing(Of(point.Z), Of(ZDefault)),
            Optional(point.Label)
        );

    private static List<TableField> DescribeEntries(EntryVector entries)
    {
        var described = new List<TableField>();
        foreach (var entry in entries)
        {
            Field weight = entry.HasWeight ? Of(entry.Weight) : Missing(Of(entry.Weight), Of(WeightDefault));
            described.Add(Nested(Of(entry.Key), Optional(entry.Note), weight));
        }

        return described;
    }

    private static Table Describe(Sample sample) =>
        Message(DefaultLayout, Of(sample.Key), sample.HasCount ? Of(sample.Count) : Missing(Of(sample.Count), Of(CountDefault)));

    /// <summary>A value, or null as often as not</summary>
    private static Gen<T?> Optional<T>(Gen<T> values)
        where T : struct => Gen.Frequency<T?>((1, Gen.Const(default(T?))), (1, values.Select(value => (T?)value)));

    /// <summary>A string, or null as often as not</summary>
    private static Gen<string?> Optional(Gen<string> values) =>
        Gen.Frequency<string?>((1, Gen.Const(default(string?))), (1, values.Select(value => (string?)value)));

    private static Field Optional(string? value) => value is null ? Absent : Of(value);

    private static Field Optional<T>(T? value, Func<T, Field> describe)
        where T : struct => value is { } present ? describe(present) : Absent;

    /// <summary>A field that the table doesn't hold, once its getter returned <paramref name="missing"/>, the default</summary>
    private static Absent Missing(Field read, Field missing)
    {
        Assert.Equal(missing, read);
        return Absent;
    }

    /// <summary>A field that the table doesn't hold, once its getter returned an empty span or view</summary>
    private static Absent Missing(bool empty)
    {
        Assert.True(empty);
        return Absent;
    }
}
