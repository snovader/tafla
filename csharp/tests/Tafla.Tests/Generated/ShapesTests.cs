using CsCheck;
using Shapes;
using Tafla.Tests.Tables;
using static Tafla.Tests.Generated.GeneratedDescriptions;
using static Tafla.Tests.Generated.Values;

namespace Tafla.Tests.Generated;

/// <summary>
/// The code generated from <c>schemas/shapes.tafla</c>: its messages, which
/// <see cref="GeneratedMessageTests"/> writes and reads with the others, its document's version, its
/// vector's items in place, and its struct.
/// </summary>
public class ShapesTests
{
    private static readonly Layout DocumentLayout = new(Sized: true, Versioned: true, OffsetType.UInt32);

    public static readonly Gen<PointFields> Points = Gen.Select(
        Float64s,
        Float64s,
        Int64s,
        Strings,
        (x, y, id, name) => new PointFields { X = x, Y = y, Id = id, Name = name }
    );

    private static readonly Gen<ColumnsFields> Columnses = Gen.Select(
        Int64s,
        ArraysOf(UInt32s),
        ArraysOf(Float32s),
        ArraysOf(Float32s),
        (@base, keys, a, b) => new ColumnsFields { Base = @base, Keys = keys, A = a, B = b }
    );

    public static readonly Gen<DocumentFields> Documents = Gen.Select(
        Float64s,
        Float64s,
        Strings,
        Columnses,
        (x, y, name, columns) => new DocumentFields { X = x, Y = y, Name = name, Columns = columns }
    );

    private static readonly Gen<ItemFields> Items = Gen.Select(
        Float32s,
        Float32s,
        Float32s,
        (value, first, second) => new ItemFields { Value = value, Pair = new() { First = first, Second = second } }
    );

    public static readonly Gen<ItemListFields> ItemLists = ArraysOf(Items, 6).Select(items => new ItemListFields { Items = items });

    private static readonly Gen<Sample> Samples = Gen.Select(
        UInt32s,
        Float32s,
        Float32s,
        (key, a, b) => new Sample { Key = key, A = a, B = b }
    );

    public static readonly GeneratedMessage[] Messages =
    [
        new GeneratedMessage<PointFields>(
            "shapes.Point",
            Points,
            Describe,
            fields => fields.Serialize(),
            (fields, bytes) => fields.SerializeInto(bytes),
            bytes => Describe(new Point(bytes))
        ),
        new GeneratedMessage<DocumentFields>(
            "shapes.Document",
            Documents,
            Describe,
            fields => fields.Serialize(),
            (fields, bytes) => fields.SerializeInto(bytes),
            bytes => Describe(new Document(bytes))
        ),
        new GeneratedMessage<ItemListFields>(
            "shapes.ItemList",
            ItemLists,
            Describe,
            fields => fields.Serialize(),
            (fields, bytes) => fields.SerializeInto(bytes),
            bytes => Describe(new ItemList(bytes))
        ),
    ];

    // Versions

    [Fact]
    public void ADocumentsVersionIsReadFromItsBytesOfAnyVersionToChooseItsReader() =>
        Gen.Select(Documents, Gen.UInt).Check(sample =>
        {
            var (fields, version) = sample;
            var bytes = (Describe(fields) with { Version = version }).Bytes();

            Assert.Equal(version, Document.VersionOf(bytes));
        });

    [Fact]
    public void TheVersionOfBytesWithoutADocumentsHeaderThrowsDecodeException() =>
        Gen.Select(Documents, Gen.Int[0, 7]).Check(sample =>
        {
            var (fields, length) = sample;
            var bytes = fields.Serialize()[..length];

            var exception = Assert.Throws<DecodeException>(() => Document.VersionOf(bytes));
            Assert.Equal(DecodeErrorKind.OutOfRange, exception.Kind);
        });

    [Fact]
    public void ADocumentOfAnotherVersionThrowsDecodeException() =>
        Gen.Select(Documents, Gen.UInt.Where(version => version != Document.FormatVersion)).Check(sample =>
        {
            var (fields, version) = sample;
            var bytes = (Describe(fields) with { Version = version }).Bytes();

            var exception = Assert.Throws<DecodeException>(() => new Document(bytes));
            Assert.Equal(DecodeErrorKind.UnsupportedVersion, exception.Kind);
            Assert.Equal(version, exception.Version);
            Assert.Equal(Document.FormatVersion, exception.SupportedVersion);
        });

    // Reading in place

    [Fact]
    public void AnItemIsReadInPlacePastItsEndWithinTheBytesOfItsItemList()
    {
        var item = new ItemFields { Value = 1.5f, Pair = new() { First = 2.5f, Second = 3.5f } };
        var bytes = new ItemListFields { Items = new[] { item } }.Serialize();

        // The vector at 4 holds its count and the item's end, 36, and then the item at 12: its two
        // offsets, its value at 20 and its Pair at 24. An end of 16 leaves the item only its offsets,
        // which point past it.
        Assert.Equal([36, 0, 0, 0], bytes[8..12]);
        bytes[8] = 16;
        var items = new ItemList(bytes).Items;

        Assert.Equal(DescribeItem(item), DescribeItem(items[0]));
        Assert.Equal(DescribeItem(item), DescribeItem(Assert.Single(items)));
    }

