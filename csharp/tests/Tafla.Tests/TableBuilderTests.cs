using CsCheck;
using Tafla.Tests.Tables;

namespace Tafla.Tests;

/// <summary>
/// Builders reject what they can't write with a <see cref="BuildException"/>, and leave the builder as
/// it was, and throw argument exceptions for mistakes of the code that calls them.
/// </summary>
public class TableBuilderTests
{
    /// <summary>Tables of the offset types whose limits a test reaches: uint32's is 2 GiB</summary>
    private static readonly Gen<Table> ShortTables =
        Strategy.TablesOf(Strategy.LayoutsOf(Gen.OneOf(OffsetType.UInt8, OffsetType.UInt16)));

    /// <summary>
    /// After any fields, an array of bytes as long as fits is written and reads back, and a byte
    /// longer is rejected with the length that it would make the table
    /// </summary>
    [Fact]
    public void TheLongestFieldThatFitsIsWrittenAndAByteMoreIsRejected() =>
        ShortTables.Check(table =>
        {
            var withEmpty = table with { Fields = new Seq<Field>([.. table.Fields, new ArrayField(ScalarType.UInt8, Array.Empty<ulong>())]) };
            if (Fits(withEmpty) is not { } empty)
            {
                // The table is too full for another field's offset and count
                return;
            }

            var room = (int)(table.Layout.OffsetType.MaxValue - empty.Length);

            var builder = withEmpty.Builder();
            foreach (var field in table.Fields)
            {
                Build.Set(builder, field);
            }

            var error = Assert.Throws<BuildException>(() => builder.SetUInt8Array(new byte[room + 1]));
            Assert.Equal(BuildErrorKind.TooLong, error.Kind);
            Assert.Equal((table.Fields.Count, table.Layout.OffsetType.MaxValue + 1L, table.Layout.OffsetType), (error.Index, error.Length, error.OffsetType));

            // The builder is as it was, and takes the field that fits
            var longest = new ArrayField(ScalarType.UInt8, new ulong[room]);
            Build.Set(builder, longest);
            var bytes = builder.ToArray();

            var full = table with { Fields = new Seq<Field>([.. table.Fields, longest]) };
            Assert.Equal(table.Layout.OffsetType.MaxValue, (uint)bytes.Length);
            Assert.Equal(full, Read.Table(bytes, full));
        });

    /// <summary>Returns the bytes of a table, or null if it's too long for its offset type</summary>
    private static byte[]? Fits(Table table)
    {
        try
        {
            return table.Bytes();
        }
        catch (BuildException error) when (error.Kind == BuildErrorKind.TooLong)
        {
            return null;
        }
    }

    /// <summary>
    /// A field too long for the offset type, set after any of the fields, is rejected, and the builder
    /// writes the same table once the other fields are set
    /// </summary>
    [Fact]
    public void AFieldTooLongIsRejectedAndLeavesTheBuilderAsItWas() =>
        ShortTables.SelectMany(table => Gen.Int[0, table.Fields.Count].Select(at => (table, at))).Check(sample =>
        {
            var (table, at) = sample;
            var builder = table.Builder();

            for (var index = 0; index < table.Fields.Count; index++)
            {
                if (index == at)
                {
                    TooLong(builder, table.Layout.OffsetType, index);
                }

                Build.Set(builder, table.Fields[index]);
            }

            if (at == table.Fields.Count)
            {
                Assert.Equal(BuildErrorKind.TooManyFields, Assert.Throws<BuildException>(() => builder.SetString("")).Kind);
            }

            Assert.Equal(table.Bytes(), builder.ToArray());
        });

    /// <summary>Tries fields of every kind that are too long for the offset type, which each throw TooLong</summary>
    private static void TooLong(TableBuilder builder, OffsetType offsetType, int index)
    {
        var length = (int)offsetType.MaxValue + 1;
        var tooLong = new Action[]
        {
            () => builder.SetUInt8Array(new byte[length]),
            () => builder.SetFloat64Array(new double[length / 8]),
            () => builder.SetString(new string('é', length / 2)),
            () => builder.SetStruct(new byte[length]),
            () => builder.SetStructArray(new byte[length], 4, 4),
            () => builder.SetTable(new byte[length]),
            () => builder.SetVector(new byte[length / 2], new byte[length / 2]),
        };

        foreach (var set in tooLong)
        {
            var error = Assert.Throws<BuildException>(set);
            Assert.Equal((BuildErrorKind.TooLong, index, offsetType), (error.Kind, error.Index, error.OffsetType));
        }
    }

