using System;

namespace Tafla;

/// <summary>
/// Counts the bytes of a table, so that a <see cref="TableWriter"/> can write it into new bytes of
/// exactly that length, or into the start of bytes that the caller reuses. It's for code that knows a
/// table's fields before it writes them, such as the code that tafla-gen generates in the other
/// languages, which writes nested tables and vector items in place rather than copying them, as
/// <see cref="TableBuilder"/> does.
/// </summary>
/// <remarks>
/// <para>
/// The fields are added in order, each with its type's method, which takes what decides its length:
/// nothing for a scalar, the count of an array, the string itself. A nested table is added with
/// <see cref="AddTable"/>, and a vector with <see cref="AddVector"/>, and the fields of each such table
/// then follow between <see cref="StartTable"/> and <see cref="EndTable"/>. The sizer pads arrays and
/// nested tables as <see cref="TableBuilder"/> does, so a writer that takes the same fields in the
/// same order writes each where the sizer counted it, and the bytes that a builder of them writes.
/// </para>
/// <para>
/// The length is counted in a <see cref="long"/>, and checked once all the fields are added, by
/// <see cref="GetByteLength"/> and <see cref="CreateWriter()"/>, against the longest table that the
/// layout's offset type addresses: 255 bytes, 65,535 bytes, or for <see cref="OffsetType.UInt32"/>, as
/// long as a .NET array can be, <see cref="Array.MaxLength"/> bytes.
/// </para>
/// <para>
/// A sizer is a mutable struct, so that sizing a table allocates nothing. Use it as a local, or pass
/// it by <c>ref</c>: a copy counts on its own. Its default value sizes a table of no fields and no
/// header, of 0 bytes.
/// </para>
/// </remarks>
/// <example>
/// <code>
/// var sizer = new TableSizer(2);
/// sizer.AddFloat64();
/// sizer.AddString("Europe/Stockholm");
///
/// var writer = sizer.CreateWriter();
/// writer.SetFloat64(0, 59.3293);
/// writer.SetString(1, "Europe/Stockholm");
/// byte[] bytes = writer.Bytes;
/// </code>
/// </example>
public struct TableSizer
{
    private readonly TableLayout _layout;
    private readonly int _fields;
    private readonly uint _version;

    /// <summary>The table's length so far, which is where the next field goes</summary>
    private long _position;

    /// <summary>The start of the table being sized, which its arrays are aligned from</summary>
    private long _table;

    /// <summary>
    /// Sizes a table of <paramref name="fields"/> fields in <paramref name="layout"/>, with the checks
    /// of <see cref="TableBuilder(int, TableLayout)"/>.
    /// </summary>
    /// <exception cref="BuildException">The header alone is too long for the layout's offset type.</exception>
    /// <exception cref="ArgumentOutOfRangeException"><paramref name="fields"/> is negative.</exception>
    /// <exception cref="ArgumentException">
    /// The layout is versioned, which the constructor that takes a version sizes.
    /// </exception>
    public TableSizer(int fields, TableLayout layout = default)
        : this(fields, layout, versioned: false, version: 0) { }

    /// <summary>
    /// Sizes a table of <paramref name="fields"/> fields in <paramref name="layout"/>, whose writer
    /// writes <paramref name="version"/> in its header, with the checks of
    /// <see cref="TableBuilder(int, TableLayout, uint)"/>.
    /// </summary>
    /// <exception cref="BuildException">
    /// The version doesn't fit in the layout's offset type, or the header alone is too long for it.
    /// </exception>
    /// <exception cref="ArgumentOutOfRangeException"><paramref name="fields"/> is negative.</exception>
    /// <exception cref="ArgumentException">The layout isn't versioned.</exception>
    public TableSizer(int fields, TableLayout layout, uint version)
        : this(fields, layout, versioned: true, version) { }

    private TableSizer(int fields, TableLayout layout, bool versioned, uint version)
    {
        _position = TableLayout.HeaderLength(fields, layout, versioned, version);
        _layout = layout;
        _fields = fields;
        _version = version;
    }

    /// <summary>The layout that the table is sized in.</summary>
    public readonly TableLayout Layout => _layout;

