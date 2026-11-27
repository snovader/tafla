using System;
using System.Buffers.Binary;
using System.Diagnostics.CodeAnalysis;
using System.Runtime.CompilerServices;
using System.Text;

namespace Tafla;

/// <summary>
/// Writes a table forward, field by field, into new bytes of exactly its length, which a
/// <see cref="TableSizer"/> counted, or into the start of bytes that the caller gives
/// <see cref="TableSizer.CreateWriter(byte[])"/>. It comes from the sizer, and takes the fields that
/// the sizer added, in the same order, each with its index in its table.
/// </summary>
/// <remarks>
/// <para>
/// Each field's offset is written as the writer reaches the field. A nested table follows its
/// parent's <see cref="SetTable"/>, between <see cref="StartTable"/> and <see cref="EndTable"/>; the
/// items of a vector follow its <see cref="SetVector"/> the same way, each followed by
/// <see cref="SetItemEnd"/>, which writes where it ends. Arrays and nested tables are padded as the
/// sizer and <see cref="TableBuilder"/> pad them, so that each field lands where they counted it, and
/// the table's bytes are those that a builder of the same fields writes. The writer writes every
/// byte of the table, its padding's zeros too, so the bytes that it's given needn't be zero. Values
/// convert to their field's type as <see cref="TableBuilder"/>'s do.
/// </para>
/// <para>
/// It's the target of code whose calls follow from a schema, such as the code that tafla-gen
/// generates, so it doesn't check them again: fields are set in index order, each once, as the sizer
/// added them, and each vector item is followed by its <see cref="SetItemEnd"/>. A field index
/// outside its table, or an item outside its vector, writes an offset or end over another slot or
/// field, so the table's bytes are undefined, though never outside the writer's bytes. Hand-written
/// tables use <see cref="TableBuilder"/>, which checks every field.
/// </para>
/// <para>
/// Fields that differ from those that the sizer added, in their lengths or number, such as values
/// that changed in between, never write past the table, even in bytes longer than it: a field that
/// would end past it throws a <see cref="BuildException"/> of kind
/// <see cref="BuildErrorKind.NotAsSized"/>, after which the writer mustn't be used, and
/// <see cref="Bytes"/> and <see cref="ByteLength"/> throw the same for fields that didn't fill it.
/// </para>
/// <para>
/// A writer is a mutable struct, so that writing a table allocates only its bytes. Use it as a local,
/// or pass it by <c>ref</c>: a copy writes on its own.
/// </para>
/// </remarks>
public struct TableWriter
{
    private readonly byte[]? _bytes;

    /// <summary>The table's length, from the start of the bytes</summary>
    private readonly int _length;

    private readonly OffsetType _offsetType;

    /// <summary>The length of the root's header slots, which its offsets follow</summary>
    private readonly int _headerSize;

    /// <summary>Where the next field goes</summary>
    private int _position;

    /// <summary>The start of the table being written, which its offsets count from: 0 for the root</summary>
    private int _table;

    /// <summary>The position of that table's first offset</summary>
    private int _slots;

    /// <summary>
    /// Writes the header of a table of <paramref name="fields"/> fields and <paramref name="length"/>
    /// bytes into the start of <paramref name="bytes"/>, which are at least that long, as the sizer
    /// checked them and the header
    /// </summary>
    internal TableWriter(byte[] bytes, int length, int fields, TableLayout layout)
    {
        _bytes = bytes;
        _length = length;
        _offsetType = layout.OffsetType;
        _headerSize = layout.HeaderSize;
        _slots = _headerSize;
        _position = _headerSize + fields * layout.OffsetSize;

        WriteHeader(bytes, length, layout);
    }

