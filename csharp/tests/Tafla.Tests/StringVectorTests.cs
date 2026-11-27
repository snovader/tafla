using System.Buffers.Binary;
using System.Text;
using CsCheck;
using Tafla.Tests.Tables;

namespace Tafla.Tests;

/// <summary>
/// The vector of strings, as SPEC.md lays it out: a count, the end of each string and the strings'
/// UTF-8. Builders, and sizers and their writers, write it as a vector of items that are each string's
/// UTF-8. Readers read every string, in order or each from its ends alone, alike, and fail alike, with
/// a <see cref="DecodeException"/>, for bytes that aren't a vector of strings.
/// </summary>
public class StringVectorTests
{
    private static readonly Gen<string[]> Vectors = Strategy.String(16).Array[0, 8];

    /// <summary>Overwrites of bytes after a table's offset of 4 bytes, at any position, wrapped to its length</summary>
    private static readonly Gen<(int At, byte Value)[]> Overwrites = Gen.Select(Gen.Int[0, int.MaxValue], Gen.Byte).Array[1, 4];

    /// <summary>A table whose only field is a vector of <paramref name="strings"/></summary>
    private static byte[] TableOf(string[] strings, TableLayout layout = default)
    {
        var builder = new TableBuilder(1, layout);
        builder.SetStringVector(strings);

        return builder.ToArray();
    }

    /// <summary>The vector of strings that is the first field of a table of <paramref name="bytes"/></summary>
    private static StringVector ViewOf(ReadOnlySpan<byte> bytes, TableLayout layout = default)
    {
        var reader = new TableReader(bytes, layout);
        return reader.GetStringVector(reader.RootOffset(0));
    }

    [Fact]
    public void ReadsBackEveryStringAtOnceInOrderAndEachFromItsEnds() =>
        Gen.Select(Vectors, Gen.OneOf(OffsetType.UInt16, OffsetType.UInt32), Gen.Int[0, 7]).Check(sample =>
        {
            var (strings, offsetType, byteOffset) = sample;
            var layout = new TableLayout { OffsetType = offsetType };
            var bytes = Bytes.AtByteOffset(TableOf(strings, layout), byteOffset);
            var view = ViewOf(bytes.Span, layout);

            Assert.Equal(strings.Length, view.Count);
            Assert.Equal(strings, view.ToArray());
            Assert.Equal(strings, Each(view).Select(text => text.Value));
            Assert.Equal(strings, InOrder(view).Select(text => text.Value));
        });

    [Fact]
    public void HoldsTheBytesOfAVectorWhoseItemsAreEachStringsUtf8() =>
        Gen.Select(Vectors, Strategy.Layouts).Check(sample =>
        {
            var (strings, layout) = sample;
            var tableLayout = layout.ToTableLayout();

            var vector = new TableBuilder(1, tableLayout);
            try
            {
                vector.SetVector([.. strings.Select(value => (NestedTable)Encoding.UTF8.GetBytes(value))]);
            }
            catch (BuildException error) when (error.Kind == BuildErrorKind.TooLong)
            {
                Assert.Equal(BuildErrorKind.TooLong, Assert.Throws<BuildException>(() => TableOf(strings, tableLayout)).Kind);
                return;
            }

            Assert.Equal(vector.ToArray(), TableOf(strings, tableLayout));
        });

    /// <summary>The table's offset, the vector's count and the end of each string, of 4 bytes each, and their UTF-8</summary>
    [Fact]
    public void TakesACountAndAnEndForEachStringBesideTheirUtf8() =>
        Vectors.Check(strings =>
        {
            var utf8 = strings.Sum(Encoding.UTF8.GetByteCount);

            Assert.Equal(4 + 4 + 4 * strings.Length + utf8, TableOf(strings).Length);
        });

    [Fact]
    public void IsSizedAndWrittenAsTableBuilderWritesIt() =>
        Gen.Select(Vectors, Strategy.Layouts, Gen.Int[0, 16], Gen.Byte).Check(sample =>
        {
            var (strings, layout, extra, fill) = sample;
            var tableLayout = layout.ToTableLayout();

            var sizer = new TableSizer(1, tableLayout);
            sizer.AddStringVector(strings);

            byte[] built;
            try
            {
                built = TableOf(strings, tableLayout);
            }
            catch (BuildException error) when (error.Kind == BuildErrorKind.TooLong)
            {
                Assert.Equal(BuildErrorKind.TooLong, Assert.Throws<BuildException>(() => sizer.GetByteLength()).Kind);
                return;
            }

            var writer = sizer.CreateWriter();
            writer.SetStringVector(0, strings);
            Assert.Equal(built, writer.Bytes);

            var bytes = new byte[built.Length + extra];
            bytes.AsSpan().Fill(fill);
            var into = sizer.CreateWriter(bytes);
            into.SetStringVector(0, strings);
            Assert.Equal(built, bytes[..into.ByteLength]);
            Assert.All(bytes[built.Length..], value => Assert.Equal(fill, value));
        });

