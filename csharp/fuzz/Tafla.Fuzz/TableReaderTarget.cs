using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;
using System.Text;
using System.Text.Unicode;

namespace Tafla.Fuzz;

/// <summary>
/// Reads the rest of its input with every getter of <see cref="TableReader"/> at a position, in an
/// offset type and header, which the input's first four bytes choose, and reads the arrays, struct
/// arrays, vector and vector of strings that it finds there.
/// </summary>
/// <remarks>
/// <para>
/// Each read returns the value or bytes that lie where the format puts them, exactly when they lie
/// within the input, and otherwise throws a <see cref="DecodeException"/> that describes the read
/// that leaves it. Structs, vector items and arrays of bytes are spans of the input. Wider arrays
/// are too where the runtime reads them in place, and copies outside it where it doesn't: on a
/// big-endian host, on 32-bit Arm, and built with <c>-p:TaflaPortable=true</c>, as
/// <c>TAFLA_PORTABLE=true fuzz/fuzz.sh</c> builds them. A read that returns allocates nothing but
/// the string or copy that it returns, and one that throws nothing but its exception.
/// </para>
/// <para>
/// Any other exception escapes, and is a finding: an <see cref="ArgumentException"/>, an
/// <see cref="IndexOutOfRangeException"/> or an <see cref="OverflowException"/> is no answer to
/// bytes, since the target never makes the mistakes of a caller that .NET's argument exceptions
/// are for. So is a failed check.
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
    /// The elements of a copied array, and items of a vector, at its start and end that are read and
    /// compared with the bytes
    /// </summary>
    private const int Checked = 8;

    /// <summary>What an object holds beyond its elements, at most: its header, its type, its length and padding</summary>
    private const long ObjectOverhead = 32;

    /// <summary>
    /// What a read that throws may allocate: its <see cref="DecodeException"/>, with its message and
    /// stack trace, and an invalid string's <see cref="DecoderFallbackException"/> within it. The
    /// most that one allocated, over 3.4 million inputs of both builds, was 1,672 bytes.
    /// </summary>
    private const long ExceptionAllowance = 4 << 10;

    private static readonly OffsetType[] OffsetTypes = [OffsetType.UInt8, OffsetType.UInt16, OffsetType.UInt32];

    /// <summary>Positions past any bytes, where a position plus a length overflows, or before them</summary>
    private static readonly int[] FarPositions = [int.MaxValue, int.MaxValue - 1, int.MaxValue - 3, int.MaxValue - 7, -1, -8, int.MinValue];

    /// <summary>
    /// Whether the runtime reads arrays of elements wider than a byte in place, as its README says: on
    /// a little-endian host other than 32-bit Arm, unless it's built with <c>-p:TaflaPortable=true</c>,
    /// which defines <c>TAFLA_PORTABLE</c> for this program too
    /// </summary>
    private static readonly bool ReadsArraysInPlace =
        !Portable
        && BitConverter.IsLittleEndian
        && RuntimeInformation.ProcessArchitecture is not (Architecture.Arm or Architecture.Armv6);

#if TAFLA_PORTABLE
    private const bool Portable = true;
#else
    private const bool Portable = false;