    [Fact]
    public void ACorruptedItemThrowsDecodeExceptionWhenItsReadRatherThanFromTheGetter()
    {
        var first = new ItemFields { Value = 1.5f, Pair = new() { First = 2.5f, Second = 3.5f } };
        var second = new ItemFields { Value = 4.5f, Pair = new() { First = 5.5f, Second = 6.5f } };
        var bytes = new ItemListFields { Items = new[] { first, second } }.Serialize();

        // The second item's end, after the vector's count and the first item's end, past the bytes
        var vector = new TableReader(bytes).RootOffset(0);
        bytes.AsSpan(vector + 2 * sizeof(uint), sizeof(uint)).Fill(0x7f);

        var items = new ItemList(bytes).Items;

        Assert.Equal(2, items.Count);
        Assert.Equal(DescribeItem(first), DescribeItem(items[0]));
        Assert.Throws<DecodeException>(() => items[1]);

        var enumerator = items.GetEnumerator();
        Assert.True(enumerator.MoveNext());
        Assert.Equal(DescribeItem(first), DescribeItem(enumerator.Current));
        Assert.Throws<DecodeException>(() => enumerator.MoveNext());
    }

    // The struct

    [Fact]
    public void ASampleIsWrittenInIts12BytesLittleEndianAtFixedOffsets() =>
        Samples.Check(sample =>
        {
            Assert.Equal(12, Sample.Size);
            Assert.Equal(Scalars.StructBytes(Describe(sample).Scalars), sample.Serialize());
        });

    [Fact]
    public void ASampleReadsBackFromTheStartOfLongerBytes() =>
        Gen.Select(Samples, Gen.Byte.Array[0, 8]).Check(sample =>
        {
            var (written, after) = sample;
            byte[] bytes = [.. written.Serialize(), .. after];

            Assert.Equal(Describe(written), Describe(new Sample(bytes)));
        });

    [Fact]
    public void ReadingASampleFromFewerThan12BytesThrowsDecodeException() =>
        Gen.Byte.Array[0, Sample.Size - 1].Check(bytes =>
        {
            var exception = Assert.Throws<DecodeException>(() => new Sample(bytes));
            Assert.Equal(DecodeErrorKind.OutOfRange, exception.Kind);
            Assert.Equal(Sample.Size, exception.ByteLength);
            Assert.Equal(bytes.Length, exception.Available);
        });

    [Fact]
    public void WritingASampleIntoFewerThan12BytesThrowsBeforeAnyIsWritten() =>
        Gen.Select(Samples, Gen.Int[0, Sample.Size - 1]).Check(sample =>
        {
            var (written, length) = sample;
            var bytes = new byte[length];
            bytes.AsSpan().Fill(0xa5);

            Assert.Throws<ArgumentOutOfRangeException>(() => written.Write(bytes));
            Assert.All(bytes, value => Assert.Equal(0xa5, value));
        });

    [Fact]
    public void SamplesAreSerializedOneAfterAnother() =>
        Samples.Array[0, 4].Check(samples =>
        {
            var bytes = Sample.SerializeArray(samples);

            Assert.Equal(samples.SelectMany(sample => sample.Serialize()), bytes);
        });

    // Descriptions of the fields that are written, and of those that the generated code reads

    public static Table Describe(PointFields fields) =>
        Message(DefaultLayout, Of(fields.X), Of(fields.Y), Of(fields.Id), Of(fields.Name));

    public static Table Describe(Point point) => Message(DefaultLayout, Of(point.X), Of(point.Y), Of(point.Id), Of(point.Name));

    public static Table Describe(DocumentFields fields) =>
        Message(
            DocumentLayout,
            Document.FormatVersion,
            Of(fields.X),
            Of(fields.Y),
            Of(fields.Name),
            Nested(Of(fields.Columns.Base), Of(fields.Columns.Keys.Span), Of(fields.Columns.A.Span), Of(fields.Columns.B.Span))
        );

    public static Table Describe(Document document)
    {
        var columns = document.Columns;
        return Message(
            DocumentLayout,
            Document.FormatVersion,
            Of(document.X),
            Of(document.Y),
            Of(document.Name),
            Nested(Of(columns.Base), Of(columns.Keys), Of(columns.A), Of(columns.B))
        );
    }

    public static Table Describe(ItemListFields fields) =>
        Message(DefaultLayout, Vector(fields.Items.ToArray().Select(DescribeItem)));

    public static Table Describe(ItemList itemList) => Message(DefaultLayout, Vector(itemList.Items.Select(DescribeItem)));

    private static TableField DescribeItem(ItemFields item) =>
        Nested(Of(item.Value), Nested(Of(item.Pair.First), Of(item.Pair.Second)));

    private static TableField DescribeItem(Item item) => Nested(Of(item.Value), Nested(Of(item.Pair.First), Of(item.Pair.Second)));

    private static StructField Describe(Sample sample) => Struct(Of(sample.Key), Of(sample.A), Of(sample.B));
}
