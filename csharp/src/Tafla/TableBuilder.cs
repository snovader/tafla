using System;
using System.Buffers.Binary;
using System.Runtime.CompilerServices;
using System.Text;

namespace Tafla;

/// <summary>
/// Builds a table, which a <see cref="TableReader"/> reads back: a header with one offset per field,
/// after the table's byte length and version when its layout has them, followed by the fields in the
/// order they're set. Arrays, strings and vectors start with their count, and a vector, of tables or
/// of strings, then holds the end of each item. Arrays of scalars and of structs are padded so that
/// their elements are aligned from the start of the table. Fields are read back by index with
/// <see cref="TableReader.RootOffset"/>, or <see cref="TableReader.FieldOffset"/> for a table nested
/// in another.
/// </summary>
/// <remarks>
/// <para>
/// Each setter writes its field into the builder's bytes, which grow as fields are set, so a value
/// may change once it's set, and a nested builder too. <see cref="ToArray"/> copies the table, at its
/// exact length, once every field is set.
/// </para>
/// <para>
/// <see cref="SetTable(int, Action{Item})"/> and <see cref="SetVector(int, int, Action{Item, int})"/>
/// record a nested table, or each item of a vector, in place: straight among this table's own bytes,
/// through the <see cref="Item"/> that their fill sets its fields on, rather than from a builder of
/// its own, which is then copied. They write the bytes that nesting builders of the same fields
/// writes.
/// </para>
/// <para>
/// A setter throws a <see cref="BuildException"/>, and leaves the builder as it was, when the table
/// already holds all its fields, or when its field would make the table longer than the layout's
/// offset type addresses: 255 bytes, 65,535 bytes, or for <see cref="OffsetType.UInt32"/>, as long as
/// a .NET array can be, <see cref="Array.MaxLength"/> bytes. A table recorded in place lies within
/// the builder's bytes, after the tables that it's in, so for <see cref="OffsetType.UInt32"/> its
/// field also throws when it would make those bytes longer than that, with their length.
/// </para>
/// <para>
/// Each setter takes its field's own C# type, such as <see cref="sbyte"/> for an <c>int8</c> and
/// <see cref="float"/> for a <c>float32</c>, so the caller converts a value to it, as C# converts.
/// </para>
/// </remarks>
public sealed partial class TableBuilder
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

    /// <summary>
    /// The bytes so far, of this table and of the tables that it records in place, and the zeroes
    /// after them, which padding leaves
    /// </summary>
    private byte[] _bytes;

    /// <summary>The length of the bytes so far, which is where the next field goes</summary>
    private int _length;

    /// <summary>
    /// The counts of the table whose fields are being set: this one's, or those of a table that it
    /// records in place
    /// </summary>
    private Counters _table;

    /// <summary>
    /// Starts a table of <paramref name="fields"/> fields in <paramref name="layout"/>, which its
    /// readers must use too, with the layout's version in its header when it has one, which readers
    /// can check with <see cref="TableReader.GetVersion"/> before reading any field. Nested tables use
    /// the default layout.
    /// </summary>
    /// <exception cref="BuildException">
    /// The layout's version doesn't fit in its offset type, or the header alone is too long for it.
    /// </exception>
    /// <exception cref="ArgumentOutOfRangeException"><paramref name="fields"/> is negative.</exception>
    public TableBuilder(int fields, TableLayout layout = default)
    {
        int header = TableLayout.HeaderLength(fields, layout);

        _layout = layout;
        _length = header;
        _bytes = new byte[Math.Min((long)header + InitialCapacity, Array.MaxLength)];
        _table = new Counters(start: 0, offsets: layout.HeaderSize, fields, limit: layout.MaxLength);

        if (layout.Version is { } version)
        {
            WriteSlot(_bytes.AsSpan(layout.Sized ? layout.OffsetSize : 0), version);
        }
    }

    /// <summary>The layout that the table is written in.</summary>
    public TableLayout Layout => _layout;

    /// <summary>Copies the table, at its exact length, into a new array.</summary>
    /// <exception cref="BuildException">Fewer fields were set than the table holds.</exception>
    /// <exception cref="InvalidOperationException">A table or vector is being recorded in place in it.</exception>
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
    /// Sets a vector of strings: its count, the end of each string, counted from the vector's start,
    /// and then each string's UTF-8, one after another, which lets readers find any string without
    /// reading the others. A lone surrogate is written as U+FFFD, as <see cref="SetString"/> writes
    /// it. It takes no more bytes for each string than its end, where a vector of tables of one string
    /// each takes each table's offset and the string's count too.
    /// </summary>
    /// <exception cref="BuildException">As <see cref="SetInt8"/> throws it.</exception>
    /// <exception cref="ArgumentNullException">A string is null.</exception>
    public void SetStringVector(params ReadOnlySpan<string> values)
    {
        int width = _layout.OffsetSize;
        long stringsStart = (values.Length + 1L) * width;
        var vector = AddField(0, stringsStart + Utf8Length(values), 1);

        // The field fits in the table, so its count and ends do too
        WriteSlot(vector, (uint)values.Length);
        int end = (int)stringsStart;
        for (var index = 0; index < values.Length; index++)
        {
            end += Encoding.UTF8.GetBytes(values[index], vector[end..]);
            WriteSlot(vector[((index + 1) * width)..], (uint)end);
        }
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
    /// Readers find its fields with <see cref="TableReader.FieldOffset"/> from the field's position,
    /// which reads offsets of the parent's offset type, so the table should have no header and its
    /// parent's offset type: that's the default layout in a parent of the default layout.
    /// </remarks>
    /// <exception cref="BuildException">
    /// The table has fields left to set, or as <see cref="SetInt8"/> throws it.
    /// </exception>
    /// <exception cref="ArgumentNullException"><paramref name="table"/> is null.</exception>
    /// <exception cref="InvalidOperationException">A table or vector is being recorded in place in <paramref name="table"/>.</exception>
    public void SetTable(TableBuilder table)
    {
        ArgumentNullException.ThrowIfNull(table);
        AddTable(table.WrittenBytes(), table._table.Alignment);
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
    /// <exception cref="InvalidOperationException">A table or vector is being recorded in place in <paramref name="table"/>.</exception>
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
    /// Records a table of <paramref name="fields"/> fields nested in this one in place, as
    /// <see cref="SetTable(TableBuilder)"/> nests a builder of them without a header and of this
    /// table's offset type, but without a builder of its own: <paramref name="fill"/> sets its fields,
    /// in order, on the <see cref="Item"/> that it's given, which writes each straight among this
    /// table's bytes. It's then padded as <see cref="SetTable(TableBuilder)"/> pads the builder, to
    /// start at a multiple of the largest alignment of its arrays and its own nested tables, so the
    /// bytes are those that nesting the builder writes.
    /// </summary>
    /// <remarks>
    /// <para>
    /// The item counts the table's fields and its length as a builder of it would, so its setters
    /// throw as the builder's would, and a table nested in it can be recorded in place too. The
    /// table's padding is known once its last field is set, so a table that needs it is moved by
    /// those bytes once <paramref name="fill"/> returns.
    /// </para>
    /// <para>
    /// While <paramref name="fill"/> runs, the builder records the nested table, so it's used only
    /// through the item, and <see cref="ToArray"/> and nesting it throw
    /// <see cref="InvalidOperationException"/>. Whatever <paramref name="fill"/> throws is passed
    /// on, and whatever is thrown, the builder is left as it was.
    /// </para>
    /// <para>
    /// A lambda that captures variables allocates each time that it's created, which
    /// <see cref="SetTable{TState}(int, TState, Action{Item, TState})"/> avoids by passing them to a
    /// <see langword="static"/> lambda instead.
    /// </para>
    /// </remarks>
    /// <exception cref="BuildException">
    /// This table holds all its fields already (<see cref="BuildErrorKind.TooManyFields"/>), the nested
    /// table's offsets alone are too long for the offset type
    /// (<see cref="BuildErrorKind.HeaderTooLong"/>), <paramref name="fill"/> leaves fields of the
    /// nested table unset (<see cref="BuildErrorKind.Incomplete"/>), the nested table makes this one
    /// too long (<see cref="BuildErrorKind.TooLong"/>), or as the item's setters throw it.
    /// </exception>
    /// <exception cref="ArgumentOutOfRangeException"><paramref name="fields"/> is negative.</exception>
    /// <exception cref="ArgumentNullException"><paramref name="fill"/> is null.</exception>
    public void SetTable(int fields, Action<Item> fill)
    {
        ArgumentNullException.ThrowIfNull(fill);

        int start = _length;
        var parent = StartTable(fields);
        try
        {
            fill(new Item(this));
            EndTable(parent, start);
        }
        catch
        {
            RollBack(parent, start);
            throw;
        }
    }

    /// <summary>
    /// Records a table nested in this one in place, as <see cref="SetTable(int, Action{Item})"/>
    /// does, with <paramref name="state"/> passed to <paramref name="fill"/>, so that a
    /// <see langword="static"/> lambda sets its fields from it without allocating.
    /// </summary>
    /// <exception cref="BuildException">As <see cref="SetTable(int, Action{Item})"/> throws it.</exception>
    /// <exception cref="ArgumentOutOfRangeException"><paramref name="fields"/> is negative.</exception>
    /// <exception cref="ArgumentNullException"><paramref name="fill"/> is null.</exception>
    public void SetTable<TState>(int fields, TState state, Action<Item, TState> fill)
        where TState : allows ref struct
    {
        ArgumentNullException.ThrowIfNull(fill);

        int start = _length;
        var parent = StartTable(fields);
        try
        {
            fill(new Item(this), state);
            EndTable(parent, start);
        }
        catch
        {
            RollBack(parent, start);
            throw;
        }
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
    /// <exception cref="InvalidOperationException">A table or vector is being recorded in place in a builder.</exception>
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

    /// <summary>
    /// Records a vector of <paramref name="count"/> tables of <paramref name="fields"/> fields each
    /// in place, as <see cref="SetTable(int, Action{Item})"/> records one: <paramref name="fill"/>
    /// sets the fields of each item, in order, on the <see cref="Item"/> that it's given with the
    /// item's index. The bytes are those that <see cref="SetVector(ReadOnlySpan{NestedTable})"/>
    /// writes from builders of the items. Items aren't padded, as it doesn't pad them, so they're
    /// never moved.
    /// </summary>
    /// <remarks>
    /// The vector's count and the ends of its items, and the offsets that every item holds whatever
    /// its fields, are checked before any item is recorded, so a vector that they alone make too
    /// long throws without calling <paramref name="fill"/>. The rest of the vector is checked once
    /// its items are recorded. Whatever <paramref name="fill"/> throws is passed on, and whatever is
    /// thrown, the builder is left as it was.
    /// </remarks>
    /// <exception cref="BuildException">
    /// As <see cref="SetTable(int, Action{Item})"/> throws it, for the vector or any item.
    /// </exception>
    /// <exception cref="ArgumentOutOfRangeException">
    /// <paramref name="count"/> or <paramref name="fields"/> is negative.
    /// </exception>
    /// <exception cref="ArgumentNullException"><paramref name="fill"/> is null.</exception>
    public void SetVector(int count, int fields, Action<Item, int> fill)
    {
        ArgumentNullException.ThrowIfNull(fill);

        int start = _length;
        var parent = StartVector(count, fields);
        try
        {
            for (var index = 0; index < count; index++)
            {
                StartItem(parent, fields);
                fill(new Item(this), index);
                EndItem(start, index);
            }

            EndVector(parent, start);
        }
        catch
        {
            RollBack(parent, start);
            throw;
        }
    }

    /// <summary>
    /// Records a vector of tables in place, as <see cref="SetVector(int, int, Action{Item, int})"/>
    /// does, with <paramref name="state"/> passed to <paramref name="fill"/>, so that a
    /// <see langword="static"/> lambda sets each item's fields from it without allocating.
    /// </summary>
    /// <exception cref="BuildException">
    /// As <see cref="SetVector(int, int, Action{Item, int})"/> throws it.
    /// </exception>
    /// <exception cref="ArgumentOutOfRangeException">
    /// <paramref name="count"/> or <paramref name="fields"/> is negative.
    /// </exception>
    /// <exception cref="ArgumentNullException"><paramref name="fill"/> is null.</exception>
    public void SetVector<TState>(int count, int fields, TState state, Action<Item, int, TState> fill)
        where TState : allows ref struct
    {
        ArgumentNullException.ThrowIfNull(fill);

        int start = _length;
        var parent = StartVector(count, fields);
        try
        {
            for (var index = 0; index < count; index++)
            {
                StartItem(parent, fields);
                fill(new Item(this), index, state);
                EndItem(start, index);
            }

            EndVector(parent, start);
        }
        catch
        {
            RollBack(parent, start);
            throw;
        }
    }

    /// <summary>
    /// The table's bytes, once every field is set, with its length in its header when it's sized,
    /// which its parent copies when it nests it
    /// </summary>
    /// <exception cref="BuildException">Fewer fields were set than the table holds.</exception>
    /// <exception cref="InvalidOperationException">A table or vector is being recorded in place in it.</exception>
    internal ReadOnlySpan<byte> WrittenBytes()
    {
        if (RecordingInPlace)
        {
            throw new InvalidOperationException(
                "A table can't be copied while a table is recorded in place in it"
            );
        }

        if (_table.FieldsSet != _table.Fields)
        {
            throw BuildException.Incomplete(_table.FieldsSet, _table.Fields);
        }

        if (_layout.Sized)
        {
            WriteSlot(_bytes, (uint)_length);
        }

        return _bytes.AsSpan(0, _length);
    }

    /// <summary>
    /// Whether a table or vector is being recorded in place, whose fields the builder counts rather
    /// than its own. Only its own table starts at 0: a table recorded in place starts after the
    /// offset of the field that it is, or the count of its vector.
    /// </summary>
    private bool RecordingInPlace => _table.Start != 0;

    private void SetArray<T>(ReadOnlySpan<T> values)
        where T : unmanaged
    {
        int size = Unsafe.SizeOf<T>();
        Elements.Write(values, AddCounted(values.Length, size, size));
    }

    private void AddTable(ReadOnlySpan<byte> table, int alignment) =>
        table.CopyTo(
            AddField(TableLayout.Padding(_length - _table.Start, alignment), table.Length, alignment)
        );

    /// <summary>
    /// Adds a field of a count of <paramref name="count"/> elements of <paramref name="size"/> bytes,
    /// padded so that the elements start at a multiple of <paramref name="alignment"/>, writes the
    /// count, and returns the elements' bytes
    /// </summary>
    private Span<byte> AddCounted(int count, int size, int alignment)
    {
        int width = _layout.OffsetSize;
        var field = AddField(
            TableLayout.Padding((long)_length - _table.Start + width, alignment),
            width + (long)count * size,
            alignment
        );

        // The field fits in the table, so its count does too
        WriteSlot(field, (uint)count);
        return field[width..];
    }

    /// <summary>
    /// Adds the next field of the table being set, of <paramref name="byteLength"/> bytes after
    /// <paramref name="padding"/>, whose elements need <paramref name="alignment"/> from the table's
    /// start, and writes its offset. Every check comes before any write, so a field that's rejected
    /// leaves the builder as it was. Every offset, count, length and item end is at most the table's
    /// length, so that's the one length to check.
    /// </summary>
    /// <returns>The field's bytes, which are zero, for the setter to write.</returns>
    private Span<byte> AddField(int padding, long byteLength, int alignment)
    {
        if (_table.FieldsSet == _table.Fields)
        {
            throw BuildException.TooManyFields(_table.Fields);
        }

        long start = (long)_length + padding;
        long end = start + byteLength;
        if (end > _table.Limit)
        {
            throw TooLong(in _table, end);
        }

        Reserve((int)end);
        WriteSlot(
            _bytes.AsSpan(_table.Offsets + _table.FieldsSet * _layout.OffsetSize),
            (uint)(start - _table.Start)
        );
        _length = (int)end;
        _table.Alignment = Math.Max(_table.Alignment, alignment);
        _table.FieldsSet++;

        return _bytes.AsSpan((int)start, (int)byteLength);
    }

    /// <summary>
    /// Starts recording a table of <paramref name="fields"/> fields in place, as the next field of
    /// the table being set, after its offsets, whose bytes are zero
    /// </summary>
    /// <returns>The counts of the table being set, which the table is a field of.</returns>
    private Counters StartTable(int fields)
    {
        ArgumentOutOfRangeException.ThrowIfNegative(fields);
        if (_table.FieldsSet == _table.Fields)
        {
            throw BuildException.TooManyFields(_table.Fields);
        }

        int offsets = NestedOffsets(fields);
        var parent = _table;

        StartCounting(in parent, fields, offsets);

        return parent;
    }

    /// <summary>
    /// Ends the table that started at <paramref name="start"/>, once its fill returned: pads it as a
    /// nested builder is padded, from <paramref name="parent"/>'s length, moving it, and sets it as
    /// <paramref name="parent"/>'s next field
    /// </summary>
    private void EndTable(in Counters parent, int start)
    {
        CheckSet();

        int alignment = _table.Alignment;
        int length = _length - start;
        int padding = TableLayout.Padding(start - parent.Start, alignment);
        long end = (long)_length + padding;
        if (end > parent.Limit)
        {
            throw TooLong(in parent, end);
        }

        if (padding != 0)
        {
            Reserve((int)end);
            _bytes.AsSpan(start, length).CopyTo(_bytes.AsSpan(start + padding));
            _bytes.AsSpan(start, padding).Clear();
        }

        _table = parent;
        WriteOffset(in _table, start + padding);
        _length = (int)end;
        _table.Alignment = Math.Max(_table.Alignment, alignment);
        _table.FieldsSet++;
    }

    /// <summary>
    /// Starts recording a vector of <paramref name="count"/> tables of <paramref name="fields"/>
    /// fields in place, as the next field of the table being set: writes its count, after which its
    /// item ends follow, which are zero until each item ends
    /// </summary>
    /// <returns>The counts of the table being set, which the vector is a field of.</returns>
    private Counters StartVector(int count, int fields)
    {
        ArgumentOutOfRangeException.ThrowIfNegative(count);
        ArgumentOutOfRangeException.ThrowIfNegative(fields);
        if (_table.FieldsSet == _table.Fields)
        {
            throw BuildException.TooManyFields(_table.Fields);
        }

        // The count and the ends, and every item's offsets, which fail before any item is recorded,
        // rather than once each item is
        int offsets = NestedOffsets(fields);
        long items = (long)_length + (count + 1L) * _layout.OffsetSize;
        long fewest = items + (long)count * offsets;
        if (fewest > _table.Limit)
        {
            throw TooLong(in _table, fewest);
        }

        Reserve((int)items);
        WriteSlot(_bytes.AsSpan(_length), (uint)count);
        _length = (int)items;

        return _table;
    }

    /// <summary>Starts recording the next item of the vector, a table of <paramref name="fields"/> fields</summary>
    private void StartItem(in Counters parent, int fields) =>
        StartCounting(in parent, fields, fields * _layout.OffsetSize);

    /// <summary>
    /// Ends item <paramref name="index"/> of the vector that started at <paramref name="vector"/>,
    /// once its fill returned, by writing where it ends
    /// </summary>
    private void EndItem(int vector, int index)
    {
        CheckSet();
        WriteSlot(_bytes.AsSpan(vector + (index + 1) * _layout.OffsetSize), (uint)(_length - vector));
    }

    /// <summary>Ends the vector that started at <paramref name="vector"/>, once its items are recorded, as <paramref name="parent"/>'s next field</summary>
    private void EndVector(in Counters parent, int vector)
    {
        if (_length > parent.Limit)
        {
            throw TooLong(in parent, _length);
        }

        _table = parent;
        WriteOffset(in _table, vector);
        _table.FieldsSet++;
    }

    /// <summary>
    /// Leaves the builder as it was before the table or vector that started at
    /// <paramref name="start"/>, as a field of <paramref name="parent"/>, was recorded: counts
    /// <paramref name="parent"/>'s fields again, and zeroes the bytes after it
    /// </summary>
    private void RollBack(in Counters parent, int start)
    {
        _bytes.AsSpan(start, _length - start).Clear();
        _length = start;
        _table = parent;
    }

    /// <summary>
    /// Returns the length of the offsets of a table of <paramref name="fields"/> fields recorded in
    /// place, which has no header and this table's offset type
    /// </summary>
    /// <exception cref="BuildException">The offsets are too long for the offset type.</exception>
    /// <exception cref="ArgumentOutOfRangeException"><paramref name="fields"/> is negative.</exception>
    private int NestedOffsets(int fields) =>
        TableLayout.HeaderLength(fields, new TableLayout { OffsetType = _layout.OffsetType });

    /// <summary>
    /// Counts the fields of a table of <paramref name="fields"/> fields recorded in place, from the
    /// end of the bytes, after its <paramref name="offsets"/> bytes of offsets, rather than those of
    /// <paramref name="parent"/>, which it's in
    /// </summary>
    private void StartCounting(in Counters parent, int fields, int offsets)
    {
        int start = _length;
        long end = (long)start + offsets;

        // Only the bytes so far, of the tables that it's in, can make it longer than an array
        if (end > Array.MaxLength)
        {
            throw TooLong(in parent, end);
        }

        Reserve((int)end);
        _length = (int)end;
        _table = new Counters(start, offsets: start, fields, (int)Math.Min(start + (long)_layout.MaxLength, Array.MaxLength));
    }

    /// <summary>Checks that every field of the table recorded in place is set, once its fill returned.</summary>
    /// <exception cref="BuildException">Fewer fields were set than the table holds.</exception>
    private void CheckSet()
    {
        if (_table.FieldsSet != _table.Fields)
        {
            throw BuildException.Incomplete(_table.FieldsSet, _table.Fields);
        }
    }

    /// <summary>
    /// Returns the error of <paramref name="table"/>'s next field, which would end at
    /// <paramref name="end"/> in the bytes, past its limit: the length that it would make the table,
    /// or, for a table recorded in place within the longest bytes that an array holds, which a
    /// shorter table can pass, the length of the bytes
    /// </summary>
    private BuildException TooLong(in Counters table, long end)
    {
        long length = end - table.Start;
        return BuildException.TooLong(table.FieldsSet, length > _layout.MaxLength ? length : end, _layout.OffsetType);
    }

    /// <summary>Writes the offset of <paramref name="table"/>'s next field, which starts at <paramref name="start"/> in the bytes</summary>
    private void WriteOffset(in Counters table, int start) =>
        WriteSlot(
            _bytes.AsSpan(table.Offsets + table.FieldsSet * _layout.OffsetSize),
            (uint)(start - table.Start)
        );

    /// <summary>Grows the bytes to hold at least <paramref name="length"/>, which is at most the longest array</summary>
    private void Reserve(int length)
    {
        if (length <= _bytes.Length)
        {
            return;
        }

        long doubled = 2L * _bytes.Length;
        Array.Resize(ref _bytes, (int)Math.Min(Math.Max(doubled, length), Array.MaxLength));
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

    /// <summary>
    /// Counts the UTF-8 bytes of every string of a vector of strings, each as
    /// <see cref="Utf8Length(ReadOnlySpan{char})"/> counts it, after checking that it isn't null.
    /// <see cref="TableSizer"/> counts them with it too.
    /// </summary>
    /// <exception cref="ArgumentNullException">A string is null.</exception>
    internal static long Utf8Length(ReadOnlySpan<string> values)
    {
        // At most 2^31 - 1 strings of at most 3 bytes a char, which a long holds
        long length = 0;
        foreach (var value in values)
        {
            ArgumentNullException.ThrowIfNull(value, nameof(values));
            length += Utf8Length(value);
        }

        return length;
    }

    /// <summary>
    /// What the builder counts of the table whose fields it's setting: its own, or a table that it
    /// records in place, which counts its fields and its length from its own start, as a builder of
    /// it would
    /// </summary>
    private struct Counters(int start, int offsets, int fields, int limit)
    {
        /// <summary>Where the table starts in the bytes, which its offsets and padding count from</summary>
        public readonly int Start = start;

        /// <summary>Where the table's field offsets start in the bytes, after its header</summary>
        public readonly int Offsets = offsets;

        public readonly int Fields = fields;

        /// <summary>
        /// Where the table's fields must end in the bytes: its start and the longest table that the
        /// layout addresses, or the longest array, which a table recorded in place can reach first
        /// </summary>
        public readonly int Limit = limit;

        public int FieldsSet;

        /// <summary>
        /// The largest alignment that a field needs from the table's start, which a parent pads the
        /// table to when it nests it
        /// </summary>
        public int Alignment = 1;
    }
}
