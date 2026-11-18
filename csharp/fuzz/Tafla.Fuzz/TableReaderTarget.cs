using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;
using System.Text;
using System.Text.Unicode;

namespace Tafla.Fuzz;

/// <summary>
/// Reads the rest of its input with every getter of <see cref="TableReader"/> at a position, in an
/// offset type and header, which the input's first four bytes choose, and reads the arrays, struct
/// arrays and vector that it finds there.
/// </summary>
/// <remarks>
/// <para>
/// Each read returns the bytes or value that lie where the format puts them, as a span or memory
/// of the input where it reads in place, exactly when they lie within it, and throws a
/// <see cref="DecodeException"/> that describes the input otherwise. Any other exception escapes,
/// and is a finding: an <see cref="ArgumentException"/>, an <see cref="IndexOutOfRangeException"/>
/// or an <see cref="OverflowException"/> is no answer to bytes, since the target never makes the
/// mistakes of a caller that .NET's argument exceptions are for. So are a failed check, and reads
/// that allocate out of proportion to the input.
/// </para>
/// <para>
/// Unlike Rust's target, which reads at every position of every input, this one reads where the
/// input's first bytes say, as C++'s does: the offset type and header, a position within the rest
/// or far past it, a field's index and a struct's size. Most reads of arbitrary bytes fail, and a
/// .NET exception takes microseconds to throw and catch.
/// </para>
/// </remarks>
internal static class TableReaderTarget
{
    /// <summary>
    /// The elements of an array, and items of a vector, at its start and end that are read and
    /// compared with the bytes
    /// </summary>
    private const int Checked = 8;

    /// <summary>
    /// What the reads of an input and their checks may allocate beyond 16 bytes per byte of it: the
    /// exceptions of the reads that fail, and the checks' closures. A string holds 2 bytes per byte
    /// of its UTF-8, which the check decodes again, and a host that copies arrays, as 32-bit Arm and
    /// a big-endian host do, copies each of the 9 arrays wider than a byte once.
    /// </summary>
    private const long AllocationAllowance = 1 << 20;

    private static readonly OffsetType[] OffsetTypes = [OffsetType.UInt8, OffsetType.UInt16, OffsetType.UInt32];

    /// <summary>Positions past any bytes, where a position plus a length overflows, or before them</summary>
    private static readonly int[] FarPositions = [int.MaxValue, int.MaxValue - 1, int.MaxValue - 3, int.MaxValue - 7, -1, -8, int.MinValue];

    public static void Run(ReadOnlySpan<byte> input)
    {
        if (input.Length < 4)
        {
            return;
        }

        // The first bytes choose what to read, and the rest are the table
        var offsetType = OffsetTypes[input[0] % OffsetTypes.Length];
        var sized = (input[0] & 0x10) != 0;
        var far = (input[0] & 0x20) != 0;
        var position = far ? FarPositions[input[1] % FarPositions.Length] : input[1];
        var index = input[2] == 0xff ? int.MaxValue : input[2];
        var structSize = input[3];

        // A copy of exactly the table's bytes, so that a read past them throws
        var bytes = input[4..].ToArray();
        var layout = new TableLayout { OffsetType = offsetType, Sized = sized };

        var allocated = GC.GetAllocatedBytesForCurrentThread();
        CheckHeader(bytes, new TableReader(bytes, layout), index);
        CheckVersion(bytes, new TableReader(bytes, layout with { Versioned = true }), index);
        CheckPosition(bytes, new TableReader(bytes, layout), position, index, structSize);
        allocated = GC.GetAllocatedBytesForCurrentThread() - allocated;

        Check.That(allocated <= 16L * bytes.Length + AllocationAllowance);
    }

    /// <summary>Reads the table's size and the offsets of its root fields 0, 1 and <paramref name="index"/></summary>
    private static void CheckHeader(byte[] bytes, TableReader reader, int index)
    {
        var layout = reader.Layout;

        if (layout.Sized)
        {
            CheckValue(bytes, () => reader.GetSize(), (uint?)Slot(bytes, 0, layout.OffsetType));
        }
        else
        {
            Check.That(reader.GetSize() == (uint)bytes.Length);
        }

        foreach (var field in (int[])[0, 1, index])
        {
            var offset = Slot(bytes, layout.HeaderSize + (long)field * layout.OffsetSize, layout.OffsetType);
            CheckValue(bytes, () => reader.RootOffset(field), ToPosition(offset));
        }
    }

