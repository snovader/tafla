using System;
using System.Buffers.Binary;
using System.Runtime.InteropServices;
using System.Text;

namespace Tafla;

/// <summary>
/// Reads a table from the supplied bytes, in the layout it was written with. Fields are found by
/// index with <see cref="RootOffset"/>, or <see cref="FieldOffset"/> for a table nested in another,
/// and then read at that position by their type's getter.
/// </summary>
/// <remarks>
/// <para>
/// Every read checks that it stays within the bytes, and throws a <see cref="DecodeException"/>
/// otherwise, so no bytes, however corrupted, make a read throw any other exception or return values
/// from outside them. Strings must be valid UTF-8. A read allocates nothing for a count that the
/// bytes can't hold.
/// </para>
/// <para>
/// Arrays, structs and vector items are read in place, as spans of the bytes rather than copies, so
/// they show changes made to the bytes later. On a big-endian host, and on 32-bit Arm, arrays of
/// elements wider than a byte are copied instead, since their bytes can't be read in place there. A
/// string is decoded into a new <see cref="string"/>.
/// </para>
/// <para>
/// A <see cref="TableReader"/> is a <c>ref struct</c> over a span of the bytes, which can be those of
/// an array, a slice of one, a <see cref="ReadOnlyMemory{T}"/>'s <see cref="ReadOnlyMemory{T}.Span"/>,
/// or stack or native memory. It's a small value, which is cheap to copy and to create for each read,
/// and it lives on the stack, as a span does: it can't be stored in a class, or in a struct other than
/// a <c>ref struct</c>, captured by a lambda, or kept across an <c>await</c> or a <c>yield</c>. Code
/// that keeps a table keeps its bytes, and creates a reader where it reads them. Its default value
/// reads no bytes in the default layout.
/// </para>
/// </remarks>
public readonly ref struct TableReader
{
    /// <summary>A UTF-8 decoder that throws for invalid UTF-8, rather than replacing it with U+FFFD</summary>
    private static readonly UTF8Encoding Utf8 = new(encoderShouldEmitUTF8Identifier: false, throwOnInvalidBytes: true);

    /// <summary>The most characters that a <see cref="string"/> holds, which .NET doesn't expose</summary>
    private const int MaxStringLength = 0x3fff_ffdf;

    private readonly ReadOnlySpan<byte> _bytes;
    private readonly Header _layout;

    /// <summary>
    /// Creates a reader of the table in <paramref name="bytes"/>, in <paramref name="layout"/>. An array,
    /// an <see cref="ArraySegment{T}"/> or a <see cref="Span{T}"/> converts to the span implicitly.
    /// </summary>
    public TableReader(ReadOnlySpan<byte> bytes, TableLayout layout = default)
    {
        _bytes = bytes;
        _layout = new Header(layout.Sized, layout.Versioned, layout.OffsetType);
    }

    /// <summary>The bytes that the reader reads.</summary>
    public ReadOnlySpan<byte> Bytes => _bytes;

    /// <summary>Whether the layout that the reader was given is sized.</summary>
    public bool Sized => _layout.Sized;

    /// <summary>The offset type of the layout that the reader was given.</summary>
    public OffsetType OffsetType => _layout.OffsetType;

    /// <summary>
    /// Returns the table's byte length from its header when the layout is sized, and otherwise the
    /// length of the bytes.
    /// </summary>
    /// <exception cref="DecodeException">The header doesn't lie within the bytes.</exception>
    public uint GetSize() => _layout.Sized ? ReadSlot(_bytes, 0) : (uint)_bytes.Length;

    /// <summary>
    /// Returns the version that the table's header carries, after its length when it's sized. It's as
    /// wide as the header's other slots. The reader doesn't compare versions: a reader of another
    /// version throws <see cref="DecodeException.UnsupportedVersion"/>.
    /// </summary>
    /// <exception cref="DecodeException">The header doesn't lie within the bytes.</exception>
    /// <exception cref="InvalidOperationException">The layout isn't versioned.</exception>
    public uint GetVersion()
    {
        if (!_layout.Versioned)
        {
            throw new InvalidOperationException("The layout has no version");
        }

        return ReadSlot(_bytes, _layout.Sized ? _layout.OffsetSize : 0);
    }

    /// <summary>Returns the position of field <paramref name="index"/> of the root table.</summary>
    /// <exception cref="DecodeException">
    /// The field's offset doesn't lie within the bytes, or leads past the largest <see cref="int"/>,
    /// past any bytes. An offset to elsewhere outside them throws from the read at its position.
    /// </exception>
    /// <exception cref="ArgumentOutOfRangeException"><paramref name="index"/> is negative.</exception>
    public int RootOffset(int index)
    {
        ArgumentOutOfRangeException.ThrowIfNegative(index);

        var bytes = _bytes;
        long offset = ReadSlot(bytes, _layout.HeaderSize + (long)index * _layout.OffsetSize);

        return ToPosition(offset, bytes.Length);
    }

    /// <summary>
    /// Returns the position of field <paramref name="index"/> of a nested table, which has no header
    /// and stores its field offsets from its own start, at <paramref name="tablePosition"/>, the
    /// position of the field that holds it.
    /// </summary>
    /// <exception cref="DecodeException">
    /// The field's offset doesn't lie within the bytes, or leads past the largest <see cref="int"/>,
    /// past any bytes. An offset to elsewhere outside them throws from the read at its position.
    /// </exception>
    /// <exception cref="ArgumentOutOfRangeException"><paramref name="index"/> is negative.</exception>
    public int FieldOffset(int tablePosition, int index)
    {
        ArgumentOutOfRangeException.ThrowIfNegative(index);

        var bytes = _bytes;
        int width = _layout.OffsetSize;
        if (tablePosition < 0)
        {
            DecodeException.ThrowOutOfRange(tablePosition, width, bytes.Length);
        }

        long offset = ReadSlot(bytes, tablePosition + (long)index * width);
        return ToPosition(tablePosition + offset, bytes.Length);
    }

    // The scalar getters read the value at a position, which must lie within the bytes. Unsigned
    // values are read into C#'s unsigned types.

    /// <summary>Reads the <c>int8</c> at <paramref name="position"/>.</summary>
    /// <exception cref="DecodeException">The value doesn't lie within the bytes.</exception>
    public sbyte GetInt8(int position) => (sbyte)GetUInt8(position);

    /// <summary>Reads the <c>uint8</c> at <paramref name="position"/>.</summary>
    /// <exception cref="DecodeException">The value doesn't lie within the bytes.</exception>
    public byte GetUInt8(int position)
    {
        var bytes = _bytes;
        if ((uint)position >= (uint)bytes.Length)
        {
            DecodeException.ThrowOutOfRange(position, sizeof(byte), bytes.Length);
        }

        return bytes[position];
    }

    /// <summary>Reads the <c>int16</c> at <paramref name="position"/>.</summary>
    /// <exception cref="DecodeException">The value doesn't lie within the bytes.</exception>
    public short GetInt16(int position) =>
        BinaryPrimitives.ReadInt16LittleEndian(Scalar(position, sizeof(short)));

    /// <summary>Reads the <c>uint16</c> at <paramref name="position"/>.</summary>
    /// <exception cref="DecodeException">The value doesn't lie within the bytes.</exception>
    public ushort GetUInt16(int position) =>
        BinaryPrimitives.ReadUInt16LittleEndian(Scalar(position, sizeof(ushort)));

    /// <summary>Reads the <c>int32</c> at <paramref name="position"/>.</summary>
    /// <exception cref="DecodeException">The value doesn't lie within the bytes.</exception>
    public int GetInt32(int position) =>
        BinaryPrimitives.ReadInt32LittleEndian(Scalar(position, sizeof(int)));

    /// <summary>Reads the <c>uint32</c> at <paramref name="position"/>.</summary>
    /// <exception cref="DecodeException">The value doesn't lie within the bytes.</exception>
    public uint GetUInt32(int position) =>
        BinaryPrimitives.ReadUInt32LittleEndian(Scalar(position, sizeof(uint)));

    /// <summary>Reads the <c>int64</c> at <paramref name="position"/>.</summary>
    /// <exception cref="DecodeException">The value doesn't lie within the bytes.</exception>
    public long GetInt64(int position) =>
        BinaryPrimitives.ReadInt64LittleEndian(Scalar(position, sizeof(long)));

    /// <summary>Reads the <c>uint64</c> at <paramref name="position"/>.</summary>
    /// <exception cref="DecodeException">The value doesn't lie within the bytes.</exception>
    public ulong GetUInt64(int position) =>
        BinaryPrimitives.ReadUInt64LittleEndian(Scalar(position, sizeof(ulong)));

    /// <summary>Reads the <c>float32</c> at <paramref name="position"/>, with all its bits, a NaN's payload too.</summary>
    /// <exception cref="DecodeException">The value doesn't lie within the bytes.</exception>
    public float GetFloat32(int position) =>
        BinaryPrimitives.ReadSingleLittleEndian(Scalar(position, sizeof(float)));

    /// <summary>Reads the <c>float64</c> at <paramref name="position"/>, with all its bits, a NaN's payload too.</summary>
    /// <exception cref="DecodeException">The value doesn't lie within the bytes.</exception>
    public double GetFloat64(int position) =>
        BinaryPrimitives.ReadDoubleLittleEndian(Scalar(position, sizeof(double)));

    // The array getters check the count at a position against the bytes, and return the elements
    // after it, as a span of the bytes where the host reads them in place.

    /// <summary>Reads the array of <c>int8</c>s at <paramref name="position"/>, as a span of the bytes.</summary>
    /// <exception cref="DecodeException">The array doesn't lie within the bytes.</exception>
    public ReadOnlySpan<sbyte> GetInt8Array(int position) =>
        MemoryMarshal.Cast<byte, sbyte>(ArrayBytes(position, sizeof(sbyte)));

    /// <summary>Reads the array of <c>uint8</c>s at <paramref name="position"/>, as a span of the bytes.</summary>
    /// <exception cref="DecodeException">The array doesn't lie within the bytes.</exception>
    public ReadOnlySpan<byte> GetUInt8Array(int position) => ArrayBytes(position, sizeof(byte));

    /// <summary>
    /// Reads the array of <c>int16</c>s at <paramref name="position"/>, as a span of the bytes on a
    /// little-endian host, and a copy otherwise.
    /// </summary>
    /// <exception cref="DecodeException">The array doesn't lie within the bytes.</exception>
    public ReadOnlySpan<short> GetInt16Array(int position) =>
        Elements.Read<short>(ArrayBytes(position, sizeof(short)));

    /// <summary>Reads the array of <c>uint16</c>s at <paramref name="position"/>, as <see cref="GetInt16Array"/> does.</summary>
    /// <exception cref="DecodeException">The array doesn't lie within the bytes.</exception>
    public ReadOnlySpan<ushort> GetUInt16Array(int position) =>
        Elements.Read<ushort>(ArrayBytes(position, sizeof(ushort)));

    /// <summary>Reads the array of <c>int32</c>s at <paramref name="position"/>, as <see cref="GetInt16Array"/> does.</summary>
    /// <exception cref="DecodeException">The array doesn't lie within the bytes.</exception>
    public ReadOnlySpan<int> GetInt32Array(int position) =>
        Elements.Read<int>(ArrayBytes(position, sizeof(int)));

    /// <summary>Reads the array of <c>uint32</c>s at <paramref name="position"/>, as <see cref="GetInt16Array"/> does.</summary>
    /// <exception cref="DecodeException">The array doesn't lie within the bytes.</exception>
    public ReadOnlySpan<uint> GetUInt32Array(int position) =>
        Elements.Read<uint>(ArrayBytes(position, sizeof(uint)));

    /// <summary>Reads the array of <c>int64</c>s at <paramref name="position"/>, as <see cref="GetInt16Array"/> does.</summary>
    /// <exception cref="DecodeException">The array doesn't lie within the bytes.</exception>
    public ReadOnlySpan<long> GetInt64Array(int position) =>
        Elements.Read<long>(ArrayBytes(position, sizeof(long)));

    /// <summary>Reads the array of <c>uint64</c>s at <paramref name="position"/>, as <see cref="GetInt16Array"/> does.</summary>
    /// <exception cref="DecodeException">The array doesn't lie within the bytes.</exception>
    public ReadOnlySpan<ulong> GetUInt64Array(int position) =>
        Elements.Read<ulong>(ArrayBytes(position, sizeof(ulong)));

    /// <summary>Reads the array of <c>float32</c>s at <paramref name="position"/>, as <see cref="GetInt16Array"/> does.</summary>
    /// <exception cref="DecodeException">The array doesn't lie within the bytes.</exception>
    public ReadOnlySpan<float> GetFloat32Array(int position) =>
        Elements.Read<float>(ArrayBytes(position, sizeof(float)));

    /// <summary>Reads the array of <c>float64</c>s at <paramref name="position"/>, as <see cref="GetInt16Array"/> does.</summary>
    /// <exception cref="DecodeException">The array doesn't lie within the bytes.</exception>
    public ReadOnlySpan<double> GetFloat64Array(int position) =>
        Elements.Read<double>(ArrayBytes(position, sizeof(double)));

    /// <summary>
    /// Reads the string at <paramref name="position"/>, whose bytes must be valid UTF-8, into a new
    /// <see cref="string"/>. A leading U+FEFF is part of the string rather than a byte order mark.
    /// </summary>
    /// <exception cref="DecodeException">
    /// The string doesn't lie within the bytes, isn't valid UTF-8, or has more characters than a
    /// <see cref="string"/> holds, 1,073,741,791, which only a string of more bytes than that can.
    /// </exception>
    public string GetString(int position)
    {
        var bytes = ArrayBytes(position, sizeof(byte));

        try
        {
            // UTF-8 has at most one character per byte, so only a string of more bytes than a string
            // holds characters is counted, before anything is allocated for it
            if (bytes.Length > MaxStringLength && Utf8.GetCharCount(bytes) > MaxStringLength)
            {
                throw DecodeException.StringTooLong(position, bytes.Length);
            }

            return Utf8.GetString(bytes);
        }
        catch (DecoderFallbackException exception)
        {
            throw DecodeException.InvalidUtf8(position, exception);
        }
    }

    /// <summary>
    /// Reads the <paramref name="size"/> bytes of the struct at <paramref name="position"/>, as a span
    /// of the bytes. Its scalars lie at fixed offsets from its start, little-endian, which
    /// <see cref="BinaryPrimitives"/> reads.
    /// </summary>
    /// <exception cref="DecodeException">The struct doesn't lie within the bytes.</exception>
    /// <exception cref="ArgumentOutOfRangeException"><paramref name="size"/> is negative.</exception>
    public ReadOnlySpan<byte> GetStruct(int position, int size)
    {
        ArgumentOutOfRangeException.ThrowIfNegative(size);
        return Range(_bytes, position, size);
    }

    /// <summary>
    /// Reads the bytes of the structs of the array at <paramref name="position"/>, each of
    /// <paramref name="size"/> bytes, one after another after the count, as a span of the bytes.
    /// Struct <c>i</c> starts at <c>i * size</c>, and their number is the span's length over
    /// <paramref name="size"/>.
    /// </summary>
    /// <exception cref="DecodeException">The array doesn't lie within the bytes.</exception>
    /// <exception cref="ArgumentOutOfRangeException"><paramref name="size"/> is less than 1.</exception>
    public ReadOnlySpan<byte> GetStructArray(int position, int size)
    {
        ArgumentOutOfRangeException.ThrowIfNegativeOrZero(size);
        return ArrayBytes(position, size);
    }

    /// <summary>
    /// Reads the vector at <paramref name="position"/>, after checking that its count and the ends of
    /// its items lie within the bytes. Each item is checked when it's read.
    /// </summary>
    /// <exception cref="DecodeException">The count or the ends don't lie within the bytes.</exception>
    public Vector GetVector(int position)
    {
        var bytes = _bytes;
        uint count = ReadSlot(bytes, position);

        // Every item has an end, so this rejects an impossible count before any item is read
        int width = _layout.OffsetSize;
        Range(bytes, (long)position + width, (long)count * width);

        return new Vector(_bytes, position, (int)count, _layout.OffsetType);
    }

    /// <summary>Returns the <paramref name="size"/> bytes of a scalar at <paramref name="position"/>.</summary>
    private ReadOnlySpan<byte> Scalar(int position, int size)
    {
        var bytes = _bytes;
        if (position < 0 || position > bytes.Length - size)
        {
            DecodeException.ThrowOutOfRange(position, size, bytes.Length);
        }

        return bytes.Slice(position, size);
    }

    /// <summary>
    /// Returns the bytes of the elements of <paramref name="elementSize"/> bytes each after the count
    /// at <paramref name="position"/>, once they're checked to lie within the bytes, so that nothing
    /// is allocated for a count that the bytes can't hold
    /// </summary>
    private ReadOnlySpan<byte> ArrayBytes(int position, int elementSize)
    {
        var bytes = _bytes;
        uint count = ReadSlot(bytes, position);

        // At most 2^32 - 1 elements of up to 2^31 - 1 bytes, which a long holds
        return Range(bytes, (long)position + _layout.OffsetSize, (long)count * elementSize);
    }

    /// <summary>Reads a header slot, count, offset or item end, as wide as the offset size, at <paramref name="position"/>.</summary>
    private uint ReadSlot(ReadOnlySpan<byte> bytes, long position)
    {
        int width = _layout.OffsetSize;
        if (position < 0 || position > bytes.Length - width)
        {
            DecodeException.ThrowOutOfRange(position, width, bytes.Length);
        }

        return ReadUnsigned(bytes.Slice((int)position, width), _layout.OffsetType);
    }

    /// <summary>Reads an unsigned value of <paramref name="offsetType"/> at the start of <paramref name="slot"/>.</summary>
    internal static uint ReadUnsigned(ReadOnlySpan<byte> slot, OffsetType offsetType) =>
        offsetType switch
        {
            OffsetType.UInt8 => slot[0],
            OffsetType.UInt16 => BinaryPrimitives.ReadUInt16LittleEndian(slot),
            _ => BinaryPrimitives.ReadUInt32LittleEndian(slot),
        };

    /// <summary>Returns the <paramref name="byteLength"/> bytes at <paramref name="position"/>.</summary>
    private static ReadOnlySpan<byte> Range(ReadOnlySpan<byte> bytes, long position, long byteLength)
    {
        if (position < 0 || position > bytes.Length || byteLength > bytes.Length - position)
        {
            DecodeException.ThrowOutOfRange(position, byteLength, bytes.Length);
        }

        return bytes.Slice((int)position, (int)byteLength);
    }

    /// <summary>
    /// Converts an offset to a position, which the read at it checks against the bytes. An offset past
    /// the largest <see cref="int"/> lies past any bytes, and throws here, since a position can't hold it.
    /// </summary>
    private static int ToPosition(long offset, int available)
    {
        if (offset > int.MaxValue)
        {
            DecodeException.ThrowOutOfRange(offset, 0, available);
        }

        return (int)offset;
    }

    /// <summary>
    /// What the reader takes from its layout. It reads tables of any version, so it doesn't keep the
    /// layout's: holding the whole layout, which is longer for it, made generated code take 1.10-1.12
    /// times as long to read a vector of nested tables.
    /// </summary>
    private readonly struct Header(bool sized, bool versioned, OffsetType offsetType)
    {
        public bool Sized { get; } = sized;

        public bool Versioned { get; } = versioned;

        public OffsetType OffsetType { get; } = offsetType;

        public int OffsetSize => OffsetType.Size;

        public int HeaderSize => ((Sized ? 1 : 0) + (Versioned ? 1 : 0)) * OffsetSize;
    }
}
