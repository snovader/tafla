using System;
using System.Buffers.Binary;
using System.Runtime.CompilerServices;
using System.Text;

namespace Tafla;

/// <summary>
/// Builds a table, which a <see cref="ByteBuffer"/> reads back: a header with one offset per field,
/// after the table's byte length and version when its layout has them, followed by the fields in the
/// order they're set. Arrays, strings and vectors start with their count, and a vector then holds the
/// end of each item. Arrays of scalars and of structs are padded so that their elements are aligned
/// from the start of the table. Fields are read back by index with <see cref="ByteBuffer.RootOffset"/>,
/// or <see cref="ByteBuffer.FieldOffset"/> for a table nested in another.
/// </summary>
/// <remarks>
/// <para>
/// Each setter writes its field into the builder's bytes, which grow as fields are set, so a value
/// may change once it's set, and a nested builder too. <see cref="ToArray"/> copies the table, at its
/// exact length, once every field is set.
/// </para>
/// <para>
/// A setter throws a <see cref="BuildException"/>, and leaves the builder as it was, when the table
/// already holds all its fields, or when its field would make the table longer than the layout's
/// offset type addresses: 255 bytes, 65,535 bytes, or for <see cref="OffsetType.UInt32"/>, as long as
/// a .NET array can be, <see cref="Array.MaxLength"/> bytes.
/// </para>
/// <para>
/// Each setter takes its field's own C# type, such as <see cref="sbyte"/> for an <c>int8</c> and
/// <see cref="float"/> for a <c>float32</c>, so the caller converts a value to it, as C# converts.
/// </para>
/// </remarks>
public sealed class TableBuilder
{
    /// <summary>The alignment of a nested table given as bytes, which suits tables of 8-, 16- and 32-bit elements.</summary>
    public const int DefaultTableAlignment = 4;

    /// <summary>The bytes that a builder starts with room for, beyond its header</summary>
    private const int InitialCapacity = 64;

    /// <summary>
    /// The longest part of a string that's counted at once: its UTF-8 is at most 3 bytes a char,
    /// which an <see cref="int"/> counts for this many chars
    /// </summary>
    private const int Utf8CountedChars = 1 << 20;

    private readonly TableLayout _layout;
    private readonly int _fields;

    /// <summary>The longest table that the layout addresses</summary>
    private readonly int _maxLength;

    /// <summary>The table's bytes so far, and the zeroes after them, which padding leaves</summary>
    private byte[] _bytes;

    /// <summary>The table's length so far, which is where the next field goes</summary>
    private int _length;

    private int _fieldsSet;

    /// <summary>
    /// The largest alignment that a field needs from the table's start, which a parent pads this
    /// table to when it nests it
    /// </summary>
    private int _alignment = 1;

    /// <summary>
    /// Starts a table of <paramref name="fields"/> fields in <paramref name="layout"/>, which its
    /// readers must use too. Nested tables use the default layout.
    /// </summary>
    /// <exception cref="BuildException">The header alone is too long for the layout's offset type.</exception>
    /// <exception cref="ArgumentOutOfRangeException"><paramref name="fields"/> is negative.</exception>
    /// <exception cref="ArgumentException">
    /// The layout is versioned, which the constructor that takes a version builds.
    /// </exception>
    public TableBuilder(int fields, TableLayout layout = default)
        : this(fields, layout, versioned: false, version: 0) { }

    /// <summary>
    /// Starts a table of <paramref name="fields"/> fields in <paramref name="layout"/>, with
    /// <paramref name="version"/> in its header, which readers can check with
    /// <see cref="ByteBuffer.GetVersion"/> before reading any field.
    /// </summary>
    /// <exception cref="BuildException">
    /// The version doesn't fit in the layout's offset type, or the header alone is too long for it.
    /// </exception>
    /// <exception cref="ArgumentOutOfRangeException"><paramref name="fields"/> is negative.</exception>
    /// <exception cref="ArgumentException">The layout isn't versioned.</exception>
    public TableBuilder(int fields, TableLayout layout, uint version)
        : this(fields, layout, versioned: true, version) { }