    private static void CheckVersion(byte[] bytes, TableReader reader, int index)
    {
        var layout = reader.Layout;
        var position = layout.Sized ? layout.OffsetSize : 0;

        CheckValue(bytes, () => reader.GetVersion(), (uint?)Slot(bytes, position, layout.OffsetType));
        CheckHeader(bytes, reader, index);
    }

    /// <summary>
    /// Reads every type at <paramref name="position"/>, a field of a table there at
    /// <paramref name="index"/>, and structs of <paramref name="structSize"/> bytes and others
    /// </summary>
    private static void CheckPosition(byte[] bytes, TableReader reader, int position, int index, int structSize)
    {
        var offsetType = reader.Layout.OffsetType;

        CheckScalars(bytes, reader, position);
        CheckArrays(bytes, reader, position);
        CheckString(bytes, reader, position);

        var remaining = Math.Max(bytes.Length - Math.Max(position, 0), 0);
        foreach (var size in (int[])[0, 1, structSize, remaining, remaining + 1, int.MaxValue])
        {
            CheckView(bytes, () => reader.GetStruct(position, size), RangeOf(bytes, position, size));

            // A size of 0 is a caller's mistake, which throws an ArgumentOutOfRangeException
            if (size > 0)
            {
                CheckView(bytes, () => reader.GetStructArray(position, size), ArrayOf(bytes, offsetType, position, size));
            }
        }

        foreach (var field in (int[])[0, index])
        {
            var offset = position < 0 ? null : Slot(bytes, position + (long)field * offsetType.Size, offsetType);
            CheckValue(bytes, () => reader.FieldOffset(position, field), ToPosition(position + offset));
        }

        CheckVector(bytes, reader, position);
    }

    /// <summary>Reads each scalar at <paramref name="position"/>, and compares their bits with the bytes'</summary>
    private static void CheckScalars(byte[] bytes, TableReader reader, int position)
    {
        CheckValue(bytes, () => (ulong)(byte)reader.GetInt8(position), ScalarAt(bytes, position, sizeof(sbyte)));
        CheckValue(bytes, () => (ulong)reader.GetUInt8(position), ScalarAt(bytes, position, sizeof(byte)));
        CheckValue(bytes, () => (ulong)(ushort)reader.GetInt16(position), ScalarAt(bytes, position, sizeof(short)));
        CheckValue(bytes, () => (ulong)reader.GetUInt16(position), ScalarAt(bytes, position, sizeof(ushort)));
        CheckValue(bytes, () => (ulong)(uint)reader.GetInt32(position), ScalarAt(bytes, position, sizeof(int)));
        CheckValue(bytes, () => (ulong)reader.GetUInt32(position), ScalarAt(bytes, position, sizeof(uint)));
        CheckValue(bytes, () => (ulong)reader.GetInt64(position), ScalarAt(bytes, position, sizeof(long)));
        CheckValue(bytes, () => reader.GetUInt64(position), ScalarAt(bytes, position, sizeof(ulong)));

        // By their bits, which keep a NaN's payload on the 64-bit hosts that fuzz
        CheckValue(bytes, () => (ulong)BitConverter.SingleToUInt32Bits(reader.GetFloat32(position)), ScalarAt(bytes, position, sizeof(float)));
        CheckValue(bytes, () => BitConverter.DoubleToUInt64Bits(reader.GetFloat64(position)), ScalarAt(bytes, position, sizeof(double)));
    }

    private static void CheckArrays(byte[] bytes, TableReader reader, int position)
    {
        var offsetType = reader.Layout.OffsetType;

        CheckView(bytes, () => reader.GetUInt8Array(position), ArrayOf(bytes, offsetType, position, sizeof(byte)));
        CheckArray(bytes, offsetType, position, () => reader.GetInt8Array(position));
        CheckArray(bytes, offsetType, position, () => reader.GetInt16Array(position));
        CheckArray(bytes, offsetType, position, () => reader.GetUInt16Array(position));
        CheckArray(bytes, offsetType, position, () => reader.GetInt32Array(position));
        CheckArray(bytes, offsetType, position, () => reader.GetUInt32Array(position));
        CheckArray(bytes, offsetType, position, () => reader.GetInt64Array(position));
        CheckArray(bytes, offsetType, position, () => reader.GetUInt64Array(position));
        CheckArray(bytes, offsetType, position, () => reader.GetFloat32Array(position));
        CheckArray(bytes, offsetType, position, () => reader.GetFloat64Array(position));
    }

