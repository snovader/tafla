import Tafla
import Testing

// Reads and rebuilds the tables of tafla's conformance suite, which the
// TypeScript runtime writes. See conformance/README.md at the root of the
// repository for the format of their descriptions in cases.json.

@Suite("Conformance")
struct ConformanceTests {
    let suite = Suite.shared

    @Test func readsEveryValidTable() throws {
        #expect(suite.valid.count > 0)

        for valid in suite.valid {
            let read = try withSpan(of: valid.bytes) { try readTable($0, valid.table) }
            #expect(read == valid.table, "\(valid.name)")
        }
    }

    // At every byte offset from an aligned address, so that arrays are read
    // in place where they're aligned in memory, and copied where they aren't
    @Test func readsEveryValidTableAtAnyAddress() throws {
        for valid in suite.valid {
            for byteOffset in 0..<8 {
                let read = try withSpan(of: valid.bytes, atByteOffset: byteOffset) {
                    try readTable($0, valid.table)
                }
                #expect(read == valid.table, "\(valid.name) at \(byteOffset)")
            }
        }
    }

    @Test func rebuildsEveryValidTableByteForByte() throws {
        for valid in suite.valid {
            #expect(try valid.table.bytes() == valid.bytes, "\(valid.name)")
        }
    }

    // Reads every equivalent table, whose bytes differ from its valid case's,
    // as the valid case's description, header included
    @Test func readsEquivalentTablesAsTheirValidCase() throws {
        #expect(suite.equivalent.count > 0)

        for equivalent in suite.equivalent {
            let table = suite.table(of: equivalent.validCase)
            let read = try withSpan(of: equivalent.bytes) { try readTable($0, table) }
            #expect(read == table, "\(equivalent.name)")
        }
    }

    // Reads the root fields before the invalid case's field, which must hold
    // their described values, and then the field, which must fail with a
    // decode error
    @Test func readsInvalidTablesUpToTheFieldThatFails() throws {
        #expect(suite.invalid.count > 0)

        for invalid in suite.invalid {
            let table = suite.table(of: invalid.validCase)

            try withSpan(of: invalid.bytes) { bytes throws in
                let reader = TableReader(bytes, layout: table.layout)

                for (index, expected) in table.fields[..<invalid.field].enumerated() {
                    #expect(try readRootField(reader, index, expected) == expected, "\(invalid.name)/\(index)")
                }

                #expect(throws: DecodeError.self, "\(invalid.name)/\(invalid.field)") {
                    try readRootField(reader, invalid.field, table.fields[invalid.field])
                }
            }
        }
    }

    // What a vector of strings adds to the format is how its items are read,
    // so a vector's items are its strings' UTF-8
    @Test func aVectorOfStringsHoldsAVectorOfItemsThatAreEachStringsUTF8() throws {
        guard let valid = suite.valid.first(where: { $0.name == "string-vectors" }),
            case .stringVector(let utf8) = valid.table.fields[0]
        else {
            Issue.record("The first field of string-vectors is its strings")
            return
        }

        let table = Table(layout: valid.table.layout, version: valid.table.version, fields: [.stringVector(utf8: utf8)])
        try withSpan(of: table.bytes()) { bytes throws in
            let reader = TableReader(bytes, layout: table.layout)
            let vector = try reader.vector(at: reader.rootOffset(0))

            #expect(try vector.indices.map { try vector.item(at: $0).withUnsafeBytes(Array.init) } == utf8)
            #expect(try readTable(bytes, table) == table)
        }
    }
}
