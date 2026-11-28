import FlatBuffers
import GeneratedFlatBuffers
import Workloads

/// Builds each message with a new `FlatBufferBuilder` of the default size,
/// which grows by doubling, and each table with the functions that
/// `flatc --swift` generates from `schemas/workloads.fbs`, in
/// `Sources/GeneratedFlatBuffers`. A message is the builder's
/// `sizedByteArray`. Readers verify each message with `getCheckedRoot()`, as
/// the Rust and TypeScript comparisons do, since the bytes may be untrusted.
public struct FlatBuffersCodec: SuiteCodec {
    public typealias Bytes = [UInt8]

    public init() {}

    public var name: String { "flatbuffers" }

    public func serializeRecord(_ record: Record) throws -> [UInt8] {
        var builder = FlatBufferBuilder()
        let name = builder.create(string: record.name)
        let root = fbs_Record.createRecord(&builder, x: record.x, y: record.y, id: record.id, nameOffset: name)
        builder.finish(offset: root)

        return builder.sizedByteArray
    }

    public func deserializeRecord(_ bytes: [UInt8]) throws -> Record {
        var buffer = ByteBuffer(bytes: bytes)
        let record: fbs_Record = try getCheckedRoot(byteBuffer: &buffer)
        guard let name = record.name else {
            throw MissingField(name: "name")
        }

        return Record(x: record.x, y: record.y, id: record.id, name: name)
    }

    public func serializeDocument(_ document: Document) throws -> [UInt8] {
        var builder = FlatBufferBuilder()
        let keys = builder.createVector(document.keys)
        let a = builder.createVector(document.a)
        let b = builder.createVector(document.b)
        let columns = fbs_Columns.createColumns(
            &builder, base: document.base, keysVectorOffset: keys, aVectorOffset: a, bVectorOffset: b)
        let name = builder.create(string: document.name)
        let root = fbs_Document.createDocument(
            &builder, x: document.x, y: document.y, nameOffset: name, columnsOffset: columns)
        builder.finish(offset: root)

        return builder.sizedByteArray
    }

    public func deserializeDocument(_ bytes: [UInt8]) throws -> Document {
        var buffer = ByteBuffer(bytes: bytes)
        let document: fbs_Document = try getCheckedRoot(byteBuffer: &buffer)
        guard let name = document.name else {
            throw MissingField(name: "name")
        }
        guard let columns = document.columns else {
            throw MissingField(name: "columns")
        }

        return Document(
            x: document.x,
            y: document.y,
            name: name,
            base: columns.base,
            keys: try copy(columns.withUnsafePointerToKeys(copy), "keys"),
            a: try copy(columns.withUnsafePointerToA(copy), "a"),
            b: try copy(columns.withUnsafePointerToB(copy), "b")
        )
    }

    public func sumA(_ bytes: [UInt8]) throws -> Double {
        var buffer = ByteBuffer(bytes: bytes)
        let document: fbs_Document = try getCheckedRoot(byteBuffer: &buffer)
        guard let a = document.columns?.a else {
            throw MissingField(name: "columns")
        }

        var sum = 0.0
        for index in 0..<a.count {
            sum += Double(a[index])
        }

        return sum
    }

    public func serializeList(_ list: List) throws -> [UInt8] {
        var builder = FlatBufferBuilder()
        var items: [Offset] = []
        items.reserveCapacity(list.items.count)
        for item in list.items {
            let pair = fbs_Pair.createPair(&builder, first: item.first, second: item.second)
            items.append(fbs_Item.createItem(&builder, value: item.value, pairOffset: pair))
        }
        let vector = builder.createVector(ofOffsets: items)
        let root = fbs_List.createList(&builder, itemsVectorOffset: vector)
        builder.finish(offset: root)

        return builder.sizedByteArray
    }

    public func deserializeList(_ bytes: [UInt8]) throws -> List {
        var buffer = ByteBuffer(bytes: bytes)
        let list: fbs_List = try getCheckedRoot(byteBuffer: &buffer)
        let vector = list.items

        var items: [Item] = []
        items.reserveCapacity(vector.count)
        for item in vector {
            guard let pair = item.pair else {
                throw MissingField(name: "pair")
            }
            items.append(Item(value: item.value, first: pair.first, second: pair.second))
        }

        return List(items: items)
    }

    public func serializeStructs(_ list: List) throws -> [UInt8] {
        var builder = FlatBufferBuilder()
        let triples = list.items.map { item in
            fbs_Triple(value: item.value, first: item.first, second: item.second)
        }
        let vector = builder.createVector(ofStructs: triples)
        let root = fbs_Triples.createTriples(&builder, itemsVectorOffset: vector)
        builder.finish(offset: root)

        return builder.sizedByteArray
    }

    public func deserializeStructs(_ bytes: [UInt8]) throws -> List {
        var buffer = ByteBuffer(bytes: bytes)
        let triples: fbs_Triples = try getCheckedRoot(byteBuffer: &buffer)
        let vector = triples.items

        var items: [Item] = []
        items.reserveCapacity(vector.count)
        for triple in vector {
            items.append(Item(value: triple.value, first: triple.first, second: triple.second))
        }

        return List(items: items)
    }

    public func serializeSample(_ sample: Sample) throws -> [UInt8] {
        var builder = FlatBufferBuilder()
        let root = fbs_Sample.createSample(&builder, key: sample.key, a: sample.a, b: sample.b)
        builder.finish(offset: root)

        return builder.sizedByteArray
    }

    public func deserializeSample(_ bytes: [UInt8]) throws -> Sample {
        var buffer = ByteBuffer(bytes: bytes)
        let sample: fbs_Sample = try getCheckedRoot(byteBuffer: &buffer)

        return Sample(key: sample.key, a: sample.a, b: sample.b)
    }
}

/// Copies a column of `count` elements out of its bytes, in one copy, as
/// tafla's `Array(_:)` of an `ArrayView` copies it.
///
/// `withUnsafePointerTo…()` passes a buffer whose `count` is the elements'
/// count rather than their bytes', so the copy takes the elements' bytes from
/// its start, which the verifier checked lie within the message.
private func copy<Element: BitwiseCopyable>(_ bytes: UnsafeRawBufferPointer, _ count: Int) -> [Element] {
    [Element](unsafeUninitializedCapacity: count) { elements, initialized in
        if let source = bytes.baseAddress, let target = elements.baseAddress {
            UnsafeMutableRawPointer(target).copyMemory(from: source, byteCount: count * MemoryLayout<Element>.stride)
        }
        initialized = count
    }
}

/// The column that `withUnsafePointerTo…()` copied, which is `nil` when the
/// table has no such vector
private func copy<Element>(_ column: [Element]?, _ name: String) throws -> [Element] {
    guard let column else {
        throw MissingField(name: name)
    }

    return column
}