    /// <summary>
    /// Checks that an array's elements are those of its bytes, comparing the first and last
    /// <see cref="Checked"/> by their little-endian bytes, and that a span of the input is the
    /// array's own bytes, while a copy lies outside the input. Only those elements are compared,
    /// since a position may hold an array as long as the bytes.
    /// </summary>
    private static void CheckArray<T>(byte[] bytes, OffsetType offsetType, int position, Func<ReadOnlySpan<T>> read)
        where T : unmanaged
    {
        var size = Unsafe.SizeOf<T>();
        var expected = ArrayOf(bytes, offsetType, position, size);

        ReadOnlySpan<T> array;
        try
        {
            array = read();
        }
        catch (DecodeException error)
        {
            CheckError(bytes, error, expected is null);
            return;
        }

        var elements = expected ?? throw new CheckFailedException($"An array outside the bytes read {array.Length} elements");
        Check.That((long)array.Length * size == elements.Length);
        var start = elements.Start;
        var arrayBytes = MemoryMarshal.AsBytes(array);

        if (arrayBytes.Overlaps(bytes))
        {
            Check.That(IsAt(arrayBytes, bytes, start));
        }

        for (var at = 0; at < Math.Min(array.Length, Checked); at++)
        {
            CheckElement(arrayBytes, bytes.AsSpan(start), at, size);
            CheckElement(arrayBytes, bytes.AsSpan(start), array.Length - 1 - at, size);
        }
    }

    /// <summary>Checks that element <paramref name="index"/> of an array is that of its little-endian bytes</summary>
    private static void CheckElement(ReadOnlySpan<byte> array, ReadOnlySpan<byte> littleEndian, int index, int size)
    {
        var element = array.Slice(index * size, size);
        var expected = littleEndian.Slice(index * size, size);

        Check.That(BitConverter.IsLittleEndian ? element.SequenceEqual(expected) : IsReversed(element, expected));
    }

    private static bool IsReversed(ReadOnlySpan<byte> value, ReadOnlySpan<byte> littleEndian)
    {
        for (var at = 0; at < value.Length; at++)
        {
            if (value[at] != littleEndian[^(at + 1)])
            {
                return false;
            }
        }

        return true;
    }

    /// <summary>Reads the string at <paramref name="position"/>, which must be its bytes' UTF-8 exactly when they're valid</summary>
    private static void CheckString(byte[] bytes, TableReader reader, int position)
    {
        var stringBytes = ArrayOf(bytes, reader.Layout.OffsetType, position, sizeof(byte));
        var utf8 = stringBytes is { } range ? bytes.AsSpan(range.Start, range.Length) : default;
        var valid = stringBytes is not null && Utf8.IsValid(utf8);

        string text;
        try
        {
            text = reader.GetString(position);
        }
        catch (DecodeException error)
        {
            CheckError(bytes, error, !valid);
            Check.That(stringBytes is null || (error.Kind == DecodeErrorKind.InvalidUtf8 && error.Position == position));
            return;
        }

        Check.That(valid && text == Encoding.UTF8.GetString(utf8));
    }

    /// <summary>Reads the vector at <paramref name="position"/> and the first and last of its items</summary>
    private static void CheckVector(byte[] bytes, TableReader reader, int position)
    {
        var offsetType = reader.Layout.OffsetType;
        var width = offsetType.Size;

        // A count, and an end for each item after it, within the bytes
        var count = Slot(bytes, position, offsetType);
        var ends = count is { } items && position + width * (items + 1) <= bytes.Length;

        Vector vector;
        try
        {
            vector = reader.GetVector(position);
        }
        catch (DecodeException error)
        {
            CheckError(bytes, error, !ends);
            return;
        }

        Check.That(ends && vector.Count == count);
        CheckItems(bytes, position, offsetType, vector);
    }

    /// <summary>The bytes of a vector's item, or the error of reading it</summary>
    private readonly record struct Item(ReadOnlyMemory<byte> Bytes, DecodeException? Error);