    [Fact]
    public void WritesALoneSurrogateAsUFFFDAsAStringFieldDoes()
    {
        string[] strings = ["a\ud800", "\udc00b", "🌍"];

        Assert.Equal(["a�", "�b", "🌍"], ViewOf(TableOf(strings)).ToArray());

        var sizer = new TableSizer(1);
        sizer.AddStringVector(strings);
        var writer = sizer.CreateWriter();
        writer.SetStringVector(0, strings);
        Assert.Equal(TableOf(strings), writer.Bytes);
    }

    [Fact]
    public void ReadsLongStringsAndStringsOfEveryWidth()
    {
        string[] strings = [new('a', 33), new('é', 40), new('東', 300), new('b', 4096), "🌍", "", "﻿Stockholm"];
        var bytes = TableOf(strings);

        Assert.Equal(strings, ViewOf(bytes).ToArray());
        Assert.Equal(strings[2], ViewOf(bytes)[2]);
    }

    /// <summary>
    /// Bytes overwritten after the table's offset read, as every string at once and in order, what
    /// reading each string from its ends reads, or fail where reading any one fails, with its error
    /// </summary>
    [Fact]
    public void ReadingEveryStringReadsWhatReadingEachReadsAndThrowsWhereAnyDoes() =>
        Gen.Select(Vectors, Overwrites).Check(sample =>
        {
            var (strings, overwrites) = sample;
            var bytes = TableOf(strings);
            foreach (var (at, value) in overwrites)
            {
                bytes[4 + at % (bytes.Length - 4)] = value;
            }

            AssertReadAlike(bytes, 4);
        });

    /// <summary>Any bytes, read as a vector of strings at any position, read alike every way, or fail alike</summary>
    [Fact]
    public void ArbitraryBytesReadAlikeEveryWayOrFailAlike() =>
        Gen.Select(Gen.Byte.Array[0, 48], Gen.Int[0, 47]).Check(sample =>
        {
            var (bytes, position) = sample;
            AssertReadAlike(bytes, position);
        });

    /// <summary>
    /// Checks that the vector of strings at <paramref name="position"/> fails to read with a decode
    /// error, or reads the same strings, or fails with the same error, each from its ends, in order and
    /// all at once
    /// </summary>
    private static void AssertReadAlike(byte[] bytes, int position)
    {
        StringVector view;
        try
        {
            view = new TableReader(bytes).GetStringVector(position);
        }
        catch (DecodeException)
        {
            return;
        }

        var each = Each(view);
        Assert.Equal(each, InOrder(view));

        var all = Attempt(view, static view => view.ToArray());
        if (each.FirstOrDefault(text => text.Error is not null) is { Error: { } error })
        {
            Assert.Equal(error, all.Error);
        }
        else
        {
            Assert.Equal(each.Select(text => text.Value), all.Value);
        }
    }

    [Fact]
    public void ThrowsArgumentOutOfRangeExceptionForAStringOutsideTheVector()
    {
        var bytes = TableOf(["a", "b"]);

        foreach (var index in (int[])[-1, 2, int.MaxValue, int.MinValue])
        {
            Assert.Throws<ArgumentOutOfRangeException>(() => ViewOf(bytes)[index]);
        }
    }

    /// <summary>The bytes hold a count of 4,294,967,295, whose ends no bytes hold, and nothing is allocated for them</summary>
    [Fact]
    public void RejectsAnImpossibleCountBeforeAllocatingForIt()
    {
        byte[] bytes = [0xff, 0xff, 0xff, 0xff, 1, 2, 3, 4];

        var error = Assert.Throws<DecodeException>(() => new TableReader(bytes).GetStringVector(0));
        Assert.Equal((DecodeErrorKind.OutOfRange, 4L, 0xffff_ffffL * 4, 8), (error.Kind, error.Position, error.ByteLength, error.Available));
    }

    /// <summary>
    /// The end of 東, the second string, a byte less, splits it, which leaves both it and 京 invalid,
    /// though their bytes together are valid UTF-8, and the strings around them read. The strings
    /// start at 24, after the table's offset, and the vector's count and four ends.
    /// </summary>
    [Fact]
    public void AnEndThatSplitsACharacterFailsTheStringsOnEitherSideOfIt()
    {
        var bytes = TableOf(["a", "東", "京", "b"]);
        // After the table's offset, the count and the first end
        bytes[4 + 4 + 4]--;

        Assert.Equal("a", ViewOf(bytes)[0]);
        var second = Assert.Throws<DecodeException>(() => ViewOf(bytes)[1]);
        Assert.Equal((DecodeErrorKind.InvalidUtf8, 25L), (second.Kind, second.Position));
        var third = Assert.Throws<DecodeException>(() => ViewOf(bytes)[2]);
        Assert.Equal((DecodeErrorKind.InvalidUtf8, 27L), (third.Kind, third.Position));
        Assert.Equal("b", ViewOf(bytes)[3]);

        Assert.Equal(second.Position, Assert.Throws<DecodeException>(() => ViewOf(bytes).ToArray()).Position);
        Assert.Equal([new("a", null), new(null, Of(second)), new(null, Of(third)), new("b", null)], InOrder(ViewOf(bytes)));
    }