    /// <summary>
    /// Returns new bytes for a table of <paramref name="fields"/> fields in <paramref name="layout"/>,
    /// of <paramref name="length"/> bytes, with its header written, the layout's version included when
    /// it has one, for the caller to write every other byte itself, each field's offset and the field,
    /// at positions that it works out, without a sizer and a writer.
    /// </summary>
    /// <remarks>
    /// It's for the code that tafla-gen generates, as Rust's <c>table_bytes()</c> and Go's
    /// <c>AppendTable()</c> are, which writes a message this way when its fields' sizes are all fixed
    /// but for a string after them, so that only the string can make it too long, in two thirds of the
    /// time that a sizer and a writer take.
    /// </remarks>
    /// <exception cref="BuildException">
    /// The layout's version doesn't fit in its offset type or the header alone is too long for it, as
    /// <see cref="TableSizer"/> rejects them, or the table is, which names its last field, before
    /// anything is allocated.
    /// </exception>
    /// <exception cref="ArgumentOutOfRangeException">
    /// <paramref name="fields"/> is negative, or <paramref name="length"/> is shorter than the header
    /// and the offsets.
    /// </exception>
    public static byte[] CreateTableBytes(int fields, TableLayout layout, long length)
    {
        var header = TableLayout.HeaderLength(fields, layout);
        ArgumentOutOfRangeException.ThrowIfLessThan(length, header);
        if (length > layout.MaxLength)
        {
            throw BuildException.TooLong(Math.Max(fields - 1, 0), length, layout.OffsetType);
        }

        var bytes = new byte[length];
        WriteHeader(bytes, (int)length, layout);

        return bytes;
    }

    /// <summary>Writes the header of a table of <paramref name="length"/> bytes in <paramref name="layout"/>: its length when it's sized, and then its version when it's versioned</summary>
    private static void WriteHeader(byte[] bytes, int length, TableLayout layout)
    {
        var slot = 0;
        if (layout.Sized)
        {
            WriteSlot(bytes, slot, (uint)length, layout.OffsetType);
            slot += layout.OffsetSize;
        }

        if (layout.Version is { } version)
        {
            WriteSlot(bytes, slot, version, layout.OffsetType);
        }
    }

    /// <summary>
    /// The bytes that the table is written into, rather than a copy: the new bytes of
    /// <see cref="TableSizer.CreateWriter()"/>, which hold exactly the table, or those given to
    /// <see cref="TableSizer.CreateWriter(byte[])"/>, whose first <see cref="ByteLength"/> bytes hold it.
    /// </summary>
    /// <exception cref="BuildException">
    /// The fields didn't fill the table, as when they differ from those that the sizer counted.
    /// </exception>
    public readonly byte[] Bytes
    {
        get
        {
            CheckFilled();
            return _bytes ?? [];
        }
    }

    /// <summary>The table's length, the number of bytes from the start of <see cref="Bytes"/> that hold it.</summary>
    /// <exception cref="BuildException">
    /// The fields didn't fill the table, as when they differ from those that the sizer counted.
    /// </exception>
    public readonly int ByteLength
    {
        get
        {
            CheckFilled();
            return _length;
        }
    }

    /// <summary>The offset size, the width of offsets, counts, item ends and header slots</summary>
    private readonly int Width => _offsetType.Size;

    /// <summary>
    /// Starts the fields of a nested table or vector item of <paramref name="fields"/> fields, after
    /// its parent's <see cref="SetTable"/> or <see cref="SetVector"/>.
    /// </summary>
    /// <returns>The parent table, which <see cref="EndTable"/> takes back.</returns>
    /// <exception cref="BuildException">The nested table's offsets end past the table.</exception>
    /// <exception cref="ArgumentOutOfRangeException"><paramref name="fields"/> is negative.</exception>
    public WriterParent StartTable(int fields)
    {
        ArgumentOutOfRangeException.ThrowIfNegative(fields);

        var parent = new WriterParent(_table);
        var start = _position;
        Reserve(0, (long)fields * Width);
        _table = start;
        _slots = start;

        return parent;
    }