    /// <summary>Returns the byte length of the table, with every field added.</summary>
    /// <exception cref="BuildException">
    /// The table is too long for its offset type. Its <see cref="BuildException.Length"/> is the
    /// table's length, and its <see cref="BuildException.Index"/> the table's last field, since the
    /// sizer checks the length once every field is added.
    /// </exception>
    public readonly int GetByteLength()
    {
        if (_position > _layout.MaxLength)
        {
            throw BuildException.TooLong(Math.Max(_fields - 1, 0), _position, _layout.OffsetType);
        }

        return (int)_position;
    }

    /// <summary>
    /// Returns a writer of the table into new bytes of its length, which takes its fields in the
    /// order that they were added. Its <see cref="TableWriter.Bytes"/> returns them.
    /// </summary>
    /// <exception cref="BuildException">The table is too long for its offset type, before anything is allocated.</exception>
    public readonly TableWriter CreateWriter()
    {
        var length = GetByteLength();
        return new TableWriter(new byte[length], length, _fields, _layout, _version);
    }

    /// <summary>
    /// Returns a writer of the table into the start of <paramref name="bytes"/>, rather than into new
    /// bytes, which takes its fields in the order that they were added. Its
    /// <see cref="TableWriter.ByteLength"/> is the table's length, the number of bytes that hold it. It
    /// writes every byte of the table, padding too, so the bytes needn't be zero, and leaves those
    /// after it as they were: fields that differ from those added throw rather than write past the
    /// table.
    /// </summary>
    /// <remarks>
    /// The caller owns the bytes, and the writer keeps them only until the table is written. The table
    /// stays in them until they're written again, such as by the next table written into them, which
    /// a <see cref="TableReader"/> of them, and the spans that it reads in place, then read too. Copy
    /// the table's bytes to keep it. Reusing bytes saves allocating and zeroing new bytes for each
    /// table, and collecting them.
    /// </remarks>
    /// <exception cref="BuildException">The table is too long for its offset type, before any byte is written.</exception>
    /// <exception cref="ArgumentNullException"><paramref name="bytes"/> is null.</exception>
    /// <exception cref="ArgumentException">
    /// <paramref name="bytes"/> are shorter than the table, before any of them is written.
    /// </exception>
    public readonly TableWriter CreateWriter(byte[] bytes)
    {
        ArgumentNullException.ThrowIfNull(bytes);

        var length = GetByteLength();
        if (bytes.Length < length)
        {
            throw new ArgumentException(
                $"The table is {length} bytes, longer than the {bytes.Length} bytes given",
                nameof(bytes)
            );
        }

        return new TableWriter(bytes, length, _fields, _layout, _version);
    }

    /// <summary>
    /// Starts the fields of a nested table or vector item of <paramref name="fields"/> fields, after
    /// its parent's <see cref="AddTable"/> or <see cref="AddVector"/>.
    /// </summary>
    /// <returns>The parent table, which <see cref="EndTable"/> takes back.</returns>
    /// <exception cref="ArgumentOutOfRangeException"><paramref name="fields"/> is negative.</exception>
    public SizerParent StartTable(int fields)
    {
        ArgumentOutOfRangeException.ThrowIfNegative(fields);

        var parent = new SizerParent(_table);
        _table = _position;
        _position += (long)fields * _layout.OffsetSize;

        return parent;
    }

    /// <summary>Ends the fields of the table that <see cref="StartTable"/> started.</summary>
    public void EndTable(SizerParent parent) => _table = parent.Table;

    /// <summary>Adds an <c>int8</c> field.</summary>
    public void AddInt8() => _position += sizeof(sbyte);

    /// <summary>Adds a <c>uint8</c> field, which is also how a <c>bool</c> is written.</summary>
    public void AddUInt8() => _position += sizeof(byte);

    /// <summary>Adds an <c>int16</c> field.</summary>
    public void AddInt16() => _position += sizeof(short);

    /// <summary>Adds a <c>uint16</c> field.</summary>
    public void AddUInt16() => _position += sizeof(ushort);

    /// <summary>Adds an <c>int32</c> field.</summary>
    public void AddInt32() => _position += sizeof(int);

    /// <summary>Adds a <c>uint32</c> field.</summary>
    public void AddUInt32() => _position += sizeof(uint);

    /// <summary>Adds an <c>int64</c> field.</summary>
    public void AddInt64() => _position += sizeof(long);

    /// <summary>Adds a <c>uint64</c> field.</summary>
    public void AddUInt64() => _position += sizeof(ulong);

    /// <summary>Adds a <c>float32</c> field.</summary>
    public void AddFloat32() => _position += sizeof(float);

