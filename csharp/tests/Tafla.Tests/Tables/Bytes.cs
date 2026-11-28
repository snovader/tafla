using System.Buffers.Binary;
using CsCheck;

namespace Tafla.Tests.Tables;

/// <summary>Helpers of the tests that read bytes: tables at any address, and slots of the offset size.</summary>
public static class Bytes
{
    /// <summary>The cases that each property test runs, as many as the other languages' tests run.</summary>
    public const long Iterations = 256;

    /// <summary>Checks a property of the values that a generator generates, shrinking any that fails it.</summary>
    public static void Check<T>(this Gen<T> gen, Action<T> property) => gen.Sample(property, iter: Iterations);

    /// <summary>
    /// Returns a copy of the bytes that starts <paramref name="byteOffset"/> bytes past a multiple of 8
    /// in a larger array of 0xff, so that its arrays are aligned in memory only when
    /// <paramref name="byteOffset"/> is a multiple of 8. A .NET array's elements start at a multiple of
    /// 8 on a 64-bit host.
    /// </summary>
    public static ReadOnlyMemory<byte> AtByteOffset(ReadOnlySpan<byte> bytes, int byteOffset)
    {
        var all = new byte[byteOffset + bytes.Length + 8];
        all.AsSpan().Fill(0xff);
        bytes.CopyTo(all.AsSpan(byteOffset));

        return all.AsMemory(byteOffset, bytes.Length);
    }

    /// <summary>Reads a little-endian slot of <paramref name="width"/> bytes at a position.</summary>
    public static long Slot(ReadOnlySpan<byte> bytes, int position, int width)
    {
        long value = 0;
        for (var index = width - 1; index >= 0; index--)
        {
            value = value << 8 | bytes[position + index];
        }

        return value;
    }

    /// <summary>Returns the little-endian bytes of a value in a slot of <paramref name="width"/> bytes, which it fits in.</summary>
    public static byte[] SlotBytes(long value, int width)
    {
        var bytes = new byte[sizeof(long)];
        BinaryPrimitives.WriteInt64LittleEndian(bytes, value);
        Assert.All(bytes[width..], high => Assert.Equal(0, high));

        return bytes[..width];
    }
}
