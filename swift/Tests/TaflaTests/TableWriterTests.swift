import Tafla
import Testing

// A TableSizer counts each table as TableBuilder counts it, padding
// included, and its TableWriter writes the same bytes from the tests' calls
// as the builder does, with nested tables written in place. They fail as the
// builder fails: for a table too long for its offsets, a header too long and
// a version too large. A writer given other fields than the sizer added
// throws notAsSized for fields that fall short of the table, and stops the
// program for a write past it, which it never makes.
//
// The bytes themselves are checked against the format's rules in
// FormatTests.swift and against the conformance suite, which the builder
// writes too.

@Suite("Table writer")
struct TableWriterTests {
    @Test func sizesAnyTableAsTableBuilderCountsItAndWritesTheSameBytes() async {
        await checkTables { table throws in
            let inPlace = table.nestedInPlace()
            guard let built = try? inPlace.bytes() else {
                return
            }

            #expect(try inPlace.sized().byteLength == built.count)
            #expect(try inPlace.write() == built)
        }
    }

    // TableBuilder throws the error of a nested table too long for its
    // offsets, which the sizer counts as part of the root, so only the
    // errors' cases are the same, and the field of the root that fails
    @Test func failsAsTableBuilderFailsForAnyTableTooLongForItsOffsets() async {
        await checkDrawn { draw in
            var table = draw.table(of: [.uint8, .uint16]).nestedInPlace()
            table.fields.append(.uint8Array(Array(repeating: 0, count: draw.int(0...300))))
            return table
        } _: { table in
            let written = Result { () throws(BuildError) in try table.write() }
            let built = Result { () throws(BuildError) in try table.bytes() }

            switch (written, built) {
            case (.success(let written), .success(let built)):
                #expect(written == built)
            case (.failure(.tooLong(let index, _, _)), .failure(.tooLong)):
                #expect(index == failingField(table))
            default:
                Issue.record("\(written) rather than \(built)")
            }
        }
    }

