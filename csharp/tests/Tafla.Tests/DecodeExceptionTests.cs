using System.Buffers.Binary;
using CsCheck;
using Tafla.Tests.Tables;

namespace Tafla.Tests;

/// <summary>
/// Readers decode untrusted bytes: any bytes, and valid tables cut short or overwritten, read as
/// values or fail with a <see cref="DecodeException"/>, and never with another exception. The
/// examples document the errors.
/// </summary>
public class DecodeExceptionTests
{
    /// <summary>A field that lies before the cut reads the value that the table was built with, and any other fails with OutOfRange</summary>
    [Fact]
    public void TruncatedTablesReadTheirValuesOrOutOfRange() =>
        Strategy.Tables.Where(table => table.Bytes().Length > 0)
            .SelectMany(table => Gen.Int[0, table.Bytes().Length - 1].Array[1, 16].Select(cuts => (table, cuts)))
            .Check(sample =>
            {
                var (table, cuts) = sample;
                var built = table.Bytes();

                foreach (var length in cuts)
                {
                    // A copy, so that no read can reach the bytes after the cut
                    var truncated = built[..length];

                    for (var index = 0; index < table.Fields.Count; index++)
                    {
                        var field = table.Fields[index];
                        try
                        {
                            Assert.Equal(field, Read.RootField(truncated, table.Layout, index, field));
                        }
                        catch (DecodeException error)
                        {
                            Assert.Equal(DecodeErrorKind.OutOfRange, error.Kind);
                        }
                    }
                }
            });

    /// <summary>
    /// Overwritten bytes can hold other values, offsets, counts and item ends, which read as other
    /// values, as vectors of another number of items or as a size other than the table's length, or
    /// fail with a DecodeException. Finding a vector item from its ends finds what reading in order
    /// finds, whatever they hold.
    /// </summary>
    [Fact]
    public void OverwrittenTablesReadValuesOrDecodeErrors() =>
        Strategy.Tables.Where(table => table.Bytes().Length > 0)
            .SelectMany(table =>
                Gen.Select(Gen.Int[0, table.Bytes().Length - 1], Gen.Byte).Array[1, 4].Select(overwrites => (table, overwrites))
            )
            .Check(sample =>
            {
                var (table, overwrites) = sample;
                var built = table.Bytes();
                foreach (var (at, value) in overwrites)
                {
                    built[at] = value;
                }

                ReadsOrFails(() => Read.Table(built, table));
                for (var index = 0; index < table.Fields.Count; index++)
                {
                    var field = table.Fields[index];
                    ReadsOrFails(() => Read.RootField(built, table.Layout, index, field));
                }
            });

    /// <summary>
    /// Every getter, at the root offsets of a table of any layout, in the tables nested in it and in
    /// its vectors, and at any position and field offset, and structs of any size, returns a value or
    /// throws a DecodeException. Bytes read the same within larger bytes as on their own, at any
    /// address, and an OutOfRange error describes a read that would leave them.
    /// </summary>
    [Fact]
    public void ArbitraryBytesReadAsValuesOrDecodeErrors() =>
        Gen.Select(
                Gen.Byte.Array[0, 64],
                Gen.Byte.Array[0, 8],
                Gen.Int[0, 7],
                Strategy.Tables,
                Gen.Frequency<int>((4, Gen.Int[-2, 80]), (1, Gen.Int)),
                Gen.Int[0, 12],
                Gen.Int[0, 16]
            )
            .Check(sample =>
            {
                var (drawn, before, byteOffset, table, position, index, size) = sample;

                var backing = before.Concat(drawn).Concat(before).ToArray();
                var within = backing.AsMemory(before.Length, drawn.Length);
                var copied = Bytes.AtByteOffset(drawn, byteOffset);

                var reads = ReadsOf(copied, table, position, index, size);
                Assert.Equal(reads, ReadsOf(within, table, position, index, size));

                foreach (var error in reads.OfType<DecodeException>().Where(error => error.Kind == DecodeErrorKind.OutOfRange))
                {
                    Assert.True(
                        error.Position < 0 || error.Position + error.ByteLength > error.Available,
                        $"{error.Message} doesn't leave the bytes"
                    );
                    Assert.Equal(drawn.Length, error.Available);
                }
            });

