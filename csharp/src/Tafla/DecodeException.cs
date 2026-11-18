using System;
using System.Diagnostics.CodeAnalysis;

namespace Tafla;

/// <summary>Why bytes couldn't be read.</summary>
public enum DecodeErrorKind
{
    /// <summary>
    /// A read of <see cref="DecodeException.ByteLength"/> bytes at <see cref="DecodeException.Position"/>
    /// that would leave the <see cref="DecodeException.Available"/> bytes.
    /// </summary>
    OutOfRange = 1,

    /// <summary>A string, whose count lies at <see cref="DecodeException.Position"/>, that isn't valid UTF-8.</summary>
    InvalidUtf8,

    /// <summary>
    /// A vector item that starts at <see cref="DecodeException.Position"/> and ends before it, at
    /// <see cref="DecodeException.End"/>. An item starts where the item before it ends, and the first
    /// after the ends.
    /// </summary>
    ItemEndsBeforeStart,

    /// <summary>
    /// A header that carries a <see cref="DecodeException.Version"/> that the reader can't read, since
    /// it reads <see cref="DecodeException.SupportedVersion"/>. <see cref="TableReader"/> doesn't compare
    /// versions: a reader throws it, from <see cref="DecodeException.UnsupportedVersion"/>, when
    /// <see cref="TableReader.GetVersion"/> isn't its own.
    /// </summary>
    UnsupportedVersion,

    /// <summary>
    /// A string, whose count lies at <see cref="DecodeException.Position"/>, of
    /// <see cref="DecodeException.ByteLength"/> bytes that hold more characters than a .NET
    /// <see cref="string"/> can, 1,073,741,791. The bytes may be a valid table, since a table of
    /// <see cref="OffsetType.UInt32"/> offsets can hold a longer string, but .NET can't. Only a string
    /// of more bytes than that can fail so, as UTF-8 has at most one character per byte.
    /// </summary>
    StringTooLong,
}

/// <summary>
/// Thrown when bytes can't be read as the table, array, string, struct, vector or value that a read
/// expects. Every read of a <see cref="TableReader"/> and a <see cref="Vector"/> checks its range
/// against the supplied bytes before it returns, so a truncated or corrupted buffer throws this rather
/// than returning values from outside them, or throwing any other exception. Strings must also be
/// valid UTF-8.
/// </summary>
/// <remarks>
/// Checks can't tell a value that was corrupted within range from a real one, such as an offset that
/// points at another field. Formats that need that guarantee can add a checksum. <see cref="Kind"/>
/// says what failed, and the properties that its documentation names describe it.
/// </remarks>
[SuppressMessage(
    "Design",
    "CA1032:Implement standard exception constructors",
    Justification = "Every decode error has a kind, which its factories set"
)]
public sealed class DecodeException : Exception
{
    private DecodeException(DecodeErrorKind kind, string message, Exception? innerException = null)
        : base(message, innerException)
    {
        Kind = kind;
    }

    /// <summary>Why the bytes couldn't be read.</summary>
    public DecodeErrorKind Kind { get; }

    /// <summary>
    /// Where the read starts, for <see cref="DecodeErrorKind.OutOfRange"/>, where the string's count lies,
    /// for <see cref="DecodeErrorKind.InvalidUtf8"/> and <see cref="DecodeErrorKind.StringTooLong"/>, and
    /// where the item starts, for <see cref="DecodeErrorKind.ItemEndsBeforeStart"/>. It can be negative,
    /// or past the largest <see cref="int"/>, for a read of a position that no bytes hold.
    /// </summary>
    public long Position { get; private init; }

    /// <summary>
    /// The length of the read, for <see cref="DecodeErrorKind.OutOfRange"/>, and of the string's bytes,
    /// for <see cref="DecodeErrorKind.StringTooLong"/>.
    /// </summary>
    public long ByteLength { get; private init; }

    /// <summary>The length of the bytes, for <see cref="DecodeErrorKind.OutOfRange"/>.</summary>
    public int Available { get; private init; }

    /// <summary>Where the item ends, for <see cref="DecodeErrorKind.ItemEndsBeforeStart"/>.</summary>
    public long End { get; private init; }

    /// <summary>The version that the header carries, for <see cref="DecodeErrorKind.UnsupportedVersion"/>.</summary>
    public uint Version { get; private init; }

    /// <summary>The version that the reader reads, for <see cref="DecodeErrorKind.UnsupportedVersion"/>.</summary>
    public uint SupportedVersion { get; private init; }

    /// <summary>
    /// Returns the error of a header that carries <paramref name="version"/>, which a reader of
    /// <paramref name="supportedVersion"/> can't read, for a reader to throw when
    /// <see cref="TableReader.GetVersion"/> isn't its own.
    /// </summary>
    public static DecodeException UnsupportedVersion(uint version, uint supportedVersion) =>
        new(
            DecodeErrorKind.UnsupportedVersion,
            $"Unsupported format version {version}; the reader reads version {supportedVersion}"
        )
        {
            Version = version,
            SupportedVersion = supportedVersion,
        };

    /// <summary>
    /// Returns the error of a read of <paramref name="byteLength"/> bytes at <paramref name="position"/>
    /// that would leave the <paramref name="available"/> bytes, for code that reads bytes without a
    /// <see cref="TableReader"/>, such as tafla-gen's structs, to throw.
    /// </summary>
    public static DecodeException OutOfRange(long position, long byteLength, int available) =>
        new(
            DecodeErrorKind.OutOfRange,
            $"Read exceeds the supplied bytes: {byteLength} bytes at {position} of {available}"
        )
        {
            Position = position,
            ByteLength = byteLength,
            Available = available,
        };

    internal static DecodeException InvalidUtf8(int position, Exception innerException) =>
        new(DecodeErrorKind.InvalidUtf8, $"The string at {position} isn't valid UTF-8", innerException)
        {
            Position = position,
        };

    internal static DecodeException StringTooLong(int position, int byteLength) =>
        new(
            DecodeErrorKind.StringTooLong,
            $"The string of {byteLength} bytes at {position} has more characters than a .NET string holds"
        )
        {
            Position = position,
            ByteLength = byteLength,
        };

    internal static DecodeException ItemEndsBeforeStart(long position, long end) =>
        new(
            DecodeErrorKind.ItemEndsBeforeStart,
            $"The vector item at {position} ends before it starts, at {end}"
        )
        {
            Position = position,
            End = end,
        };

    // The getters throw through these, which aren't inlined, so that the code that builds the
    // exception stays out of the getters, which then inline into their callers

    [DoesNotReturn]
    internal static void ThrowOutOfRange(long position, long byteLength, int available) =>
        throw OutOfRange(position, byteLength, available);

    [DoesNotReturn]
    internal static void ThrowItemEndsBeforeStart(long position, long end) =>
        throw ItemEndsBeforeStart(position, end);
}
