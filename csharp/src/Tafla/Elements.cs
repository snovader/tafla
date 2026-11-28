using System;
using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;
using System.Runtime.Intrinsics;

namespace Tafla;

/// <summary>
/// Reads and writes the little-endian elements of arrays: in place, as spans of the table's bytes,
/// where the host reads them so, and otherwise copied, in the host's byte order. Bools are always
/// copied, into bytes of 1 and 0 and out of any bytes.
/// </summary>
internal static class Elements
{
    /// <summary>
    /// Whether arrays are read in place: on a little-endian host that reads elements at any address,
    /// which every 64-bit and x86 host .NET runs on does. 32-bit Arm faults on some unaligned loads of
    /// floats and 64-bit integers, and a byte array's elements are aligned only to 4 bytes there, and
    /// may move to another multiple of 4 when the garbage collector compacts the heap, so it copies
    /// them, as a big-endian host does. Defining <c>TAFLA_PORTABLE</c>, as building with
    /// <c>-p:TaflaPortable=true</c> does, copies them on any host, so that the tests cover that path.
    /// </summary>
    internal static readonly bool ReadInPlace =
        !Portable
        && BitConverter.IsLittleEndian
        && RuntimeInformation.ProcessArchitecture is not (Architecture.Arm or Architecture.Armv6);

    /// <summary>Whether the runtime was built to copy every array it reads, with <c>-p:TaflaPortable=true</c></summary>
#if TAFLA_PORTABLE
    private const bool Portable = true;
#else
    private const bool Portable = false;
#endif

    /// <summary>
    /// Returns the elements of an array from their little-endian bytes, which are a whole number of
    /// elements: the bytes themselves where the host reads them in place, and otherwise a copy.
    /// </summary>
    internal static ReadOnlySpan<T> Read<T>(ReadOnlySpan<byte> bytes)
        where T : unmanaged
    {
        if (ReadInPlace)
        {
            return MemoryMarshal.Cast<byte, T>(bytes);
        }

        // A new array is aligned to its elements, whatever the bytes' address
        var values = new T[bytes.Length / Unsafe.SizeOf<T>()];
        var destination = MemoryMarshal.AsBytes(values.AsSpan());
        bytes.CopyTo(destination);
        ToLittleEndian(destination, Unsafe.SizeOf<T>());

        return values;
    }

    /// <summary>Writes the elements' little-endian bytes into <paramref name="destination"/>, which is as long.</summary>
    internal static void Write<T>(ReadOnlySpan<T> values, Span<byte> destination)
        where T : unmanaged
    {
        MemoryMarshal.AsBytes(values).CopyTo(destination);
        ToLittleEndian(destination, Unsafe.SizeOf<T>());
    }

    /// <summary>
    /// Writes bools as bytes, 1 for true and 0 for false, into <paramref name="destination"/>, which is
    /// as long. A .NET bool that unsafe code or interop makes can hold a byte other than 1 or 0, so
    /// each byte written is the least of the bool's byte and 1, as <see cref="ToBoolBytes"/> converts
    /// them, which took 19 ns to write 1,000 bools, where a copy of their bytes took 13 ns and a
    /// loop that converts each 490 ns, in .NET 10 on an Apple M2 Pro.
    /// </summary>
    internal static void WriteBools(ReadOnlySpan<bool> values, Span<byte> destination) =>
        ToBoolBytes(MemoryMarshal.AsBytes(values), destination);

    /// <summary>
    /// Reads bytes into new bools, each true where its byte isn't 0. The bytes are never viewed as
    /// bools, since a .NET bool of a byte other than 1 or 0 misbehaves: the bools' own bytes are
    /// written instead, as <see cref="ToBoolBytes"/> converts them, which took 57 ns to read 1,000
    /// bools, their new array included, where a loop took 350 ns, in .NET 10 on an Apple M2 Pro.
    /// </summary>
    internal static bool[] ReadBools(ReadOnlySpan<byte> bytes)
    {
        if (bytes.IsEmpty)
        {
            return [];
        }

        var bools = new bool[bytes.Length];
        ToBoolBytes(bytes, MemoryMarshal.AsBytes(bools.AsSpan()));

        return bools;
    }

    /// <summary>
    /// Writes each byte of <paramref name="bytes"/> into <paramref name="destination"/>, which is as
    /// long, as the byte of a bool: 0 for 0 and 1 for any other byte, the least of the byte and 1. It
    /// takes 16 bytes at a time where the host compares vectors.
    /// </summary>
    private static void ToBoolBytes(ReadOnlySpan<byte> bytes, Span<byte> destination)
    {
        if (!Vector128.IsHardwareAccelerated || bytes.Length < Vector128<byte>.Count)
        {
            for (var index = 0; index < bytes.Length; index++)
            {
                destination[index] = Math.Min(bytes[index], (byte)1);
            }

            return;
        }

        var one = Vector128<byte>.One;
        var vectors = MemoryMarshal.Cast<byte, Vector128<byte>>(bytes);
        var results = MemoryMarshal.Cast<byte, Vector128<byte>>(destination);
        for (var index = 0; index < vectors.Length; index++)
        {
            results[index] = Vector128.Min(vectors[index], one);
        }

        // The last 16 bytes, which overlap those before them unless the length is a multiple of 16
        var last = bytes.Length - Vector128<byte>.Count;
        Vector128.Min(Vector128.Create(bytes[last..]), one).CopyTo(destination[last..]);
    }

    /// <summary>
    /// Reverses the bytes of each element of <paramref name="size"/> bytes on a big-endian host, which
    /// converts them between its byte order and little-endian, either way. Floats are reversed as
    /// their bits, so that every NaN keeps its payload.
    /// </summary>
    private static void ToLittleEndian(Span<byte> bytes, int size)
    {
        if (BitConverter.IsLittleEndian || size == 1)
        {
            return;
        }

        for (var start = 0; start < bytes.Length; start += size)
        {
            bytes.Slice(start, size).Reverse();
        }
    }
}
