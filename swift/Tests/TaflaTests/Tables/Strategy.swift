import PropertyBased
import Tafla
import Testing

// Generators of table descriptions in the conformance suite's shape: tables
// of every layout, with fields of every type holding values across their
// range, and tables nested in them, from builders and from bytes, to a
// bounded depth.
//
// Tables follow the rules of the format and the builder's documentation: a
// version fits in its slot, a table nested with setTable has no header, and a
// table and the tables in it fit in their offset types.
//
// PropertyBased runs each property, and a failing table shrinks by losing
// fields, vector items and the strings of vectors of strings. Other values
// are drawn from a Draw, which the property's generator seeds, and don't
// shrink.

/// The number of cases of each property, as proptest runs in the Rust tests
let propertyChecks = 256

/// Checks a property of every table that `tables` generates
func checkTables(
    _ tables: Generator<Table, [Table]> = Tables.any,
    sourceLocation: SourceLocation = #_sourceLocation,
    _ body: (Table) throws -> Void
) async {
    await propertyCheck(count: propertyChecks, input: tables, perform: body, sourceLocation: sourceLocation)
}

/// Checks a property of every value that `draw` draws
func checkDrawn<Value: Sendable>(
    sourceLocation: SourceLocation = #_sourceLocation,
    _ draw: @escaping @Sendable (inout Draw) -> Value,
    _ body: (Value) throws -> Void
) async {
    let values = Generator { rng in
        var drawer = Draw(seed: rng.next())
        return draw(&drawer)
    }

    await propertyCheck(count: propertyChecks, input: values, perform: body, sourceLocation: sourceLocation)
}

/// Generators of tables
enum Tables {
    /// Tables of any layout
    static let any = of { draw in draw.layout() }

    /// Tables of the layouts that `layouts` draws, which shrink by losing
    /// fields, vector items and the strings of vectors of strings
    static func of(_ layouts: @escaping @Sendable (inout Draw) -> TableLayout) -> Generator<Table, [Table]> {
        Generator(
            run: { rng in
                var draw = Draw(seed: rng.next())
                return draw.table(layouts: layouts, depth: depth, small: false)
            },
            shrink: { table in shrinks(of: table) }
        )
    }

    /// How deeply tables nest: the root's fields can hold tables whose fields
    /// hold tables
    static let depth = 2
}

/// Smaller tables than `table`, each without one of its fields, of the
/// fields or items of a table nested in it, or of the strings of a vector of
/// strings
func shrinks(of table: Table) -> [Table] {
    var candidates: [Table] = []

    for index in table.fields.indices {
        var smaller = table
        smaller.fields.remove(at: index)
        candidates.append(smaller)
    }

    for (index, field) in table.fields.enumerated() {
        switch field {
        case .table(let nested, let alignment):
            for shrunk in shrinks(of: nested.table) {
                var smaller = table
                smaller.fields[index] = .table(Nested(from: nested.from, table: shrunk), alignment: alignment)
                candidates.append(smaller)
            }
        case .vector(let items):
            for item in items.indices {
                var fewer = items
                fewer.remove(at: item)
                var smaller = table
                smaller.fields[index] = .vector(fewer)
                candidates.append(smaller)
            }
            for (item, nested) in items.enumerated() {
                for shrunk in shrinks(of: nested.table) {
                    var shrunkItems = items
                    shrunkItems[item].table = shrunk
                    var smaller = table
                    smaller.fields[index] = .vector(shrunkItems)
                    candidates.append(smaller)
                }
            }
        case .stringVector(let utf8):
            for string in utf8.indices {
                var fewer = utf8
                fewer.remove(at: string)
                var smaller = table
                smaller.fields[index] = .stringVector(utf8: fewer)
                candidates.append(smaller)
            }
        default:
            break
        }
    }

    return candidates
}

/// Draws values from a seeded SplitMix64 generator, the same values for the
/// same seed on every host
struct Draw: RandomNumberGenerator {
    private var state: UInt64

    init(seed: UInt64) {
        state = seed
    }

    mutating func next() -> UInt64 {
        state &+= 0x9e37_79b9_7f4a_7c15
        var z = state
        z = (z ^ (z >> 30)) &* 0xbf58_476d_1ce4_e5b9
        z = (z ^ (z >> 27)) &* 0x94d0_49bb_1331_11eb
        return z ^ (z >> 31)
    }

