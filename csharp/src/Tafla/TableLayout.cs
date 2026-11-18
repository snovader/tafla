using System;

namespace Tafla;

/// <summary>
/// The header and offset type of a table. A format creates its layout once and passes it to both the
/// <see cref="TableBuilder"/> that writes the table and the <see cref="ByteBuffer"/> that reads it, so
/// that the two can't disagree.
/// </summary>
/// <remarks>
/// The default layout, <see cref="Default"/>, has no header slots and the offset type
/// <see cref="OffsetType.UInt32"/>. Nested tables use it.
/// </remarks>
/// <example>
/// <code>
/// // The response's length and format version, then its field offsets
/// static readonly TableLayout Layout = new() { Sized = true, Versioned = true };
/// </code>
/// </example>
public readonly record struct TableLayout
{
    /// <summary>No header slots, and offsets of the offset type <see cref="OffsetType.UInt32"/>.</summary>
    public static TableLayout Default => default;

    /// <summary>Whether the header starts with the table's byte length. False by default.</summary>
    public bool Sized { get; init; }

    /// <summary>
    /// Whether the header has a slot for a version, after the length when sized. False by default.
    /// Readers can check it before reading any field, and a <see cref="TableBuilder"/> of the layout
    /// needs a version to write to it.
    /// </summary>
    public bool Versioned { get; init; }

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
}
