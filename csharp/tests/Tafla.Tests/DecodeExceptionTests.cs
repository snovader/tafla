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
            var buffer = new ByteBuffer(bytes, new TableLayout { OffsetType = offsetType });
            var getters = new Func<object>[]
            {
                () => buffer.GetInt8(position),
                () => buffer.GetUInt8(position),
                () => buffer.GetInt16(position),
                () => buffer.GetUInt16(position),
                () => buffer.GetInt32(position),
                () => buffer.GetUInt32(position),
                () => buffer.GetInt64(position),
                () => buffer.GetUInt64(position),
                () => BitConverter.SingleToUInt32Bits(buffer.GetFloat32(position)),
                () => BitConverter.DoubleToUInt64Bits(buffer.GetFloat64(position)),
                () => Text(buffer.GetInt8Array(position)),
                () => Text(buffer.GetUInt8Array(position)),
                () => Text(buffer.GetInt16Array(position)),
                () => Text(buffer.GetUInt16Array(position)),
                () => Text(buffer.GetInt32Array(position)),
                () => Text(buffer.GetUInt32Array(position)),
                () => Text(buffer.GetInt64Array(position)),
                () => Text(buffer.GetUInt64Array(position)),
                () => Text(buffer.GetFloat32Array(position)),
                () => Text(buffer.GetFloat64Array(position)),
                () => buffer.GetString(position),
                () => Text(buffer.GetStruct(position, size)),
                () => Text(buffer.GetStructArray(position, size + 1)),
                () => buffer.RootOffset(index),
                () => buffer.FieldOffset(position, index),
                () => VectorText(buffer.GetVector(position)),
                () => buffer.GetSize(),
                () => new ByteBuffer(bytes, new TableLayout { Versioned = true, OffsetType = offsetType }).GetVersion(),
            };

            reads.AddRange(getters.Select(Attempt));
        }

        reads.Add(Attempt(() => Read.Table(bytes, table)));
        for (var field = 0; field < table.Fields.Count; field++)
        {
            var described = table.Fields[field];
            reads.Add(Attempt(() => Read.RootField(bytes, table.Layout, field, described)));
        }

        return reads;
    }

    /// <summary>Returns what a read returns, or its decode error or mismatch, and fails for any other exception</summary>
    private static object Attempt(Func<object> read)
    {
        try
        {
            return read();
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
            items.Add(Attempt(() => Text(vector.Get(item).Span)));
            items.Add(Attempt(() => vector.GetInPlace(item).Length));
        }

        return string.Join(";", items);
    }

    [Fact]
    public void DescribesAReadOutsideTheBytes()
    {
        var buffer = new ByteBuffer(new byte[] { 1, 2, 3, 4, 5, 6 });

        var error = Assert.Throws<DecodeException>(() => buffer.GetInt32(4));
        Assert.Equal(DecodeErrorKind.OutOfRange, error.Kind);
        Assert.Equal((4, 4, 6), (error.Position, error.ByteLength, error.Available));
        Assert.Equal("Read exceeds the supplied bytes: 4 bytes at 4 of 6", error.Message);

        Assert.Equal(DecodeErrorKind.OutOfRange, Assert.Throws<DecodeException>(() => buffer.GetUInt8(6)).Kind);
        Assert.Equal(-1, Assert.Throws<DecodeException>(() => buffer.GetUInt8(-1)).Position);
        Assert.Equal(int.MinValue, Assert.Throws<DecodeException>(() => buffer.GetFloat64(int.MinValue)).Position);
        Assert.Equal(-4, Assert.Throws<DecodeException>(() => buffer.FieldOffset(-4, 1)).Position);
        Assert.Throws<DecodeException>(() => buffer.RootOffset(2));
        Assert.Throws<DecodeException>(() => buffer.GetStruct(5, 2));
    }

    /// <summary>The bytes hold a count of 4,294,967,295, which no bytes hold that many elements of, and nothing is allocated for them</summary>
    [Fact]
    public void RejectsAnArrayLongerThanTheBytesBeforeAllocatingForIt()
    {
        var buffer = new ByteBuffer(new byte[] { 0xff, 0xff, 0xff, 0xff, 1, 2, 3, 4 });

        var error = Assert.Throws<DecodeException>(() => buffer.GetFloat64Array(0));
        Assert.Equal((4L, 0xffff_ffffL * 8, 8), (error.Position, error.ByteLength, error.Available));
        Assert.Throws<DecodeException>(() => buffer.GetStructArray(0, int.MaxValue));
        Assert.Throws<DecodeException>(() => buffer.GetString(0));
        Assert.Throws<DecodeException>(() => buffer.GetVector(0));
    }

    /// <summary>An offset past the largest int leads past any bytes, and a position can't hold it</summary>
    [Fact]
    public void RejectsAnOffsetPastTheLargestInt()
    {
        var buffer = new ByteBuffer(new byte[] { 0xff, 0xff, 0xff, 0xff });

        var error = Assert.Throws<DecodeException>(() => buffer.RootOffset(0));
        Assert.Equal((DecodeErrorKind.OutOfRange, 0xffff_ffffL), (error.Kind, error.Position));
        Assert.Throws<DecodeException>(() => buffer.FieldOffset(0, 0));
        Assert.Throws<DecodeException>(() => buffer.FieldOffset(int.MaxValue, 0));
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
            var buffer = new ByteBuffer(bytes);

            var error = Assert.Throws<DecodeException>(() => buffer.GetString(0));
            Assert.Equal((DecodeErrorKind.InvalidUtf8, 0L), (error.Kind, error.Position));
            Assert.Equal("The string at 0 isn't valid UTF-8", error.Message);
        }
    }

    /// <summary>A vector of two items whose second ends before the first does, at 11 rather than 12</summary>
    [Fact]
    public void RejectsAVectorItemThatEndsBeforeItStarts()
    {
        var vector = new ByteBuffer(new byte[] { 2, 0, 0, 0, 12, 0, 0, 0, 11, 0, 0, 0 }).GetVector(0);

        Assert.Equal(0, vector.Get(0).Length);
        var error = Assert.Throws<DecodeException>(() => vector.Get(1));
        Assert.Equal((DecodeErrorKind.ItemEndsBeforeStart, 12L, 11L), (error.Kind, error.Position, error.End));
        Assert.Equal("The vector item at 12 ends before it starts, at 11", error.Message);
        Assert.Throws<DecodeException>(() => vector.GetInPlace(1));
    }

    [Fact]
    public void RejectsAVectorWhoseEndsLieOutsideTheBytes()
    {
        var error = Assert.Throws<DecodeException>(() => new ByteBuffer(new byte[] { 3, 0, 0, 0, 4, 0, 0, 0 }).GetVector(0));
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
        var buffer = new ByteBuffer(new byte[] { 1, 0, 0, 0, 5, 0, 0, 0 });

        Assert.Throws<ArgumentOutOfRangeException>(() => buffer.RootOffset(-1));
        Assert.Throws<ArgumentOutOfRangeException>(() => buffer.FieldOffset(0, -1));
        Assert.Throws<ArgumentOutOfRangeException>(() => buffer.GetStruct(0, -1));
        Assert.Throws<ArgumentOutOfRangeException>(() => buffer.GetStructArray(0, 0));
        Assert.Throws<InvalidOperationException>(() => buffer.GetVersion());
        Assert.Throws<ArgumentOutOfRangeException>(() => new TableLayout { OffsetType = (OffsetType)3 });
    }
}
