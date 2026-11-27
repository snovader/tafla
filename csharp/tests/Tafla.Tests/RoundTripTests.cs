using System.Runtime.InteropServices;
using System.Text;
using CsCheck;
using Tafla.Tests.Tables;

namespace Tafla.Tests;

/// <summary>Tables read back the values that they were built from, and examples of every boundary.</summary>
public class RoundTripTests
{
    /// <summary>
    /// Floats compare by their bits, so that -0 and NaN count. The table is read at any byte offset
    /// from an aligned address, so that its arrays are read in place both aligned and unaligned.
    /// </summary>
    [Fact]
    public void TablesReadBackTheValuesTheyWereBuiltFrom() =>
        Gen.Select(Strategy.Tables, Gen.Int[0, 7]).Check(sample =>
        {
            var (table, byteOffset) = sample;
            var bytes = Bytes.AtByteOffset(table.Bytes(), byteOffset);

            Assert.Equal(table, Read.Table(bytes.Span, table));
        });

    /// <summary>A reader reads a span of any memory, such as memory allocated outside the managed heap.</summary>
    [Fact]
    public void TablesReadBackFromNativeMemory() =>
        Strategy.Tables.Check(table =>
        {
            var built = table.Bytes();
            unsafe
            {
                var native = NativeMemory.Alloc((nuint)built.Length);
                try
                {
                    var bytes = new Span<byte>(native, built.Length);
                    built.CopyTo(bytes);

                    Assert.Equal(table, Read.Table(bytes, table));
                }
                finally
                {
                    NativeMemory.Free(native);
                }
            }
        });

    /// <summary>
    /// A reader reads a table on the stack, and a slice of an array at an odd address, which an
    /// <see cref="ArraySegment{T}"/> converts to, and only the slice: the bytes after it lie outside.
    /// </summary>
    [Fact]
    public void ReadsATableOnTheStackOrInASliceOfAnArray()
    {
        var builder = new TableBuilder(3);
        builder.SetInt32(-7);
        builder.SetString("tafla");
        builder.SetFloat32Array([1.5f, -2.5f]);
        var bytes = builder.ToArray();

        Span<byte> stack = stackalloc byte[bytes.Length];
        bytes.CopyTo(stack);
        AssertFields(new TableReader(stack));

        var backing = new byte[3 + bytes.Length + 4];
        backing.AsSpan().Fill(0xff);
        bytes.CopyTo(backing.AsSpan(3));
        var slice = new ArraySegment<byte>(backing, 3, bytes.Length);
        AssertFields(new TableReader(slice));

        var error = Assert.Throws<DecodeException>(() => new TableReader(slice).GetUInt8(bytes.Length));
        Assert.Equal((bytes.Length, bytes.Length), (error.Position, error.Available));

        static void AssertFields(TableReader reader)
        {
            Assert.Equal(-7, reader.GetInt32(reader.RootOffset(0)));
            Assert.Equal("tafla", reader.GetString(reader.RootOffset(1)));
            Assert.Equal([1.5f, -2.5f], reader.GetFloat32Array(reader.RootOffset(2)).ToArray());
        }
    }

    [Fact]
    public void ReadsBackSignedZeroAndNaNsByTheirBits()
    {
        uint[] float32s = [0x8000_0000, 0x7fc0_0000, 0xffc0_0000, 0x7f80_0001, 0xff80_1234];
        ulong[] float64s = [0x8000_0000_0000_0000, 0x7ff8_0000_0000_0000, 0xfff8_0000_0000_0000, 0x7ff0_0000_0000_0001];

        // From their bits in memory, since 32-bit x86 may quiet a signalling NaN that a method returns
        var singles = MemoryMarshal.Cast<uint, float>(float32s);
        var doubles = MemoryMarshal.Cast<ulong, double>(float64s);

        var builder = new TableBuilder(float32s.Length + float64s.Length + 2);
        foreach (var value in singles)
        {
            builder.SetFloat32(value);
        }

        foreach (var value in doubles)
        {
            builder.SetFloat64(value);
        }

        builder.SetFloat32Array(singles);
        builder.SetFloat64Array(doubles);

        // The bytes keep every bit, and so do the getters, but for a signalling NaN on 32-bit x86
        var reader = new TableReader(builder.ToArray());
        var index = 0;
        foreach (var bits in float32s)
        {
            var position = reader.RootOffset(index++);
            Assert.Equal(bits, reader.GetUInt32(position));

            var read = BitConverter.SingleToUInt32Bits(reader.GetFloat32(position));
            Assert.True(Scalars.ReadsAs(ScalarType.Float32, bits, read), $"Read {read:x8} for {bits:x8}");
        }

        foreach (var bits in float64s)
        {
            var position = reader.RootOffset(index++);
            Assert.Equal(bits, reader.GetUInt64(position));

            var read = BitConverter.DoubleToUInt64Bits(reader.GetFloat64(position));
            Assert.True(Scalars.ReadsAs(ScalarType.Float64, bits, read), $"Read {read:x16} for {bits:x16}");
        }

        Assert.Equal(float32s, MemoryMarshal.Cast<float, uint>(reader.GetFloat32Array(reader.RootOffset(index++))).ToArray());
        Assert.Equal(float64s, MemoryMarshal.Cast<double, ulong>(reader.GetFloat64Array(reader.RootOffset(index))).ToArray());

        // Comparing bits tells -0 from 0, which comparing values doesn't
        Assert.NotEqual(BitConverter.DoubleToUInt64Bits(0.0), BitConverter.DoubleToUInt64Bits(-0.0));
    }

