// Ways to read the items of tafla's vector of nested tables and of its array
// of structs, which tafla and tafla-gen write alike.
//
// Each reads the last item alone, which a reader can find without reading
// the others, and every item. It sums the three float32s of the items it
// reads as `Double`s, as the TypeScript comparison adds them, so that it
// returns a number rather than collecting items of its own.

import GeneratedTafla
import Tafla
import Workloads

/// A way to read tafla's items
public protocol ItemAccess {
    /// Sums the last item of the vector of nested tables
    func lastTable(_ bytes: [UInt8]) throws -> Double
    /// Sums every item of the vector of nested tables, in order
    func everyTable(_ bytes: [UInt8]) throws -> Double
    /// Sums the last struct of the array
    func lastStruct(_ bytes: [UInt8]) throws -> Double
    /// Sums every struct of the array, in order
    func everyStruct(_ bytes: [UInt8]) throws -> Double
}

/// The sum of an item's three float32s, in the order that every way adds them
@inlinable
public func sumItem(_ value: Float, _ first: Float, _ second: Float) -> Double {
    Double(value) + Double(first) + Double(second)
}

/// The error of an access to the last item of an empty vector or array
public struct NoItems: Error, CustomStringConvertible {
    public var description: String { "The list has no items" }
}

/// Reads with `TableReader`'s getters by hand: each item in place, from
/// `Vector.itemInPlace(at:)`, and the structs from `structArray(at:size:)`'s
/// bytes, with `Float(littleEndianBytes:at:)`
extension TaflaCodec: ItemAccess {
    public func lastTable(_ bytes: [UInt8]) throws -> Double {
        let reader = TableReader(bytes.span.bytes)
        let vector = try reader.vector(at: reader.rootOffset(0))
        guard let last = vector.indices.last else {
            throw NoItems()
        }

        return try sumTable(TableReader(vector.itemInPlace(at: last)))
    }

    public func everyTable(_ bytes: [UInt8]) throws -> Double {
        let reader = TableReader(bytes.span.bytes)
        let vector = try reader.vector(at: reader.rootOffset(0))

        var sum = 0.0
        for index in vector.indices {
            sum += try sumTable(TableReader(vector.itemInPlace(at: index)))
        }

        return sum
    }

    /// Sums an item of the vector, whose table starts at 0
    func sumTable(_ reader: TableReader) throws -> Double {
        let pair = try reader.fieldOffset(1, inTableAt: 0)

        return sumItem(
            try reader.float32(at: reader.fieldOffset(0, inTableAt: 0)),
            try reader.float32(at: reader.fieldOffset(0, inTableAt: pair)),
            try reader.float32(at: reader.fieldOffset(1, inTableAt: pair))
        )
    }

    public func lastStruct(_ bytes: [UInt8]) throws -> Double {
        let reader = TableReader(bytes.span.bytes)
        let structs = try reader.structArray(at: reader.rootOffset(0), size: Self.tripleSize)
        let count = structs.byteCount / Self.tripleSize
        guard count > 0 else {
            throw NoItems()
        }

        return sumStruct(structs, at: (count - 1) * Self.tripleSize)
    }

    public func everyStruct(_ bytes: [UInt8]) throws -> Double {
        let reader = TableReader(bytes.span.bytes)
        let structs = try reader.structArray(at: reader.rootOffset(0), size: Self.tripleSize)

        var sum = 0.0
        for index in 0..<structs.byteCount / Self.tripleSize {
            sum += sumStruct(structs, at: index * Self.tripleSize)
        }

        return sum
    }

    /// Sums the struct at `offset` in the array's bytes
    func sumStruct(_ structs: RawSpan, at offset: Int) -> Double {
        sumItem(
            Float(littleEndianBytes: structs, at: offset),
            Float(littleEndianBytes: structs, at: offset + 4),
            Float(littleEndianBytes: structs, at: offset + 8)
        )
    }
}

/// Reads with the generated getters, `items`, which return views of the
/// vector and of the array, whose subscripts read one item
extension TaflaGenCodec: ItemAccess {
    public func lastTable(_ bytes: [UInt8]) throws -> Double {
        let items = try ItemList(bytes.span.bytes).items
        guard let last = items.indices.last else {
            throw NoItems()
        }

        return try sumTable(items[last])
    }

    public func everyTable(_ bytes: [UInt8]) throws -> Double {
        let items = try ItemList(bytes.span.bytes).items

        var sum = 0.0
        for index in items.indices {
            sum += try sumTable(items[index])
        }

        return sum
    }

    func sumTable(_ item: GeneratedTafla.Item) throws -> Double {
        let pair = try item.pair

        return sumItem(try item.value, try pair.first, try pair.second)
    }

    public func lastStruct(_ bytes: [UInt8]) throws -> Double {
        let items = try Triples(bytes.span.bytes).items
        guard let last = items.indices.last else {
            throw NoItems()
        }

        return sumStruct(items[last])
    }

    public func everyStruct(_ bytes: [UInt8]) throws -> Double {
        let items = try Triples(bytes.span.bytes).items

        var sum = 0.0
        for index in items.indices {
            sum += sumStruct(items[index])
        }

        return sum
    }

    func sumStruct(_ triple: Triple) -> Double {
        sumItem(triple.value, triple.first, triple.second)
    }
}
