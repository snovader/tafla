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
    /// as long. It relies on each bool being the byte 1 or 0, as C# makes it, and copies their bytes
    /// at once, which took 13 ns to write 1,000 bools, where a loop that converts each took 490 ns, in
    /// .NET 10 on an Apple M2 Pro. A bool of another byte, which only unsafe code or interop makes, is
    /// written as that byte, which readers read as true.
    /// </summary>
    internal static void WriteBools(ReadOnlySpan<bool> values, Span<byte> destination) =>
        MemoryMarshal.AsBytes(values).CopyTo(destination);

    /// <summary>
    /// Reads bytes into new bools, each true where its byte isn't 0. The bytes are never viewed as
    /// bools, since a .NET bool of a byte other than 1 or 0 misbehaves: the bools' own bytes are
    /// written instead, each the least of its byte and 1, 16 at a time where the host compares
    /// vectors, which took 57 ns to read 1,000 bools, their new array included, where a loop took
    /// 350 ns, in .NET 10 on an Apple M2 Pro.
    /// </summary>
    internal static bool[] ReadBools(ReadOnlySpan<byte> bytes)
    {
        if (bytes.IsEmpty)
        {
            return [];
        }

        var bools = new bool[bytes.Length];
        if (!Vector128.IsHardwareAccelerated || bytes.Length < Vector128<byte>.Count)
        {
            for (var index = 0; index < bools.Length; index++)
            {
                bools[index] = bytes[index] != 0;
            }

            return bools;
        }

        // The least of a byte and 1 is 0 for 0 and 1 for any other byte
        var one = Vector128<byte>.One;
        var destination = MemoryMarshal.AsBytes(bools.AsSpan());
        var vectors = MemoryMarshal.Cast<byte, Vector128<byte>>(bytes);
        var results = MemoryMarshal.Cast<byte, Vector128<byte>>(destination);
        for (var index = 0; index < vectors.Length; index++)
        {
            results[index] = Vector128.Min(vectors[index], one);
        }

        // The last 16 bytes, which overlap those before them unless the length is a multiple of 16
        var last = bytes.Length - Vector128<byte>.Count;
        Vector128.Min(Vector128.Create(bytes[last..]), one).CopyTo(destination[last..]);

        return bools;
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
