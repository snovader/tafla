using System;

namespace Tafla;

/// <summary>
/// The header and offset type of a table. A format creates its layout once and passes it to both the
/// <see cref="TableBuilder"/> that writes the table and the <see cref="TableReader"/> that reads it, so
/// that the two can't disagree.
/// </summary>
/// <remarks>
/// The default layout, <see cref="Default"/>, has no header slots and the offset type
/// <see cref="OffsetType.UInt32"/>. Nested tables use it.
/// </remarks>
/// <example>
/// <code>
/// // The response's length and format version, then its field offsets
/// static readonly TableLayout Layout = new() { Sized = true, Version = 1 };
/// </code>
/// </example>
public readonly record struct TableLayout
{
    /// <summary>No header slots, and offsets of the offset type <see cref="OffsetType.UInt32"/>.</summary>
    public static TableLayout Default => default;

    /// <summary>Whether the header starts with the table's byte length. False by default.</summary>
    public bool Sized { get; init; }

    /// <summary>
    /// The version that the header holds, after the length when sized, which gives it a slot for one.
    /// Null, for no version, by default. Builders and writers write it, and readers can check the one
    /// that a table holds, with <see cref="TableReader.GetVersion"/>, before reading any field.
    /// </summary>
    /// <remarks>
    /// A layout with a version is the layout of that version of the format, and its readers read
    /// tables of any version, so that a program can read a table's version before it chooses how to
    /// read the fields. The layouts of a format's versions should differ in the version only, as
    /// <c>layout with { Version = 2 }</c> makes them. A version must fit in the offset type, which may
    /// be set after it, so the builders and sizers check it, and throw a <see cref="BuildException"/>.
    /// </remarks>
    public uint? Version { get; init; }

    /// <summary>Whether the header has a slot for a version, after the length when sized.</summary>
    public bool Versioned => Version.HasValue;

    /// <summary>
    /// The offset type, the unsigned integer type of the header slots, field offsets, counts, lengths
    /// and item ends, which limits the table's length: <see cref="OffsetType.UInt8"/> for up to 255
    /// bytes, <see cref="OffsetType.UInt16"/> for 65,535, and <see cref="OffsetType.UInt32"/>, the
    /// default, for as long as a .NET array can be, <see cref="Array.MaxLength"/> bytes, about 2 GiB.
    /// </summary>
    /// <exception cref="ArgumentOutOfRangeException">The value isn't one of the offset types.</exception>
    public OffsetType OffsetType
    {
        get;
        init
        {
            if (!OffsetTypeExtensions.IsDefined(value))
            {
                throw new ArgumentOutOfRangeException(nameof(value), value, "Not an offset type");
            }

            field = value;
        }
    }

    /// <summary>The offset size, the width in bytes of the offset type: 1, 2 or 4.</summary>
    public int OffsetSize => OffsetType.Size;

    /// <summary>The byte length of the header slots before the field offsets.</summary>
    public int HeaderSize => HeaderSlots * OffsetSize;

    /// <summary>The number of header slots: the length when sized, and the version when versioned.</summary>
    internal int HeaderSlots => (Sized ? 1 : 0) + (Versioned ? 1 : 0);

    /// <summary>
    /// The longest table that the offset type addresses and a .NET array holds: 255 bytes, 65,535
    /// bytes, or <see cref="Array.MaxLength"/> for <see cref="OffsetType.UInt32"/>.
    /// </summary>
    internal int MaxLength => (int)Math.Min(OffsetType.MaxValue, (uint)Array.MaxLength);

    /// <summary>
    /// Returns the length of the header and the offsets of a table of <paramref name="fields"/> fields
    /// in <paramref name="layout"/>, after the checks of the constructors of <see cref="TableBuilder"/>
    /// and <see cref="TableSizer"/>.
    /// </summary>
    /// <exception cref="BuildException">
    /// The layout's version doesn't fit in its offset type, or the header alone is too long for it.
    /// </exception>
    /// <exception cref="ArgumentOutOfRangeException"><paramref name="fields"/> is negative.</exception>
    internal static int HeaderLength(int fields, TableLayout layout)
    {
        ArgumentOutOfRangeException.ThrowIfNegative(fields);

        var offsetType = layout.OffsetType;
        if (layout.Version.GetValueOrDefault() > offsetType.MaxValue)
        {
            throw BuildException.VersionTooLarge(layout.Version.GetValueOrDefault(), offsetType);
        }

        long header = ((long)layout.HeaderSlots + fields) * layout.OffsetSize;
        if (header > layout.MaxLength)
        {
            throw BuildException.HeaderTooLong(fields, offsetType);
        }

        return (int)header;
    }

    /// <summary>The padding that brings <paramref name="offset"/> to a multiple of <paramref name="alignment"/>, a power of 2.</summary>
    internal static int Padding(long offset, int alignment) => (int)(-offset & (alignment - 1));

    /// <summary>Checks an alignment of arrays of structs and nested tables.</summary>
    /// <exception cref="ArgumentOutOfRangeException"><paramref name="alignment"/> isn't 1, 2, 4 or 8.</exception>
    internal static void CheckAlignment(int alignment)
    {
        if (alignment is not (1 or 2 or 4 or 8))
        {
            throw new ArgumentOutOfRangeException(
                nameof(alignment),
                alignment,
                "An alignment is 1, 2, 4 or 8 bytes"
            );
        }
    }
}