    /// <summary>
    /// Reads the first <see cref="Checked"/> items, in order and by index, as tables of their own and
    /// in place, and the last by index
    /// </summary>
    private static void CheckItems(byte[] bytes, int position, OffsetType offsetType, Vector vector)
    {
        var inOrder = vector.Items().GetEnumerator();
        var inPlace = vector.ItemsInPlace().GetEnumerator();
        var first = Math.Min(vector.Count, Checked);

        for (var index = 0; index < first; index++)
        {
            var item = Next(ref inOrder);
            var itemInPlace = Next(ref inPlace);
            CheckItem(bytes, position, offsetType, index, item, itemInPlace);

            Check.That(SameItem(Find(() => vector.Get(index)), item));
            Check.That(SameItem(Find(() => vector.GetInPlace(index)), itemInPlace));
        }

        // The enumerators end after the last item
        if (vector.Count <= Checked)
        {
            Check.That(!inOrder.MoveNext() && !inPlace.MoveNext());
        }

        if (vector.Count > 0)
        {
            var last = vector.Count - 1;
            CheckItem(bytes, position, offsetType, last, Find(() => vector.Get(last)), Find(() => vector.GetInPlace(last)));
        }

        // Indices that aren't an item's are a caller's mistake
        Check.That(ThrowsOutOfRange(() => vector.Get(vector.Count)));
        Check.That(ThrowsOutOfRange(() => vector.GetInPlace(int.MaxValue)));
    }

    /// <summary>Moves to the next item, which lies within the vector</summary>
    private static Item Next(ref VectorItemEnumerator items)
    {
        try
        {
            Check.That(items.MoveNext());
            return new Item(items.Current, null);
        }
        catch (DecodeException error)
        {
            return new Item(default, error);
        }
    }

    private static Item Find(Func<ReadOnlyMemory<byte>> find)
    {
        try
        {
            return new Item(find(), null);
        }
        catch (DecodeException error)
        {
            return new Item(default, error);
        }
    }

    /// <summary>
    /// Checks item <paramref name="index"/> of the vector at <paramref name="position"/>, which starts
    /// where the one before it ends, and the first after the count and the ends, read as a table of
    /// its own, from its start to its end, and in place, from its start to the end of the bytes
    /// </summary>
    private static void CheckItem(byte[] bytes, int position, OffsetType offsetType, int index, Item item, Item itemInPlace)
    {
        // The count and the ends, which the vector's reader checked lie within the bytes
        var width = offsetType.Size;
        long SlotAt(long at) => Slot(bytes, at, offsetType) ?? throw new CheckFailedException($"No slot at {at}");
        long EndOf(int at) => SlotAt(position + (at + 1L) * width);

        var start = index == 0 ? (SlotAt(position) + 1) * width : EndOf(index - 1);
        var end = EndOf(index);
        (int Start, int Length)? expected = start <= end && position + end <= bytes.Length ? ((int)(position + start), (int)(end - start)) : null;

        if (item.Error is { Kind: DecodeErrorKind.ItemEndsBeforeStart } error)
        {
            Check.That(start > end && error.Position == position + start && error.End == position + end);
        }
        else if (item.Error is { } other)
        {
            CheckError(bytes, other, expected is null);
        }
        else
        {
            Check.That(expected is { } range && IsAt(item.Bytes, bytes, range.Start, range.Length));
        }

        // In place, from the item's start to the end of the bytes, failing as the item does
        if (itemInPlace.Error is { } inPlaceError)
        {
            Check.That(item.Error is { } itemError && SameError(inPlaceError, itemError));
        }
        else
        {
            Check.That(expected is { } range && IsAt(itemInPlace.Bytes, bytes, range.Start, bytes.Length - range.Start));
        }
    }

    private static bool SameError(DecodeException x, DecodeException y) =>
        (x.Kind, x.Position, x.ByteLength, x.Available, x.End) == (y.Kind, y.Position, y.ByteLength, y.Available, y.End);

    /// <summary>Whether two items are the same bytes, from the same position, or the same error</summary>
    private static bool SameItem(Item a, Item b)
    {
        if (a.Error is not null || b.Error is not null)
        {
            return a.Error is { } x && b.Error is { } y && SameError(x, y);
        }

        return a.Bytes.Length == b.Bytes.Length
            && (a.Bytes.IsEmpty || Unsafe.AreSame(ref MemoryMarshal.GetReference(a.Bytes.Span), ref MemoryMarshal.GetReference(b.Bytes.Span)));
    }

    private static bool ThrowsOutOfRange(Func<ReadOnlyMemory<byte>> read)
    {
        try
        {
            read();
            return false;
        }
        catch (ArgumentOutOfRangeException)
        {
            return true;
        }
    }