    [Fact]
    public void ReadsBackTheLimitsOfEveryIntegerType()
    {
        var builder = new TableBuilder(16);
        builder.SetInt8(sbyte.MinValue);
        builder.SetInt8(sbyte.MaxValue);
        builder.SetUInt8(byte.MaxValue);
        builder.SetInt16(short.MinValue);
        builder.SetInt16(short.MaxValue);
        builder.SetUInt16(ushort.MaxValue);
        builder.SetInt32(int.MinValue);
        builder.SetInt32(int.MaxValue);
        builder.SetUInt32(uint.MaxValue);
        builder.SetInt64(long.MinValue);
        builder.SetInt64(long.MaxValue);
        builder.SetUInt64(ulong.MaxValue);
        builder.SetInt64Array([long.MinValue, -1, 0, long.MaxValue]);
        builder.SetUInt64Array([0, ulong.MaxValue]);
        builder.SetInt32Array([int.MinValue, int.MaxValue]);
        builder.SetUInt16Array([0, ushort.MaxValue]);

        var reader = new TableReader(builder.ToArray());
        Assert.Equal(sbyte.MinValue, reader.GetInt8(reader.RootOffset(0)));
        Assert.Equal(sbyte.MaxValue, reader.GetInt8(reader.RootOffset(1)));
        Assert.Equal(byte.MaxValue, reader.GetUInt8(reader.RootOffset(2)));
        Assert.Equal(short.MinValue, reader.GetInt16(reader.RootOffset(3)));
        Assert.Equal(short.MaxValue, reader.GetInt16(reader.RootOffset(4)));
        Assert.Equal(ushort.MaxValue, reader.GetUInt16(reader.RootOffset(5)));
        Assert.Equal(int.MinValue, reader.GetInt32(reader.RootOffset(6)));
        Assert.Equal(int.MaxValue, reader.GetInt32(reader.RootOffset(7)));
        Assert.Equal(uint.MaxValue, reader.GetUInt32(reader.RootOffset(8)));
        Assert.Equal(long.MinValue, reader.GetInt64(reader.RootOffset(9)));
        Assert.Equal(long.MaxValue, reader.GetInt64(reader.RootOffset(10)));
        Assert.Equal(ulong.MaxValue, reader.GetUInt64(reader.RootOffset(11)));
        Assert.Equal([long.MinValue, -1, 0, long.MaxValue], reader.GetInt64Array(reader.RootOffset(12)).ToArray());
        Assert.Equal([0, ulong.MaxValue], reader.GetUInt64Array(reader.RootOffset(13)).ToArray());
        Assert.Equal([int.MinValue, int.MaxValue], reader.GetInt32Array(reader.RootOffset(14)).ToArray());
        Assert.Equal([0, ushort.MaxValue], reader.GetUInt16Array(reader.RootOffset(15)).ToArray());
    }

