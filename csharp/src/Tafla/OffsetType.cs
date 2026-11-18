using System.Diagnostics.CodeAnalysis;

namespace Tafla;

/// <summary>
/// A table's offset type: the unsigned integer type of its header slots, field offsets, counts,
/// lengths and item ends, which limits how long the table can be. Each value is named after its
/// type, as SPEC.md and schemas name it, and its offset size is its width in bytes.
/// </summary>
/// <remarks>
/// <see cref="UInt32"/> is 0, the default value, so that the default <see cref="TableLayout"/> has
/// the default offset type.
/// </remarks>
[SuppressMessage(
    "Naming",
    "CA1720:Identifier contains type name",
    Justification = "Each offset type is named after the unsigned integer type that it is"
)]
public enum OffsetType : byte
{
    /// <summary><c>uint32</c>, for tables of up to 4 GiB, or as long as a .NET array can be. The default.</summary>
    UInt32 = 0,

    /// <summary><c>uint16</c>, for tables of up to 65,535 bytes.</summary>
    UInt16 = 1,

    /// <summary><c>uint8</c>, for tables of up to 255 bytes.</summary>
    UInt8 = 2,
}

/// <summary>The offset size and the largest value of an <see cref="OffsetType"/>.</summary>
public static class OffsetTypeExtensions
{
    extension(OffsetType offsetType)
    {
        /// <summary>The offset size, the type's width in bytes: 1, 2 or 4.</summary>
        public int Size => 4 >> (int)offsetType;

        /// <summary>The type's largest value, which is also the longest table that its offsets address.</summary>
        public uint MaxValue => uint.MaxValue >> (32 - 8 * offsetType.Size);

        /// <summary>The type's name as SPEC.md spells it: <c>uint8</c>, <c>uint16</c> or <c>uint32</c>.</summary>
        public string Name =>
            offsetType switch
            {
                OffsetType.UInt8 => "uint8",
                OffsetType.UInt16 => "uint16",
                OffsetType.UInt32 => "uint32",
                _ => offsetType.ToString(),
            };
    }

    /// <summary>Whether the value is one of the offset types, rather than another byte.</summary>
    internal static bool IsDefined(OffsetType offsetType) => offsetType <= OffsetType.UInt8;
}
