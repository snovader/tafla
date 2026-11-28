using CsCheck;
using Tafla.Tests.Tables;
using static Tafla.Tests.Generated.GeneratedDescriptions;
using static Tafla.Tests.Generated.Values;

namespace Tafla.Tests.Generated;

/// <summary>
/// The vectors of strings of <c>schemas/all_types.tafla</c>, whose getters return the runtime's
/// <see cref="StringVector"/> and whose fields hold them as <see cref="ReadOnlyMemory{T}"/>s of
/// strings: <c>StringVectors</c>, with one beside other fields, in a nested table and in the items of
/// a vector, and <c>NarrowStringVectors</c>, of uint16 offsets, which
/// <see cref="GeneratedMessageTests"/> writes and reads with the other messages.
/// </summary>
public class StringVectorsTests
{
    private static readonly Gen<ReadOnlyMemory<string>> StringLists = ArraysOf(Strings, 6);

    private static readonly Gen<AllTypes.EntryFields> Entries = Gen.Select(
        Strings,
        StringLists,
        (key, aliases) => new AllTypes.EntryFields { Key = key, Aliases = aliases }
    );

    private static readonly Gen<AllTypes.StringVectorsFields> StringVectorsFields = Gen.Select(
        UInt32s,
        StringLists,
        StringLists,
        ArraysOf(Entries, 3),
        (id, tags, labels, entries) => new AllTypes.StringVectorsFields
        {
            Id = id,
            Tags = tags,
            Labels = new() { Values = labels },
            Entries = entries,
        }
    );

    /// <summary>Names of up to 8 characters, which fit in a table of uint16 offsets</summary>
    private static readonly Gen<AllTypes.NarrowStringVectorsFields> NarrowStringVectorsFields = Gen.Select(
        ArraysOf(Strategy.String(8), 6),
        UInt8s,
        (names, count) => new AllTypes.NarrowStringVectorsFields { Names = names, Count = count }
    );

    private static readonly Layout NarrowLayout = Layout.Headerless(OffsetType.UInt16);

    public static readonly GeneratedMessage[] Messages =
    [
        new GeneratedMessage<AllTypes.StringVectorsFields>(
            "all_types.StringVectors",
            StringVectorsFields,
            Describe,
            fields => fields.Serialize(),
            (fields, bytes) => fields.SerializeInto(bytes),
            bytes => Describe(new AllTypes.StringVectors(bytes))
        ),
        new GeneratedMessage<AllTypes.NarrowStringVectorsFields>(
            "all_types.NarrowStringVectors",
            NarrowStringVectorsFields,
            Describe,
            fields => fields.Serialize(),
            (fields, bytes) => fields.SerializeInto(bytes),
            bytes => Describe(new AllTypes.NarrowStringVectors(bytes))
        ),
    ];

    [Fact]
    public void AViewsIndexerAndToArrayReadTheStringsThatItsEnumeratorReads() =>
        StringVectorsFields.Check(fields =>
        {
            var read = new AllTypes.StringVectors(fields.Serialize());

            AssertReadAlike(read.Tags);
            AssertReadAlike(read.Labels.Values);
            foreach (var entry in read.Entries)
            {
                AssertReadAlike(entry.Aliases);
            }
        });

    /// <summary>
    /// The view's strings are read from ends that corrupted bytes can lead anywhere, through the
    /// indexers, as values or a DecodeException, as they're read through the enumerators
    /// </summary>
    [Fact]
    public void CorruptedBytesReadThroughTheIndexersAsValuesOrThrowDecodeException()
    {
        var corruptions = Gen.Select(Gen.Int[0, int.MaxValue], Gen.Byte).Array[1, 4];

        Gen.Select(StringVectorsFields, corruptions).Check(sample =>
        {
            var (fields, replacements) = sample;
            var bytes = fields.Serialize();
            foreach (var (position, value) in replacements)
            {
                bytes[position % bytes.Length] = value;
            }

            GeneratedMessageTests.ReadsAsValuesOrThrowsDecodeException(() =>
            {
                var read = new AllTypes.StringVectors(bytes);
                var entries = read.Entries;
                var aliases = new List<List<string>>();
                for (var index = 0; index < entries.Count; index++)
                {
                    aliases.Add(Indexed(entries[index].Aliases));
                }

                return (Indexed(read.Tags), Indexed(read.Labels.Values), aliases);
            });
        });
    }