    /// <summary>
    /// Reads the bytes with every getter at a position, and as the table at its root, and returns
    /// what each read: a value, rendered as text, or its <see cref="DecodeException"/>
    /// </summary>
    private static List<object> ReadsOf(ReadOnlyMemory<byte> bytes, Table table, int position, int index, int size)
    {
        var reads = new List<object>();
        foreach (var offsetType in Strategy.OffsetTypes)
        {
            var layout = new TableLayout { OffsetType = offsetType };
            var getters = new Func<TableReader, object>[]
            {
                reader => reader.GetInt8(position),
                reader => reader.GetUInt8(position),
                reader => reader.GetInt16(position),
                reader => reader.GetUInt16(position),
                reader => reader.GetInt32(position),
                reader => reader.GetUInt32(position),
                reader => reader.GetInt64(position),
                reader => reader.GetUInt64(position),
                reader => BitConverter.SingleToUInt32Bits(reader.GetFloat32(position)),
                reader => BitConverter.DoubleToUInt64Bits(reader.GetFloat64(position)),
                reader => Text(reader.GetInt8Array(position)),
                reader => Text(reader.GetUInt8Array(position)),
                reader => Text(reader.GetInt16Array(position)),
                reader => Text(reader.GetUInt16Array(position)),
                reader => Text(reader.GetInt32Array(position)),
                reader => Text(reader.GetUInt32Array(position)),
                reader => Text(reader.GetInt64Array(position)),
                reader => Text(reader.GetUInt64Array(position)),
                reader => Text(reader.GetFloat32Array(position)),
                reader => Text(reader.GetFloat64Array(position)),
                reader => reader.GetString(position),
                reader => Text(reader.GetStruct(position, size)),
                reader => Text(reader.GetStructArray(position, size + 1)),
                reader => reader.RootOffset(index),
                reader => reader.FieldOffset(position, index),
                reader => VectorText(reader.GetVector(position)),
                reader => reader.GetSize(),
                reader => new TableReader(reader.Bytes, layout with { Versioned = true }).GetVersion(),
            };

            reads.AddRange(getters.Select(getter => Attempt(new TableReader(bytes.Span, layout), getter)));
        }

        reads.Add(Attempt(() => Read.Table(bytes.Span, table)));
        for (var field = 0; field < table.Fields.Count; field++)
        {
            var described = table.Fields[field];
            reads.Add(Attempt(() => Read.RootField(bytes.Span, table.Layout, field, described)));
        }

        return reads;
    }

    /// <summary>Returns what a read returns, or its decode error or mismatch, and fails for any other exception</summary>
    private static object Attempt(Func<object> read) => Attempt(read, static read => read());

    /// <summary>
    /// Returns what a read of <paramref name="value"/> returns, as <see cref="Attempt(Func{object})"/>
    /// does, for a reader or a vector, which a lambda can't capture
    /// </summary>
    private static object Attempt<T>(T value, Func<T, object> read)
        where T : allows ref struct
    {
        try
        {
            return read(value);
        }
        catch (DecodeException error)
        {
            return new DecodeError(error.Kind, error.Position, error.ByteLength, error.Available, error.End);
        }
        catch (MismatchException error)
        {
            return error.Message;
        }
    }

    private static void ReadsOrFails(Func<object> read) => Attempt(read);

    /// <summary>A decode error's description, which compares by value</summary>
    private sealed record DecodeError(DecodeErrorKind Kind, long Position, long ByteLength, int Available, long End);

    private static string Text<T>(ReadOnlySpan<T> values) => string.Join(",", values.ToArray());

