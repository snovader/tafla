// Generic table shapes, with the TypeScript comparison's values.
//
// The shapes are sized to bracket how tafla is used: small tables of scalars
// and a string, documents whose header nests a table of numeric columns,
// vectors of small nested tables, arrays of structs, and fixed-size records.
// The values come from a port of the TypeScript comparison's seeded
// generator, so that the languages' numbers compare.
//
// Deserialized values own their strings and columns, since Swift's views of
// bytes, such as tafla's `ArrayView`, can't outlive the call that reads them.
// `isSame(as:)` compares floats by their bits, so that a codec reads back
// exactly what it wrote.

/// The rows of the documents: a small one and a large one
public let documentRows = [100, 100_000]

/// The items of the vector of nested tables and of the array of structs
public let listItems = 100

/// A small table: two float64s, an int64 and a 16-byte string.
///
/// Its coding keys name its fields for MessagePack, as serde names them in
/// Rust.
public struct Record: Codable, Sendable {
    public var x: Double
    public var y: Double
    public var id: Int64
    public var name: String

    public init(x: Double, y: Double, id: Int64, name: String) {
        self.x = x
        self.y = y
        self.id = id
        self.name = name
    }

    /// The record that every codec writes
    public static let workload = Record(x: 59.3293, y: 18.0686, id: -631_152_000, name: "tafla-benchmarks")

    /// Whether the records are the same, with floats by their bits
    public func isSame(as other: Record) -> Bool {
        sameFloat(x, other.x) && sameFloat(y, other.y) && id == other.id && name == other.name
    }
}

/// A sized, versioned table of two float64s and a string, with a nested table
/// of an int64 and three columns of the same length
public struct Document: Codable, Sendable {
    public var x: Double
    public var y: Double
    public var name: String
    public var base: Int64
    public var keys: [UInt32]
    public var a: [Float]
    public var b: [Float]

    public init(x: Double, y: Double, name: String, base: Int64, keys: [UInt32], a: [Float], b: [Float]) {
        self.x = x
        self.y = y
        self.name = name
        self.base = base
        self.keys = keys
        self.a = a
        self.b = b
    }

    /// The document of `rows` rows
    public init(rows: Int) {
        var random = Xorshift(seed: rows)

        // Keys below 2^24 take 4 bytes in protobuf's varints, like the floats
        var keys: [UInt32] = []
        var a: [Float] = []
        var b: [Float] = []
        keys.reserveCapacity(rows)
        a.reserveCapacity(rows)
        b.reserveCapacity(rows)
        for _ in 0..<rows {
            keys.append(UInt32((random.next() * 16_777_216).rounded(.down)))
            a.append(Float((random.next() - 0.5) * 100))
            b.append(Float((random.next() - 0.5) * 100))
        }

        let record = Record.workload
        self.init(x: record.x, y: record.y, name: record.name, base: record.id, keys: keys, a: a, b: b)
    }

    /// Whether the documents are the same, with floats by their bits
    public func isSame(as other: Document) -> Bool {
        sameFloat(x, other.x) && sameFloat(y, other.y) && name == other.name && base == other.base
            && keys == other.keys && a.elementsEqual(other.a, by: sameFloat)
            && b.elementsEqual(other.b, by: sameFloat)
    }
}

/// A table of a float32 and a nested table of two float32s, `first` and
/// `second`, or a struct of the three float32s
public struct Item: Codable, Sendable {
    public var value: Float
    public var first: Float
    public var second: Float

    public init(value: Float, first: Float, second: Float) {
        self.value = value
        self.first = first
        self.second = second
    }

    /// Whether the items are the same, with floats by their bits
    public func isSame(as other: Item) -> Bool {
        sameFloat(value, other.value) && sameFloat(first, other.first) && sameFloat(second, other.second)
    }
}

/// A table holding a vector of item tables, or an array of item structs
public struct List: Codable, Sendable {
    public var items: [Item]

    public init(items: [Item]) {
        self.items = items
    }

    /// The list of `items` items
    public init(items count: Int) {
        var random = Xorshift(seed: count)

        // Float32 values, so that every format reads them back exactly.
        // Fields are evaluated in the order they're written, as in
        // JavaScript.
        var items: [Item] = []
        items.reserveCapacity(count)
        for _ in 0..<count {
            let value = Float(random.next() * 30)
            let first = Float(random.next() * 4 - 2)
            let second = Float(random.next() * 50)
            items.append(Item(value: value, first: first, second: second))
        }

        self.init(items: items)
    }

    /// Whether the lists are the same, with floats by their bits
    public func isSame(as other: List) -> Bool {
        items.elementsEqual(other.items) { $0.isSame(as: $1) }
    }
}

/// A fixed-size record of a uint32 and two float32s
public struct Sample: Codable, Sendable {
    public var key: UInt32
    public var a: Float
    public var b: Float

    public init(key: UInt32, a: Float, b: Float) {
        self.key = key
        self.a = a
        self.b = b
    }

    /// The fixed-size record that every codec writes
    public static let workload = Sample(key: 2_743_826_400, a: 21.75, b: 3.25)

    /// Whether the samples are the same, with floats by their bits
    public func isSame(as other: Sample) -> Bool {
        key == other.key && sameFloat(a, other.a) && sameFloat(b, other.b)
    }
}

/// Returns the sum of `values` in their order, in a `Double`, as the
/// TypeScript comparison adds them to a number. Every codec sums its column in
/// this order, so that their sums have the same bits.
@inlinable
public func sumColumn(_ values: some Sequence<Float>) -> Double {
    var sum = 0.0
    for value in values {
        sum += Double(value)
    }

    return sum
}

/// The TypeScript generator's xorshift, in [0, 1), so that every run sees the
/// same values. Its state is a `UInt32`, whose bits JavaScript's 32-bit shifts
/// keep the same.
struct Xorshift {
    var state: UInt32

    /// Seeds a generator, wrapping the seed to 32 bits, as JavaScript's `^`
    /// wraps its operands
    init(seed: Int) {
        state = 0x2545_f491 ^ UInt32(truncatingIfNeeded: seed)
    }

    mutating func next() -> Double {
        state ^= state << 13
        state ^= state >> 17
        state ^= state << 5

        return Double(state) / 4_294_967_296
    }
}

func sameFloat(_ a: Float, _ b: Float) -> Bool {
    a.bitPattern == b.bitPattern
}

func sameFloat(_ a: Double, _ b: Double) -> Bool {
    a.bitPattern == b.bitPattern
}