    private TableBuilder(int fields, TableLayout layout, bool versioned, uint version)
    {
        int header = TableLayout.HeaderLength(fields, layout, versioned, version);

        _layout = layout;
        _fields = fields;
        _maxLength = layout.MaxLength;
        _length = header;
        _bytes = new byte[Math.Min((long)header + InitialCapacity, Array.MaxLength)];

        if (versioned)
        {
            WriteSlot(_bytes.AsSpan(layout.Sized ? layout.OffsetSize : 0), version);
        }

        if (fields == 0)
        {
            Complete();
        }
    }

    /// <summary>The layout that the table is written in.</summary>
    public TableLayout Layout => _layout;

    /// <summary>Copies the table, at its exact length, into a new array.</summary>
    /// <exception cref="BuildException">Fewer fields were set than the table holds.</exception>
    public byte[] ToArray() => WrittenBytes().ToArray();

    // Scalars aren't aligned, so they need no padding

    /// <summary>Sets an <c>int8</c> field.</summary>
    /// <exception cref="BuildException">
    /// The table holds all its fields already, or the field would make it too long for its offset type.
    /// </exception>
    public void SetInt8(sbyte value) => AddField(0, sizeof(sbyte), 1)[0] = (byte)value;

    /// <summary>Sets a <c>uint8</c> field, which is also how a <c>bool</c> is written, as 0 or 1.</summary>
    /// <exception cref="BuildException">As <see cref="SetInt8"/> throws it.</exception>
    public void SetUInt8(byte value) => AddField(0, sizeof(byte), 1)[0] = value;

    /// <summary>Sets an <c>int16</c> field.</summary>
    /// <exception cref="BuildException">As <see cref="SetInt8"/> throws it.</exception>
    public void SetInt16(short value) =>
        BinaryPrimitives.WriteInt16LittleEndian(AddField(0, sizeof(short), 1), value);

    /// <summary>Sets a <c>uint16</c> field.</summary>
    /// <exception cref="BuildException">As <see cref="SetInt8"/> throws it.</exception>
    public void SetUInt16(ushort value) =>
        BinaryPrimitives.WriteUInt16LittleEndian(AddField(0, sizeof(ushort), 1), value);

    /// <summary>Sets an <c>int32</c> field.</summary>
    /// <exception cref="BuildException">As <see cref="SetInt8"/> throws it.</exception>
    public void SetInt32(int value) =>
        BinaryPrimitives.WriteInt32LittleEndian(AddField(0, sizeof(int), 1), value);

    /// <summary>Sets a <c>uint32</c> field.</summary>
    /// <exception cref="BuildException">As <see cref="SetInt8"/> throws it.</exception>
    public void SetUInt32(uint value) =>
        BinaryPrimitives.WriteUInt32LittleEndian(AddField(0, sizeof(uint), 1), value);

    /// <summary>Sets an <c>int64</c> field.</summary>
    /// <exception cref="BuildException">As <see cref="SetInt8"/> throws it.</exception>
    public void SetInt64(long value) =>
        BinaryPrimitives.WriteInt64LittleEndian(AddField(0, sizeof(long), 1), value);

    /// <summary>Sets a <c>uint64</c> field.</summary>
    /// <exception cref="BuildException">As <see cref="SetInt8"/> throws it.</exception>
    public void SetUInt64(ulong value) =>
        BinaryPrimitives.WriteUInt64LittleEndian(AddField(0, sizeof(ulong), 1), value);

    /// <summary>Sets a <c>float32</c> field, with all its bits, a NaN's payload too.</summary>
    /// <exception cref="BuildException">As <see cref="SetInt8"/> throws it.</exception>
    public void SetFloat32(float value) =>
        BinaryPrimitives.WriteSingleLittleEndian(AddField(0, sizeof(float), 1), value);

    /// <summary>Sets a <c>float64</c> field, with all its bits, a NaN's payload too.</summary>
    /// <exception cref="BuildException">As <see cref="SetInt8"/> throws it.</exception>
    public void SetFloat64(double value) =>
        BinaryPrimitives.WriteDoubleLittleEndian(AddField(0, sizeof(double), 1), value);

    // Arrays are padded so that their elements start at a multiple of their size from the table's
    // start, and copied in bulk

    /// <summary>Sets an array field of <c>int8</c>s.</summary>
    /// <exception cref="BuildException">As <see cref="SetInt8"/> throws it.</exception>
    public void SetInt8Array(ReadOnlySpan<sbyte> values) => SetArray(values);

    /// <summary>Sets an array field of <c>uint8</c>s.</summary>
    /// <exception cref="BuildException">As <see cref="SetInt8"/> throws it.</exception>
    public void SetUInt8Array(ReadOnlySpan<byte> values) => SetArray(values);