    [Fact]
    public void RejectsAFieldBeyondTheCount()
    {
        var builder = new TableBuilder(1);
        builder.SetUInt8(1);
        var expected = builder.ToArray();

        var error = Assert.Throws<BuildException>(() => builder.SetUInt8(2));
        Assert.Equal((BuildErrorKind.TooManyFields, 1), (error.Kind, error.Fields));
        Assert.Equal("Can't add field 2 to a table of 1", error.Message);
        Assert.Equal(expected, builder.ToArray());
    }

    [Fact]
    public void RejectsWritingBeforeEveryFieldIsSet()
    {
        for (var set = 0; set < 3; set++)
        {
            var builder = new TableBuilder(set + 1);
            for (var field = 0; field < set; field++)
            {
                builder.SetInt32(field);
            }

            var error = Assert.Throws<BuildException>(builder.ToArray);
            Assert.Equal((BuildErrorKind.Incomplete, set, set + 1), (error.Kind, error.FieldsSet, error.Fields));
            Assert.Equal($"Only {set} of the table's {set + 1} fields were set", error.Message);
        }
    }

    /// <summary>A nested builder must have every field set, and the parent is left as it was otherwise</summary>
    [Fact]
    public void RejectsAnIncompleteNestedBuilder()
    {
        var incomplete = new TableBuilder(2);
        incomplete.SetUInt8(1);

        var complete = new TableBuilder(0);
        var builder = new TableBuilder(1);

        Assert.Equal(BuildErrorKind.Incomplete, Assert.Throws<BuildException>(() => builder.SetTable(incomplete)).Kind);
        Assert.Equal(BuildErrorKind.Incomplete, Assert.Throws<BuildException>(() => builder.SetTable(incomplete, 8)).Kind);
        Assert.Equal(BuildErrorKind.Incomplete, Assert.Throws<BuildException>(() => builder.SetVector(complete, incomplete)).Kind);
        Assert.Equal(BuildErrorKind.Incomplete, Assert.Throws<BuildException>(() => builder.SetTable(builder)).Kind);

        builder.SetVector(complete, default(NestedTable));
        Assert.Equal(new byte[] { 4, 0, 0, 0, 2, 0, 0, 0, 12, 0, 0, 0, 12, 0, 0, 0 }, builder.ToArray());
    }

    [Fact]
    public void RejectsBytesThatArentWholeStructs()
    {
        var builder = new TableBuilder(1);

        var error = Assert.Throws<BuildException>(() => builder.SetStructArray(new byte[7], 2, 2));
        Assert.Equal((BuildErrorKind.NotWholeStructs, 7L, 2), (error.Kind, error.Length, error.StructSize));
        Assert.Equal("7 bytes aren't a whole number of 2-byte structs", error.Message);

        builder.SetStructArray(new byte[6], 3, 1);
        Assert.Equal(new byte[] { 4, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0 }, builder.ToArray());
    }

    [Theory]
    [InlineData(OffsetType.UInt8, false, false, 255)]
    [InlineData(OffsetType.UInt8, true, true, 253)]
    [InlineData(OffsetType.UInt16, false, false, 32767)]
    [InlineData(OffsetType.UInt16, true, false, 32766)]
    public void RejectsAHeaderTooLong(OffsetType offsetType, bool sized, bool versioned, int most)
    {
        var layout = new TableLayout { Sized = sized, Versioned = versioned, OffsetType = offsetType };
        TableBuilder Start(int fields) => versioned ? new TableBuilder(fields, layout, 1) : new TableBuilder(fields, layout);

        var builder = Start(most);
        for (var field = 0; field < most; field++)
        {
            builder.SetTable(ReadOnlySpan<byte>.Empty, 1);
        }

        Assert.Equal(layout.HeaderSize + most * layout.OffsetSize, builder.ToArray().Length);

        var error = Assert.Throws<BuildException>(() => Start(most + 1));
        Assert.Equal((BuildErrorKind.HeaderTooLong, most + 1, offsetType), (error.Kind, error.Fields, error.OffsetType));
        Assert.Throws<BuildException>(() => Start(int.MaxValue));
    }