    /// <summary>Every item of a vector, in order and found from its ends, in place and on its own</summary>
    private static string VectorText(Vector vector)
    {
        var items = new List<object>();
        for (var item = 0; item < vector.Count; item++)
        {
            items.Add(Attempt(vector, vector => Text(vector.Get(item))));
            items.Add(Attempt(vector, vector => vector.GetInPlace(item).Length));
        }

        return string.Join(";", items);
    }

    [Fact]
    public void DescribesAReadOutsideTheBytes()
    {
        byte[] bytes = [1, 2, 3, 4, 5, 6];

        var error = Assert.Throws<DecodeException>(() => new TableReader(bytes).GetInt32(4));
        Assert.Equal(DecodeErrorKind.OutOfRange, error.Kind);
        Assert.Equal((4, 4, 6), (error.Position, error.ByteLength, error.Available));
        Assert.Equal("Read exceeds the supplied bytes: 4 bytes at 4 of 6", error.Message);

        Assert.Equal(DecodeErrorKind.OutOfRange, Assert.Throws<DecodeException>(() => new TableReader(bytes).GetUInt8(6)).Kind);
        Assert.Equal(-1, Assert.Throws<DecodeException>(() => new TableReader(bytes).GetUInt8(-1)).Position);
        Assert.Equal(int.MinValue, Assert.Throws<DecodeException>(() => new TableReader(bytes).GetFloat64(int.MinValue)).Position);
        Assert.Equal(-4, Assert.Throws<DecodeException>(() => new TableReader(bytes).FieldOffset(-4, 1)).Position);
        Assert.Throws<DecodeException>(() => new TableReader(bytes).RootOffset(2));
        Assert.Throws<DecodeException>(() => new TableReader(bytes).GetStruct(5, 2));
    }

    /// <summary>The bytes hold a count of 4,294,967,295, which no bytes hold that many elements of, and nothing is allocated for them</summary>
    [Fact]
    public void RejectsAnArrayLongerThanTheBytesBeforeAllocatingForIt()
    {
        byte[] bytes = [0xff, 0xff, 0xff, 0xff, 1, 2, 3, 4];

        var error = Assert.Throws<DecodeException>(() => new TableReader(bytes).GetFloat64Array(0));
        Assert.Equal((4L, 0xffff_ffffL * 8, 8), (error.Position, error.ByteLength, error.Available));
        Assert.Throws<DecodeException>(() => new TableReader(bytes).GetStructArray(0, int.MaxValue));
        Assert.Throws<DecodeException>(() => new TableReader(bytes).GetString(0));
        Assert.Throws<DecodeException>(() => new TableReader(bytes).GetVector(0));
    }

    /// <summary>An offset past the largest int leads past any bytes, and a position can't hold it</summary>
    [Fact]
    public void RejectsAnOffsetPastTheLargestInt()
    {
        byte[] bytes = [0xff, 0xff, 0xff, 0xff];

        var error = Assert.Throws<DecodeException>(() => new TableReader(bytes).RootOffset(0));
        Assert.Equal((DecodeErrorKind.OutOfRange, 0xffff_ffffL), (error.Kind, error.Position));
        Assert.Throws<DecodeException>(() => new TableReader(bytes).FieldOffset(0, 0));
        Assert.Throws<DecodeException>(() => new TableReader(bytes).FieldOffset(int.MaxValue, 0));
    }

    [Fact]
    public void RejectsInvalidUtf8()
    {
        byte[][] strings =
        [
            [0x80], // a continuation byte first
            [0xc0, 0x80], // an overlong NUL
            [0xe0, 0x80, 0x80], // an overlong three bytes
            [0xed, 0xa0, 0x80], // a surrogate, U+D800
            [0xf4, 0x90, 0x80, 0x80], // past U+10FFFF
            [0x61, 0xe2, 0x82], // cut short
            [0xff],
        ];

        foreach (var value in strings)
        {
            byte[] bytes = [(byte)value.Length, 0, 0, 0, .. value];

            var error = Assert.Throws<DecodeException>(() => new TableReader(bytes).GetString(0));
            Assert.Equal((DecodeErrorKind.InvalidUtf8, 0L), (error.Kind, error.Position));
            Assert.Equal("The string at 0 isn't valid UTF-8", error.Message);
        }
    }

