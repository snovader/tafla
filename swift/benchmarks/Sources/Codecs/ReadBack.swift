// The checks that every codec reads back exactly what it wrote, with floats
// compared by their bits, and sums column a to the same bits, which the
// comparison runs before timing a codec, and the tests run on every codec.

import Workloads

/// The error of a codec that doesn't read back what it wrote
public struct ReadBackError: Error, CustomStringConvertible {
    public let description: String
}

/// Checks that `codec` reads back every workload
public func checkReadBack(_ codec: some SuiteCodec) throws {
    try checkRecord(codec)
    try checkDocuments(codec)
    try checkList(codec)
    try checkStructs(codec)
    try checkSample(codec)
}

public func checkRecord(_ codec: some RecordCodec) throws {
    try checkRoundTrip(codec, "the record", Record.workload, codec.serializeRecord, codec.deserializeRecord) {
        $0.isSame(as: $1)
    }
}

/// Checks that `codec` reads back each document, and sums its column `a` to
/// the bits of `sumColumn(_:)`'s sum
public func checkDocuments(_ codec: some DocumentCodec) throws {
    for rows in documentRows {
        let document = Document(rows: rows)
        try checkRoundTrip(codec, "a document", document, codec.serializeDocument, codec.deserializeDocument) {
            $0.isSame(as: $1)
        }

        let sum = try codec.sumA(codec.serializeDocument(document))
        let expected = sumColumn(document.a)
        guard sum.bitPattern == expected.bitPattern else {
            throw ReadBackError(description: "\(codec.name) sums column a to \(sum) rather than \(expected)")
        }
    }
}

public func checkList(_ codec: some ListCodec) throws {
    try checkRoundTrip(codec, "the vector", List(items: listItems), codec.serializeList, codec.deserializeList) {
        $0.isSame(as: $1)
    }
}

public func checkStructs(_ codec: some StructsCodec) throws {
    try checkRoundTrip(codec, "the structs", List(items: listItems), codec.serializeStructs, codec.deserializeStructs) {
        $0.isSame(as: $1)
    }
}

public func checkSample(_ codec: some SampleCodec) throws {
    let sample = Sample.workload
    try checkRoundTrip(codec, "the fixed-size record", sample, codec.serializeSample, codec.deserializeSample) {
        $0.isSame(as: $1)
    }
}

/// Checks that `access`, named `name`, sums the last item and every item of
/// tafla's bytes of the list to the sums of its items, with the same bits
public func checkItemAccess(_ access: some ItemAccess, name: String) throws {
    let list = List(items: listItems)
    let tables = try TaflaCodec().serializeList(list)
    let structs = try TaflaCodec().serializeStructs(list)

    let sums = list.items.map { sumItem($0.value, $0.first, $0.second) }
    guard let lastSum = sums.last else {
        throw NoItems()
    }
    let everySum = sums.reduce(0, +)

    let reads = [
        ("the vector's last item", try access.lastTable(tables), lastSum),
        ("every item of the vector", try access.everyTable(tables), everySum),
        ("the array's last struct", try access.lastStruct(structs), lastSum),
        ("every struct of the array", try access.everyStruct(structs), everySum),
    ]
    for (items, sum, expected) in reads where sum.bitPattern != expected.bitPattern {
        throw ReadBackError(description: "\(name) sums \(items) to \(sum) rather than \(expected)")
    }
}

func checkRoundTrip<C: Codec, Value>(
    _ codec: C,
    _ name: String,
    _ value: Value,
    _ serialize: (Value) throws -> C.Bytes,
    _ deserialize: (C.Bytes) throws -> Value,
    isSame: (Value, Value) -> Bool
) throws {
    let bytes: C.Bytes
    do {
        bytes = try serialize(value)
    } catch {
        throw ReadBackError(description: "\(codec.name) can't write \(name): \(error)")
    }

    let read: Value
    do {
        read = try deserialize(bytes)
    } catch {
        throw ReadBackError(description: "\(codec.name) can't read back \(name): \(error)")
    }

    guard isSame(read, value) else {
        throw ReadBackError(description: "\(codec.name) doesn't read back \(name)")
    }
}