    mutating func int(_ range: ClosedRange<Int>) -> Int {
        Int.random(in: range, using: &self)
    }

    mutating func bool() -> Bool {
        Bool.random(using: &self)
    }

    mutating func element<T>(of values: [T]) -> T {
        values[int(0...values.count - 1)]
    }

    /// Draws a value in one of the ways, as often as their weights say
    mutating func weighted<T>(_ choices: [(weight: Int, draw: (inout Draw) -> T)]) -> T {
        var drawn = int(0...choices.reduce(0) { $0 + $1.weight } - 1)
        for choice in choices {
            if drawn < choice.weight {
                return choice.draw(&self)
            }
            drawn -= choice.weight
        }

        preconditionFailure("The weights add up")
    }

    mutating func array<T>(most: Int, _ element: (inout Draw) -> T) -> [T] {
        (0..<int(0...most)).map { _ in element(&self) }
    }

    /// Integers across their range, and their limits and 0 more often than
    /// chance
    mutating func integer<T: FixedWidthInteger>() -> T {
        weighted([
            (1, { $0.element(of: [T.min, 0, T.max]) }),
            (3, { T(truncatingIfNeeded: $0.next()) }),
        ])
    }

    /// The bits of float32s of every class and sign, NaNs with any payload
    /// among them, from any bits, and -0, NaN, the infinities and the limits
    /// more often than chance
    mutating func float32() -> UInt32 {
        let special: [Float] = [
            -0.0, .nan, -.nan, .infinity, -.infinity, -.greatestFiniteMagnitude, .greatestFiniteMagnitude,
            .leastNormalMagnitude, .leastNonzeroMagnitude,
        ]

        return weighted([
            (1, { $0.element(of: special).bitPattern }),
            (3, { $0.integer() }),
        ])
    }

    /// The bits of float64s, drawn as `float32()` draws them
    mutating func float64() -> UInt64 {
        let special: [Double] = [
            -0.0, .nan, -.nan, .infinity, -.infinity, -.greatestFiniteMagnitude, .greatestFiniteMagnitude,
            .leastNormalMagnitude, .leastNonzeroMagnitude,
        ]

        return weighted([
            (1, { $0.element(of: special).bitPattern }),
            (3, { $0.integer() }),
        ])
    }

    /// Strings of up to `most` characters: of ASCII, of characters of two,
    /// three and four bytes in UTF-8, the last outside the Basic Multilingual
    /// Plane, and of U+FEFF, which isn't a byte order mark within a table,
    /// leading or not
    mutating func string(most: Int) -> String {
        var string = ""
        for scalar in array(most: most, { $0.character() }) {
            string.unicodeScalars.append(scalar)
        }
        return string
    }

    mutating func character() -> Unicode.Scalar {
        let value: Int = weighted([
            (3, { $0.int(0x61...0x7a) }),
            (1, { _ in 0xfeff }),
            (1, { $0.int(0x80...0x7ff) }),
            // Of three bytes, but the surrogates, which aren't characters
            (
                1,
                { draw in
                    let value = draw.int(0x800...0xffff - 0x800)
                    return value >= 0xd800 ? value + 0x800 : value
                }
            ),
            (1, { $0.int(0x1_0000...0x10_ffff) }),
        ])

        return Unicode.Scalar(UInt32(value))!
    }

    mutating func offsetType() -> OffsetType {
        element(of: OffsetType.allCases)
    }

    /// A table of any layout, as `Tables.any` generates, without shrinking
    mutating func table() -> Table {
        table(layouts: { $0.layout() }, depth: Tables.depth, small: false)
    }

    /// A table of any header and one of `offsetTypes`, without shrinking
    mutating func table(of offsetTypes: [OffsetType]) -> Table {
        let layouts = { (draw: inout Draw) in
            TableLayout(sized: draw.bool(), version: draw.bool() ? 0 : nil, offsetType: draw.element(of: offsetTypes))
        }

        return table(layouts: layouts, depth: Tables.depth, small: false)
    }