    /// <summary>Sets an array field of <c>int16</c>s.</summary>
    /// <exception cref="BuildException">As <see cref="SetInt8"/> throws it.</exception>
    public void SetInt16Array(ReadOnlySpan<short> values) => SetArray(values);

    /// <summary>Sets an array field of <c>uint16</c>s.</summary>
    /// <exception cref="BuildException">As <see cref="SetInt8"/> throws it.</exception>
    public void SetUInt16Array(ReadOnlySpan<ushort> values) => SetArray(values);

    /// <summary>Sets an array field of <c>int32</c>s.</summary>
    /// <exception cref="BuildException">As <see cref="SetInt8"/> throws it.</exception>
    public void SetInt32Array(ReadOnlySpan<int> values) => SetArray(values);

    /// <summary>Sets an array field of <c>uint32</c>s.</summary>
    /// <exception cref="BuildException">As <see cref="SetInt8"/> throws it.</exception>
    public void SetUInt32Array(ReadOnlySpan<uint> values) => SetArray(values);

    /// <summary>Sets an array field of <c>int64</c>s.</summary>
    /// <exception cref="BuildException">As <see cref="SetInt8"/> throws it.</exception>
    public void SetInt64Array(ReadOnlySpan<long> values) => SetArray(values);

    /// <summary>Sets an array field of <c>uint64</c>s.</summary>
    /// <exception cref="BuildException">As <see cref="SetInt8"/> throws it.</exception>
    public void SetUInt64Array(ReadOnlySpan<ulong> values) => SetArray(values);

    /// <summary>Sets an array field of <c>float32</c>s, with all their bits.</summary>
    /// <exception cref="BuildException">As <see cref="SetInt8"/> throws it.</exception>
    public void SetFloat32Array(ReadOnlySpan<float> values) => SetArray(values);

    /// <summary>Sets an array field of <c>float64</c>s, with all their bits.</summary>
    /// <exception cref="BuildException">As <see cref="SetInt8"/> throws it.</exception>
    public void SetFloat64Array(ReadOnlySpan<double> values) => SetArray(values);

    /// <summary>
    /// Sets a string field, of the string's UTF-8. A lone surrogate, which UTF-8 can't encode, is
    /// written as U+FFFD, as <see cref="Encoding.UTF8"/> writes it.
    /// </summary>
    /// <exception cref="BuildException">As <see cref="SetInt8"/> throws it.</exception>
    /// <exception cref="ArgumentNullException"><paramref name="value"/> is null.</exception>
    public void SetString(string value)
    {
        ArgumentNullException.ThrowIfNull(value);

        int width = _layout.OffsetSize;
        long byteCount = Utf8Length(value);
        var field = AddField(0, width + byteCount, 1);

        // The field fits in the table, so its count does too
        WriteSlot(field, (uint)byteCount);
        Encoding.UTF8.GetBytes(value, field[width..]);
    }

    /// <summary>
    /// Sets a struct field from its bytes: its scalars, little-endian, one after another, which are
    /// written as they are, without padding.
    /// </summary>
    /// <exception cref="BuildException">As <see cref="SetInt8"/> throws it.</exception>
    public void SetStruct(ReadOnlySpan<byte> bytes) => bytes.CopyTo(AddField(0, bytes.Length, 1));

    /// <summary>
    /// Sets an array of structs of <paramref name="size"/> bytes each, from their bytes, one after
    /// another, which it counts. It's padded so that the structs start at a multiple of
    /// <paramref name="alignment"/> from the start of the table, which is the size of the structs'
    /// largest scalar.
    /// </summary>
    /// <exception cref="BuildException">
    /// The bytes aren't a whole number of structs, or as <see cref="SetInt8"/> throws it.
    /// </exception>
    /// <exception cref="ArgumentOutOfRangeException">
    /// <paramref name="size"/> is less than 1, or <paramref name="alignment"/> isn't 1, 2, 4 or 8.
    /// </exception>
    public void SetStructArray(ReadOnlySpan<byte> bytes, int size, int alignment)
    {
        ArgumentOutOfRangeException.ThrowIfNegativeOrZero(size);
        TableLayout.CheckAlignment(alignment);
        if (bytes.Length % size != 0)
        {
            throw BuildException.NotWholeStructs(bytes.Length, size);
        }

        bytes.CopyTo(AddCounted(bytes.Length / size, size, alignment));
    }

