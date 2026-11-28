import GeneratedTafla
import Workloads

/// Writes and reads tafla's tables with the code that tafla-gen generates
/// from `workloads.tafla`, in `Sources/GeneratedTafla`. A message's fields
/// write it with a `TableSizer` and a `TableWriter`, in one allocation of its
/// exact length, and its readers are views of the bytes, whose getters read
/// each field when it's asked for.
///
/// It takes the workloads' types, as the other codecs do, so it collects the
/// generated fields of the vector's items and the array's structs into an
/// array in each call, as the protobuf codec collects its messages.
public struct TaflaGenCodec: SuiteCodec {
    public typealias Bytes = [UInt8]

    public init() {}

    public var name: String { "tafla-gen" }

    public func serializeRecord(_ record: Workloads.Record) throws -> [UInt8] {
        try RecordFields(x: record.x, y: record.y, id: record.id, name: record.name).serialize()
    }

    public func deserializeRecord(_ bytes: [UInt8]) throws -> Workloads.Record {
        let record = GeneratedTafla.Record(bytes.span.bytes)

        return Workloads.Record(x: try record.x, y: try record.y, id: try record.id, name: try record.name)
    }

    public func serializeDocument(_ document: Workloads.Document) throws -> [UInt8] {
        let columns = ColumnsFields(base: document.base, keys: document.keys, a: document.a, b: document.b)

        return try DocumentFields(x: document.x, y: document.y, name: document.name, columns: columns).serialize()
    }

    public func deserializeDocument(_ bytes: [UInt8]) throws -> Workloads.Document {
        let document = try GeneratedTafla.Document(bytes.span.bytes)
        let columns = try document.columns

        return Workloads.Document(
            x: try document.x,
            y: try document.y,
            name: try document.name,
            base: try columns.base,
            keys: Array(try columns.keys),
            a: Array(try columns.a),
            b: Array(try columns.b)
        )
    }

    public func sumA(_ bytes: [UInt8]) throws -> Double {
        let a = try GeneratedTafla.Document(bytes.span.bytes).columns.a

        var sum = 0.0
        for index in a.indices {
            sum += Double(a[index])
        }

        return sum
    }

    public func serializeList(_ list: List) throws -> [UInt8] {
        let items = list.items.map { item in
            ItemFields(value: item.value, pair: PairFields(first: item.first, second: item.second))
        }

        return try ItemListFields(items: items).serialize()
    }

    public func deserializeList(_ bytes: [UInt8]) throws -> List {
        let vector = try ItemList(bytes.span.bytes).items

        var items: [Workloads.Item] = []
        items.reserveCapacity(vector.count)
        for index in vector.indices {
            let item = try vector[index]
            let pair = try item.pair
            items.append(Workloads.Item(value: try item.value, first: try pair.first, second: try pair.second))
        }

        return List(items: items)
    }

    public func serializeStructs(_ list: List) throws -> [UInt8] {
        let items = list.items.map { item in
            Triple(value: item.value, first: item.first, second: item.second)
        }

        return try TriplesFields(items: items).serialize()
    }

    public func deserializeStructs(_ bytes: [UInt8]) throws -> List {
        let triples = try Triples(bytes.span.bytes).items

        var items: [Workloads.Item] = []
        items.reserveCapacity(triples.count)
        for index in triples.indices {
            let triple = triples[index]
            items.append(Workloads.Item(value: triple.value, first: triple.first, second: triple.second))
        }

        return List(items: items)
    }

    public func serializeSample(_ sample: Workloads.Sample) throws -> [UInt8] {
        GeneratedTafla.Sample(key: sample.key, a: sample.a, b: sample.b).serialize()
    }

    public func deserializeSample(_ bytes: [UInt8]) throws -> Workloads.Sample {
        let sample = try GeneratedTafla.Sample(bytes.span.bytes)

        return Workloads.Sample(key: sample.key, a: sample.a, b: sample.b)
    }
}