    /// <summary>Adds a <c>float64</c> field.</summary>
    public void AddFloat64() => _position += sizeof(double);

    // The array adders take the count of the elements, and pad them as TableBuilder pads them

    /// <summary>Adds an array of <paramref name="count"/> <c>int8</c>s.</summary>
    /// <exception cref="ArgumentOutOfRangeException"><paramref name="count"/> is negative.</exception>
    public void AddInt8Array(int count) => AddArray(count, sizeof(sbyte), sizeof(sbyte));

    /// <summary>Adds an array of <paramref name="count"/> <c>uint8</c>s.</summary>
    /// <exception cref="ArgumentOutOfRangeException"><paramref name="count"/> is negative.</exception>
    public void AddUInt8Array(int count) => AddArray(count, sizeof(byte), sizeof(byte));

    /// <summary>Adds an array of <paramref name="count"/> <c>int16</c>s.</summary>
    /// <exception cref="ArgumentOutOfRangeException"><paramref name="count"/> is negative.</exception>
    public void AddInt16Array(int count) => AddArray(count, sizeof(short), sizeof(short));

    /// <summary>Adds an array of <paramref name="count"/> <c>uint16</c>s.</summary>
    /// <exception cref="ArgumentOutOfRangeException"><paramref name="count"/> is negative.</exception>
    public void AddUInt16Array(int count) => AddArray(count, sizeof(ushort), sizeof(ushort));

    /// <summary>Adds an array of <paramref name="count"/> <c>int32</c>s.</summary>
    /// <exception cref="ArgumentOutOfRangeException"><paramref name="count"/> is negative.</exception>
    public void AddInt32Array(int count) => AddArray(count, sizeof(int), sizeof(int));

    /// <summary>Adds an array of <paramref name="count"/> <c>uint32</c>s.</summary>
    /// <exception cref="ArgumentOutOfRangeException"><paramref name="count"/> is negative.</exception>
    public void AddUInt32Array(int count) => AddArray(count, sizeof(uint), sizeof(uint));

    /// <summary>Adds an array of <paramref name="count"/> <c>int64</c>s.</summary>
    /// <exception cref="ArgumentOutOfRangeException"><paramref name="count"/> is negative.</exception>
    public void AddInt64Array(int count) => AddArray(count, sizeof(long), sizeof(long));

    /// <summary>Adds an array of <paramref name="count"/> <c>uint64</c>s.</summary>
    /// <exception cref="ArgumentOutOfRangeException"><paramref name="count"/> is negative.</exception>
    public void AddUInt64Array(int count) => AddArray(count, sizeof(ulong), sizeof(ulong));

    /// <summary>Adds an array of <paramref name="count"/> <c>float32</c>s.</summary>
    /// <exception cref="ArgumentOutOfRangeException"><paramref name="count"/> is negative.</exception>
    public void AddFloat32Array(int count) => AddArray(count, sizeof(float), sizeof(float));

    /// <summary>Adds an array of <paramref name="count"/> <c>float64</c>s.</summary>
    /// <exception cref="ArgumentOutOfRangeException"><paramref name="count"/> is negative.</exception>
    public void AddFloat64Array(int count) => AddArray(count, sizeof(double), sizeof(double));

    /// <summary>
    /// Adds a string field, of the string's UTF-8, as <see cref="TableBuilder.SetString"/> counts it,
    /// with each lone surrogate as U+FFFD.
    /// </summary>
    /// <exception cref="ArgumentNullException"><paramref name="value"/> is null.</exception>
    public void AddString(string value)
    {
        ArgumentNullException.ThrowIfNull(value);
        _position += _layout.OffsetSize + TableBuilder.Utf8Length(value);
    }

    /// <summary>
    /// Returns the byte length of a string's UTF-8, as <see cref="AddString"/> counts it and
    /// <see cref="TableWriter.SetString"/> writes it, with each lone surrogate as U+FFFD. It's counted
    /// in a <see cref="long"/>, so that a string too long for any table counts past the longest, rather
    /// than overflowing, for code that lays out a table itself, such as tafla-gen's, to reject it.
    /// </summary>
    /// <exception cref="ArgumentNullException"><paramref name="value"/> is null.</exception>
    public static long Utf8Length(string value)
    {
        ArgumentNullException.ThrowIfNull(value);
        return TableBuilder.Utf8Length(value);
    }

