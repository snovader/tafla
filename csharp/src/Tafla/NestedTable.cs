using System;

namespace Tafla;

/// <summary>
/// A table to nest in a vector with <see cref="TableBuilder.SetVector"/>: a complete
/// <see cref="TableBuilder"/>, or the bytes of a table, as its builder wrote them. It converts
/// implicitly from either, so that a vector can mix them.
/// </summary>
/// <remarks>
/// The default value is a table of no bytes, which a table of no fields and no header is.
/// </remarks>
public readonly struct NestedTable
{
    private readonly TableBuilder? _builder;
    private readonly ReadOnlyMemory<byte> _bytes;

    private NestedTable(TableBuilder? builder, ReadOnlyMemory<byte> bytes)
    {
        _builder = builder;
        _bytes = bytes;
    }

    /// <summary>Returns the table that <paramref name="builder"/> writes, which must have all its fields set when it's nested.</summary>
    /// <exception cref="ArgumentNullException"><paramref name="builder"/> is null.</exception>
    public static NestedTable FromBuilder(TableBuilder builder)
    {
        ArgumentNullException.ThrowIfNull(builder);
        return new(builder, default);
    }

    /// <summary>Returns the table of <paramref name="bytes"/>, as a builder wrote them.</summary>
    public static NestedTable FromBytes(ReadOnlyMemory<byte> bytes) => new(null, bytes);

    /// <summary>Returns the table that <paramref name="builder"/> writes, as <see cref="FromBuilder"/> does.</summary>
    /// <exception cref="ArgumentNullException"><paramref name="builder"/> is null.</exception>
    public static implicit operator NestedTable(TableBuilder builder) => FromBuilder(builder);

    /// <summary>Returns the table of <paramref name="bytes"/>, as <see cref="FromBytes"/> does.</summary>
    /// <exception cref="ArgumentNullException"><paramref name="bytes"/> is null.</exception>
    public static implicit operator NestedTable(byte[] bytes)
    {
        ArgumentNullException.ThrowIfNull(bytes);
        return FromBytes(bytes);
    }

    /// <summary>Returns the table of <paramref name="bytes"/>, as <see cref="FromBytes"/> does.</summary>
    public static implicit operator NestedTable(ReadOnlyMemory<byte> bytes) => FromBytes(bytes);

    /// <summary>The table's bytes, which a builder must have written all its fields of</summary>
    /// <exception cref="BuildException">The builder has fields left to set.</exception>
    internal ReadOnlySpan<byte> Bytes => _builder is null ? _bytes.Span : _builder.WrittenBytes();
}