    /// <summary>Every array type, empty, read at any address, which no element of an empty array needs</summary>
    [Fact]
    public void ReadsEmptyArraysAtAnyAddress()
    {
        var table = new Table(
            Layout.Headerless(OffsetType.UInt32),
            [.. Scalars.All.Select(type => (Field)new ArrayField(type, Array.Empty<ulong>())), new StringField(""), new StructArray(new[] { ScalarType.Int64 }, Array.Empty<Seq<Scalar>>())]
        );

        for (var byteOffset = 0; byteOffset < 8; byteOffset++)
        {
            var bytes = Bytes.AtByteOffset(table.Bytes(), byteOffset);
            Assert.Equal(table, Read.Table(bytes.Span, table));
        }
    }

    /// <summary>Strings outside the Basic Multilingual Plane are their UTF-8, of four bytes a character, from their surrogate pairs</summary>
    [Fact]
    public void RoundTripsStringsOutsideTheBasicMultilingualPlane()
    {
        const string Value = "\U0001F600 \U00010000\U0010FFFF﻿";

        var builder = new TableBuilder(1);
        builder.SetString(Value);
        var bytes = builder.ToArray();

        Assert.Equal(Encoding.UTF8.GetBytes(Value), bytes[8..]);
        Assert.Equal(Value, new TableReader(bytes).GetString(4));
    }

    /// <summary>
    /// A string of any length, short or long, and of characters of any width in UTF-8, is written as
    /// its UTF-8 and reads back as it was
    /// </summary>
    [Fact]
    public void RoundTripsStringsOfAnyLength() => Strategy.String(300).Check(AssertRoundTrips);

    /// <summary>
    /// ASCII, of any of its 128 characters, reads back at any length, and so does ASCII with one
    /// character of two, three or four bytes anywhere in it
    /// </summary>
    [Fact]
    public void RoundTripsAsciiStringsOfAnyLength() =>
        Gen.Select(Gen.Int[0, 0x7f].Array[0, 1100], Gen.OneOf("", "é", "€", "😀"), Gen.Int[0, 1100], AsciiWith)
            .Check(AssertRoundTrips);

    /// <summary>
    /// Strings of exactly as many bytes as the reader decodes in each of its ways, 128 and 1,024, and
    /// of one byte more, read back: of characters of each width in UTF-8, with ASCII before or after
    /// them where their width doesn't divide the length
    /// </summary>
    [Fact]
    public void RoundTripsStringsAtTheLimitsOfEachDecode()
    {
        foreach (var byteLength in (int[])[128, 129, 1024, 1025])
        {
            foreach (var character in (string[])["a", "é", "€", "😀"])
            {
                var width = Encoding.UTF8.GetByteCount(character);
                var characters = string.Concat(Enumerable.Repeat(character, byteLength / width));
                var ascii = new string('a', byteLength % width);

                AssertRoundTrips(characters + ascii);
                AssertRoundTrips(ascii + characters);
            }
        }
    }

    /// <summary>The ASCII of <paramref name="codes"/>, with <paramref name="other"/> inserted at <paramref name="index"/> or its end</summary>
    private static string AsciiWith(int[] codes, string other, int index)
    {
        var ascii = string.Concat(codes.Select(code => (char)code));

        return ascii.Insert(Math.Min(index, ascii.Length), other);
    }

    /// <summary>Writes <paramref name="value"/> as a table's string, which it reads back</summary>
    private static void AssertRoundTrips(string value)
    {
        var builder = new TableBuilder(1);
        builder.SetString(value);
        var bytes = builder.ToArray();

        Assert.Equal(Encoding.UTF8.GetBytes(value), bytes[8..]);
        Assert.Equal(value, new TableReader(bytes).GetString(4));
    }

    /// <summary>UTF-8 can't encode a lone surrogate, which the builder writes as U+FFFD, as Encoding.UTF8 does</summary>
    [Fact]
    public void WritesALoneSurrogateAsTheReplacementCharacter()
    {
        var builder = new TableBuilder(2);
        builder.SetString("a\ud800b");
        builder.SetString("\udc00");
        var reader = new TableReader(builder.ToArray());

        Assert.Equal("a�b", reader.GetString(reader.RootOffset(0)));
        Assert.Equal("�", reader.GetString(reader.RootOffset(1)));
    }

    /// <summary>
    /// The builder counts a long string's UTF-8 in parts of 2^20 characters, and a surrogate pair
    /// across the end of a part counts as the four bytes of its character
    /// </summary>
    [Fact]
    public void RoundTripsALongStringWithASurrogatePairAtAnyPosition()
    {
        const int Part = 1 << 20;

        foreach (var at in new[] { Part - 2, Part - 1, Part, 2 * Part - 1 })
        {
            var characters = new string('a', 2 * Part + 3).ToCharArray();
            characters[at] = '\ud83d';
            characters[at + 1] = '\ude00';
            var value = new string(characters);

            var builder = new TableBuilder(1);
            builder.SetString(value);
            var bytes = builder.ToArray();

            Assert.Equal(8 + Encoding.UTF8.GetByteCount(value), bytes.Length);
            Assert.Equal(value, new TableReader(bytes).GetString(4));
        }
    }

