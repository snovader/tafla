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
/// that writes the table and the `TableReader` that reads it, so that the two
/// can't disagree. Tables nested with `TableBuilder.setTable(_:alignment:)`
/// have no header, as ``TableLayout/default`` has none.
///
/// ```swift
/// let layout = TableLayout(sized: true, version: 1)
/// let frame = TableLayout(offsetType: .uint8)
///
/// layout.headerSize  // 8
/// frame.headerSize  // 0
/// ```
///
/// A layout with a version is the layout of that version of the format: its
/// builders and writers write the version, and its readers read tables of
/// any version, so that a program can read a table's version before it
/// chooses how to read the fields. The layouts of a format's versions should
/// differ in the version only, as ``withVersion(_:)`` makes them.
public struct TableLayout: Sendable, Hashable {
    /// The version that the header holds, after the length when sized, or
    /// `nil` for a header without one. Builders and writers write it, and
    /// readers can check the version that a table holds, with
    /// `TableReader.version`, before reading any field.
    ///
    /// It's the first stored property, before the two of a byte each, so that
    /// the layout takes 8 bytes rather than 12.
    public let version: UInt32?
    /// Whether the header starts with the table's total byte length
    public let isSized: Bool
    /// The unsigned integer type of the header slots, field offsets, counts,
    /// lengths and item ends, which limits how long the table can be
    public let offsetType: OffsetType

    /// No header slots, and offsets and lengths of the offset type `uint32`
    @inlinable
    public static var `default`: TableLayout {
        // Computed rather than stored, so that it's a constant in the code
        // that reads with it: another module loads a stored global from
        // memory, and a reader built with it would then switch on its offset
        // type at every read
        TableLayout()
    }

    /// A version must fit in the offset type: the builders and sizers check
    /// it, and throw ``BuildError/versionTooLarge(version:offsetType:)``.
    @inlinable
    public init(sized: Bool = false, version: UInt32? = nil, offsetType: OffsetType = .uint32) {
        self.isSized = sized
        self.version = version
        self.offsetType = offsetType
    }

    /// Whether the header holds a version, after the length when sized
    @inlinable
    public var isVersioned: Bool { version != nil }

    /// The layout with `version` in place of its own, for another version of
    /// the same format
    @inlinable
    public func withVersion(_ version: UInt32) -> TableLayout {
        TableLayout(sized: isSized, version: version, offsetType: offsetType)
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

    /// Checks that the layout's version fits in the header's slot.
    ///
    /// Throws ``BuildError/versionTooLarge(version:offsetType:)`` if it
    /// doesn't fit in the layout's offset type.
    func checkVersion() throws(BuildError) {
        if let version, version > offsetType.maxValue {
            throw .versionTooLarge(version: version, offsetType: offsetType)
        }
    }
}