    /// A layout of any header and offset type
    mutating func layout() -> TableLayout {
        TableLayout(sized: bool(), version: bool() ? 0 : nil, offsetType: offsetType())
    }

    /// A table of `layouts`, whose fields hold tables `depth` deep, and hold
    /// few values when `small`
    mutating func table(layouts: (inout Draw) -> TableLayout, depth: Int, small: Bool) -> Table {
        let layout = layouts(&self)
        let small = small || layout.offsetType == .uint8

        var version: UInt32?
        if layout.isVersioned {
            let maxVersion = layout.offsetType.maxValue
            version = weighted([
                (1, { _ in 0 }),
                (1, { _ in maxVersion }),
                (1, { UInt32($0.int(0...Int(maxVersion))) }),
            ])
        }

        let fields = array(most: Limits(small: small).fields) { draw in
            draw.field(offsetType: layout.offsetType, depth: depth, small: small)
        }

        return fitted(Table(layout: layout, version: version, fields: fields))
    }

    /// A field of any type, or a table nested `depth` deep
    mutating func field(offsetType: OffsetType, depth: Int, small: Bool) -> Field {
        if depth == 0 {
            return value(Limits(small: small))
        }

        return weighted([
            (8, { $0.value(Limits(small: small)) }),
            (1, { $0.nestedTable(parent: offsetType, depth: depth - 1, small: small) }),
            (1, { $0.vector(depth: depth - 1, small: small) }),
        ])
    }

    /// A scalar, array, string or struct, which hold values rather than
    /// tables
    mutating func value(_ limits: Limits) -> Field {
        let elements = limits.elements
        var choices: [(weight: Int, draw: (inout Draw) -> Field)] = ScalarType.allCases.map { type in
            (1, { $0.scalar(type) })
        }

        choices += [
            (1, { .bool($0.bool()) }),
            (1, { .int8Array($0.array(most: elements) { $0.integer() }) }),
            (1, { .uint8Array($0.array(most: elements) { $0.integer() }) }),
            (1, { .int16Array($0.array(most: elements) { $0.integer() }) }),
            (1, { .uint16Array($0.array(most: elements) { $0.integer() }) }),
            (1, { .int32Array($0.array(most: elements) { $0.integer() }) }),
            (1, { .uint32Array($0.array(most: elements) { $0.integer() }) }),
            (1, { .int64Array($0.array(most: elements) { $0.integer() }) }),
            (1, { .uint64Array($0.array(most: elements) { $0.integer() }) }),
            (1, { .float32Array(bits: $0.array(most: elements) { $0.float32() }) }),
            (1, { .float64Array(bits: $0.array(most: elements) { $0.float64() }) }),
            (1, { .boolArray($0.array(most: elements) { $0.bool() }) }),
            // Each as likely as three types of number
            (3, { .string($0.string(most: limits.characters)) }),
            (3, { draw in .struct(draw.scalars(draw.structTypes())) }),
            (3, { $0.structArray(most: limits.structs) }),
            // Empty too, and of empty strings
            (3, { draw in .stringVector(draw.array(most: limits.strings) { $0.string(most: limits.characters) }) }),
        ]

        return weighted(choices)
    }

    mutating func scalar(_ type: ScalarType) -> Field {
        switch type {
        case .int8: .int8(integer())
        case .uint8: .uint8(integer())
        case .int16: .int16(integer())
        case .uint16: .uint16(integer())
        case .int32: .int32(integer())
        case .uint32: .uint32(integer())
        case .int64: .int64(integer())
        case .uint64: .uint64(integer())
        case .float32: .float32(bits: float32())
        case .float64: .float64(bits: float64())
        }
    }

    /// The types of the scalars of a struct, of one to four
    mutating func structTypes() -> [ScalarType] {
        (0..<int(1...4)).map { _ in element(of: ScalarType.allCases) }
    }

    mutating func scalars(_ types: [ScalarType]) -> [Field] {
        types.map { scalar($0) }
    }

    /// An array of up to `most` structs of the same scalars, empty too
    mutating func structArray(most: Int) -> Field {
        let types = structTypes()
        return .structArray(types: types, items: array(most: most) { $0.scalars(types) })
    }