    /// <summary>
    /// A table can't be longer than a .NET array, which a test can't allocate, but a vector that nests
    /// bytes of 64 MiB 32 times is that long without allocating more
    /// </summary>
    [Fact]
    public void RejectsATableOfUInt32OffsetsLongerThanAnArrayBeforeWritingIt()
    {
        var item = new byte[64 << 20];
        var builder = new TableBuilder(1);

        var error = Assert.Throws<BuildException>(() => builder.SetVector([.. Enumerable.Repeat<NestedTable>(item, 32)]));
        Assert.Equal((BuildErrorKind.TooLong, 4 + 33 * 4 + (32L << 26), OffsetType.UInt32), (error.Kind, error.Length, error.OffsetType));
    }

    [Theory]
    [InlineData(OffsetType.UInt8, 255u)]
    [InlineData(OffsetType.UInt16, 65535u)]
    public void RejectsAVersionTooLargeForItsSlot(OffsetType offsetType, uint largest)
    {
        var layout = new TableLayout { Versioned = true, OffsetType = offsetType };
        var buffer = new ByteBuffer(new TableBuilder(0, layout, largest).ToArray(), layout);
        Assert.Equal(largest, buffer.GetVersion());

        var error = Assert.Throws<BuildException>(() => new TableBuilder(0, layout, largest + 1));
        Assert.Equal((BuildErrorKind.VersionTooLarge, largest + 1, offsetType), (error.Kind, error.Version, error.OffsetType));
        Assert.Equal($"Version {largest + 1} doesn't fit in a {offsetType.Name} slot", error.Message);
    }

    /// <summary>Mistakes of the code that calls the builder, which no values cause, throw argument exceptions</summary>
    [Fact]
    public void ThrowsArgumentExceptionsForMistakesOfTheCaller()
    {
        var versioned = new TableLayout { Versioned = true };
        var builder = new TableBuilder(4);

        Assert.Throws<ArgumentOutOfRangeException>(() => new TableBuilder(-1));
        Assert.Throws<ArgumentException>(() => new TableBuilder(1, versioned));
        Assert.Throws<ArgumentException>(() => new TableBuilder(1, TableLayout.Default, 1));
        Assert.Throws<ArgumentOutOfRangeException>(() => builder.SetTable(new byte[4], 3));
        Assert.Throws<ArgumentOutOfRangeException>(() => builder.SetTable(new TableBuilder(0), 16));
        Assert.Throws<ArgumentOutOfRangeException>(() => builder.SetStructArray(new byte[4], 0, 1));
        Assert.Throws<ArgumentOutOfRangeException>(() => builder.SetStructArray(new byte[4], 4, 0));
        Assert.Throws<ArgumentNullException>(() => builder.SetString(null!));
        Assert.Throws<ArgumentNullException>(() => builder.SetTable((TableBuilder)null!));
        Assert.Throws<ArgumentNullException>(() => builder.SetVector((TableBuilder)null!));
        Assert.Throws<ArgumentNullException>(() => builder.SetVector((byte[])null!));

        // None of them set a field
        builder.SetUInt8(1);
        builder.SetUInt8(2);
        builder.SetUInt8(3);
        builder.SetUInt8(4);
        Assert.Equal(new byte[] { 16, 0, 0, 0, 17, 0, 0, 0, 18, 0, 0, 0, 19, 0, 0, 0, 1, 2, 3, 4 }, builder.ToArray());
    }

    /// <summary>
    /// A setter takes its field's own C# type, so a value of another converts as C# converts it: an
    /// integer out of range keeps its low bits in an unchecked cast, and a double narrows to the
    /// nearest float, or to infinity of its sign
    /// </summary>
    [Fact]
    public void ConvertsValuesAsCSharpConvertsThem()
    {
        var wide = 300;
        var large = 1e300;
        var builder = new TableBuilder(3);
        builder.SetUInt8(unchecked((byte)wide));
        builder.SetFloat32((float)large);
        builder.SetFloat32((float)0.1);

        var buffer = new ByteBuffer(builder.ToArray());
        Assert.Equal(44, buffer.GetUInt8(buffer.RootOffset(0)));
        Assert.Equal(float.PositiveInfinity, buffer.GetFloat32(buffer.RootOffset(1)));
        Assert.Equal(0x3dcc_cccdu, BitConverter.SingleToUInt32Bits(buffer.GetFloat32(buffer.RootOffset(2))));
    }
}