    /// <summary>Adds a struct field of <paramref name="size"/> bytes, which isn't padded.</summary>
    /// <exception cref="ArgumentOutOfRangeException"><paramref name="size"/> is negative.</exception>
    public void AddStruct(int size)
    {
        ArgumentOutOfRangeException.ThrowIfNegative(size);
        _position += size;
    }

    /// <summary>
    /// Adds an array of <paramref name="count"/> structs of <paramref name="size"/> bytes each, padded
    /// so that they start at a multiple of <paramref name="alignment"/> from the start of the table,
    /// as <see cref="TableBuilder.SetStructArray"/> pads them.
    /// </summary>
    /// <exception cref="ArgumentOutOfRangeException">
    /// <paramref name="count"/> is negative, <paramref name="size"/> is less than 1, or
    /// <paramref name="alignment"/> isn't 1, 2, 4 or 8.
    /// </exception>
    public void AddStructArray(int count, int size, int alignment)
    {
        ArgumentOutOfRangeException.ThrowIfNegativeOrZero(size);
        TableLayout.CheckAlignment(alignment);
        AddArray(count, size, alignment);
    }

    /// <summary>
    /// Adds a nested table, padded to start at a multiple of <paramref name="alignment"/>: the largest
    /// alignment of its arrays and of its own nested tables, or 1 if it has neither, as
    /// <see cref="TableBuilder.SetTable(TableBuilder)"/> pads a builder. Its fields then follow,
    /// between <see cref="StartTable"/> and <see cref="EndTable"/>.
    /// </summary>
    /// <exception cref="ArgumentOutOfRangeException"><paramref name="alignment"/> isn't 1, 2, 4 or 8.</exception>
    public void AddTable(int alignment)
    {
        TableLayout.CheckAlignment(alignment);
        _position += TableLayout.Padding(_position - _table, alignment);
    }

    /// <summary>
    /// Adds a vector of <paramref name="count"/> tables: its count and the end of each item. The items'
    /// fields then follow, each item's between <see cref="StartTable"/> and <see cref="EndTable"/>.
    /// Items aren't padded, as <see cref="TableBuilder.SetVector(ReadOnlySpan{NestedTable})"/> doesn't pad them.
    /// </summary>
    /// <exception cref="ArgumentOutOfRangeException"><paramref name="count"/> is negative.</exception>
    public void AddVector(int count)
    {
        ArgumentOutOfRangeException.ThrowIfNegative(count);
        _position += (count + 1L) * _layout.OffsetSize;
    }

    /// <summary>
    /// Adds <paramref name="count"/> nested tables of <paramref name="size"/> bytes each, whose fields'
    /// sizes their schema fixes, which their writer lays out itself and writes whole into the bytes
    /// that <see cref="TableWriter.ReserveTableBytes"/> returns: a field's table, after
    /// <see cref="AddTable"/>, or a vector's items of the same length, after <see cref="AddVector"/>. It
    /// takes the place of each table's <see cref="StartTable"/>, adders and <see cref="EndTable"/>.
    /// </summary>
    /// <remarks>
    /// It's for the code that tafla-gen generates, as Go's <c>AddTableBytes()</c> and Rust's
    /// <c>add_table_bytes()</c> are, which writes a vector of such tables in a third of the time that
    /// writing each field takes.
    /// </remarks>
    /// <exception cref="ArgumentOutOfRangeException"><paramref name="count"/> or <paramref name="size"/> is negative.</exception>
    public void AddTableBytes(int count, int size)
    {
        ArgumentOutOfRangeException.ThrowIfNegative(count);
        ArgumentOutOfRangeException.ThrowIfNegative(size);
        _position += (long)count * size;
    }

    /// <summary>
    /// Adds a count and then <paramref name="count"/> elements of <paramref name="size"/> bytes, padded
    /// so that the elements start at a multiple of <paramref name="alignment"/> from the table's start
    /// </summary>
    private void AddArray(int count, int size, int alignment)
    {
        ArgumentOutOfRangeException.ThrowIfNegative(count);

        long start = _position + _layout.OffsetSize;
        _position = start + TableLayout.Padding(start - _table, alignment) + (long)count * size;
    }
}

/// <summary>The table that a nested table or vector item was started in, which <see cref="TableSizer.EndTable"/> takes back.</summary>
public readonly struct SizerParent
{
    internal SizerParent(long table)
    {
        Table = table;
    }

    /// <summary>The parent's start</summary>
    internal long Table { get; }
}