    /// A table nested with setTable, which has no header, from its builder or
    /// bytes, aligned by default or explicitly. It's read in place when its
    /// offsets are as wide as its parent's, as they are more often than not.
    mutating func nestedTable(parent: OffsetType, depth: Int, small: Bool) -> Field {
        let offsetType: OffsetType = weighted([
            (2, { _ in parent }),
            (1, { $0.offsetType() }),
        ])
        let alignment = element(of: [nil] + Alignment.allCases)

        return .table(
            nested(layouts: { _ in headerless(offsetType) }, depth: depth, small: small), alignment: alignment)
    }

    /// A vector of tables of any layout, from their builders or bytes
    mutating func vector(depth: Int, small: Bool) -> Field {
        .vector(
            array(most: Limits(small: small).items) { $0.nested(layouts: { $0.layout() }, depth: depth, small: small) })
    }

    mutating func nested(layouts: (inout Draw) -> TableLayout, depth: Int, small: Bool) -> Nested {
        Nested(from: element(of: Source.allCases), table: table(layouts: layouts, depth: depth, small: small))
    }
}

/// The most fields, array elements, characters, structs in an array, vector
/// items and strings in a vector of strings that a table holds. A table of
/// uint8 offsets, or nested in one, holds few enough to fit in its 255 bytes
/// more often than not, and others hold enough for uint16 lengths and offsets
/// past 255.
struct Limits {
    var fields = 8
    var elements = 24
    var characters = 16
    var structs = 6
    var items = 4
    var strings = 6

    init(small: Bool) {
        if small {
            (fields, elements, characters, structs, items) = (5, 6, 6, 2, 2)
            strings = 3
        }
    }
}

/// Keeps the table's first fields that surely fit in its offset type, as many
/// as its header and their longest lengths leave room for. The tables nested
/// in it were fitted to theirs.
func fitted(_ table: Table) -> Table {
    let width = UInt64(table.layout.offsetSize)
    let maxLength = UInt64(table.layout.offsetType.maxValue)

    var fitted = table
    var length = UInt64(table.layout.headerSize)
    for (index, field) in table.fields.enumerated() {
        length += width + longest(field, width: width)
        if length > maxLength {
            fitted.fields.removeSubrange(index...)
            break
        }
    }

    return fitted
}

/// The longest that a table can be, with the most padding that its fields can
/// need
func longestTable(_ table: Table) -> UInt64 {
    let width = UInt64(table.layout.offsetSize)
    return table.fields.reduce(UInt64(table.layout.headerSize)) { $0 + width + longest($1, width: width) }
}

/// The longest that a field can be in a table of `width`-byte offsets, with
/// the most padding that it can need
func longest(_ field: Field, width: UInt64) -> UInt64 {
    func array(_ count: Int, _ size: UInt64) -> UInt64 {
        size - 1 + width + UInt64(count) * size
    }

    switch field {
    case .int8, .uint8, .bool: return 1
    case .int16, .uint16: return 2
    case .int32, .uint32, .float32: return 4
    case .int64, .uint64, .float64: return 8
    case .int8Array(let values): return array(values.count, 1)
    case .uint8Array(let values): return array(values.count, 1)
    case .int16Array(let values): return array(values.count, 2)
    case .uint16Array(let values): return array(values.count, 2)
    case .int32Array(let values): return array(values.count, 4)
    case .uint32Array(let values): return array(values.count, 4)
    case .float32Array(let bits): return array(bits.count, 4)
    case .int64Array(let values): return array(values.count, 8)
    case .uint64Array(let values): return array(values.count, 8)
    case .float64Array(let bits): return array(bits.count, 8)
    case .boolArray(let values): return array(values.count, 1)
    case .string(let utf8): return width + UInt64(utf8.count)
    case .struct(let scalars): return UInt64(structBytes(scalars).count)
    case .structArray(let types, let items):
        return UInt64(structAlignment(types).bytes) - 1 + width + UInt64(items.count * structSize(types))
    case .table(let nested, _): return 7 + longestTable(nested.table)
    case .vector(let items): return items.reduce(width) { $0 + width + longestTable($1.table) }
    case .stringVector(let utf8): return utf8.reduce(width) { $0 + width + UInt64($1.count) }
    }
}