    /// <summary>A setter copies its value, so that the value can change once it's set, and a nested builder too</summary>
    [Fact]
    public void ValuesThatChangeAfterTheyreSetLeaveTheTableAsItWas()
    {
        int[] values = [1, 2, 3];
        byte[] structBytes = [4, 5];
        var nested = new TableBuilder(1);
        nested.SetInt32Array(values);

        var builder = new TableBuilder(4);
        builder.SetInt32Array(values);
        builder.SetStruct(structBytes);
        builder.SetTable(nested);
        builder.SetVector(nested, nested);
        var expected = builder.ToArray();

        values[0] = 9;
        structBytes[0] = 9;

        Assert.Equal(expected, builder.ToArray());
    }

    [Fact]
    public void NestsABuilderSeveralTimes()
    {
        var item = new TableBuilder(2);
        item.SetUInt16(513);
        item.SetString("item");

        var builder = new TableBuilder(3);
        builder.SetTable(item);
        builder.SetTable(item);
        builder.SetVector(item, item, item.ToArray());

        var nested = new Nested(Source.Builder, new Table(Layout.Headerless(OffsetType.UInt32), new Scalar(ScalarType.UInt16, 513), new StringField("item")));
        var table = new Table(
            Layout.Headerless(OffsetType.UInt32),
            new TableField(nested, null),
            new TableField(nested, null),
            new VectorField(new[] { nested, nested, nested with { From = Source.Bytes } })
        );

        Assert.Equal(table.Bytes(), builder.ToArray());
        Assert.Equal(table, Read.Table(builder.ToArray(), table));
    }

    [Fact]
    public void FindsVectorItemsFromTheirEnds()
    {
        var items = new NestedTable[3];
        for (var index = 0; index < items.Length; index++)
        {
            var item = new TableBuilder(1);
            item.SetString(new string('x', index));
            items[index] = item;
        }

        var builder = new TableBuilder(1);
        builder.SetVector(items);
        var bytes = builder.ToArray();

        var vector = new TableReader(bytes).GetVector(4);
        Assert.Equal(3, vector.Count);

        var index2 = 0;
        foreach (var item in vector.Items())
        {
            Assert.True(item.SequenceEqual(vector.Get(index2)));
            Assert.Equal(new string('x', index2), new TableReader(item).GetString(new TableReader(item).RootOffset(0)));
            index2++;
        }

        Assert.Equal(3, index2);
        Assert.Equal(bytes.Length - (4 + 16), vector.GetInPlace(0).Length);
        Assert.Throws<ArgumentOutOfRangeException>(() => new TableReader(bytes).GetVector(4).Get(3));
        Assert.Throws<ArgumentOutOfRangeException>(() => new TableReader(bytes).GetVector(4).GetInPlace(-1));
    }

    /// <summary>
    /// On a little-endian host that reads unaligned elements, arrays are spans of the table's bytes,
    /// which show writes made to them later, at any alignment. A portable build copies them, as a
    /// big-endian host does.
    /// </summary>
    [Fact]
    public void ReadsArraysInPlaceOnALittleEndianHost()
    {
#if TAFLA_PORTABLE
        var inPlace = false;
#else
        var inPlace = BitConverter.IsLittleEndian && RuntimeInformation.ProcessArchitecture is not (Architecture.Arm or Architecture.Armv6);
#endif
        var builder = new TableBuilder(2);
        builder.SetUInt8(0);
        builder.SetFloat64Array([1.5, 2.5]);

        for (var byteOffset = 0; byteOffset < 8; byteOffset++)
        {
            var memory = Bytes.AtByteOffset(builder.ToArray(), byteOffset);
            var reader = new TableReader(memory.Span);
            var values = reader.GetFloat64Array(reader.RootOffset(1));

            MemoryMarshal.AsMemory(memory).Span[^8..].Clear();

            Assert.Equal(inPlace ? [1.5, 0.0] : [1.5, 2.5], values.ToArray());
        }
    }
}