    /// <summary>
    /// Nests the table that <paramref name="table"/> wrote, without a length prefix, padded to start
    /// at a multiple of the largest alignment of its arrays and of its own nested tables, or 1 if it
    /// has neither, so that its arrays are aligned from its start. The table is copied, so the
    /// builder may be nested again, or elsewhere.
    /// </summary>
    /// <remarks>
    /// Readers find its fields with <see cref="ByteBuffer.FieldOffset"/> from the field's position,
    /// which reads offsets of the parent's offset type, so the table should have no header and its
    /// parent's offset type: that's the default layout in a parent of the default layout.
    /// </remarks>
    /// <exception cref="BuildException">
    /// The table has fields left to set, or as <see cref="SetInt8"/> throws it.
    /// </exception>
    /// <exception cref="ArgumentNullException"><paramref name="table"/> is null.</exception>
    public void SetTable(TableBuilder table)
    {
        ArgumentNullException.ThrowIfNull(table);
        AddTable(table.WrittenBytes(), table._alignment);
    }

    /// <summary>
    /// Nests the table that <paramref name="table"/> wrote, as <see cref="SetTable(TableBuilder)"/>
    /// does, padded to start at a multiple of <paramref name="alignment"/> instead.
    /// </summary>
    /// <exception cref="BuildException">
    /// The table has fields left to set, or as <see cref="SetInt8"/> throws it.
    /// </exception>
    /// <exception cref="ArgumentNullException"><paramref name="table"/> is null.</exception>
    /// <exception cref="ArgumentOutOfRangeException"><paramref name="alignment"/> isn't 1, 2, 4 or 8.</exception>
    public void SetTable(TableBuilder table, int alignment)
    {
        ArgumentNullException.ThrowIfNull(table);
        TableLayout.CheckAlignment(alignment);
        AddTable(table.WrittenBytes(), alignment);
    }

    /// <summary>
    /// Nests a table from its bytes, as a builder wrote them, without a length prefix, padded to start
    /// at a multiple of <paramref name="alignment"/>, which is 4 by default, as
    /// <see cref="SetTable(TableBuilder)"/> nests a builder. A table of 64-bit arrays needs 8.
    /// </summary>
    /// <exception cref="BuildException">As <see cref="SetInt8"/> throws it.</exception>
    /// <exception cref="ArgumentOutOfRangeException"><paramref name="alignment"/> isn't 1, 2, 4 or 8.</exception>
    public void SetTable(ReadOnlySpan<byte> table, int alignment = DefaultTableAlignment)
    {
        TableLayout.CheckAlignment(alignment);
        AddTable(table, alignment);
    }

    /// <summary>
    /// Sets a vector of tables, from their builders or their bytes, after the end of each, which lets
    /// readers find any item without reading the others. Builders must have all their fields set, and
    /// are copied as <see cref="SetTable(TableBuilder)"/> copies them. Items aren't padded, so their
    /// arrays are read in place only where they happen to be aligned.
    /// </summary>
    /// <exception cref="BuildException">
    /// A builder has fields left to set, or as <see cref="SetInt8"/> throws it.
    /// </exception>
    public void SetVector(params ReadOnlySpan<NestedTable> items)
    {
        int width = _layout.OffsetSize;
        long itemsStart = (items.Length + 1L) * width;

        // Every item is at most as long as an array, so their sum fits in a long
        long length = itemsStart;
        foreach (var item in items)
        {
            length += item.Bytes.Length;
        }

        var vector = AddField(0, length, 1);
        WriteSlot(vector, (uint)items.Length);

        long end = itemsStart;
        for (var index = 0; index < items.Length; index++)
        {
            var item = items[index].Bytes;
            item.CopyTo(vector[(int)end..]);
            end += item.Length;
            WriteSlot(vector[((index + 1) * width)..], (uint)end);
        }
    }

    /// <summary>The table's bytes, once every field is set, which its parent copies when it nests it</summary>
    /// <exception cref="BuildException">Fewer fields were set than the table holds.</exception>
    internal ReadOnlySpan<byte> WrittenBytes()
    {
        if (_fieldsSet != _fields)
        {
            throw BuildException.Incomplete(_fieldsSet, _fields);
        }

        return _bytes.AsSpan(0, _length);
    }