    // The root's fields are counted between them, so the error names the
    // field that made the table too long, with the length after it, as the
    // builder's setter of the field throws it
    @Test func namesTheRootFieldThatMakesTheTableTooLongWithTheLengthAfterIt() throws {
        let table = Table(
            layout: TableLayout(offsetType: .uint8),
            fields: [.uint8Array(Array(repeating: 7, count: 250)), .string("ab"), .uint8(1)]
        )
        // 3 offsets, a count and 250 bytes, and a string of a count and 2
        let tooLong = BuildError.tooLong(index: 1, length: 257, offsetType: .uint8)

        #expect(throws: tooLong) {
            try table.bytes()
        }
        #expect(throws: tooLong) {
            try table.sized().byteLength
        }
        #expect(throws: tooLong) {
            try table.write()
        }

        // A nested table counts as part of the root, which its offset, its
        // own and an array of a count and 252 bytes make too long
        var nested = try TableSizer(fields: 2, layout: TableLayout(offsetType: .uint8))
        nested.addTable(alignment: .one)
        nested.add(nested: Table(layout: .default, fields: [.uint8Array(Array(repeating: 7, count: 252))]))
        nested.addUInt8()
        #expect(throws: BuildError.tooLong(index: 0, length: 2 + 1 + 1 + 252, offsetType: .uint8)) {
            try nested.byteLength
        }
    }

    // The sizer counts a table of 4 GiB without allocating it, and writes
    // none of it
    @Test func rejectsATableOfUInt32OffsetsPast4GiBBeforeAllocatingIt() throws {
        var sizer = try TableSizer(fields: 1)
        sizer.addUInt8Array(count: Int(UInt32.max))

        let tooLong = BuildError.tooLong(index: 0, length: 4 + 4 + UInt64(UInt32.max), offsetType: .uint32)
        #expect(throws: tooLong) {
            try sizer.byteLength
        }
        #expect(throws: tooLong) {
            try sizer.bytes { _ in Issue.record("The table was written") }
        }
    }

    @Test func countsLengthsPastAnyIntWithoutOverflowing() throws {
        var sizer = try TableSizer(fields: 3)
        sizer.addFloat64Array(count: .max)
        sizer.addStructArray(count: .max, size: .max, alignment: .eight)
        sizer.addVector(count: .max)

        #expect(throws: BuildError.tooLong(index: 0, length: .max, offsetType: .uint32)) {
            try sizer.byteLength
        }
    }

    @Test func rejectsAHeaderTooLongForItsOffsetsAsTableBuilderDoes() {
        for (fields, layout) in [
            (256, TableLayout(offsetType: .uint8)),
            (255, TableLayout(sized: true, offsetType: .uint8)),
            (32_767, TableLayout(sized: true, versioned: true, offsetType: .uint16)),
            (Int.max, TableLayout.default),
        ] {
            let version: UInt32? = layout.isVersioned ? 0 : nil
            let error = BuildError.headerTooLong(fields: fields, offsetType: layout.offsetType)

            #expect(throws: error) {
                try TableBuilder(fields: fields, layout: layout, version: version)
            }
            #expect(throws: error) {
                try TableSizer(fields: fields, layout: layout, version: version)
            }
        }
    }

    @Test func rejectsAVersionTooLargeForItsSlotAsTableBuilderDoes() throws {
        let layout = TableLayout(versioned: true, offsetType: .uint8)

        #expect(throws: BuildError.versionTooLarge(version: 256, offsetType: .uint8)) {
            try TableSizer(fields: 0, layout: layout, version: 256)
        }
        #expect(try TableSizer(fields: 0, layout: layout, version: 255).bytes { _ in } == [255])
    }

    // An array of structs of 0 bytes can't be counted, so the sizer throws
    // after a field before it that made the table too long, which the
    // builder's setter of that field throws first
    @Test func rejectsAnArrayOf0ByteStructsAfterAFieldTooLongAsTableBuilderDoes() throws {
        var sizer = try TableSizer(fields: 1)
        sizer.addStructArray(count: 3, size: 0, alignment: .one)
        #expect(throws: BuildError.notWholeStructs(length: 0, size: 0)) {
            try sizer.byteLength
        }

        let tooLong = Table(
            layout: TableLayout(offsetType: .uint8),
            fields: [.uint8Array(Array(repeating: 0, count: 255)), .uint8(0)]
        )
        var after = try TableSizer(fields: 2, layout: tooLong.layout)
        after.add(tooLong.fields[0])
        after.addStructArray(count: 0, size: 0, alignment: .one)

        let error = BuildError.tooLong(index: 0, length: 2 + 1 + 255, offsetType: .uint8)
        #expect(throws: error) {
            try after.byteLength
        }
        #expect(throws: error) {
            try tooLong.bytes()
        }
    }

    @Test func rejectsBytesThatArentWholeStructsAsTableBuilderDoes() throws {
        var sizer = try TableSizer(fields: 1)
        sizer.addStructArray(count: 2, size: 2, alignment: .two)

        #expect(throws: BuildError.notWholeStructs(length: 5, size: 2)) {
            try sizer.bytes { writer in
                writer.setStructArray(0, [1, 2, 3, 4, 5], size: 2, alignment: .two)
            }
        }
        #expect(throws: BuildError.notWholeStructs(length: 2, size: 0)) {
            try sizer.bytes { writer in
                writer.setStructArray(0, [1, 2], size: 0, alignment: .two)
            }
        }
    }

    // A struct is written from its bytes as from its scalars, and an array
    // of them from all their bytes as struct by struct
    @Test func writesStructsFromTheirBytesAsFromTheirScalars() async {
        await checkDrawn { draw in
            (scalars: draw.scalars(draw.structTypes()), array: draw.structArray(most: 4))
        } _: { drawn in
            guard case .structArray(let types, let items) = drawn.array else {
                return
            }
            let table = Table(layout: .default, fields: [.uint8(1), .struct(drawn.scalars), drawn.array])

            let fromBytes = try table.sized().bytes { writer in
                writer.setUInt8(0, 1)
                writer.setStruct(1, structBytes(drawn.scalars))
                writer.setStructArray(
                    2,
                    items.flatMap(structBytes),
                    size: structSize(types),
                    alignment: structAlignment(types)
                )
            }

            #expect(fromBytes == (try table.write()))
            #expect(fromBytes == (try table.bytes()))
        }
    }

    // A struct of no bytes takes none, so the next field starts where it does
    @Test func writesAStructOf0BytesAtTheNextFieldsOffset() throws {
        var sizer = try TableSizer(fields: 2)
        sizer.addStruct(size: 0)
        sizer.addUInt8()
        let written = try sizer.bytes { writer in
            writer.setStruct(0)
            writer.setUInt8(1, 7)
        }

        var builder = try TableBuilder(fields: 2)
        try builder.setStruct([])
        try builder.setUInt8(7)
        #expect(written == (try builder.bytes()))
        #expect(written == [8, 0, 0, 0, 8, 0, 0, 0, 7])
    }

    // Fields that fall short of the table, such as a shorter string than the
    // one sized, or one left out, fill only some of its bytes
    @Test func throwsForFieldsShorterThanThoseSized() throws {
        var sizer = try TableSizer(fields: 2)
        sizer.addString("abc")
        sizer.addUInt8()

        #expect(throws: BuildError.notAsSized(written: 15, length: 16)) {
            try sizer.bytes { writer in
                writer.setString(0, "ab")
                writer.setUInt8(1, 1)
            }
        }
        #expect(throws: BuildError.notAsSized(written: 15, length: 16)) {
            try sizer.bytes { writer in
                writer.setString(0, "abc")
            }
        }
        #expect("\(BuildError.notAsSized(written: 15, length: 16))" == "The fields wrote 15 of the table's 16 bytes")
    }

    // A field of the same length as the one sized writes the table as the
    // builder writes it with that field, since the bytes come from the
    // fields written
    @Test func writesOtherFieldsOfTheSameLengthAsTableBuilderDoes() throws {
        let sized = Table(layout: TableLayout(sized: true), fields: [.string("abc"), .int32Array([1, 2])])
        let written = Table(layout: sized.layout, fields: [.string("xyz"), .float32Array(bits: [3, 4])])

        #expect(try sized.write(as: written) == written.bytes())
    }

    @Test func writesTheExampleOfItsDocumentation() throws {
        var sizer = try TableSizer(fields: 2)
        sizer.addFloat64()
        sizer.addString("Europe/Stockholm")

        let bytes = try sizer.bytes { writer in
            writer.setFloat64(0, 59.3293)
            writer.setString(1, "Europe/Stockholm")
        }

        try withSpan(of: bytes) { bytes throws in
            let reader = TableReader(bytes)
            #expect(try reader.float64(at: reader.rootOffset(0)) == 59.3293)
            #expect(try reader.string(at: reader.rootOffset(1)) == "Europe/Stockholm")
        }
    }

    // Fields longer than those sized would write past the table's bytes,
    // which the writer never does
    @Test func stopsTheProgramForAWritePastTheTable() async {
        await #expect(processExitsWith: .failure) {
            var sizer = try TableSizer(fields: 1)
            sizer.addString("ab")
            _ = try sizer.bytes { writer in
                writer.setString(0, "abc")
            }
        }
        await #expect(processExitsWith: .failure) {
            var sizer = try TableSizer(fields: 1)
            sizer.addVector(count: 1)
            _ = try sizer.bytes { writer in
                let vector = writer.setVector(0, count: 1)
                writer.setItemEnd(vector, item: 1)
            }
        }
        await #expect(processExitsWith: .failure) {
            var sizer = try TableSizer(fields: 1)
            sizer.addUInt8()
            _ = try sizer.bytes { writer in
                writer.setUInt16(0, 1)
            }
        }
    }

    @Test func stopsTheProgramForAMistakeOfTheCaller() async {
        await #expect(processExitsWith: .failure) {
            _ = try? TableSizer(fields: -1)
        }
        await #expect(processExitsWith: .failure) {
            _ = try? TableSizer(fields: 0, layout: TableLayout(versioned: true))
        }
        await #expect(processExitsWith: .failure) {
            _ = try? TableSizer(fields: 0, version: 1)
        }
        await #expect(processExitsWith: .failure) {
            var sizer = try TableSizer(fields: 1)
            sizer.addInt8Array(count: -1)
        }
        await #expect(processExitsWith: .failure) {
            var sizer = try TableSizer(fields: 1)
            sizer.addUInt8()
            _ = try sizer.bytes { writer in
                writer.setUInt8(-1, 1)
            }
        }
        await #expect(processExitsWith: .failure) {
            var sizer = try TableSizer(fields: 1)
            sizer.addVector(count: 0)
            _ = try sizer.bytes { writer in
                _ = writer.setVector(0, count: -1)
            }
        }
    }
}

/// The index of the root field that `TableBuilder` fails to set, whether the
/// root or a table nested in that field is too long
func failingField(_ table: Table) -> Int? {
    guard var builder = try? table.builder() else {
        return nil
    }

    for (index, field) in table.fields.enumerated() {
        do {
            try builder.set(field)
        } catch {
            return index
        }
    }

    return nil
}
