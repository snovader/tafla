/// Thrown when bytes can't be read as the table, array, string, struct,
/// vector or value that a read expects.
///
/// Every read checks its range against the bytes it was given before it
/// returns, so truncated or corrupted bytes throw this rather than return
/// values from outside them. Strings must also be valid UTF-8.
///
/// Checks can't tell a value that was corrupted within range from a real one,
/// such as an offset that points at another field. Formats that need that
/// guarantee can add a checksum.
///
/// Later versions may add cases, so a `switch` over them should have a
/// `default`.
public enum DecodeError: Error, Hashable, Sendable, CustomStringConvertible {
    /// The read of `byteLength` bytes at `position` would leave the
    /// `available` bytes. An offset or length that an `Int` can't hold, on a
    /// 32-bit host, fails so too, as a read of that many bytes at its slot.
    case outOfRange(position: Int, byteLength: UInt64, available: Int)
    /// The string at `position` isn't valid UTF-8
    case invalidUTF8(position: Int)
    /// The vector item at `start` ends before it starts, at `end`. An item
    /// starts where the item before it ends, and the first after the ends.
    case itemEndsBeforeStart(start: Int, end: Int)
    /// The header carries format `version`, which the reader can't read,
    /// since it reads version `supported`. `TableReader` doesn't compare
    /// versions: a reader throws this when ``TableReader/version`` isn't its
    /// own.
    case unsupportedVersion(version: UInt32, supported: UInt32)

    public var description: String {
        switch self {
        case .outOfRange(let position, let byteLength, let available):
            "Read exceeds the supplied buffer: \(byteLength) bytes at \(position) of \(available)"
        case .invalidUTF8(let position):
            "The string at \(position) isn't valid UTF-8"
        case .itemEndsBeforeStart(let start, let end):
            "The vector item at \(start) ends before it starts, at \(end)"
        case .unsupportedVersion(let version, let supported):
            "Unsupported format version \(version); the reader reads version \(supported)"
        }
    }
}

/// Thrown when a `TableBuilder` can't write a table, whatever the values'
/// bytes. A setter that throws leaves the builder as it was.
///
/// Later versions may add cases, so a `switch` over them should have a
/// `default`.
public enum BuildError: Error, Hashable, Sendable, CustomStringConvertible {
    /// The version doesn't fit in the layout's header slots
    case versionTooLarge(version: UInt32, offsetType: OffsetType)
    /// The header of `fields` offsets alone is longer than the layout's
    /// offsets address
    case headerTooLong(fields: Int, offsetType: OffsetType)
    /// A field was set on a table that already holds its `fields` fields
    case tooManyFields(fields: Int)
    /// The field at `index` would make the table `length` bytes, longer than
    /// the layout's offsets address, or than an `Int` holds
    case tooLong(index: Int, length: UInt64, offsetType: OffsetType)
    /// The table was written, or nested, with `set` of its `fields` fields
    /// set
    case incomplete(set: Int, fields: Int)
    /// The `length` bytes of an array of structs aren't a whole number of
    /// `size`-byte structs, or `size` isn't positive
    case notWholeStructs(length: Int, size: Int)

    public var description: String {
        switch self {
        case .versionTooLarge(let version, let offsetType):
            "Version \(version) doesn't fit in a \(offsetType) slot"
        case .headerTooLong(let fields, let offsetType):
            "A header of \(fields) fields doesn't fit in \(offsetType) offsets"
        case .tooManyFields(let fields):
            "Can't add field \(fields + 1) to a table of \(fields)"
        case .tooLong(let index, let length, let offsetType):
            "Field \(index + 1) makes the table \(length) bytes, too long for \(offsetType) offsets"
        case .incomplete(let set, let fields):
            "Only \(set) of the table's \(fields) fields were set"
        case .notWholeStructs(_, let size) where size <= 0:
            "A struct can't be \(size) bytes"
        case .notWholeStructs(let length, let size):
            "\(length) bytes aren't a whole number of \(size)-byte structs"
        }
    }
}
