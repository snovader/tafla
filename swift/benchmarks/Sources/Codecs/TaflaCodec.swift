import Tafla
import Workloads

/// Writes tables with `TableBuilder`, recording the document's columns and the
/// vector's items, with their pairs, in place among their parent's fields, and
/// each array in bulk, into a table of its exact size.
/// It reads the tables with `TableReader`'s getters, and copies each column
/// out of its `ArrayView` in bulk.
public struct TaflaCodec: SuiteCodec {
    public typealias Bytes = [UInt8]

    /// The document's header: its length and version
    static let documentLayout = TableLayout(sized: true, versioned: true)
    static let documentVersion: UInt32 = 1

    /// The size of a struct of an item's three float32s
    static let tripleSize = 12

    public init() {}

    public var name: String { "tafla" }

    public func serializeRecord(_ record: Record) throws -> [UInt8] {
        var builder = try TableBuilder(fields: 4)
        try builder.setFloat64(record.x)
        try builder.setFloat64(record.y)
        try builder.setInt64(record.id)
        try builder.setString(record.name)

        return try builder.bytes()
    }

    public func deserializeRecord(_ bytes: [UInt8]) throws -> Record {
        let reader = TableReader(bytes.span.bytes)

        return Record(
            x: try reader.float64(at: reader.rootOffset(0)),
            y: try reader.float64(at: reader.rootOffset(1)),
            id: try reader.int64(at: reader.rootOffset(2)),
            name: try reader.string(at: reader.rootOffset(3))
        )
    }

    public func serializeDocument(_ document: Document) throws -> [UInt8] {
        var builder = try TableBuilder(fields: 4, layout: Self.documentLayout, version: Self.documentVersion)
        try builder.setFloat64(document.x)
        try builder.setFloat64(document.y)
        try builder.setString(document.name)
        try builder.setTable(fields: 4) { columns throws(BuildError) in
            try columns.setInt64(document.base)
            try columns.setUInt32Array(document.keys)
            try columns.setFloat32Array(document.a)
            try columns.setFloat32Array(document.b)
        }

        return try builder.bytes()
    }

    public func deserializeDocument(_ bytes: [UInt8]) throws -> Document {
        let reader = TableReader(bytes.span.bytes, layout: Self.documentLayout)
        try checkVersion(reader)
        let columns = try reader.rootOffset(3)

        return Document(
            x: try reader.float64(at: reader.rootOffset(0)),
            y: try reader.float64(at: reader.rootOffset(1)),
            name: try reader.string(at: reader.rootOffset(2)),
            base: try reader.int64(at: reader.fieldOffset(0, inTableAt: columns)),
            keys: Array(try reader.uint32Array(at: reader.fieldOffset(1, inTableAt: columns))),
            a: Array(try reader.float32Array(at: reader.fieldOffset(2, inTableAt: columns))),
            b: Array(try reader.float32Array(at: reader.fieldOffset(3, inTableAt: columns)))
        )
    }

    public func sumA(_ bytes: [UInt8]) throws -> Double {
        let reader = TableReader(bytes.span.bytes, layout: Self.documentLayout)
        try checkVersion(reader)
        let columns = try reader.rootOffset(3)
        let a = try reader.float32Array(at: reader.fieldOffset(2, inTableAt: columns))

        var sum = 0.0
        for index in a.indices {
            sum += Double(a[index])
        }

        return sum
    }

    public func serializeList(_ list: List) throws -> [UInt8] {
        var builder = try TableBuilder(fields: 1)
        try builder.setVector(count: list.items.count, fields: 2) { index, item throws(BuildError) in
            try Self.setItem(list.items[index], on: &item)
        }

        return try builder.bytes()
    }

    /// Sets the fields of an item of the vector, with its pair nested in it
    /// in place
    static func setItem(_ value: Item, on item: inout TableBuilder.Item) throws(BuildError) {
        try item.setFloat32(value.value)
        try item.setTable(fields: 2) { pair throws(BuildError) in
            try pair.setFloat32(value.first)
            try pair.setFloat32(value.second)
        }
    }

    public func deserializeList(_ bytes: [UInt8]) throws -> List {
        let reader = TableReader(bytes.span.bytes)
        let vector = try reader.vector(at: reader.rootOffset(0))

        var items: [Item] = []
        items.reserveCapacity(vector.count)
        for index in vector.indices {
            // In place, at its start, with a reader of its parent's offset type
            items.append(try readItem(TableReader(vector.itemInPlace(at: index))))
        }

        return List(items: items)
    }

    /// Reads an item of the vector, whose table starts at 0
    func readItem(_ reader: TableReader) throws -> Item {
        let pair = try reader.fieldOffset(1, inTableAt: 0)

        return Item(
            value: try reader.float32(at: reader.fieldOffset(0, inTableAt: 0)),
            first: try reader.float32(at: reader.fieldOffset(0, inTableAt: pair)),
            second: try reader.float32(at: reader.fieldOffset(1, inTableAt: pair))
        )
    }

    public func serializeStructs(_ list: List) throws -> [UInt8] {
        let size = Self.tripleSize
        let structs = [UInt8](unsafeUninitializedCapacity: list.items.count * size) { buffer, initialized in
            let bytes = UnsafeMutableRawBufferPointer(buffer)
            for (index, item) in list.items.enumerated() {
                let offset = index * size
                storeFloat(item.value, in: bytes, at: offset)
                storeFloat(item.first, in: bytes, at: offset + 4)
                storeFloat(item.second, in: bytes, at: offset + 8)
            }
            initialized = buffer.count
        }

        var builder = try TableBuilder(fields: 1)
        try builder.setStructArray(structs, size: size, alignment: .four)

        return try builder.bytes()
    }

    /// Stores a float32's little-endian bytes at `offset`
    func storeFloat(_ value: Float, in bytes: UnsafeMutableRawBufferPointer, at offset: Int) {
        bytes.storeBytes(of: value.bitPattern.littleEndian, toByteOffset: offset, as: UInt32.self)
    }

    public func deserializeStructs(_ bytes: [UInt8]) throws -> List {
        let reader = TableReader(bytes.span.bytes)
        let size = Self.tripleSize
        let structs = try reader.structArray(at: reader.rootOffset(0), size: size)
        let count = structs.byteCount / size

        var items: [Item] = []
        items.reserveCapacity(count)
        for index in 0..<count {
            let offset = index * size
            items.append(
                Item(
                    value: Float(littleEndianBytes: structs, at: offset),
                    first: Float(littleEndianBytes: structs, at: offset + 4),
                    second: Float(littleEndianBytes: structs, at: offset + 8)
                )
            )
        }

        return List(items: items)
    }

    /// Writes the fixed-size record with the standard library, as tafla's
    /// README recommends, since it has no header or offsets
    public func serializeSample(_ sample: Sample) throws -> [UInt8] {
        try RawSpanCodec().serializeSample(sample)
    }

    /// Reads the fixed-size record with `TableReader`'s getters, which check
    /// each read's range
    public func deserializeSample(_ bytes: [UInt8]) throws -> Sample {
        let reader = TableReader(bytes.span.bytes)

        return Sample(key: try reader.uint32(at: 0), a: try reader.float32(at: 4), b: try reader.float32(at: 8))
    }

    func checkVersion(_ reader: TableReader) throws {
        let version = try reader.version
        guard version == Self.documentVersion else {
            throw DecodeError.unsupportedVersion(version: version, supported: Self.documentVersion)
        }
    }
}
