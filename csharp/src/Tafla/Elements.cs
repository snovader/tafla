using System;
using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;

namespace Tafla;

/// <summary>
/// Reads and writes the little-endian elements of arrays: in place, as spans of the table's bytes,
/// where the host reads them so, and otherwise copied, in the host's byte order.
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
