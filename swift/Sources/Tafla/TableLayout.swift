/// A table's offset type, which limits how long the table can be.
///
/// It's the unsigned integer type of the table's header slots, field offsets,
/// counts, lengths and item ends. Each case is named after its type, as
/// `SPEC.md` and schemas name it, and describes itself as that name.
public enum OffsetType: Sendable, Hashable, CaseIterable, CustomStringConvertible {
    /// `uint8`, for tables of up to 255 bytes
    case uint8
    /// `uint16`, for tables of up to 65,535 bytes
    case uint16
    /// `uint32`, for tables of up to 4 GiB
    case uint32

    /// The offset size, the type's width in bytes: 1, 2 or 4
    @inlinable
    public var size: Int {
        switch self {
        case .uint8: 1
        case .uint16: 2
        case .uint32: 4
        }
    }

    /// The type's largest value, which is also the longest table that its
    /// offsets address
    @inlinable
    public var maxValue: UInt32 {
        switch self {
        case .uint8: 0xff
        case .uint16: 0xffff
        case .uint32: 0xffff_ffff
        }
    }

    /// The type's name, as `SPEC.md` and schemas spell it: `uint8`, `uint16`
    /// or `uint32`
    public var description: String {
        switch self {
        case .uint8: "uint8"
        case .uint16: "uint16"
        case .uint32: "uint32"
        }
    }
}

/// A multiple of bytes from a table's start that a field starts at, so that
/// its elements can be read in place.
public enum Alignment: Int, Sendable, Hashable, Comparable, CaseIterable {
    case one = 1
    case two = 2
    case four = 4
    case eight = 8

    /// The alignment in bytes: 1, 2, 4 or 8
    @inlinable
    public var bytes: Int { rawValue }

    public static func < (lhs: Alignment, rhs: Alignment) -> Bool {
        lhs.rawValue < rhs.rawValue
    }

    /// The bytes of padding that bring `offset` to a multiple of the
    /// alignment
    @inlinable
    func padding(at offset: UInt64) -> UInt64 {
        (0 &- offset) & UInt64(rawValue &- 1)
    }
}

/// The header and offset type of a table.
///
/// A format creates its layout once and passes it to both the `TableBuilder`
/// that writes the table and the `ByteBuffer` that reads it, so that the two
/// can't disagree. Tables nested with `TableBuilder.setTable(_:alignment:)`
/// have no header, as ``TableLayout/default`` has none.
///
/// ```swift
/// let layout = TableLayout(sized: true, versioned: true)
/// let frame = TableLayout(offsetType: .uint8)
///
/// layout.headerSize  // 8
/// frame.headerSize  // 0
/// ```
public struct TableLayout: Sendable, Hashable {
    /// Whether the header starts with the table's total byte length
    public let isSized: Bool
    /// Whether the header holds a version, after the length when sized.
    /// Readers can check it before reading any field.
    public let isVersioned: Bool
    /// The unsigned integer type of the header slots, field offsets, counts,
    /// lengths and item ends, which limits how long the table can be
    public let offsetType: OffsetType

    /// No header slots, and offsets and lengths of the offset type `uint32`
    public static let `default` = TableLayout()

    public init(sized: Bool = false, versioned: Bool = false, offsetType: OffsetType = .uint32) {
        self.isSized = sized
        self.isVersioned = versioned
        self.offsetType = offsetType
    }

    /// The offset size, the width in bytes of the offset type: 1, 2 or 4
    @inlinable
    public var offsetSize: Int { offsetType.size }

    /// The byte length of the header slots before the field offsets
    @inlinable
    public var headerSize: Int { headerSlots * offsetType.size }

    @inlinable
    var headerSlots: Int { (isSized ? 1 : 0) + (isVersioned ? 1 : 0) }

    /// The byte length of the header of a table of `fields` fields: its slots
    /// and an offset per field, which `TableBuilder` starts a table with.
    ///
    /// Throws ``BuildError/headerTooLong(fields:offsetType:)`` if the header
    /// is longer than the layout's offsets address.
    func headerLength(fields: Int) throws(BuildError) -> UInt32 {
        let slots = UInt64(fields) + UInt64(headerSlots)
        let (length, overflow) = slots.multipliedReportingOverflow(by: UInt64(offsetType.size))

        guard !overflow, length <= UInt64(offsetType.maxValue) else {
            throw .headerTooLong(fields: fields, offsetType: offsetType)
        }

        return UInt32(length)
    }

    /// Checks that `version` fits in the header's slot.
    ///
    /// Throws ``BuildError/versionTooLarge(version:offsetType:)`` if it
    /// doesn't fit in the layout's offset type.
    func check(version: UInt32) throws(BuildError) {
        guard version <= offsetType.maxValue else {
            throw .versionTooLarge(version: version, offsetType: offsetType)
        }
    }
}
