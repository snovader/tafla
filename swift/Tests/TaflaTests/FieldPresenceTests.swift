import Tafla
import Testing

// Absent fields, whose offsets are 0 and which take no bytes, and the fields
// that a reader of a later revision of a table finds missing, in tables of
// the conformance suite's model with fields left out. ConformanceTests.swift
// reads the cases of presence in conformance/presence.

@Suite("Field presence")
struct FieldPresenceTests {
    // At every byte offset from an aligned address, with absent fields in
    // nested tables and vector items too
    @Test func readsBackATableWithAbsentFieldsWhereverItsBytesLie() async {
        await checkTables(Tables.leavingFieldsOut) { table throws in
            let bytes = try table.bytes()

            for byteOffset in 0..<8 {
                let read = try withSpan(of: bytes, atByteOffset: byteOffset) { try readTable($0, table) }
                #expect(read == table, "at \(byteOffset)")
            }
        }
    }

    @Test func writesTheOffset0ForAnAbsentFieldAndEveryOtherAfterTheOffsets() async {
        await checkTables(Tables.leavingFieldsOut) { table throws in
            let layout = table.layout
            let firstValue = layout.headerSize + table.fields.count * layout.offsetSize

            try withSpan(of: table.bytes()) { bytes throws in
                let reader = TableReader(bytes, layout: layout)

                for (index, field) in table.fields.enumerated() {
                    let offset = try reader.rootOffset(index)
                    if field == .absent {
                        #expect(offset == 0)
                    } else {
                        #expect(offset >= firstValue)
                    }
                }
            }
        }
    }

    // TableSizer adds nothing for an absent field, and TableWriter writes its
    // offset, as do a builder's items of tables recorded in place
    @Test func writesWithTableSizerAndTableWriterTheBytesThatTableBuilderWrites() async {
        await checkTables(Tables.leavingFieldsOut) { table throws in
            let inPlace = table.nestedInPlace()
            let built = try inPlace.bytes()

            #expect(try inPlace.sized().byteLength == built.count)
            #expect(try inPlace.write() == built)
            #expect(try inPlace.bytesInPlace() == built)
        }
    }

    @Test func countsAnAbsentFieldAsAFieldOfTheBuilder() throws {
        var builder = try TableBuilder(fields: 1)
        try builder.setAbsent()

        #expect(throws: BuildError.tooManyFields(fields: 1)) {
            try builder.setAbsent()
        }
        #expect(try builder.bytes() == [0, 0, 0, 0])

        var nested = try TableBuilder(fields: 2)
        try nested.setUInt8(1)
        try nested.setTable(fields: 2) { table throws(BuildError) in
            try table.setAbsent()
            try table.setAbsent()
        }
        // The nested table's two offsets of 0 follow the root's and its uint8
        #expect(try nested.bytes() == [8, 0, 0, 0, 9, 0, 0, 0, 1] + Array(repeating: 0, count: 8))
    }

    @Test func writesTheOffset0OfAnAbsentFieldWithTableWriter() throws {
        var sizer = try TableSizer(fields: 2, layout: TableLayout(sized: true, offsetType: .uint16))
        sizer.addUInt8()

        let bytes = try sizer.bytes { writer in
            writer.setAbsent(0)
            writer.setUInt8(1, 7)
        }
        #expect(bytes == [7, 0, 0, 0, 6, 0, 7])
    }

    // A table's fields after its own read as missing, from the first field's
    // offset, which every revision holds
    @Test func findsEveryFieldAfterTheTablesOwnMissing() async {
        await checkDrawn { draw in
            (table: draw.tableLeavingFieldsOut(), missing: draw.int(1...4))
        } _: { drawn throws in
            guard !drawn.table.fields.isEmpty else {
                return
            }

            var described = drawn.table
            described.missing = drawn.missing

            #expect(try withSpan(of: drawn.table.bytes()) { try readTable($0, described) } == described)
        }
    }

    // A reader of an earlier revision reads the fields that it knows, and
    // finds those that it doesn't missing, but those that it doesn't know
    // that a table holds
    @Test func readsAnEarlierRevisionsFieldsAndFindsTheOthersMissingUnlessTheyreAbsent() async {
        await checkDrawn { draw in
            (table: draw.tableLeavingFieldsOut(), split: draw.int(0...Int(UInt16.max)))
        } _: { drawn throws in
            let fields = drawn.table.fields
            guard !fields.isEmpty else {
                return
            }

            let known = 1 + drawn.split % fields.count
            var earlier = drawn.table
            earlier.fields = Array(fields[..<known])
            earlier.missing = fields.count - known

            var expected = earlier
            expected.missing = fields[known...].count(where: { $0 == .absent })

            #expect(try withSpan(of: drawn.table.bytes()) { try readTable($0, earlier) } == expected)
        }
    }

    // Overwritten and cut bytes read as values, as other shapes or decode
    // errors, and the same within other bytes as on their own, so that no
    // read reaches past them
    @Test func readsACorruptedTableAsValuesOrDecodeErrorsWithinItsBytes() async {
        await checkDrawn { draw in
            (
                table: draw.tableLeavingFieldsOut(),
                missing: draw.int(0...4),
                overwrites: draw.array(most: 3) { (at: $0.int(0...Int(UInt32.max)), byte: $0.byte()) },
                cut: draw.int(0...Int(UInt32.max))
            )
        } _: { drawn throws in
            var bytes = try drawn.table.bytes()
            guard !bytes.isEmpty else {
                return
            }

            for overwrite in drawn.overwrites {
                bytes[overwrite.at % bytes.count] = overwrite.byte
            }
            bytes.removeLast(drawn.cut % (bytes.count + 1))

            var described = drawn.table
            described.missing = drawn.missing

            let alone = withSpan(of: bytes) { readOutcome($0, described) }
            let within = withSpan(of: [0] + bytes + [0xff]) {
                readOutcome($0.extracting(1..<1 + bytes.count), described)
            }
            #expect(alone == within)
            if case .mismatch(.disagreement, let description) = alone {
                Issue.record("\(description)")
            }
        }
    }
}

/// What reading a table as its description sets it out gives: the table
/// read, the mismatch of bytes that read as another shape, or the decode error
enum ReadOutcome: Equatable {
    case table(Table)
    case mismatch(Mismatch.Kind, String)
    case error(DecodeError)
}

/// Reads a table as its description sets it out, and returns what it reads,
/// failing the test for any other error than a mismatch or a decode error
func readOutcome(_ bytes: RawSpan, _ table: Table) -> ReadOutcome {
    do {
        return .table(try readTable(bytes, table))
    } catch let error as DecodeError {
        return .error(error)
    } catch let mismatch as Mismatch {
        return .mismatch(mismatch.kind, mismatch.description)
    } catch {
        Issue.record("\(error)")
        return .mismatch(.disagreement, "\(error)")
    }
}