#endif

    /// <summary>
    /// Whether the reads' allocations are checked, which the static constructor sets once the target
    /// has warmed up, since the runtime and .NET allocate some things once, such as the reader's UTF-8
    /// decoder and an exception's resources
    /// </summary>
    private static readonly bool ChecksAllocations;

    static TableReaderTarget()
    {
        CheckDefaultVector();
        CheckDefaultStringVector();
        WarmUp();
        ChecksAllocations = true;
    }

    /// <summary>
    /// Reads that throw, decode an invalid and a valid string, and copy arrays where the runtime
    /// copies them, unchecked, so that what the runtime and .NET allocate once, such as the reader's
    /// UTF-8 decoder and an exception's resources, is allocated before the reads that are checked
    /// </summary>
    private static void WarmUp()
    {
        var layout = new TableLayout { OffsetType = OffsetType.UInt8 };
        var invalid = new TableReader(new byte[] { 2, 0xc0, 0x80 }, layout);
        var valid = new TableReader(new byte[] { 1, (byte)'a' }, layout);
        var empty = new TableReader(new byte[] { 0 }, layout);

        try
        {
            _ = invalid.GetString(0);
        }
        catch (DecodeException) { }

        try
        {
            _ = invalid.GetUInt64(0);
        }
        catch (DecodeException) { }

        _ = valid.GetString(0);

        var strings = new TableReader(new byte[] { 2, 3, 4, 0xc0, 0x80 }, layout).GetStringVector(0);
        _ = strings[0];
        try
        {
            _ = strings[1];
        }
        catch (DecodeException) { }
        _ = empty.GetInt16Array(0).Length + empty.GetUInt16Array(0).Length;
        _ = empty.GetInt32Array(0).Length + empty.GetUInt32Array(0).Length + empty.GetFloat32Array(0).Length;
        _ = empty.GetInt64Array(0).Length + empty.GetUInt64Array(0).Length + empty.GetFloat64Array(0).Length;
    }

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

        // A copy of exactly the table's bytes, so that a read past them throws. The default reader
        // reads no bytes in the default layout, as a reader of an empty table in it does.
        var copy = input[4..].ToArray();
        var layout = new TableLayout { OffsetType = offsetType, Sized = sized };
        var reader = copy.Length == 0 && layout == TableLayout.Default ? default : new TableReader(copy, layout);
        var bytes = reader.Bytes;

        Check.That(reader.Sized == sized && reader.OffsetType == offsetType && bytes.Length == copy.Length);
        Check.That(copy.Length == 0 || Unsafe.AreSame(ref MemoryMarshal.GetReference(bytes), ref copy[0]));

        CheckHeader(bytes, reader, layout, index);
        CheckVersion(bytes, layout with { Version = 0 }, index);
        CheckPosition(bytes, reader, position, index, structSize);
    }

    /// <summary>Reads the table's size and the offsets of its root fields 0, 1 and <paramref name="index"/></summary>
    private static void CheckHeader(ReadOnlySpan<byte> bytes, TableReader reader, TableLayout layout, int index)
    {
        var size = layout.Sized ? SlotAt(bytes, 0, layout.OffsetType) : new Expected<long>(bytes.Length, 0, 0);
        CheckValue(bytes, reader, static reader => (long)reader.GetSize(), size);

        foreach (var field in (int[])[0, 1, index])
        {
            var offset = SlotAt(bytes, layout.HeaderSize + (long)field * layout.OffsetSize, layout.OffsetType);
            CheckValue(bytes, reader, reader => reader.RootOffset(field), ToPosition(offset));
        }
    }

    private static void CheckVersion(ReadOnlySpan<byte> bytes, TableLayout layout, int index)
    {
        var reader = new TableReader(bytes, layout);
        var version = SlotAt(bytes, layout.Sized ? layout.OffsetSize : 0, layout.OffsetType);

        CheckValue(bytes, reader, static reader => (long)reader.GetVersion(), version);
        CheckHeader(bytes, reader, layout, index);
    }

    /// <summary>
    /// Reads every type at <paramref name="position"/>, a field of a table there at
    /// <paramref name="index"/>, and structs of <paramref name="structSize"/> bytes and others
    /// </summary>
    private static void CheckPosition(ReadOnlySpan<byte> bytes, TableReader reader, int position, int index, int structSize)
    {
        var offsetType = reader.OffsetType;

        CheckScalars(bytes, reader, position);
        CheckArrays(bytes, reader, position);
        CheckString(bytes, reader, position);

        var remaining = Math.Max(bytes.Length - Math.Max(position, 0), 0);
        foreach (var size in (int[])[0, 1, structSize, remaining, remaining + 1, int.MaxValue])
        {
            CheckView(bytes, reader, reader => reader.GetStruct(position, size), RangeAt(bytes, position, size));

            // A size of 0 is a caller's mistake, which throws an ArgumentOutOfRangeException
            if (size > 0)
            {
                CheckView(bytes, reader, reader => reader.GetStructArray(position, size), ArrayAt(bytes, offsetType, position, size));
            }
        }

        foreach (var field in (int[])[0, index])
        {
            CheckValue(bytes, reader, reader => reader.FieldOffset(position, field), FieldOffsetAt(bytes, offsetType, position, field));
        }

        CheckVector(bytes, reader, position);
        CheckStringVector(bytes, reader, position);
    }

    /// <summary>Reads each scalar at <paramref name="position"/>, and compares their bits with the bytes'</summary>
    private static void CheckScalars(ReadOnlySpan<byte> bytes, TableReader reader, int position)
    {
        CheckValue(bytes, reader, reader => (ulong)(byte)reader.GetInt8(position), ScalarAt(bytes, position, sizeof(sbyte)));
        CheckValue(bytes, reader, reader => (ulong)reader.GetUInt8(position), ScalarAt(bytes, position, sizeof(byte)));
        CheckValue(bytes, reader, reader => (ulong)(ushort)reader.GetInt16(position), ScalarAt(bytes, position, sizeof(short)));
        CheckValue(bytes, reader, reader => (ulong)reader.GetUInt16(position), ScalarAt(bytes, position, sizeof(ushort)));
        CheckValue(bytes, reader, reader => (ulong)(uint)reader.GetInt32(position), ScalarAt(bytes, position, sizeof(int)));
        CheckValue(bytes, reader, reader => (ulong)reader.GetUInt32(position), ScalarAt(bytes, position, sizeof(uint)));
        CheckValue(bytes, reader, reader => (ulong)reader.GetInt64(position), ScalarAt(bytes, position, sizeof(long)));
        CheckValue(bytes, reader, reader => reader.GetUInt64(position), ScalarAt(bytes, position, sizeof(ulong)));

        // By their bits, which keep a NaN's payload on the 64-bit hosts that fuzz
        CheckValue(bytes, reader, reader => (ulong)BitConverter.SingleToUInt32Bits(reader.GetFloat32(position)), ScalarAt(bytes, position, sizeof(float)));
        CheckValue(bytes, reader, reader => BitConverter.DoubleToUInt64Bits(reader.GetFloat64(position)), ScalarAt(bytes, position, sizeof(double)));
    }

    private static void CheckArrays(ReadOnlySpan<byte> bytes, TableReader reader, int position)
    {
        CheckView(bytes, reader, reader => reader.GetUInt8Array(position), ArrayAt(bytes, reader.OffsetType, position, sizeof(byte)));
        CheckArray(bytes, reader, position, reader => reader.GetInt8Array(position));
        CheckArray(bytes, reader, position, reader => reader.GetInt16Array(position));
        CheckArray(bytes, reader, position, reader => reader.GetUInt16Array(position));
        CheckArray(bytes, reader, position, reader => reader.GetInt32Array(position));
        CheckArray(bytes, reader, position, reader => reader.GetUInt32Array(position));
        CheckArray(bytes, reader, position, reader => reader.GetInt64Array(position));
        CheckArray(bytes, reader, position, reader => reader.GetUInt64Array(position));
        CheckArray(bytes, reader, position, reader => reader.GetFloat32Array(position));
        CheckArray(bytes, reader, position, reader => reader.GetFloat64Array(position));
    }

    /// <summary>
    /// Checks that an array is its bytes in the input, where the runtime reads it in place, and
    /// otherwise a copy outside the input, whose first and last <see cref="Checked"/> elements are
    /// compared with the bytes. Only those are compared, since a position may hold an array as long
    /// as the bytes.
    /// </summary>
    private static void CheckArray<T>(ReadOnlySpan<byte> bytes, TableReader reader, int position, Func<TableReader, ReadOnlySpan<T>> read)
        where T : unmanaged
    {
        var size = Unsafe.SizeOf<T>();
        var expected = ArrayAt(bytes, reader.OffsetType, position, size);

        var start = Allocated();
        ReadOnlySpan<T> array;
        try
        {
            array = read(reader);
        }
        catch (DecodeException error)
        {
            CheckFailure(bytes, error, expected, start);
            return;
        }

        var allocated = Allocated() - start;
        var range = expected.Value ?? throw new CheckFailedException($"An array outside the bytes read {array.Length} elements");
        var arrayBytes = MemoryMarshal.AsBytes(array);

        if (size == 1 || ReadsArraysInPlace)
        {
            CheckAllocation(allocated, 0);
            Check.That(IsAt(arrayBytes, bytes, range.Start, range.Length));
            return;
        }

        CheckAllocation(allocated, range.Length + ObjectOverhead);
        Check.That(arrayBytes.Length == range.Length && !arrayBytes.Overlaps(bytes));

        var elements = bytes.Slice(range.Start, range.Length);
        for (var at = 0; at < Math.Min(array.Length, Checked); at++)
        {
            CheckElement(arrayBytes, elements, at, size);
            CheckElement(arrayBytes, elements, array.Length - 1 - at, size);
        }
    }

    /// <summary>Checks that element <paramref name="index"/> of a copied array is that of its little-endian bytes</summary>
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

    /// <summary>
    /// Reads the string at <paramref name="position"/>, which is its bytes' UTF-8 exactly when they're
    /// valid, and otherwise throws <see cref="DecodeErrorKind.InvalidUtf8"/>
    /// </summary>
    private static void CheckString(ReadOnlySpan<byte> bytes, TableReader reader, int position)
    {
        var expected = ArrayAt(bytes, reader.OffsetType, position, sizeof(byte));
        var utf8 = expected.Value is { } range ? bytes.Slice(range.Start, range.Length) : default;
        var valid = expected.Value is not null && Utf8.IsValid(utf8);

        var start = Allocated();
        string text;
        try
        {
            text = reader.GetString(position);
        }
        catch (DecodeException error) when (expected.Value is null)
        {
            CheckFailure(bytes, error, expected, start);
            return;
        }
        catch (DecodeException error)
        {
            CheckAllocation(Allocated() - start, ExceptionAllowance);
            Check.That(!valid && error.Kind == DecodeErrorKind.InvalidUtf8 && error.Position == position);
            Check.That(error.Message.Length > 0 && error.InnerException is DecoderFallbackException);
            return;
        }

        CheckAllocation(Allocated() - start, 2L * text.Length + ObjectOverhead);
        Check.That(valid && text == Encoding.UTF8.GetString(utf8));
    }

    /// <summary>Reads the vector at <paramref name="position"/> and the first and last of its items</summary>
    private static void CheckVector(ReadOnlySpan<byte> bytes, TableReader reader, int position)
    {
        var offsetType = reader.OffsetType;

        // A count, and an end for each item after it, within the bytes
        var expected = SlotAt(bytes, position, offsetType);
        if (expected.Value is { } count && RangeAt(bytes, position + (long)offsetType.Size, count * offsetType.Size) is { Value: null } ends)
        {
            expected = new Expected<long>(null, ends.Position, ends.ByteLength);
        }

        var start = Allocated();
        Vector vector;
        try
        {
            vector = reader.GetVector(position);
        }
        catch (DecodeException error)
        {
            CheckFailure(bytes, error, expected, start);
            return;
        }

        CheckAllocation(Allocated() - start, 0);
        Check.That(expected.Value is { } length && vector.Count == length);
        CheckItems(bytes, position, offsetType, vector);
    }

    /// <summary>A vector's item: where it starts in the input and its length, or the error of reading it</summary>
    private readonly record struct Item(long Start, int Length, DecodeException? Error);

    /// <summary>
    /// Reads the first <see cref="Checked"/> items, in order and by index, as tables of their own and
    /// in place, and the last by index
    /// </summary>
    private static void CheckItems(ReadOnlySpan<byte> bytes, int position, OffsetType offsetType, Vector vector)
    {
        var inOrder = vector.Items().GetEnumerator();
        var inPlace = vector.ItemsInPlace().GetEnumerator();
        var first = Math.Min(vector.Count, Checked);

        for (var index = 0; index < first; index++)
        {
            var item = Next(bytes, ref inOrder);
            var itemInPlace = Next(bytes, ref inPlace);
            CheckItem(bytes, position, offsetType, index, item, itemInPlace);

            Check.That(SameItem(Find(bytes, vector, vector => vector.Get(index)), item));
            Check.That(SameItem(Find(bytes, vector, vector => vector.GetInPlace(index)), itemInPlace));
        }

        // The enumerators end after the last item
        if (vector.Count <= Checked)
        {
            Check.That(!inOrder.MoveNext() && !inPlace.MoveNext());
        }

        if (vector.Count > 0)
        {
            var last = vector.Count - 1;
            var item = Find(bytes, vector, vector => vector.Get(last));
            CheckItem(bytes, position, offsetType, last, item, Find(bytes, vector, vector => vector.GetInPlace(last)));
        }

        // Indices that aren't an item's are a caller's mistake
        foreach (var index in (int[])[vector.Count, int.MaxValue, -1, int.MinValue])
        {
            Check.That(ThrowsOutOfRange(vector, vector => vector.Get(index)));
            Check.That(ThrowsOutOfRange(vector, vector => vector.GetInPlace(index)));
        }
    }

    /// <summary>Moves to the next item, which lies within the vector</summary>
    private static Item Next(ReadOnlySpan<byte> bytes, ref VectorItemEnumerator items)
    {
        var start = Allocated();
        try
        {
            var moved = items.MoveNext();
            CheckAllocation(Allocated() - start, 0);
            Check.That(moved);

            var item = items.Current;
            return new Item(StartOf(item, bytes), item.Length, null);
        }
        catch (DecodeException error)
        {
            CheckAllocation(Allocated() - start, ExceptionAllowance);
            return new Item(0, 0, error);
        }
    }

    private static Item Find(ReadOnlySpan<byte> bytes, Vector vector, Func<Vector, ReadOnlySpan<byte>> find)
    {
        var start = Allocated();
        try
        {
            var item = find(vector);
            CheckAllocation(Allocated() - start, 0);

            return new Item(StartOf(item, bytes), item.Length, null);
        }
        catch (DecodeException error)
        {
            CheckAllocation(Allocated() - start, ExceptionAllowance);
            return new Item(0, 0, error);
        }
    }

    /// <summary>
    /// Checks item <paramref name="index"/> of the vector at <paramref name="position"/>, which starts
    /// where the one before it ends, and the first after the count and the ends, read as a table of
    /// its own, from its start to its end, and in place, from its start to the end of the bytes
    /// </summary>
    private static void CheckItem(ReadOnlySpan<byte> bytes, int position, OffsetType offsetType, int index, Item item, Item itemInPlace)
    {
        // The count and the ends, which the vector's reader checked lie within the bytes
        var width = offsetType.Size;
        var start = index == 0
            ? (SlotWithin(bytes, position, offsetType) + 1) * width
            : SlotWithin(bytes, position + (long)index * width, offsetType);
        var end = SlotWithin(bytes, position + (index + 1L) * width, offsetType);

        if (start > end)
        {
            var error = item.Error ?? throw new CheckFailedException("An item that ends before it starts was read");
            Check.That(error.Kind == DecodeErrorKind.ItemEndsBeforeStart && error.Message.Length > 0);
            Check.That(error.Position == position + start && error.End == position + end);
        }
        else if (position + end > bytes.Length)
        {
            var failure = item.Error ?? throw new CheckFailedException("An item past the bytes was read");
            CheckOutOfRange(bytes, failure, new Expected<long>(null, position + start, end - start));
        }
        else
        {
            Check.That(item.Error is null && item.Start == position + start && item.Length == end - start);
        }

        // In place, from the item's start to the end of the bytes, failing as the item does
        if (item.Error is { } itemError)
        {
            Check.That(itemInPlace.Error is { } inPlaceError && SameError(inPlaceError, itemError));
        }
        else
        {
            Check.That(itemInPlace.Error is null && itemInPlace.Start == position + start && itemInPlace.Length == bytes.Length - (position + start));
        }
    }

    /// <summary>The slot at a position that a vector's reader checked lies within the bytes</summary>
    private static long SlotWithin(ReadOnlySpan<byte> bytes, long position, OffsetType offsetType) =>
        SlotAt(bytes, position, offsetType).Value ?? throw new CheckFailedException($"No slot at {position}");

    private static bool SameError(DecodeException x, DecodeException y) =>
        (x.Kind, x.Position, x.ByteLength, x.Available, x.End) == (y.Kind, y.Position, y.ByteLength, y.Available, y.End);

    /// <summary>Whether two items are the same bytes, from the same position, or the same error</summary>
    private static bool SameItem(Item a, Item b) =>
        a.Error is not null || b.Error is not null
            ? a.Error is { } x && b.Error is { } y && SameError(x, y)
            : a.Start == b.Start && a.Length == b.Length;

    private static bool ThrowsOutOfRange(Vector vector, Func<Vector, ReadOnlySpan<byte>> read)
    {
        try
        {
            read(vector);
            return false;
        }
        catch (ArgumentOutOfRangeException)
        {
            return true;
        }
    }

    /// <summary>The default vector is empty</summary>
    private static void CheckDefaultVector()
    {
        var vector = default(Vector);
        var inOrder = vector.Items().GetEnumerator();
        var inPlace = vector.ItemsInPlace().GetEnumerator();

        Check.That(vector.Count == 0 && !inOrder.MoveNext() && !inPlace.MoveNext());
        Check.That(ThrowsOutOfRange(vector, vector => vector.Get(0)) && ThrowsOutOfRange(vector, vector => vector.GetInPlace(0)));
    }

    /// <summary>
    /// Reads the vector of strings at <paramref name="position"/>, whose count and ends lie as a
    /// vector's, and checks each of its strings, by index and in order, and all of them at once
    /// </summary>
    private static void CheckStringVector(ReadOnlySpan<byte> bytes, TableReader reader, int position)
    {
        var offsetType = reader.OffsetType;

        // A count, and an end for each string after it, within the bytes
        var expected = SlotAt(bytes, position, offsetType);
        if (expected.Value is { } count && RangeAt(bytes, position + (long)offsetType.Size, count * offsetType.Size) is { Value: null } ends)
        {
            expected = new Expected<long>(null, ends.Position, ends.ByteLength);
        }

        var start = Allocated();
        StringVector vector;
        try
        {
            vector = reader.GetStringVector(position);
        }
        catch (DecodeException error)
        {
            CheckFailure(bytes, error, expected, start);
            return;
        }

        CheckAllocation(Allocated() - start, 0);
        Check.That(expected.Value is { } length && vector.Count == length);
        CheckStrings(bytes, position, offsetType, vector);
    }

    /// <summary>A string of a vector of strings, or the error of reading it</summary>
    private readonly record struct Text(string? Value, DecodeException? Error);

    /// <summary>
    /// Reads the strings by index and in order, each of which must be what its bytes hold, the first
    /// <see cref="Checked"/> and then on to the first that fails, and the last by index, and all of
    /// them at once, which must read the same strings, or fail with the error of the first that fails.
    /// Reading stops at the first failure past the first <see cref="Checked"/> strings, since a .NET
    /// exception takes microseconds, and a vector may hold as many strings as the input has bytes.
    /// </summary>
    private static void CheckStrings(ReadOnlySpan<byte> bytes, int position, OffsetType offsetType, StringVector vector)
    {
        var found = new List<string>();
        var inOrder = vector.GetEnumerator();
        DecodeException? first = null;
        long allowed = ObjectOverhead + (long)IntPtr.Size * vector.Count;

        var index = 0;
        for (; index < vector.Count && (index < Checked || first is null); index++)
        {
            var text = FindString(vector, index);
            CheckVectorString(bytes, position, offsetType, index, text);
            Check.That(SameText(NextString(ref inOrder), text));

            if (text.Value is { } value)
            {
                found.Add(value);
                allowed += 2L * value.Length + ObjectOverhead;
            }
            else
            {
                first ??= text.Error;
            }
        }

        // The enumerator ends after the last string
        if (index == vector.Count)
        {
            Check.That(!inOrder.MoveNext());
        }

        if (vector.Count > 0)
        {
            var last = vector.Count - 1;
            CheckVectorString(bytes, position, offsetType, last, FindString(vector, last));
        }

        var start = Allocated();
        try
        {
            var all = vector.ToArray();
            CheckAllocation(Allocated() - start, allowed);
            Check.That(first is null && all.AsSpan().SequenceEqual(found.ToArray()));
        }
        catch (DecodeException error)
        {
            Check.That(first is not null && SameError(error, first));
        }

        // Indices that aren't a string's are a caller's mistake
        foreach (var outside in (int[])[vector.Count, int.MaxValue, -1, int.MinValue])
        {
            Check.That(ThrowsOutOfRange(vector, vector => vector[outside]));
        }
    }

    /// <summary>Reads string <paramref name="index"/> by its ends, which allocates the string, or its exception</summary>
    private static Text FindString(StringVector vector, int index)
    {
        var start = Allocated();
        try
        {
            var value = vector[index];
            CheckAllocation(Allocated() - start, 2L * value.Length + ObjectOverhead);

            return new Text(value, null);
        }
        catch (DecodeException error)
        {
            CheckAllocation(Allocated() - start, ExceptionAllowance);
            return new Text(null, error);
        }
    }

    /// <summary>Moves to the next string, which lies within the vector</summary>
    private static Text NextString(ref StringVector.Enumerator strings)
    {
        try
        {
            Check.That(strings.MoveNext());
            return new Text(strings.Current, null);
        }
        catch (DecodeException error)
        {
            return new Text(null, error);
        }
    }

    /// <summary>
    /// Checks string <paramref name="index"/> of the vector of strings at <paramref name="position"/>,
    /// which starts where the one before it ends, and the first after the count and the ends, and is
    /// its bytes' UTF-8 exactly when they lie within the bytes and are valid
    /// </summary>
    private static void CheckVectorString(ReadOnlySpan<byte> bytes, int position, OffsetType offsetType, int index, Text text)
    {
        // The count and the ends, which the vector's reader checked lie within the bytes
        var width = offsetType.Size;
        var start = index == 0
            ? (SlotWithin(bytes, position, offsetType) + 1) * width
            : SlotWithin(bytes, position + (long)index * width, offsetType);
        var end = SlotWithin(bytes, position + (index + 1L) * width, offsetType);

        if (start > end)
        {
            var error = text.Error ?? throw new CheckFailedException("A string that ends before it starts was read");
            Check.That(error.Kind == DecodeErrorKind.ItemEndsBeforeStart && error.Message.Length > 0);
            Check.That(error.Position == position + start && error.End == position + end);
        }
        else if (position + end > bytes.Length)
        {
            var failure = text.Error ?? throw new CheckFailedException("A string past the bytes was read");
            CheckOutOfRange(bytes, failure, new Expected<long>(null, position + start, end - start));
        }
        else
        {
            var utf8 = bytes.Slice((int)(position + start), (int)(end - start));
            if (Utf8.IsValid(utf8))
            {
                Check.That(text.Value == Encoding.UTF8.GetString(utf8));
            }
            else
            {
                var error = text.Error ?? throw new CheckFailedException("A string that isn't valid UTF-8 was read");
                Check.That(error.Kind == DecodeErrorKind.InvalidUtf8 && error.Position == position + start);
                Check.That(error.Message.Length > 0 && error.InnerException is DecoderFallbackException);
            }
        }
    }

    private static bool SameText(Text a, Text b) =>
        a.Error is not null || b.Error is not null
            ? a.Error is { } x && b.Error is { } y && SameError(x, y)
            : a.Value == b.Value;

    private static bool ThrowsOutOfRange(StringVector vector, Func<StringVector, string> read)
    {
        try
        {
            read(vector);
            return false;
        }
        catch (ArgumentOutOfRangeException)
        {
            return true;
        }
    }

    /// <summary>The default vector of strings is empty</summary>
    private static void CheckDefaultStringVector()
    {
        var vector = default(StringVector);
        var strings = vector.GetEnumerator();

        Check.That(vector.Count == 0 && vector.ToArray().Length == 0 && !strings.MoveNext());
        Check.That(ThrowsOutOfRange(vector, vector => vector[0]));
    }

    /// <summary>Checks that a read returns the expected value, or throws for the read that leaves the bytes</summary>
    private static void CheckValue<T>(ReadOnlySpan<byte> bytes, TableReader reader, Func<TableReader, T> read, Expected<T> expected)
        where T : struct, IEquatable<T>
    {
        var start = Allocated();
        T value;
        try
        {
            value = read(reader);
        }
        catch (DecodeException error)
        {
            CheckFailure(bytes, error, expected, start);
            return;
        }

        CheckAllocation(Allocated() - start, 0);
        Check.That(expected.Value is { } same && value.Equals(same));
    }

    /// <summary>
    /// Checks that a read returns the expected bytes, as a span of the input, or throws for the read
    /// that leaves them
    /// </summary>
    private static void CheckView(ReadOnlySpan<byte> bytes, TableReader reader, Func<TableReader, ReadOnlySpan<byte>> read, Expected<(int Start, int Length)> expected)
    {
        var start = Allocated();
        ReadOnlySpan<byte> view;
        try
        {
            view = read(reader);
        }
        catch (DecodeException error)
        {
            CheckFailure(bytes, error, expected, start);
            return;
        }

        CheckAllocation(Allocated() - start, 0);
        Check.That(expected.Value is { } range && IsAt(view, bytes, range.Start, range.Length));
    }

    /// <summary>
    /// Checks that a read that threw, after the current thread had allocated <paramref name="start"/>
    /// bytes, allocated no more than its exception, and threw for the read that leaves the bytes
    /// </summary>
    private static void CheckFailure<T>(ReadOnlySpan<byte> bytes, DecodeException error, Expected<T> expected, long start)
        where T : struct
    {
        CheckAllocation(Allocated() - start, ExceptionAllowance);
        CheckOutOfRange(bytes, error, expected);
    }

    /// <summary>
    /// Checks that a read failed where it should have, with an <see cref="DecodeErrorKind.OutOfRange"/>
    /// error that describes the read that leaves the bytes
    /// </summary>
    private static void CheckOutOfRange<T>(ReadOnlySpan<byte> bytes, DecodeException error, Expected<T> expected)
        where T : struct
    {
        if (expected.Value is not null)
        {
            Check.Fail($"A read within the bytes threw {error}");
        }

        Check.That(error.Kind == DecodeErrorKind.OutOfRange && error.Message.Length > 0);
        Check.That(error.Available == bytes.Length);
        Check.That(error.Position == expected.Position && error.ByteLength == expected.ByteLength);
    }

    /// <summary>The bytes that the current thread has allocated</summary>
    private static long Allocated() => GC.GetAllocatedBytesForCurrentThread();

    private static void CheckAllocation(long allocated, long allowed) =>
        Check.That(!ChecksAllocations || allocated <= allowed);

    /// <summary>Whether <paramref name="view"/> is the <paramref name="length"/> bytes at <paramref name="start"/></summary>
    private static bool IsAt(ReadOnlySpan<byte> view, ReadOnlySpan<byte> bytes, int start, int length) =>
        view.Length == length && StartOf(view, bytes) == start;

    /// <summary>Where <paramref name="view"/> starts, from the start of the bytes, even when it's empty</summary>
    private static long StartOf(ReadOnlySpan<byte> view, ReadOnlySpan<byte> bytes) =>
        Unsafe.ByteOffset(ref MemoryMarshal.GetReference(bytes), ref MemoryMarshal.GetReference(view));

    /// <summary>
    /// The position that a field's offset leads to, or the read that fails: of the offset's slot, or
    /// at the position, past the largest int, that no bytes reach
    /// </summary>
    private static Expected<int> ToPosition(Expected<long> offset) =>
        offset.Value is not { } position ? new(null, offset.Position, offset.ByteLength)
        : position > int.MaxValue ? new(null, position, 0)
        : new((int)position, 0, 0);

    /// <summary>The position of field <paramref name="field"/> of the table nested at <paramref name="table"/></summary>
    private static Expected<int> FieldOffsetAt(ReadOnlySpan<byte> bytes, OffsetType offsetType, int table, int field)
    {
        if (table < 0)
        {
            return new(null, table, offsetType.Size);
        }

        var offset = SlotAt(bytes, table + (long)field * offsetType.Size, offsetType);
        return ToPosition(offset.Value is { } value ? new(table + value, 0, 0) : offset);
    }

    /// <summary>The value of the slot as wide as <paramref name="offsetType"/> at <paramref name="position"/></summary>
    private static Expected<long> SlotAt(ReadOnlySpan<byte> bytes, long position, OffsetType offsetType)
    {
        var slot = ScalarAt(bytes, position, offsetType.Size);
        return new((long?)slot.Value, slot.Position, slot.ByteLength);
    }

    /// <summary>The little-endian value of <paramref name="size"/> bytes at <paramref name="position"/>, read a byte at a time</summary>
    private static Expected<ulong> ScalarAt(ReadOnlySpan<byte> bytes, long position, int size)
    {
        if (RangeAt(bytes, position, size).Value is not { } range)
        {
            return new(null, position, size);
        }

        ulong value = 0;
        for (var at = size - 1; at >= 0; at--)
        {
            value = value << 8 | bytes[range.Start + at];
        }

        return new(value, 0, 0);
    }

    /// <summary>
    /// The bytes of the elements of <paramref name="elementSize"/> bytes after the count at
    /// <paramref name="position"/>, or the read that fails: of the count, or of the elements
    /// </summary>
    private static Expected<(int Start, int Length)> ArrayAt(ReadOnlySpan<byte> bytes, OffsetType offsetType, long position, int elementSize)
    {
        var count = SlotAt(bytes, position, offsetType);

        // At most 2^32 - 1 elements of up to 2^31 - 1 bytes, which a long holds
        return count.Value is { } elements
            ? RangeAt(bytes, position + offsetType.Size, elements * elementSize)
            : new(null, count.Position, count.ByteLength);
    }

    /// <summary>The <paramref name="byteLength"/> bytes at <paramref name="position"/>, or that read, which fails</summary>
    private static Expected<(int Start, int Length)> RangeAt(ReadOnlySpan<byte> bytes, long position, long byteLength) =>
        position >= 0 && position <= bytes.Length && byteLength <= bytes.Length - position
            ? new(((int)position, (int)byteLength), 0, 0)
            : new(null, position, byteLength);
}

/// <summary>
/// What a read returns, <see cref="Value"/>, or, when it's null, the read of
/// <see cref="ByteLength"/> bytes at <see cref="Position"/> that leaves the bytes, which it throws for
/// </summary>
internal readonly record struct Expected<T>(T? Value, long Position, long ByteLength)
    where T : struct;