    /// <summary>
    /// A string of 1,073,741,792 NULs, one more than a .NET string holds, which a uint32 table can
    /// hold. It takes 1 GiB, which only a 64-bit process can allocate.
    /// </summary>
    [Fact]
    public void RejectsAStringLongerThanAStringCanBe()
    {
        Assert.SkipUnless(Environment.Is64BitProcess, "A 32-bit process can't allocate the string's 1 GiB");

        const int length = 0x3fff_ffe0;
        var bytes = new byte[sizeof(uint) + length];
        BinaryPrimitives.WriteUInt32LittleEndian(bytes, length);

        var error = Assert.Throws<DecodeException>(() => new TableReader(bytes).GetString(0));
        Assert.Equal((DecodeErrorKind.StringTooLong, 0L, length), (error.Kind, error.Position, error.ByteLength));
        Assert.Equal("The string of 1073741792 bytes at 0 has more characters than a .NET string holds", error.Message);

        // Invalid UTF-8 in so long a string fails as invalid, since its characters can't be counted
        bytes[^1] = 0xff;
        Assert.Equal(DecodeErrorKind.InvalidUtf8, Assert.Throws<DecodeException>(() => new TableReader(bytes).GetString(0)).Kind);
    }

    /// <summary>A vector of two items whose second ends before the first does, at 11 rather than 12</summary>
    [Fact]
    public void RejectsAVectorItemThatEndsBeforeItStarts()
    {
        byte[] bytes = [2, 0, 0, 0, 12, 0, 0, 0, 11, 0, 0, 0];

        Assert.Equal(0, new TableReader(bytes).GetVector(0).Get(0).Length);
        var error = Assert.Throws<DecodeException>(() => new TableReader(bytes).GetVector(0).Get(1));
        Assert.Equal((DecodeErrorKind.ItemEndsBeforeStart, 12L, 11L), (error.Kind, error.Position, error.End));
        Assert.Equal("The vector item at 12 ends before it starts, at 11", error.Message);
        Assert.Throws<DecodeException>(() => new TableReader(bytes).GetVector(0).GetInPlace(1));
    }

    [Fact]
    public void RejectsAVectorWhoseEndsLieOutsideTheBytes()
    {
        var error = Assert.Throws<DecodeException>(() => new TableReader(new byte[] { 3, 0, 0, 0, 4, 0, 0, 0 }).GetVector(0));
        Assert.Equal((4L, 12L, 8), (error.Position, error.ByteLength, error.Available));
    }

    [Fact]
    public void DescribesAnUnsupportedVersion()
    {
        var error = DecodeException.UnsupportedVersion(2, 1);

        Assert.Equal((DecodeErrorKind.UnsupportedVersion, 2u, 1u), (error.Kind, error.Version, error.SupportedVersion));
        Assert.Equal("Unsupported format version 2; the reader reads version 1", error.Message);
    }

    /// <summary>Mistakes of the code that calls the reader, which no bytes cause, throw other exceptions</summary>
    [Fact]
    public void ThrowsArgumentExceptionsForMistakesOfTheCaller()
    {
        byte[] bytes = [1, 0, 0, 0, 5, 0, 0, 0];

        Assert.Throws<ArgumentOutOfRangeException>(() => new TableReader(bytes).RootOffset(-1));
        Assert.Throws<ArgumentOutOfRangeException>(() => new TableReader(bytes).FieldOffset(0, -1));
        Assert.Throws<ArgumentOutOfRangeException>(() => new TableReader(bytes).GetStruct(0, -1));
        Assert.Throws<ArgumentOutOfRangeException>(() => new TableReader(bytes).GetStructArray(0, 0));
        Assert.Throws<InvalidOperationException>(() => new TableReader(bytes).GetVersion());
        Assert.Throws<ArgumentOutOfRangeException>(() => new TableLayout { OffsetType = (OffsetType)3 });
    }
}