    [Theory]
    [InlineData(3)]
    [InlineData(40)]
    public void NamesThePositionOfAStringThatIsntValidUtf8(int length)
    {
        var bytes = TableOf(["a", new string('b', length)]);
        // The second string starts after the table's offset, the count, two ends and the first string
        bytes[17] = 0xff;

        Assert.Equal("a", ViewOf(bytes)[0]);
        var error = Assert.Throws<DecodeException>(() => ViewOf(bytes)[1]);
        Assert.Equal((DecodeErrorKind.InvalidUtf8, 17L), (error.Kind, error.Position));
        Assert.Equal("The string at 17 isn't valid UTF-8", error.Message);
        Assert.IsType<DecoderFallbackException>(error.InnerException);
    }

    /// <summary>
    /// The vector at 4 holds its strings from 16, a byte each, which end at 17, 18 and 19. With the
    /// second's end 19 and the third's 18, the second reads "bc", and the third ends before it starts.
    /// </summary>
    [Fact]
    public void RejectsAStringThatEndsBeforeItStarts()
    {
        var bytes = TableOf(["a", "b", "c"]);
        BinaryPrimitives.WriteUInt32LittleEndian(bytes.AsSpan(4 + 2 * 4), 19);
        BinaryPrimitives.WriteUInt32LittleEndian(bytes.AsSpan(4 + 3 * 4), 18);

        Assert.Equal("bc", ViewOf(bytes)[1]);
        var error = Assert.Throws<DecodeException>(() => ViewOf(bytes)[2]);
        Assert.Equal((DecodeErrorKind.ItemEndsBeforeStart, 4L + 19, 4L + 18), (error.Kind, error.Position, error.End));
        Assert.Equal("The vector item at 23 ends before it starts, at 22", error.Message);
        Assert.Equal(error.Position, Assert.Throws<DecodeException>(() => ViewOf(bytes).ToArray()).Position);
    }

    /// <summary>A table cut within its last string, whose ends lie within the bytes, fails when that string is read</summary>
    [Fact]
    public void RejectsAStringPastTheBytes()
    {
        var bytes = TableOf(["a", "bc"])[..^1];

        Assert.Equal("a", ViewOf(bytes)[0]);
        var error = Assert.Throws<DecodeException>(() => ViewOf(bytes)[1]);
        Assert.Equal((DecodeErrorKind.OutOfRange, 4L + 13, 2L, 18), (error.Kind, error.Position, error.ByteLength, error.Available));
        Assert.Throws<DecodeException>(() => ViewOf(bytes).ToArray());
    }

    [Fact]
    public void TheDefaultStringVectorIsEmpty()
    {
        var view = default(StringVector);

        Assert.Equal(0, view.Count);
        Assert.Empty(view.ToArray());
        Assert.Empty(InOrder(view));
        Assert.Throws<ArgumentOutOfRangeException>(() => default(StringVector)[0]);
    }

    /// <summary>
    /// After a uint8, a vector of one string as long as fits fills a table of uint8 or uint16 offsets,
    /// and a byte more is rejected, with the length that it would make the table, leaving the builder
    /// as it was
    /// </summary>
    [Theory]
    [InlineData(OffsetType.UInt8)]
    [InlineData(OffsetType.UInt16)]
    public void FillsATableOfNarrowOffsetsAndRejectsAByteMore(OffsetType offsetType)
    {
        var layout = new TableLayout { OffsetType = offsetType };
        var most = (int)offsetType.MaxValue;
        // Two offsets, the uint8, and the vector's count and its one end
        var room = most - 4 * offsetType.Size - 1;

        var builder = new TableBuilder(2, layout);
        builder.SetUInt8(7);
        var error = Assert.Throws<BuildException>(() => builder.SetStringVector(new string('a', room + 1)));
        Assert.Equal((BuildErrorKind.TooLong, 1, most + 1L, offsetType), (error.Kind, error.Index, error.Length, error.OffsetType));

        builder.SetStringVector(new string('a', room));
        var bytes = builder.ToArray();
        var reader = new TableReader(bytes, layout);
        Assert.Equal(most, bytes.Length);
        Assert.Equal(7, reader.GetUInt8(reader.RootOffset(0)));
        Assert.Equal([new string('a', room)], reader.GetStringVector(reader.RootOffset(1)).ToArray());

        var sizer = new TableSizer(2, layout);
        sizer.AddUInt8();
        sizer.AddStringVector([new string('a', room + 1)]);
        Assert.Equal(BuildErrorKind.TooLong, Assert.Throws<BuildException>(() => sizer.GetByteLength()).Kind);
    }