    /// <summary>Ends the fields of the table that <see cref="StartTable"/> started.</summary>
    public void EndTable(WriterParent parent)
    {
        // Only the root starts at 0, and only its offsets follow a header
        _table = parent.Table;
        _slots = parent.Table == 0 ? _headerSize : parent.Table;
    }

    // Scalars aren't aligned, so they need no padding

    /// <summary>Writes field <paramref name="index"/>, an <c>int8</c>.</summary>
    /// <exception cref="BuildException">The field ends past the table.</exception>
    public void SetInt8(int index, sbyte value) => StartField(index, 0, sizeof(sbyte))[0] = (byte)value;

    /// <summary>Writes field <paramref name="index"/>, a <c>uint8</c>, which is also how a <c>bool</c> is written, as 0 or 1.</summary>
    /// <exception cref="BuildException">The field ends past the table.</exception>
    public void SetUInt8(int index, byte value) => StartField(index, 0, sizeof(byte))[0] = value;

    /// <summary>Writes field <paramref name="index"/>, an <c>int16</c>.</summary>
    /// <exception cref="BuildException">The field ends past the table.</exception>
    public void SetInt16(int index, short value) =>
        BinaryPrimitives.WriteInt16LittleEndian(StartField(index, 0, sizeof(short)), value);

    /// <summary>Writes field <paramref name="index"/>, a <c>uint16</c>.</summary>
    /// <exception cref="BuildException">The field ends past the table.</exception>
    public void SetUInt16(int index, ushort value) =>
        BinaryPrimitives.WriteUInt16LittleEndian(StartField(index, 0, sizeof(ushort)), value);

    /// <summary>Writes field <paramref name="index"/>, an <c>int32</c>.</summary>
    /// <exception cref="BuildException">The field ends past the table.</exception>
    public void SetInt32(int index, int value) =>
        BinaryPrimitives.WriteInt32LittleEndian(StartField(index, 0, sizeof(int)), value);

    /// <summary>Writes field <paramref name="index"/>, a <c>uint32</c>.</summary>
    /// <exception cref="BuildException">The field ends past the table.</exception>
    public void SetUInt32(int index, uint value) =>
        BinaryPrimitives.WriteUInt32LittleEndian(StartField(index, 0, sizeof(uint)), value);

    /// <summary>Writes field <paramref name="index"/>, an <c>int64</c>.</summary>
    /// <exception cref="BuildException">The field ends past the table.</exception>
    public void SetInt64(int index, long value) =>
        BinaryPrimitives.WriteInt64LittleEndian(StartField(index, 0, sizeof(long)), value);

    /// <summary>Writes field <paramref name="index"/>, a <c>uint64</c>.</summary>
    /// <exception cref="BuildException">The field ends past the table.</exception>
    public void SetUInt64(int index, ulong value) =>
        BinaryPrimitives.WriteUInt64LittleEndian(StartField(index, 0, sizeof(ulong)), value);

    /// <summary>Writes field <paramref name="index"/>, a <c>float32</c>, with all its bits, a NaN's payload too.</summary>
    /// <exception cref="BuildException">The field ends past the table.</exception>
    public void SetFloat32(int index, float value) =>
        BinaryPrimitives.WriteSingleLittleEndian(StartField(index, 0, sizeof(float)), value);

    /// <summary>Writes field <paramref name="index"/>, a <c>float64</c>, with all its bits, a NaN's payload too.</summary>
    /// <exception cref="BuildException">The field ends past the table.</exception>
    public void SetFloat64(int index, double value) =>
        BinaryPrimitives.WriteDoubleLittleEndian(StartField(index, 0, sizeof(double)), value);

    // The array setters write the count and then the elements, padded so that the elements start at
    // a multiple of their size from the start of the table, and copied in bulk

    /// <summary>Writes field <paramref name="index"/>, an array of <c>int8</c>s.</summary>
    /// <exception cref="BuildException">The field ends past the table.</exception>
    public void SetInt8Array(int index, ReadOnlySpan<sbyte> values) => SetArray(index, values);

