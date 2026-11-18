using System;
using System.Diagnostics.CodeAnalysis;

namespace Tafla;

/// <summary>Why a table couldn't be written.</summary>
public enum BuildErrorKind
{
    /// <summary>
    /// A <see cref="BuildException.Version"/> that doesn't fit in the header slot of
    /// <see cref="BuildException.OffsetType"/>.
    /// </summary>
    VersionTooLarge = 1,

    /// <summary>
    /// A header of <see cref="BuildException.Fields"/> offsets that alone is longer than offsets of
    /// <see cref="BuildException.OffsetType"/> address.
    /// </summary>
    HeaderTooLong,

    /// <summary>A field set on a table that already holds its <see cref="BuildException.Fields"/> fields.</summary>
    TooManyFields,

    /// <summary>
    /// The field at <see cref="BuildException.Index"/>, which would make the table
    /// <see cref="BuildException.Length"/> bytes, longer than offsets of
    /// <see cref="BuildException.OffsetType"/> address, or than a .NET array holds.
    /// </summary>
    TooLong,

    /// <summary>
    /// A table written, or nested, with <see cref="BuildException.FieldsSet"/> of its
    /// <see cref="BuildException.Fields"/> fields set.
    /// </summary>
    Incomplete,

    /// <summary>
    /// An array of structs whose <see cref="BuildException.Length"/> bytes aren't a whole number of
    /// structs of <see cref="BuildException.StructSize"/> bytes.
    /// </summary>
    NotWholeStructs,
}

/// <summary>
/// Thrown when a <see cref="TableBuilder"/> can't write a table, whatever the values, rather than
/// writing a table that readers would misread. A setter that throws it leaves the builder as it was.
/// </summary>
/// <remarks>
/// <see cref="Kind"/> says what failed, and the properties that its documentation names describe it.
/// Mistakes of the code that calls the builder, which no values cause, such as a negative number of
/// fields or an alignment that isn't 1, 2, 4 or 8, throw an <see cref="ArgumentException"/> instead.
/// </remarks>
[SuppressMessage(
    "Design",
    "CA1032:Implement standard exception constructors",
    Justification = "Every build error has a kind, which its factories set"
)]
public sealed class BuildException : Exception
{
    private BuildException(BuildErrorKind kind, string message)
        : base(message)
    {
        Kind = kind;
    }

    /// <summary>Why the table couldn't be written.</summary>
    public BuildErrorKind Kind { get; }

    /// <summary>
    /// The offset type of the table, for <see cref="BuildErrorKind.VersionTooLarge"/>,
    /// <see cref="BuildErrorKind.HeaderTooLong"/> and <see cref="BuildErrorKind.TooLong"/>.
    /// </summary>
    public OffsetType OffsetType { get; private init; }

    /// <summary>The version that doesn't fit, for <see cref="BuildErrorKind.VersionTooLarge"/>.</summary>
    public uint Version { get; private init; }

    /// <summary>
    /// The number of the table's fields, for <see cref="BuildErrorKind.HeaderTooLong"/>,
    /// <see cref="BuildErrorKind.TooManyFields"/> and <see cref="BuildErrorKind.Incomplete"/>.
    /// </summary>
    public int Fields { get; private init; }

    /// <summary>The number of fields set, for <see cref="BuildErrorKind.Incomplete"/>.</summary>
    public int FieldsSet { get; private init; }

    /// <summary>The index of the field that makes the table too long, for <see cref="BuildErrorKind.TooLong"/>.</summary>
    public int Index { get; private init; }

    /// <summary>
    /// The table's length with the field, for <see cref="BuildErrorKind.TooLong"/>, and the length of
    /// the structs' bytes, for <see cref="BuildErrorKind.NotWholeStructs"/>.
    /// </summary>
    public long Length { get; private init; }

    /// <summary>The size of a struct, for <see cref="BuildErrorKind.NotWholeStructs"/>.</summary>
    public int StructSize { get; private init; }

    internal static BuildException VersionTooLarge(uint version, OffsetType offsetType) =>
        new(BuildErrorKind.VersionTooLarge, $"Version {version} doesn't fit in a {offsetType.Name} slot")
        {
            Version = version,
            OffsetType = offsetType,
        };

    internal static BuildException HeaderTooLong(int fields, OffsetType offsetType) =>
        new(
            BuildErrorKind.HeaderTooLong,
            $"A header of {fields} fields doesn't fit in {offsetType.Name} offsets"
        )
        {
            Fields = fields,
            OffsetType = offsetType,
        };

    internal static BuildException TooManyFields(int fields) =>
        new(BuildErrorKind.TooManyFields, $"Can't add field {fields + 1} to a table of {fields}")
        {
            Fields = fields,
        };

    internal static BuildException TooLong(int index, long length, OffsetType offsetType) =>
        new(
            BuildErrorKind.TooLong,
            $"Field {index + 1} makes the table {length} bytes, too long for {offsetType.Name} offsets"
        )
        {
            Index = index,
            Length = length,
            OffsetType = offsetType,
        };

    internal static BuildException Incomplete(int fieldsSet, int fields) =>
        new(BuildErrorKind.Incomplete, $"Only {fieldsSet} of the table's {fields} fields were set")
        {
            FieldsSet = fieldsSet,
            Fields = fields,
        };

    internal static BuildException NotWholeStructs(long length, int structSize) =>
        new(
            BuildErrorKind.NotWholeStructs,
            $"{length} bytes aren't a whole number of {structSize}-byte structs"
        )
        {
            Length = length,
            StructSize = structSize,
        };
}