    private void SetArray<T>(ReadOnlySpan<T> values)
        where T : unmanaged
    {
        int size = Unsafe.SizeOf<T>();
        Elements.Write(values, AddCounted(values.Length, size, size));
    }

    private void AddTable(ReadOnlySpan<byte> table, int alignment) =>
        table.CopyTo(AddField(TableLayout.Padding(_length, alignment), table.Length, alignment));

    /// <summary>
    /// Adds a field of a count of <paramref name="count"/> elements of <paramref name="size"/> bytes,
    /// padded so that the elements start at a multiple of <paramref name="alignment"/>, writes the
    /// count, and returns the elements' bytes
    /// </summary>
    private Span<byte> AddCounted(int count, int size, int alignment)
    {
        int width = _layout.OffsetSize;
        var field = AddField(
            TableLayout.Padding((long)_length + width, alignment),
            width + (long)count * size,
            alignment
        );

        // The field fits in the table, so its count does too
        WriteSlot(field, (uint)count);
        return field[width..];
    }

    /// <summary>
    /// Adds the next field, of <paramref name="byteLength"/> bytes after <paramref name="padding"/>,
    /// whose elements need <paramref name="alignment"/> from the table's start, and writes its offset.
    /// Every check comes before any write, so a field that's rejected leaves the builder as it was.
    /// Every offset, count, length and item end is at most the table's length, so that's the one
    /// length to check.
    /// </summary>
    /// <returns>The field's bytes, which are zero, for the setter to write.</returns>
    private Span<byte> AddField(int padding, long byteLength, int alignment)
    {
        if (_fieldsSet == _fields)
        {
            throw BuildException.TooManyFields(_fields);
        }

        long start = (long)_length + padding;
        long end = start + byteLength;
        if (end > _maxLength)
        {
            throw BuildException.TooLong(_fieldsSet, end, _layout.OffsetType);
        }

        Reserve((int)end);
        WriteSlot(_bytes.AsSpan(_layout.HeaderSize + _fieldsSet * _layout.OffsetSize), (uint)start);
        _length = (int)end;
        _alignment = Math.Max(_alignment, alignment);

        _fieldsSet++;
        if (_fieldsSet == _fields)
        {
            Complete();
        }

        return _bytes.AsSpan((int)start, (int)byteLength);
    }

    /// <summary>Grows the bytes to hold at least <paramref name="length"/>, which is at most the longest table</summary>
    private void Reserve(int length)
    {
        if (length <= _bytes.Length)
        {
            return;
        }

        long doubled = 2L * _bytes.Length;
        Array.Resize(ref _bytes, (int)Math.Min(Math.Max(doubled, length), Array.MaxLength));
    }

    /// <summary>Writes the table's length into its header once every field is set, when it's sized</summary>
    private void Complete()
    {
        if (_layout.Sized)
        {
            WriteSlot(_bytes, (uint)_length);
        }
    }

    /// <summary>Writes <paramref name="value"/> at the start of <paramref name="destination"/>, as wide as the offset size.</summary>
    private void WriteSlot(Span<byte> destination, uint value)
    {
        switch (_layout.OffsetType)
        {
            case OffsetType.UInt8:
                destination[0] = (byte)value;
                break;

            case OffsetType.UInt16:
                BinaryPrimitives.WriteUInt16LittleEndian(destination, (ushort)value);
                break;

            default:
                BinaryPrimitives.WriteUInt32LittleEndian(destination, value);
                break;
        }
    }

    /// <summary>
    /// Counts the UTF-8 bytes of <paramref name="value"/>, as <see cref="Encoding.UTF8"/> encodes it,
    /// in parts that an <see cref="int"/> counts, so that a string too long for any table is rejected
    /// as too long rather than overflowing the count. A surrogate pair stays within one part.
    /// <see cref="TableSizer"/> counts strings with it too.
    /// </summary>
    internal static long Utf8Length(ReadOnlySpan<char> value)
    {
        long length = 0;
        while (value.Length > Utf8CountedChars)
        {
            int split = char.IsHighSurrogate(value[Utf8CountedChars - 1])
                ? Utf8CountedChars - 1
                : Utf8CountedChars;

            length += Encoding.UTF8.GetByteCount(value[..split]);
            value = value[split..];
        }

        return length + Encoding.UTF8.GetByteCount(value);
    }
}