    /// <summary>
    /// In bytes longer than the table, strings longer than those sized throw NotAsSized where they
    /// would end past the table, the count and ends as well as the strings, and every byte after it
    /// stays as it was
    /// </summary>
    [Fact]
    public void StringsLongerThanThoseSizedThrowNotAsSizedWithoutWritingPastTheTable()
    {
        const byte Fill = 0xaa;
        var sizer = new TableSizer(1);
        sizer.AddStringVector(["a", "b"]);
        var length = sizer.GetByteLength();

        foreach (string[] strings in (string[][])[["a", "bc"], ["ab", "c"], ["a", "b", ""], ["东京a"]])
        {
            var bytes = new byte[32];
            bytes.AsSpan().Fill(Fill);
            var writer = sizer.CreateWriter(bytes);

            var error = Assert.Throws<BuildException>(() => writer.SetStringVector(0, strings));
            Assert.Equal((BuildErrorKind.NotAsSized, 4, (long)length), (error.Kind, error.Written, error.Length));
            Assert.All(bytes[length..], value => Assert.Equal(Fill, value));
        }
    }

    [Fact]
    public void StringsShorterThanThoseSizedThrowNotAsSizedFromBytes()
    {
        var sizer = new TableSizer(1);
        sizer.AddStringVector(["ab"]);
        var writer = sizer.CreateWriter();
        writer.SetStringVector(0, ["a"]);

        var error = Assert.Throws<BuildException>(() => writer.Bytes);
        Assert.Equal((BuildErrorKind.NotAsSized, 13, 14L), (error.Kind, error.Written, error.Length));
    }

    /// <summary>A null string is a mistake of the caller, which the builder rejects before writing anything, as the sizer does</summary>
    [Fact]
    public void RejectsANullStringAndChangesNothing()
    {
        var builder = new TableBuilder(1);
        Assert.Throws<ArgumentNullException>(() => builder.SetStringVector("a", null!));
        builder.SetStringVector("a", "b");
        Assert.Equal(TableOf(["a", "b"]), builder.ToArray());

        var sizer = new TableSizer(1);
        Assert.Throws<ArgumentNullException>(() => sizer.AddStringVector(["a", null!]));
        sizer.AddStringVector(["a", "b"]);
        Assert.Equal(18, sizer.GetByteLength());
        var writer = sizer.CreateWriter();
        Assert.Throws<ArgumentNullException>(() => writer.SetStringVector(0, ["a", null!]));
    }

    /// <summary>A string of a vector of strings, or the error of reading it, which compares by value</summary>
    private sealed record Text(string? Value, Error? Error);

    /// <summary>A decode error's description, which compares by value</summary>
    private sealed record Error(DecodeErrorKind Kind, long Position, long ByteLength, int Available, long End);

    private static Error Of(DecodeException error) => new(error.Kind, error.Position, error.ByteLength, error.Available, error.End);

    /// <summary>Every string of a view, each from its ends with the indexer, or the error of reading it</summary>
    private static List<Text> Each(StringVector view)
    {
        var each = new List<Text>();
        for (var index = 0; index < view.Count; index++)
        {
            each.Add(Attempt(view, view => view[index]));
        }

        return each;
    }

    /// <summary>Every string of a view, read in order by its enumerator, which moves on past a string that fails</summary>
    private static List<Text> InOrder(StringVector view)
    {
        var strings = new List<Text>();
        var enumerator = view.GetEnumerator();
        while (true)
        {
            try
            {
                if (!enumerator.MoveNext())
                {
                    return strings;
                }

                strings.Add(new Text(enumerator.Current, null));
            }
            catch (DecodeException error)
            {
                strings.Add(new Text(null, Of(error)));
            }
        }
    }

    /// <summary>Reads a view, which a lambda can't capture, and returns the string that it reads or its error</summary>
    private static Text Attempt(StringVector view, Func<StringVector, string> read)
    {
        try
        {
            return new Text(read(view), null);
        }
        catch (DecodeException error)
        {
            return new Text(null, Of(error));
        }
    }

    private static (string[]? Value, Error? Error) Attempt(StringVector view, Func<StringVector, string[]> read)
    {
        try
        {
            return (read(view), null);
        }
        catch (DecodeException error)
        {
            return (null, Of(error));
        }
    }
}