    [Fact]
    public void NarrowStringVectorsTooLongForUInt16OffsetsThrowBuildException()
    {
        var fields = new AllTypes.NarrowStringVectorsFields { Names = new[] { new string('a', 0x8000), new string('b', 0x8000) }, Count = 0 };

        var error = Assert.Throws<BuildException>(() => fields.Serialize());
        Assert.Equal((BuildErrorKind.TooLong, OffsetType.UInt16), (error.Kind, error.OffsetType));
        Assert.Equal(BuildErrorKind.TooLong, Assert.Throws<BuildException>(() => fields.SerializeInto(new byte[0x2_0000])).Kind);
    }

    /// <summary>The second tag's first byte, after its vector's count, two ends and "ok", made a lead byte that nothing continues</summary>
    [Fact]
    public void ATagThatIsntValidUtf8FailsAndTheOtherFieldsRead()
    {
        var fields = new AllTypes.StringVectorsFields
        {
            Id = 1,
            Tags = new[] { "ok", "bad" },
            Labels = new() { Values = new[] { "label" } },
            Entries = default,
        };
        var bytes = fields.Serialize();
        var reader = new TableReader(bytes);
        bytes[reader.RootOffset(1) + 3 * sizeof(uint) + 2] = 0xc0;

        var read = new AllTypes.StringVectors(bytes);
        Assert.Equal(1u, read.Id);
        Assert.Equal("ok", read.Tags[0]);
        var error = Assert.Throws<DecodeException>(() => new AllTypes.StringVectors(bytes).Tags[1]);
        Assert.Equal(DecodeErrorKind.InvalidUtf8, error.Kind);
        Assert.Equal(DecodeErrorKind.InvalidUtf8, Assert.Throws<DecodeException>(() => new AllTypes.StringVectors(bytes).Tags.ToArray()).Kind);
        Assert.Equal(["label"], read.Labels.Values.ToArray());
    }

    [Fact]
    public void ADefaultReadersGetterThrowsDecodeException()
    {
        Assert.Throws<DecodeException>(() => default(AllTypes.StringVectors).Tags.Count);
        Assert.Throws<DecodeException>(() => default(AllTypes.Labels).Values.Count);
        Assert.Throws<DecodeException>(() => default(AllTypes.NarrowStringVectors).Names.Count);
    }

    // Descriptions of the fields that are written, and of those that the generated code reads

    public static Table Describe(AllTypes.StringVectorsFields fields) =>
        Message(
            DefaultLayout,
            Of(fields.Id),
            Of(fields.Tags.Span),
            Nested(Of(fields.Labels.Values.Span)),
            Vector(fields.Entries.ToArray().Select(entry => Nested(Of(entry.Key), Of(entry.Aliases.Span))))
        );

    public static Table Describe(AllTypes.StringVectors read)
    {
        var entries = new List<TableField>();
        foreach (var entry in read.Entries)
        {
            entries.Add(Nested(Of(entry.Key), Of(entry.Aliases)));
        }

        return Message(DefaultLayout, Of(read.Id), Of(read.Tags), Nested(Of(read.Labels.Values)), Vector(entries));
    }

    public static Table Describe(AllTypes.NarrowStringVectorsFields fields) =>
        Message(NarrowLayout, Of(fields.Names.Span), Of(fields.Count));

    public static Table Describe(AllTypes.NarrowStringVectors read) => Message(NarrowLayout, Of(read.Names), Of(read.Count));

    /// <summary>Checks that a view reads, by index and all at once, the strings that its enumerator reads</summary>
    private static void AssertReadAlike(StringVector strings)
    {
        var inOrder = Of(strings).Values;

        Assert.Equal(inOrder, Indexed(strings));
        Assert.Equal(inOrder, strings.ToArray());
    }

    private static List<string> Indexed(StringVector strings)
    {
        var values = new List<string>();
        for (var index = 0; index < strings.Count; index++)
        {
            values.Add(strings[index]);
        }

        return values;
    }
}
