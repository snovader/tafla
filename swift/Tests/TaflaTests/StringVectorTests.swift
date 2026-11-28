import Tafla
import Testing

// A vector of strings holds its count, the end of each string, counted from
// the vector's position, and then the strings' UTF-8, one after another.
// StringVector reads each string from its ends alone, and Array(_:) reads
// every string, as reading each does, failing where the first string that
// fails on its own fails. Each string is checked as UTF-8 on its own, so an
// end that splits a character fails both strings around it. The properties
// of tables that hold vectors of strings among other fields are checked
// with every other field's, from the conformance suite's descriptions.

@Suite("Vector of strings")
struct StringVectorTests {
    @Test func writesTheCountTheEndOfEachStringAndTheirUTF8() throws {
        var builder = try TableBuilder(fields: 1, layout: TableLayout(offsetType: .uint8))
        try builder.setStringVector(["ab", "", "é"])

        // The offset, the count, the ends from the vector's position, and the
        // strings, without padding
        #expect(try builder.bytes() == [1, 3, 6, 6, 8, 0x61, 0x62, 0xc3, 0xa9])
    }

    @Test(arguments: 0..<8)
    func readsEachStringAndEveryString(byteOffset: Int) throws {
        let strings = ["", "a", "før", "𝄞", "\u{feff}leading", String(repeating: "16 bytes, ASCII ", count: 4)]
        var builder = try TableBuilder(fields: 2, layout: TableLayout(sized: true, offsetType: .uint16))
        try builder.setUInt8(1)
        try builder.setStringVector(strings)

        try withSpan(of: builder.bytes(), atByteOffset: byteOffset) { bytes throws in
            let reader = TableReader(bytes, layout: builder.layout)
            let view = try reader.stringVector(at: reader.rootOffset(1))

            #expect(view.count == strings.count)
            #expect(view.isEmpty == false)
            #expect(view.indices == strings.indices)
            for index in strings.indices {
                #expect(try view[index].utf8.elementsEqual(strings[index].utf8))
            }
            #expect(try Array(view).map { Array($0.utf8) } == strings.map { Array($0.utf8) })
        }
    }

    @Test func readsAnEmptyVector() throws {
        var builder = try TableBuilder(fields: 1)
        try builder.setStringVector([])
        let bytes = try builder.bytes()
        #expect(bytes == [4, 0, 0, 0, 0, 0, 0, 0])

        try withSpan(of: bytes) { bytes throws in
            let reader = TableReader(bytes)
            let view = try reader.stringVector(at: reader.rootOffset(0))

            #expect(view.count == 0)
            #expect(view.isEmpty == true)
            #expect(try Array(view) == [])
        }
    }