    /// <summary>Writes field <paramref name="index"/>, an array of <c>uint8</c>s.</summary>
    /// <exception cref="BuildException">The field ends past the table.</exception>
    public void SetUInt8Array(int index, ReadOnlySpan<byte> values) => SetArray(index, values);

    /// <summary>Writes field <paramref name="index"/>, an array of <c>int16</c>s.</summary>
    /// <exception cref="BuildException">The field ends past the table.</exception>
    public void SetInt16Array(int index, ReadOnlySpan<short> values) => SetArray(index, values);

    /// <summary>Writes field <paramref name="index"/>, an array of <c>uint16</c>s.</summary>
    /// <exception cref="BuildException">The field ends past the table.</exception>
    public void SetUInt16Array(int index, ReadOnlySpan<ushort> values) => SetArray(index, values);

    /// <summary>Writes field <paramref name="index"/>, an array of <c>int32</c>s.</summary>
    /// <exception cref="BuildException">The field ends past the table.</exception>
    public void SetInt32Array(int index, ReadOnlySpan<int> values) => SetArray(index, values);

    /// <summary>Writes field <paramref name="index"/>, an array of <c>uint32</c>s.</summary>
    /// <exception cref="BuildException">The field ends past the table.</exception>
    public void SetUInt32Array(int index, ReadOnlySpan<uint> values) => SetArray(index, values);

    /// <summary>Writes field <paramref name="index"/>, an array of <c>int64</c>s.</summary>
    /// <exception cref="BuildException">The field ends past the table.</exception>
    public void SetInt64Array(int index, ReadOnlySpan<long> values) => SetArray(index, values);

    /// <summary>Writes field <paramref name="index"/>, an array of <c>uint64</c>s.</summary>
    /// <exception cref="BuildException">The field ends past the table.</exception>
    public void SetUInt64Array(int index, ReadOnlySpan<ulong> values) => SetArray(index, values);

    /// <summary>Writes field <paramref name="index"/>, an array of <c>float32</c>s, with all their bits.</summary>
    /// <exception cref="BuildException">The field ends past the table.</exception>
    public void SetFloat32Array(int index, ReadOnlySpan<float> values) => SetArray(index, values);

    /// <summary>Writes field <paramref name="index"/>, an array of <c>float64</c>s, with all their bits.</summary>
    /// <exception cref="BuildException">The field ends past the table.</exception>
    public void SetFloat64Array(int index, ReadOnlySpan<double> values) => SetArray(index, values);

    /// <summary>
    /// Writes field <paramref name="index"/>, a string, of the string's UTF-8, encoded straight into
    /// the table, with each lone surrogate as U+FFFD, as <see cref="TableBuilder.SetString"/> writes it.
    /// </summary>
    /// <exception cref="ArgumentNullException"><paramref name="value"/> is null.</exception>
    /// <exception cref="BuildException">The field ends past the table.</exception>
    public void SetString(int index, string value)
    {
        ArgumentNullException.ThrowIfNull(value);

        var width = Width;
        var start = _position;

        // The string is encoded into the rest of the table, rather than of the bytes, and its count, as
        // well as its bytes, may end past it. Compared unsigned, as the slice compares it, so that the
        // JIT drops the slice's check.
        var table = _bytes.AsSpan(0, _length);
        if ((uint)(start + width) > (uint)table.Length)
        {
            ThrowNotAsSized(start, _length);
        }

        if (!Encoding.UTF8.TryGetBytes(value, table[(start + width)..], out var count))
        {
            ThrowNotAsSized(start, _length);
        }

        WriteOffset(index, start);
        WriteSlot(start, (uint)count);
        _position = start + width + count;
    }

