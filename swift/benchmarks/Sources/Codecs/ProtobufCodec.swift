import GeneratedProtobuf
import SwiftProtobuf
import Workloads

/// Builds the messages that protoc-gen-swift generates from
/// `schemas/messages.proto`, in `Sources/GeneratedProtobuf`, from the input,
/// and writes them with `serializedBytes()`, which sizes the bytes first, and
/// reads them with `init(serializedBytes:)`. Its messages hold their strings
/// and repeated fields as Swift's values, so building one shares the input's
/// columns, and the decoded columns move into the output. protobuf has no
/// structs, so the array of them is a repeated message of the three floats,
/// which protobuf writes as fixed32s.
public struct ProtobufCodec: SuiteCodec {
    public typealias Bytes = [UInt8]

    public init() {}

    public var name: String { "protobuf" }

    public func serializeRecord(_ record: Record) throws -> [UInt8] {
        var message = Tafla_Benchmarks_Record()
        message.x = record.x
        message.y = record.y
        message.id = record.id
        message.name = record.name

        return try message.serializedBytes()
    }

    public func deserializeRecord(_ bytes: [UInt8]) throws -> Record {
        let message = try Tafla_Benchmarks_Record(serializedBytes: bytes)

        return Record(x: message.x, y: message.y, id: message.id, name: message.name)
    }

    public func serializeDocument(_ document: Document) throws -> [UInt8] {
        var message = Tafla_Benchmarks_Document()
        message.x = document.x
        message.y = document.y
        message.name = document.name
        message.columns.base = document.base
        message.columns.keys = document.keys
        message.columns.a = document.a
        message.columns.b = document.b

        return try message.serializedBytes()
    }

    public func deserializeDocument(_ bytes: [UInt8]) throws -> Document {
        let message = try Tafla_Benchmarks_Document(serializedBytes: bytes)
        guard message.hasColumns else {
            throw MissingField(name: "columns")
        }
        let columns = message.columns

        return Document(
            x: message.x,
            y: message.y,
            name: message.name,
            base: columns.base,
            keys: columns.keys,
            a: columns.a,
            b: columns.b
        )
    }

    /// Decodes the whole document, as protobuf has no access to one field of
    /// the bytes, and sums its column
    public func sumA(_ bytes: [UInt8]) throws -> Double {
        let message = try Tafla_Benchmarks_Document(serializedBytes: bytes)
        guard message.hasColumns else {
            throw MissingField(name: "columns")
        }

        return sumColumn(message.columns.a)
    }

    public func serializeList(_ list: List) throws -> [UInt8] {
        var message = Tafla_Benchmarks_List()
        message.items = list.items.map { item in
            var message = Tafla_Benchmarks_Item()
            message.value = item.value
            message.pair.first = item.first
            message.pair.second = item.second
            return message
        }

        return try message.serializedBytes()
    }

    public func deserializeList(_ bytes: [UInt8]) throws -> List {
        let message = try Tafla_Benchmarks_List(serializedBytes: bytes)

        var items: [Item] = []
        items.reserveCapacity(message.items.count)
        for item in message.items {
            guard item.hasPair else {
                throw MissingField(name: "pair")
            }
            items.append(Item(value: item.value, first: item.pair.first, second: item.pair.second))
        }

        return List(items: items)
    }

    public func serializeStructs(_ list: List) throws -> [UInt8] {
        var message = Tafla_Benchmarks_Triples()
        message.items = list.items.map { item in
            var message = Tafla_Benchmarks_Triple()
            message.value = item.value
            message.first = item.first
            message.second = item.second
            return message
        }

        return try message.serializedBytes()
    }

    public func deserializeStructs(_ bytes: [UInt8]) throws -> List {
        let message = try Tafla_Benchmarks_Triples(serializedBytes: bytes)

        var items: [Item] = []
        items.reserveCapacity(message.items.count)
        for triple in message.items {
            items.append(Item(value: triple.value, first: triple.first, second: triple.second))
        }

        return List(items: items)
    }

    public func serializeSample(_ sample: Sample) throws -> [UInt8] {
        var message = Tafla_Benchmarks_Sample()
        message.key = sample.key
        message.a = sample.a
        message.b = sample.b

        return try message.serializedBytes()
    }

    public func deserializeSample(_ bytes: [UInt8]) throws -> Sample {
        let message = try Tafla_Benchmarks_Sample(serializedBytes: bytes)

        return Sample(key: message.key, a: message.a, b: message.b)
    }
}