    /// <summary>Checks that a read returns the expected value, or throws the decode error when there's none</summary>
    private static void CheckValue<T>(byte[] bytes, Func<T> read, T? expected)
        where T : struct, IEquatable<T>
    {
        T value;
        try
        {
            value = read();
        }
        catch (DecodeException error)
        {
            CheckError(bytes, error, expected is null);
            return;
        }

        Check.That(expected is { } same && value.Equals(same));
    }

    /// <summary>
    /// Checks that a read returns the expected bytes, as a span of the input, or throws the decode
    /// error when there are none
    /// </summary>
    private static void CheckView(byte[] bytes, Func<ReadOnlySpan<byte>> read, (int Start, int Length)? expected)
    {
        ReadOnlySpan<byte> view;
        try
        {
            view = read();
        }
        catch (DecodeException error)
        {
            CheckError(bytes, error, expected is null);
            return;
        }

        Check.That(expected is { } range && view.Length == range.Length && IsAt(view, bytes, range.Start));
    }

    /// <summary>Checks that a read failed where it should have, with an error that describes the bytes</summary>
    private static void CheckError(byte[] bytes, DecodeException error, bool expected)
    {
        if (!expected)
        {
            Check.Fail($"A read within the bytes threw {error}");
        }

        Check.That(error.Message.Length > 0);

        switch (error.Kind)
        {
            case DecodeErrorKind.OutOfRange:
                Check.That(error.Available == bytes.Length);
                break;
            case DecodeErrorKind.InvalidUtf8:
                Check.That(error.Position >= 0 && error.Position < bytes.Length);
                break;
            case DecodeErrorKind.ItemEndsBeforeStart:
                Check.That(error.End < error.Position);
                break;
            default:
                // A string too long for .NET needs more bytes than libfuzzer-dotnet's inputs hold
                Check.Fail($"A read threw {error.Kind}");
                break;
        }
    }

    /// <summary>Whether non-empty <paramref name="view"/> starts at <paramref name="start"/> in the bytes</summary>
    private static bool IsAt(ReadOnlySpan<byte> view, byte[] bytes, int start) =>
        view.IsEmpty || Unsafe.AreSame(ref MemoryMarshal.GetReference(view), ref bytes[start]);

    /// <summary>Whether memory is the <paramref name="length"/> bytes at <paramref name="start"/></summary>
    private static bool IsAt(ReadOnlyMemory<byte> memory, byte[] bytes, int start, int length) =>
        MemoryMarshal.TryGetArray(memory, out var segment)
        && segment.Array == bytes
        && segment.Offset == start
        && segment.Count == length;

    /// <summary>A position from an offset, which one past the largest int can't be</summary>
    private static int? ToPosition(long? offset) => offset <= int.MaxValue ? (int?)offset : null;

    /// <summary>
    /// The value of the slot as wide as <paramref name="offsetType"/> at <paramref name="position"/>, read
    /// a byte at a time, or null when it doesn't lie within the bytes
    /// </summary>
    private static long? Slot(byte[] bytes, long position, OffsetType offsetType) =>
        (long?)ScalarAt(bytes, position, offsetType.Size);

    /// <summary>The little-endian value of <paramref name="size"/> bytes at <paramref name="position"/></summary>
    private static ulong? ScalarAt(byte[] bytes, long position, int size)
    {
        if (RangeOf(bytes, position, size) is not { } range)
        {
            return null;
        }

        ulong value = 0;
        for (var at = size - 1; at >= 0; at--)
        {
            value = value << 8 | bytes[range.Start + at];
        }

        return value;
    }

    /// <summary>
    /// The bytes of the elements of <paramref name="elementSize"/> bytes after the count at
    /// <paramref name="position"/>, or null when the count or the elements don't lie within the bytes
    /// </summary>
    private static (int Start, int Length)? ArrayOf(byte[] bytes, OffsetType offsetType, long position, int elementSize) =>
        Slot(bytes, position, offsetType) is { } count
            // At most 2^32 - 1 elements of up to 2^31 - 1 bytes, which a long holds
            ? RangeOf(bytes, position + offsetType.Size, count * elementSize)
            : null;

    /// <summary>The <paramref name="byteLength"/> bytes at <paramref name="position"/>, or null when they don't lie within the bytes</summary>
    private static (int Start, int Length)? RangeOf(byte[] bytes, long position, long byteLength) =>
        position >= 0 && position <= bytes.Length && byteLength <= bytes.Length - position
            ? ((int)position, (int)byteLength)
            : null;
}