    /// <summary>
    /// Writes field <paramref name="index"/> as absent: an offset of 0 and no value, which a reader finds
    /// by comparing the field's offset, from <see cref="TableReader.RootOffset"/>, with 0, or one from
    /// <see cref="TableReader.FieldOffset"/> with the nested table's position, before it reads the
    /// field. The sizer adds nothing for it. Only a format whose readers make that check for the field,
    /// such as code generated from a schema that declares it optional, can leave it out.
    /// </summary>
    /// <remarks>It writes the 0, so the bytes that the writer was given needn't be zero there.</remarks>
    /// <exception cref="ArgumentOutOfRangeException">The field's offset lies outside the bytes.</exception>
    public readonly void SetAbsent(int index) => WriteSlot(_slots + index * Width, 0);

    /// <summary>
    /// Writes field <paramref name="index"/>, a struct, from its bytes: its scalars, little-endian, one
    /// after another, which are written as they are, without padding.
    /// </summary>
    /// <exception cref="BuildException">The field ends past the table.</exception>
    public void SetStruct(int index, ReadOnlySpan<byte> bytes) => bytes.CopyTo(StartField(index, 0, bytes.Length));

    /// <summary>
    /// Returns the <paramref name="size"/> bytes of field <paramref name="index"/>, a struct, for the
    /// caller to write every one of them: its scalars, little-endian, one after another.
    /// </summary>
    /// <remarks>The bytes aren't zero in bytes that the writer was given, or that it allocated without zeroing.</remarks>
    /// <exception cref="BuildException">The field ends past the table.</exception>
    /// <exception cref="ArgumentOutOfRangeException"><paramref name="size"/> is negative.</exception>
    public Span<byte> ReserveStruct(int index, int size)
    {
        ArgumentOutOfRangeException.ThrowIfNegative(size);
        return StartField(index, 0, size);
    }

    /// <summary>
    /// Writes field <paramref name="index"/>, an array of structs of <paramref name="size"/> bytes
    /// each, from their bytes, one after another, after their count, padded so that they start at a
    /// multiple of <paramref name="alignment"/> from the start of the table, as
    /// <see cref="TableBuilder.SetStructArray"/> writes them.
    /// </summary>
    /// <exception cref="BuildException">
    /// The bytes aren't a whole number of structs, or the field ends past the table.
    /// </exception>
    /// <exception cref="ArgumentOutOfRangeException">
    /// <paramref name="size"/> is less than 1, or <paramref name="alignment"/> isn't 1, 2, 4 or 8.
    /// </exception>
    public void SetStructArray(int index, ReadOnlySpan<byte> bytes, int size, int alignment)
    {
        ArgumentOutOfRangeException.ThrowIfNegativeOrZero(size);
        if (bytes.Length % size != 0)
        {
            throw BuildException.NotWholeStructs(bytes.Length, size);
        }

        bytes.CopyTo(ReserveStructArray(index, bytes.Length / size, size, alignment));
    }

    /// <summary>
    /// Returns the bytes of field <paramref name="index"/>, an array of <paramref name="count"/>
    /// structs of <paramref name="size"/> bytes each, after writing their count, padded so that they
    /// start at a multiple of <paramref name="alignment"/> from the start of the table, for the caller
    /// to write every byte of each struct, one after another, as <see cref="SetStructArray"/> copies
    /// them, without collecting them first.
    /// </summary>
    /// <remarks>The bytes aren't zero in bytes that the writer was given, or that it allocated without zeroing.</remarks>
    /// <exception cref="BuildException">The field ends past the table.</exception>
    /// <exception cref="ArgumentOutOfRangeException">
    /// <paramref name="count"/> is negative, <paramref name="size"/> is less than 1, or
    /// <paramref name="alignment"/> isn't 1, 2, 4 or 8.
    /// </exception>
    public Span<byte> ReserveStructArray(int index, int count, int size, int alignment)
    {
        ArgumentOutOfRangeException.ThrowIfNegative(count);
        ArgumentOutOfRangeException.ThrowIfNegativeOrZero(size);
        TableLayout.CheckAlignment(alignment);

        return StartCounted(index, count, size, alignment);
    }