    // A vector of uint8 offsets, whose count says more strings than the
    // bytes hold ends for, fails before any string is read
    @Test func rejectsACountPastTheBytes() throws {
        try withSpan(of: [5, 1]) { bytes throws in
            let reader = TableReader(bytes, layout: TableLayout(offsetType: .uint8))
            #expect(throws: DecodeError.outOfRange(position: 1, byteLength: 5, available: 2)) {
                _ = try reader.stringVector(at: 0)
            }
        }
    }

    // A string that ends before the string before it fails, and so does
    // reading every string, though the strings before it read
    @Test func rejectsAStringThatEndsBeforeItStarts() throws {
        try withSpan(of: [2, 4, 3, 0x61]) { bytes throws in
            let view = try TableReader(bytes, layout: TableLayout(offsetType: .uint8)).stringVector(at: 0)

            #expect(try view[0] == "a")
            #expect(throws: DecodeError.itemEndsBeforeStart(start: 4, end: 3)) {
                _ = try view[1]
            }
            #expect(throws: DecodeError.itemEndsBeforeStart(start: 4, end: 3)) {
                _ = try Array(view)
            }
        }
    }

    @Test func rejectsAStringThatEndsPastTheBytes() throws {
        try withSpan(of: [2, 4, 9, 0x61]) { bytes throws in
            let view = try TableReader(bytes, layout: TableLayout(offsetType: .uint8)).stringVector(at: 0)

            #expect(try view[0] == "a")
            #expect(throws: DecodeError.outOfRange(position: 4, byteLength: 5, available: 4)) {
                _ = try view[1]
            }
            #expect(throws: DecodeError.outOfRange(position: 4, byteLength: 5, available: 4)) {
                _ = try Array(view)
            }
        }
    }

    // The second string, between "a" and "b", reads as invalid UTF-8 at its
    // first byte, and reading every string fails at it, though the strings
    // around it read
    @Test(
        arguments: [
            [0x80],  // A continuation byte first
            [0xc0, 0x80],  // An overlong encoding of U+0000
            [0xe0, 0x9f, 0xbf],  // An overlong encoding of U+07FF
            [0xed, 0xa0, 0x80],  // The surrogate U+D800
            [0xf4, 0x90, 0x80, 0x80],  // U+110000, past the last code point
            [0xe2, 0x82],  // A sequence cut short
            [0x61, 0xff],  // A byte that UTF-8 never holds
        ] as [[UInt8]])
    func rejectsAStringThatIsntValidUTF8(_ utf8: [UInt8]) throws {
        // The count, the three ends, and the strings from 4 on
        let ends = [5, 5 + utf8.count, 6 + utf8.count].map(UInt8.init)
        let vector = [3] + ends + [0x61] + utf8 + [0x62]

        try withSpan(of: vector) { bytes throws in
            let view = try TableReader(bytes, layout: TableLayout(offsetType: .uint8)).stringVector(at: 0)

            #expect(try view[0] == "a")
            #expect(throws: DecodeError.invalidUTF8(position: 5)) {
                _ = try view[1]
            }
            #expect(try view[2] == "b")
            #expect(throws: DecodeError.invalidUTF8(position: 5)) {
                _ = try Array(view)
            }
        }
    }

    // Each string is checked on its own, so an end within "é" leaves a lead
    // byte without its continuation and a continuation without its lead,
    // though the two together are valid UTF-8
    @Test func rejectsBothStringsAroundAnEndThatSplitsACharacter() throws {
        try withSpan(of: [2, 4, 5, 0xc3, 0xa9]) { bytes throws in
            let reader = TableReader(bytes, layout: TableLayout(offsetType: .uint8))
            let view = try reader.stringVector(at: 0)

            #expect(throws: DecodeError.invalidUTF8(position: 3)) {
                _ = try view[0]
            }
            #expect(throws: DecodeError.invalidUTF8(position: 4)) {
                _ = try view[1]
            }
            #expect(throws: DecodeError.invalidUTF8(position: 3)) {
                _ = try Array(view)
            }
        }

        try withSpan(of: [1, 4, 0xc3, 0xa9]) { bytes throws in
            let view = try TableReader(bytes, layout: TableLayout(offsetType: .uint8)).stringVector(at: 0)
            #expect(try Array(view) == ["é"])
        }
    }

    // String 1 starts where string 0 ends, at 0, which isn't checked to lie
    // after the ends, as a vector item's start isn't, so it reads the count
    // and the first end as its bytes, while string 0 ends before it starts
    @Test func readsAStringThatStartsWithinTheEndsFromItsEndsAlone() throws {
        try withSpan(of: [2, 0, 2, 0x61]) { bytes throws in
            let view = try TableReader(bytes, layout: TableLayout(offsetType: .uint8)).stringVector(at: 0)

            #expect(throws: DecodeError.itemEndsBeforeStart(start: 3, end: 0)) {
                _ = try view[0]
            }
            #expect(try view[1] == "\u{2}\u{0}")
            #expect(throws: DecodeError.itemEndsBeforeStart(start: 3, end: 0)) {
                _ = try Array(view)
            }
        }
    }

    // A vector of strings is a vector whose items are the strings' UTF-8
    @Test func readsAsAVectorOfItemsOfEachStringsUTF8() throws {
        let strings = ["a", "", "𝄞", "før"]
        var builder = try TableBuilder(fields: 1)
        try builder.setStringVector(strings)

        try withSpan(of: builder.bytes()) { bytes throws in
            let reader = TableReader(bytes)
            let vector = try reader.vector(at: reader.rootOffset(0))

            #expect(
                try vector.indices.map { try vector.item(at: $0).withUnsafeBytes(Array.init) }
                    == strings.map { Array($0.utf8) })
        }
    }

    // The offset, the count and an end take 3 bytes of a table of uint8
    // offsets, which leaves 252 for a string, and a second string takes its
    // end and its bytes, the 2 of "é"
    @Test func rejectsAVectorThatMakesTheTableTooLong() throws {
        let fits = [String(repeating: "a", count: 252)]
        let tooLong = [String(repeating: "a", count: 250), "é"]
        let layout = TableLayout(offsetType: .uint8)

        var builder = try TableBuilder(fields: 1, layout: layout)
        #expect(throws: BuildError.tooLong(index: 0, length: 256, offsetType: .uint8)) {
            try builder.setStringVector(tooLong)
        }
        try builder.setStringVector(fits)
        let bytes = try builder.bytes()
        #expect(bytes.count == 255)

        var sizer = try TableSizer(fields: 1, layout: layout)
        sizer.addStringVector(tooLong)
        #expect(throws: BuildError.tooLong(index: 0, length: 256, offsetType: .uint8)) {
            try sizer.byteLength
        }

        var fitting = try TableSizer(fields: 1, layout: layout)
        fitting.addStringVector(fits)
        #expect(try fitting.bytes { writer in writer.setStringVector(0, fits) } == bytes)
    }

    // Recorded in place, a vector of strings writes what it writes in a
    // builder of its own
    @Test func writesTheSameBytesInATableRecordedInPlace() throws {
        let strings = ["a", "𝄞", ""]
        var nested = try TableBuilder(fields: 1)
        try nested.setStringVector(strings)

        var fromBuilder = try TableBuilder(fields: 1)
        try fromBuilder.setTable(.builder(nested))

        var inPlace = try TableBuilder(fields: 1)
        try inPlace.setTable(fields: 1) { table throws(BuildError) in
            try table.setStringVector(strings)
        }

        #expect(try inPlace.bytes() == fromBuilder.bytes())
    }

    // Reading every string reads what reading each string reads, and fails
    // where the first string that fails on its own fails, in vectors whose
    // bytes are overwritten anywhere, which can split characters, hold
    // bytes that aren't UTF-8, and move the count and the ends
    @Test func readsEveryStringAsEachOrFailsWhereTheFirstFails() async {
        await checkDrawn { draw in
            (
                strings: draw.array(most: 6) { $0.string(most: 8) },
                offsetType: draw.offsetType(),
                overwrites: (0..<draw.int(0...3)).map { _ in (at: draw.int(0...Int(UInt32.max)), byte: draw.byte()) }
            )
        } _: { drawn in
            var builder = try TableBuilder(fields: 1, layout: TableLayout(offsetType: drawn.offsetType))
            guard (try? builder.setStringVector(drawn.strings)) != nil else {
                return
            }
            var bytes = try builder.bytes()
            for overwrite in drawn.overwrites {
                bytes[overwrite.at % bytes.count] = overwrite.byte
            }

            try withSpan(of: bytes) { bytes throws in
                let reader = TableReader(bytes, layout: builder.layout)
                checkReadError {
                    let read = try readStringVector(reader, at: reader.rootOffset(0))
                    if drawn.overwrites.isEmpty {
                        #expect(read.map { Array($0.utf8) } == drawn.strings.map { Array($0.utf8) })
                    }
                }
            }
        }
    }

    // The same, at any position of arbitrary bytes, in every offset type
    @Test func readsEveryStringOfArbitraryBytesAsEachOrFailsWhereTheFirstFails() async {
        await checkDrawn { draw in
            (bytes: draw.bytes(most: 48), position: draw.large(below: 48))
        } _: { drawn in
            withSpan(of: drawn.bytes) { bytes in
                for offsetType in OffsetType.allCases {
                    let reader = TableReader(bytes, layout: TableLayout(offsetType: offsetType))
                    checkReadError { _ = try readStringVector(reader, at: drawn.position) }
                }
            }
        }
    }

    @Test func stopsTheProgramForAnIndexOutsideTheVector() async {
        await #expect(processExitsWith: .failure) {
            try? withSpan(of: [1, 2, 0x61]) { bytes in
                _ = try TableReader(bytes, layout: TableLayout(offsetType: .uint8)).stringVector(at: 0)[1]
            }
        }
        await #expect(processExitsWith: .failure) {
            try? withSpan(of: [1, 2, 0x61]) { bytes in
                _ = try TableReader(bytes, layout: TableLayout(offsetType: .uint8)).stringVector(at: 0)[-1]
            }
        }
    }
}
