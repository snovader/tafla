import Tafla

// The tests' one model of tables: the conformance suite's descriptions, which
// Cases.swift parses from cases.json and Strategy.swift generates, with the
// builder and reader that every test builds and reads them with. See
// conformance/README.md at the root of the repository.
//
// Floats are held by their bits and strings by their UTF-8, so that equal
// descriptions hold the same bytes: NaN equals itself, -0 doesn't equal 0,
// and strings that Unicode deems equivalent aren't equal.

/// A table: its layout, its version and its fields, in the order that they're
/// set
struct Table: Hashable, Sendable {
    var layout: TableLayout
    /// The version in the header, present only when the layout is versioned
    var version: UInt32?
    var fields: [Field]
}

/// A field of each type that a table holds, with its value
enum Field: Hashable, Sendable {
    case int8(Int8)
    case uint8(UInt8)
    case int16(Int16)
    case uint16(UInt16)
    case int32(Int32)
    case uint32(UInt32)
    case int64(Int64)
    case uint64(UInt64)
    case float32(bits: UInt32)
    case float64(bits: UInt64)
    /// A bool, which the runtimes have no API for: it's written as the uint8
    /// 1 or 0, and read as true when its byte isn't 0. It's a field only,
    /// rather than a scalar of structs.
    case bool(Bool)
    case int8Array([Int8])
    case uint8Array([UInt8])
    case int16Array([Int16])
    case uint16Array([UInt16])
    case int32Array([Int32])
    case uint32Array([UInt32])
    case int64Array([Int64])
    case uint64Array([UInt64])
    case float32Array(bits: [UInt32])
    case float64Array(bits: [UInt64])
    case string(utf8: [UInt8])
    /// A struct of scalar fields, one after another
    case `struct`([Field])
    /// An array of structs of scalars of `types`, as the scalar fields of
    /// each struct
    case structArray(types: [ScalarType], items: [[Field]])
    /// A table nested with `setTable(_:alignment:)`, with the alignment that
    /// it's set with when the description gives one
    case table(Nested, alignment: Alignment?)
    case vector([Nested])

    static func float32(_ value: Float) -> Field { .float32(bits: value.bitPattern) }

    static func float64(_ value: Double) -> Field { .float64(bits: value.bitPattern) }

    static func string(_ value: String) -> Field { .string(utf8: Array(value.utf8)) }

    /// The type of a scalar field, or `nil` for a field of any other type
    var scalarType: ScalarType? {
        switch self {
        case .int8: .int8
        case .uint8: .uint8
        case .int16: .int16
        case .uint16: .uint16
        case .int32: .int32
        case .uint32: .uint32
        case .int64: .int64
        case .uint64: .uint64
        case .float32: .float32
        case .float64: .float64
        default: nil
        }
    }

    /// The little-endian bytes of a scalar field, which a struct holds
    var scalarBytes: [UInt8] {
        switch self {
        case .int8(let value): littleEndianBytes(value)
        case .uint8(let value): littleEndianBytes(value)
        case .int16(let value): littleEndianBytes(value)
        case .uint16(let value): littleEndianBytes(value)
        case .int32(let value): littleEndianBytes(value)
        case .uint32(let value): littleEndianBytes(value)
        case .int64(let value): littleEndianBytes(value)
        case .uint64(let value): littleEndianBytes(value)
        case .float32(let bits): littleEndianBytes(bits)
        case .float64(let bits): littleEndianBytes(bits)
        default: preconditionFailure("A struct holds scalars")
        }
    }
}

/// A table nested from its builder or from the bytes that its builder writes
struct Nested: Hashable, Sendable {
    var from: Source
    var table: Table
}

/// What a nested table is set from
enum Source: Hashable, Sendable, CaseIterable {
    case builder
    case bytes
}

/// The type of a scalar, of which structs are made
enum ScalarType: Hashable, Sendable, CaseIterable {
    case int8
    case uint8
    case int16
    case uint16
    case int32
    case uint32
    case int64
    case uint64
    case float32
    case float64

    /// The scalar's size in bytes, which is also its alignment in an array
    var size: Int {
        switch self {
        case .int8, .uint8: 1
        case .int16, .uint16: 2
        case .int32, .uint32, .float32: 4
        case .int64, .uint64, .float64: 8
        }
    }

    var alignment: Alignment { Alignment(rawValue: size)! }
}

/// The size of a struct of scalars of `types`: the sum of theirs
func structSize(_ types: [ScalarType]) -> Int {
    types.reduce(0) { $0 + $1.size }
}

/// The alignment of an array of structs of scalars of `types`: the largest of
/// their sizes
func structAlignment(_ types: [ScalarType]) -> Alignment {
    types.map(\.alignment).max() ?? .one
}

/// The bytes of a struct of scalar fields: theirs, one after another
func structBytes(_ scalars: [Field]) -> [UInt8] {
    scalars.flatMap(\.scalarBytes)
}

func littleEndianBytes<T: FixedWidthInteger>(_ value: T) -> [UInt8] {
    withUnsafeBytes(of: value.littleEndian, Array.init)
}

/// A layout without header slots, which tables nested with `setTable` have
func headerless(_ offsetType: OffsetType) -> TableLayout {
    TableLayout(offsetType: offsetType)
}
