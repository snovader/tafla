import Tafla
import Testing

// The code that tafla-gen generates from the example schemas in schemas/, a
// module of each, whose golden files lie in Tests/Generated: the messages and
// structs that the tests write and read, and what they compare them with.
//
// TableBuilder writes each message from its fields' struct, as the tests'
// `build()` walks it with `Mirror`: each field by its Swift type, a struct
// from its scalars' bytes, a nested table from its fields' struct and a
// vector from an array of them. Values read back compare with those written
// by their bits, so that -0 and NaN count, and strings by their UTF-8.

/// A message of an example schema, with its generated code
struct Message: Sendable, CustomTestStringConvertible {
    let name: String
    /// Its layout, as its attributes set it
    let layout: TableLayout
    /// The version that its writer writes and its reader reads, when it's
    /// versioned
    let version: UInt32?
    /// Draws the values of its fields, into their struct
    let draw: @Sendable (inout Draw) -> any Sendable
    /// Writes the fields' struct with the generated code
    let serialize: @Sendable (any Sendable) throws(BuildError) -> [UInt8]
    /// Reads every field through the generated getters, and every item of
    /// the views that they return, into the fields' struct
    let read: @Sendable (RawSpan) throws(DecodeError) -> any Sendable
    /// The generated `version(of:)` of a versioned message
    let versionOf: (@Sendable (RawSpan) throws(DecodeError) -> UInt32)?

    var testDescription: String { name }

    init<Fields: GeneratedFields>(
        _ name: String,
        layout: TableLayout,
        version: UInt32? = nil,
        draw: @escaping @Sendable (inout Draw) -> Fields,
        read: @escaping @Sendable (RawSpan) throws(DecodeError) -> Fields,
        versionOf: (@Sendable (RawSpan) throws(DecodeError) -> UInt32)? = nil
    ) {
        self.name = name
        self.layout = layout
        self.version = version
        self.draw = { draw(&$0) }
        self.serialize = { fields throws(BuildError) in try (fields as! Fields).serialize() }
        self.read = { bytes throws(DecodeError) in try read(bytes) }
        self.versionOf = versionOf
    }
}

/// The struct of a message's fields, which writes it
protocol GeneratedFields: Sendable {
    func serialize() throws(BuildError) -> [UInt8]
}

/// A struct that tafla-gen generates
protocol GeneratedStruct: Sendable {
    static var size: Int { get }
    static var alignment: Alignment { get }

    init(_ bytes: RawSpan) throws(DecodeError)

    func serialize() -> [UInt8]

    static func serializeArray(_ items: [Self]) -> [UInt8]
}

/// An array of scalars, which a fields' struct holds for an array field
protocol ScalarArray {
    func set(on builder: inout TableBuilder) throws(BuildError)
}

extension Array: ScalarArray where Element: ArrayElement {
    func set(on builder: inout TableBuilder) throws(BuildError) {
        try builder.setArray(self)
    }
}

/// An array of a generated struct, which a fields' struct holds for an array
/// of structs
protocol GeneratedStructArray {
    var structSize: Int { get }
    var structAlignment: Alignment { get }
    /// The bytes of every struct, one after another
    var structBytes: [UInt8] { get }
}

extension Array: GeneratedStructArray where Element: GeneratedStruct {
    var structSize: Int { Element.size }

    var structAlignment: Alignment { Element.alignment }

    var structBytes: [UInt8] { flatMap { scalarBytes(of: $0) } }
}

/// A struct that tafla-gen generates, with how to check it
struct StructCase: Sendable, CustomTestStringConvertible {
    let name: String
    /// Draws two structs and checks them with `checkStruct()`
    let check: @Sendable (inout Draw) throws -> Void

    var testDescription: String { name }

    init<S: GeneratedStruct>(_ name: String, _ type: S.Type, size: Int, draw: @escaping @Sendable (inout Draw) -> S) {
        self.name = name
        self.check = { random in
            try checkStruct(
                draw(&random),
                draw(&random),
                size: size,
                after: random.int(0...8),
                byteOffset: random.int(0...7)
            )
        }
    }
}

/// Writes a message's fields with `TableBuilder`, in `layout`, as its
/// generated code writes them
func build(_ fields: Any, layout: TableLayout, version: UInt32?) throws(BuildError) -> [UInt8] {
    try builder(of: fields, layout: layout, version: version).bytes()
}

