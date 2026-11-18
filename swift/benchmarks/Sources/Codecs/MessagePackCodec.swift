import Foundation
import SwiftMsgpack
import Workloads

extension Data: EncodedBytes {
    public func truncated(to count: Int) -> Data {
        prefix(count)
    }
}

/// Packs the workloads' own types through `Codable`, with swift-msgpack's
/// `MsgPackEncoder`, which packs a struct as a map with its field names, as
/// msgpackr packs an object, and reads them with `MsgPackDecoder`. The codec
/// keeps an encoder and a decoder, which hold only their options, as an
/// application would keep them.
///
/// The array of structs packs each struct, and the table that holds them, as
/// an array of its fields in order, without their names, as every language's
/// MessagePack codec packs that fixed shape.
public struct MessagePackCodec: SuiteCodec {
    public typealias Bytes = Data

    let encoder = MsgPackEncoder()
    let decoder = MsgPackDecoder()

    public init() {}

    public var name: String { "msgpack" }

    public func serializeRecord(_ record: Record) throws -> Data {
        try encoder.encode(record)
    }

    public func deserializeRecord(_ bytes: Data) throws -> Record {
        try decoder.decode(Record.self, from: bytes)
    }

    public func serializeDocument(_ document: Document) throws -> Data {
        try encoder.encode(document)
    }

    public func deserializeDocument(_ bytes: Data) throws -> Document {
        try decoder.decode(Document.self, from: bytes)
    }

    /// Decodes the whole document, as MessagePack has no access to one field
    /// of the bytes, and sums its column
    public func sumA(_ bytes: Data) throws -> Double {
        sumColumn(try decoder.decode(Document.self, from: bytes).a)
    }

    public func serializeList(_ list: List) throws -> Data {
        try encoder.encode(list)
    }

    public func deserializeList(_ bytes: Data) throws -> List {
        try decoder.decode(List.self, from: bytes)
    }

    public func serializeStructs(_ list: List) throws -> Data {
        try encoder.encode(PackedStructs(items: list.items))
    }

    public func deserializeStructs(_ bytes: Data) throws -> List {
        List(items: try decoder.decode(PackedStructs.self, from: bytes).items)
    }

    public func serializeSample(_ sample: Sample) throws -> Data {
        try encoder.encode(sample)
    }

    public func deserializeSample(_ bytes: Data) throws -> Sample {
        try decoder.decode(Sample.self, from: bytes)
    }
}

/// The list as a table of an array of structs: an array of one field, the
/// items, each an array of its three floats
struct PackedStructs: Codable {
    var items: [Item]

    init(items: [Item]) {
        self.items = items
    }

    func encode(to encoder: any Encoder) throws {
        var table = encoder.unkeyedContainer()
        var structs = table.nestedUnkeyedContainer()
        for item in items {
            var fields = structs.nestedUnkeyedContainer()
            try fields.encode(item.value)
            try fields.encode(item.first)
            try fields.encode(item.second)
        }
    }

    init(from decoder: any Decoder) throws {
        var table = try decoder.unkeyedContainer()
        var structs = try table.nestedUnkeyedContainer()

        items = []
        items.reserveCapacity(structs.count ?? 0)
        while !structs.isAtEnd {
            var fields = try structs.nestedUnkeyedContainer()
            let value = try fields.decode(Float.self)
            let first = try fields.decode(Float.self)
            let second = try fields.decode(Float.self)
            items.append(Item(value: value, first: first, second: second))
        }
    }
}