    /// <summary>
    /// Writes the offset of field <paramref name="index"/>, a nested table, padded to start at a
    /// multiple of <paramref name="alignment"/>, as the sizer's <see cref="TableSizer.AddTable"/> padded
    /// it. Its fields then follow between <see cref="StartTable"/> and <see cref="EndTable"/>.
    /// </summary>
    /// <exception cref="BuildException">The padding ends past the table.</exception>
    /// <exception cref="ArgumentOutOfRangeException"><paramref name="alignment"/> isn't 1, 2, 4 or 8.</exception>
    public void SetTable(int index, int alignment)
    {
        TableLayout.CheckAlignment(alignment);
        StartField(index, TableLayout.Padding(_position - _table, alignment), 0);
    }

    /// <summary>
    /// Writes the offset and count of field <paramref name="index"/>, a vector of
    /// <paramref name="count"/> tables, whose items then follow, each between <see cref="StartTable"/>
    /// and <see cref="EndTable"/>, and then <see cref="SetItemEnd"/>.
    /// </summary>
    /// <returns>The vector's position, which <see cref="SetItemEnd"/> takes.</returns>
    /// <exception cref="BuildException">The vector's count and ends end past the table.</exception>
    /// <exception cref="ArgumentOutOfRangeException"><paramref name="count"/> is negative.</exception>
    public VectorPosition SetVector(int index, int count)
    {
        ArgumentOutOfRangeException.ThrowIfNegative(count);

        var position = _position;
        StartField(index, 0, (count + 1L) * Width);
        WriteSlot(position, (uint)count);

        return new VectorPosition(position);
    }

    /// <summary>
    /// Returns the <paramref name="size"/> bytes of a nested table whose fields' sizes its schema fixes,
    /// as <see cref="TableSizer.AddTableBytes"/> added it, for the caller to write every one of them:
    /// its offsets, from its own start, and its fields, little-endian. It's a field's table, after
    /// <see cref="SetTable"/>, or a vector's item, before its <see cref="SetItemEnd"/>, and takes the
    /// place of the table's <see cref="StartTable"/>, setters and <see cref="EndTable"/>.
    /// </summary>
    /// <remarks>
    /// It's for the code that tafla-gen generates, as Go's <c>ReserveTableBytes()</c> is. The bytes
    /// aren't zero in bytes that the writer was given, or that it allocated without zeroing.
    /// </remarks>
    /// <exception cref="BuildException">The table ends past the table being written.</exception>
    /// <exception cref="ArgumentOutOfRangeException"><paramref name="size"/> is negative.</exception>
    public Span<byte> ReserveTableBytes(int size)
    {
        ArgumentOutOfRangeException.ThrowIfNegative(size);
        return Reserve(0, size);
    }

    /// <summary>Writes the end of item <paramref name="item"/> of <paramref name="vector"/>, which was just written.</summary>
    /// <exception cref="ArgumentOutOfRangeException">The item's end lies outside the bytes.</exception>
    public readonly void SetItemEnd(VectorPosition vector, int item) =>
        WriteSlot(vector.Position + (item + 1) * Width, (uint)(_position - vector.Position));

    private void SetArray<T>(int index, ReadOnlySpan<T> values)
        where T : unmanaged
    {
        int size = Unsafe.SizeOf<T>();
        Elements.Write(values, StartCounted(index, values.Length, size, size));
    }

    /// <summary>
    /// Starts a field of a count of <paramref name="count"/> elements of <paramref name="size"/> bytes,
    /// padded so that the elements start at a multiple of <paramref name="alignment"/> from the table's
    /// start, writes the count, and returns the elements' bytes
    /// </summary>
    private Span<byte> StartCounted(int index, int count, int size, int alignment)
    {
        var width = Width;
        var padding = TableLayout.Padding((long)_position + width - _table, alignment);
        var start = _position + padding;
        var field = StartField(index, padding, width + (long)count * size);

        WriteSlot(start, (uint)count);
        return field[width..];
    }