/// A builder of a table with every field of `fields`, a struct of its
/// fields, set
func builder(of fields: Any, layout: TableLayout, version: UInt32? = nil) throws(BuildError) -> TableBuilder {
    let children = Mirror(reflecting: fields).children
    var builder = try TableBuilder(fields: children.count, layout: layout, version: version)
    for child in children {
        try builder.set(value: child.value)
    }

    return builder
}

extension TableBuilder {
    /// Sets a field of a generated fields' struct, by its type: a bool as
    /// the `uint8` 0 or 1, and a struct of fields as a table nested from its
    /// builder, which it's padded to the alignment of, as a fields' struct
    /// is
    mutating func set(value: Any) throws(BuildError) {
        switch value {
        case let value as Int8: try setInt8(value)
        case let value as UInt8: try setUInt8(value)
        case let value as Int16: try setInt16(value)
        case let value as UInt16: try setUInt16(value)
        case let value as Int32: try setInt32(value)
        case let value as UInt32: try setUInt32(value)
        case let value as Int64: try setInt64(value)
        case let value as UInt64: try setUInt64(value)
        case let value as Float: try setFloat32(value)
        case let value as Double: try setFloat64(value)
        case let value as Bool: try setUInt8(value ? 1 : 0)
        // By the array's own type, since an empty array casts to an array of
        // any other type
        case let values as any ScalarArray: try values.set(on: &self)
        case let value as String: try setString(value)
        case let value as any GeneratedStruct: try setStruct(scalarBytes(of: value))
        case let values as any GeneratedStructArray:
            try setStructArray(values.structBytes, size: values.structSize, alignment: values.structAlignment)
        case let items as [Any]:
            var tables: [NestedTable] = []
            for item in items {
                tables.append(.builder(try builder(of: item, layout: .default)))
            }
            try setVector(tables)
        default:
            try setTable(.builder(builder(of: value, layout: .default)))
        }
    }
}

/// The bytes of a struct's scalars, little-endian, one after another, and of
/// the scalars of the structs within it in their place
func scalarBytes(of value: Any) -> [UInt8] {
    Mirror(reflecting: value).children.flatMap { child -> [UInt8] in
        switch child.value {
        case let value as Int8: [UInt8(bitPattern: value)]
        case let value as UInt8: [value]
        case let value as Int16: littleEndianBytes(value)
        case let value as UInt16: littleEndianBytes(value)
        case let value as Int32: littleEndianBytes(value)
        case let value as UInt32: littleEndianBytes(value)
        case let value as Int64: littleEndianBytes(value)
        case let value as UInt64: littleEndianBytes(value)
        case let value as Float: littleEndianBytes(value.bitPattern)
        case let value as Double: littleEndianBytes(value.bitPattern)
        case let value as Bool: [value ? 1 : 0]
        default: scalarBytes(of: child.value)
        }
    }
}

/// Whether two values of the generated types are the same: floats by their
/// bits, strings by their UTF-8, and others by their parts, as `Mirror`
/// finds them
func sameValues(_ left: Any, _ right: Any) -> Bool {
    switch (left, right) {
    case (let left as Float, let right as Float):
        return left.bitPattern == right.bitPattern
    case (let left as Double, let right as Double):
        return left.bitPattern == right.bitPattern
    case (let left as String, let right as String):
        return left.utf8.elementsEqual(right.utf8)
    default:
        let (leftParts, rightParts) = (Mirror(reflecting: left), Mirror(reflecting: right))
        guard leftParts.subjectType == rightParts.subjectType,
            leftParts.children.count == rightParts.children.count
        else {
            return false
        }

        // A scalar, a bool or an empty array or struct
        if leftParts.children.isEmpty, let left = left as? AnyHashable, let right = right as? AnyHashable {
            return left == right
        }

        return zip(leftParts.children, rightParts.children).allSatisfy { sameValues($0.value, $1.value) }
    }
}

extension Draw {
    mutating func float() -> Float {
        Float(bitPattern: float32())
    }

    mutating func double() -> Double {
        Double(bitPattern: float64())
    }

    mutating func floats(most: Int) -> [Float] {
        array(most: most) { $0.float() }
    }

    mutating func doubles(most: Int) -> [Double] {
        array(most: most) { $0.double() }
    }

    mutating func integers<T: FixedWidthInteger>(most: Int) -> [T] {
        array(most: most) { $0.integer() }
    }
}
