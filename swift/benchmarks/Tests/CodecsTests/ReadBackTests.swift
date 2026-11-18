// Every codec reads back exactly what it wrote, and the generator makes the
// TypeScript comparison's values, which these were printed from, so that the
// languages' benchmarks time the same work

import Codecs
import Testing
import Workloads

@Test func taflaReadsBackEveryWorkload() throws {
    try checkReadBack(TaflaCodec())
}

@Test func taflaGenReadsBackEveryWorkload() throws {
    try checkReadBack(TaflaGenCodec())
}

@Test func flatBuffersReadsBackEveryWorkload() throws {
    try checkReadBack(FlatBuffersCodec())
}

@Test func protobufReadsBackEveryWorkload() throws {
    try checkReadBack(ProtobufCodec())
}

@Test func messagePackReadsBackEveryWorkload() throws {
    try checkReadBack(MessagePackCodec())
}

@Test func rawSpanReadsBackTheFixedSizeRecord() throws {
    try checkSample(RawSpanCodec())
}

@Test func eachWaySumsTaflasItems() throws {
    try checkItemAccess(TaflaCodec(), name: "tafla")
    try checkItemAccess(TaflaGenCodec(), name: "tafla-gen")
}

@Test func taflaAndTaflaGenWriteTheSameBytes() throws {
    let tafla = TaflaCodec()
    let taflaGen = TaflaGenCodec()
    let list = List(items: listItems)

    #expect(try tafla.serializeRecord(.workload) == taflaGen.serializeRecord(.workload))
    for rows in documentRows {
        let document = Document(rows: rows)
        #expect(try tafla.serializeDocument(document) == taflaGen.serializeDocument(document))
    }
    #expect(try tafla.serializeList(list) == taflaGen.serializeList(list))
    #expect(try tafla.serializeStructs(list) == taflaGen.serializeStructs(list))
    #expect(try tafla.serializeSample(.workload) == taflaGen.serializeSample(.workload))
}

@Test func generatesTheTypeScriptDocumentOf100Rows() {
    let document = Document(rows: 100)

    #expect(document.keys[0] == 14_727_226)
    #expect(document.keys[99] == 10_335_322)
    #expect(document.keys.reduce(0) { $0 + UInt64($1) } == 825_833_448)
    #expect(document.a[0].bitPattern == 0xc221_9b29)
    #expect(document.b[99].bitPattern == 0xc163_647e)
    #expect(hashColumns(document) == 1_493_961_445)
}

@Test func generatesTheTypeScriptDocumentOf100000Rows() {
    let document = Document(rows: 100_000)

    #expect(document.keys[0] == 13_320_945)
    #expect(document.keys[99_999] == 11_621_954)
    #expect(document.keys.reduce(0) { $0 + UInt64($1) } == 839_050_642_924)
    #expect(document.a[0].bitPattern == 0xc1d4_d7f6)
    #expect(document.b[99_999].bitPattern == 0x3e8c_91f5)
    #expect(hashColumns(document) == 1_138_446_770)
}

@Test func generatesTheTypeScriptVectorOfNestedTables() {
    let items = List(items: listItems).items

    #expect(items.count == 100)
    #expect(bits(items[0]) == [0x41d2_acb6, 0xbfce_db15, 0x41e3_2e23])
    #expect(bits(items[1]) == [0x41c1_2b3a, 0x3ea9_43c1, 0x422e_ae51])
    #expect(bits(items[99]) == [0x4193_d915, 0x3f97_9e05, 0x418f_26e1])
}

/// Hashes the float columns' bits, row by row, as the TypeScript script that
/// printed these did with 32-bit integers
func hashColumns(_ document: Document) -> UInt32 {
    var hash: UInt32 = 0
    for row in document.a.indices {
        hash = hash &* 31 &+ document.a[row].bitPattern
        hash = hash &* 31 &+ document.b[row].bitPattern
    }

    return hash
}

func bits(_ item: Item) -> [UInt32] {
    [item.value.bitPattern, item.first.bitPattern, item.second.bitPattern]
}