    /// <summary>
    /// Writes <paramref name="padding"/> zeros and the offset of field <paramref name="index"/> after
    /// them, moves past the field's <paramref name="byteLength"/> bytes, and returns them
    /// </summary>
    /// <exception cref="BuildException">The field ends past the table, before anything is written.</exception>
    private Span<byte> StartField(int index, int padding, long byteLength)
    {
        var start = _position + padding;
        var field = Reserve(padding, byteLength);
        WriteOffset(index, start);

        return field;
    }

    /// <summary>
    /// Writes <paramref name="padding"/> zeros, moves past the <paramref name="byteLength"/> bytes after
    /// them, and returns those bytes
    /// </summary>
    /// <exception cref="BuildException">They end past the table, before anything is written.</exception>
    private Span<byte> Reserve(int padding, long byteLength)
    {
        var start = (long)_position + padding;
        if (start + byteLength > _length)
        {
            ThrowNotAsSized(_position, _length);
        }

        if (padding != 0)
        {
            _bytes.AsSpan(_position, padding).Clear();
        }

        _position = (int)(start + byteLength);
        return _bytes.AsSpan((int)start, (int)byteLength);
    }

    /// <summary>Writes the offset of field <paramref name="index"/> of the table being written, to <paramref name="position"/></summary>
    private readonly void WriteOffset(int index, int position) =>
        WriteSlot(_slots + index * Width, (uint)(position - _table));

    /// <summary>Writes an offset, count, item end or header slot at <paramref name="position"/>, as wide as the offset size</summary>
    private readonly void WriteSlot(int position, uint value) => WriteSlot(_bytes, position, value, _offsetType);

    /// <summary>Writes a slot of <paramref name="offsetType"/> at <paramref name="position"/> in <paramref name="bytes"/></summary>
    private static void WriteSlot(byte[]? bytes, int position, uint value, OffsetType offsetType)
    {
        switch (offsetType)
        {
            case OffsetType.UInt8:
                bytes.AsSpan(position, sizeof(byte))[0] = (byte)value;
                break;

            case OffsetType.UInt16:
                BinaryPrimitives.WriteUInt16LittleEndian(bytes.AsSpan(position, sizeof(ushort)), (ushort)value);
                break;

            default:
                BinaryPrimitives.WriteUInt32LittleEndian(bytes.AsSpan(position, sizeof(uint)), value);
                break;
        }
    }

    private readonly void CheckFilled()
    {
        if (_position != _length)
        {
            ThrowNotAsSized(_position, _length);
        }
    }

    /// <summary>
    /// Throws the error of fields that differ from those sized, which wrote <paramref name="written"/>
    /// of the table's <paramref name="length"/> bytes, in all or before one that would end past the
    /// table
    /// </summary>
    /// <remarks>Static, and given the values, so that a writer kept in registers needn't be stored for it</remarks>
    [DoesNotReturn]
    private static void ThrowNotAsSized(int written, int length) => throw BuildException.NotAsSized(written, length);
}

/// <summary>The table that a nested table or vector item was started in, which <see cref="TableWriter.EndTable"/> takes back.</summary>
public readonly struct WriterParent
{
    internal WriterParent(int table)
    {
        Table = table;
    }

    /// <summary>The parent's start, which its offsets count from</summary>
    internal int Table { get; }
}

/// <summary>The position of a vector that <see cref="TableWriter.SetVector"/> wrote, which <see cref="TableWriter.SetItemEnd"/> takes.</summary>
public readonly struct VectorPosition
{
    internal VectorPosition(int position)
    {
        Position = position;
    }

    /// <summary>The vector's position, which its items' ends count from</summary>
    internal int Position { get; }
}
