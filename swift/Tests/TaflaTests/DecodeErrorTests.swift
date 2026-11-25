import Tafla
import Testing

// Readers decode untrusted bytes: any bytes, and valid tables cut short or
// overwritten, read as values or fail with a DecodeError, and never stop the
// program. The examples document the errors.

@Suite("Decode errors")
struct DecodeErrorTests {
    // A field that lies before the cut reads the value that the table was
    // built with, and any other fails with outOfRange
    @Test func truncatedTablesReadTheirValuesOrOutOfRange() async {
        await checkDrawn { draw in
            (
                table: draw.table(),
                cuts: draw.array(most: 16) { $0.int(0...Int(UInt32.max)) }
            )
        } _: { drawn in
            let built = try drawn.table.bytes()
            guard !built.isEmpty else {
                return
            }

            for cut in drawn.cuts.map({ $0 % built.count }) {
                // A copy, so that no read can reach the bytes after the cut
                try withSpan(of: Array(built[..<cut])) { bytes throws in
                    let reader = TableReader(bytes, layout: drawn.table.layout)

                    for (index, field) in drawn.table.fields.enumerated() {
                        do {
                            #expect(try readRootField(reader, index, field) == field, "field \(index) cut at \(cut)")
                        } catch let error as DecodeError {
                            guard case .outOfRange = error else {
                                Issue.record("Field \(index) cut at \(cut): \(error)")
                                continue
                            }
                        }
                    }
                }
            }
        }
    }

    // Overwritten bytes can hold other values, offsets, counts and item ends,
    // which read as other values, as vectors of another number of items or
    // as a size other than the table's length, or fail with a DecodeError.
    // Finding a vector item in place finds what finding it as a table of its
    // own finds, whatever they hold.
    @Test func overwrittenTablesReadValuesOrDecodeErrors() async {
        await checkDrawn { draw in
            (
                table: draw.table(),
                overwrites: (0..<draw.int(1...4)).map { _ in (at: draw.int(0...Int(UInt32.max)), byte: draw.byte()) }
            )
        } _: { drawn in
            var built = try drawn.table.bytes()
            guard !built.isEmpty else {
                return
            }

            for overwrite in drawn.overwrites {
                built[overwrite.at % built.count] = overwrite.byte
            }

            try withSpan(of: built) { bytes throws in
                checkReadError { _ = try readTable(bytes, drawn.table) }

                let reader = TableReader(bytes, layout: drawn.table.layout)
                for (index, field) in drawn.table.fields.enumerated() {
                    checkReadError { _ = try readRootField(reader, index, field) }
                }
            }
        }
    }

    // Every getter, at the root offsets of a table of any layout, in the
    // tables nested in it and in its vectors, and at any position and field
    // offset, and structs of any size, returns a value or a DecodeError.
    // Bytes read the same within larger bytes as on their own, at any
    // address, and an outOfRange error describes a read that would leave
    // them.
    @Test func arbitraryBytesReadAsValuesOrDecodeErrors() async {
        await checkDrawn { draw in
            ArbitraryReads(
                bytes: draw.bytes(most: 64),
                before: draw.bytes(most: 8),
                after: draw.bytes(most: 8),
                byteOffset: draw.int(0...7),
                table: draw.table(),
                position: draw.large(below: 80),
                index: draw.count(below: 12),
                size: draw.count(below: 16)
            )
        } _: { drawn in
            let read = withSpan(of: drawn.bytes, atByteOffset: drawn.byteOffset) { drawn.reads(of: $0) }
            let readWithin = withSpan(of: drawn.before + drawn.bytes + drawn.after) { bytes in
                drawn.reads(of: bytes.extracting(drawn.before.count..<drawn.before.count + drawn.bytes.count))
            }
            #expect(read == readWithin)

            for result in read {
                switch result {
                case .error(.outOfRange(let position, let byteLength, let available)):
                    let end = Double(position) + Double(byteLength)
                    #expect(position < 0 || end > Double(available), "\(result)")
                    #expect(available <= drawn.bytes.count, "\(result)")
                case .mismatch(.disagreement, let description):
                    Issue.record("\(description)")
                default:
                    break
                }
            }
        }
    }

    // rootOffset and fieldOffset read the offset at a field's slot, as wide
    // as the offset type, after the root's header or from the nested table's
    // position, in every offset type, and fail at a negative table's
    // position, or at a slot outside the bytes
    @Test func fieldOffsetsFailAtTheirSlotInEveryOffsetType() async {
        await checkDrawn { draw in
            (bytes: draw.bytes(most: 64), table: draw.large(below: 80), index: draw.int(0...20))
        } _: { drawn in
            let count = drawn.bytes.count

            for offsetType in OffsetType.allCases {
                let size = offsetType.size

                func expected(at slot: Int) -> Result<Int, DecodeError> {
                    guard slot >= 0, slot <= count - size else {
                        return .failure(.outOfRange(position: slot, byteLength: UInt64(size), available: count))
                    }
                    return .success(slotValue(drawn.bytes, at: slot, size))
                }

                for layout in layouts(of: offsetType) {
                    let slot = layout.headerSize + drawn.index * size
                    let read = withSpan(of: drawn.bytes) { bytes in
                        Result { () throws(DecodeError) in
                            try TableReader(bytes, layout: layout).rootOffset(drawn.index)
                        }
                    }
                    #expect(read == expected(at: slot), "\(layout)")
                }

                let read = withSpan(of: drawn.bytes) { bytes in
                    Result { () throws(DecodeError) in
                        try TableReader(bytes, layout: TableLayout(offsetType: offsetType))
                            .fieldOffset(drawn.index, inTableAt: drawn.table)
                    }
                }

                let (slot, overflow) = drawn.table.addingReportingOverflow(drawn.index * size)
                let expectedPosition: Result<Int, DecodeError> =
                    if drawn.table < 0 {
                        .failure(.outOfRange(position: drawn.table, byteLength: UInt64(size), available: count))
                    } else {
                        expected(at: overflow ? .max : slot).map { drawn.table + $0 }
                    }
                #expect(read == expectedPosition, "\(offsetType)")
            }
        }
    }

    @Test func throwsOutOfRangeWithTheReadAndTheBytes() throws {
        let bytes: [UInt8] = [4, 0, 0, 0, 0xff, 1, 0]

        try withSpan(of: bytes) { bytes throws in
            let reader = TableReader(bytes)

            // An offset past the end, and a value
            #expect(throws: DecodeError.outOfRange(position: 4, byteLength: 4, available: 7)) {
                _ = try reader.rootOffset(1)
            }
            #expect(throws: DecodeError.outOfRange(position: 4, byteLength: 8, available: 7)) {
                _ = try reader.float64(at: reader.rootOffset(0))
            }
            #expect(throws: DecodeError.outOfRange(position: 8, byteLength: 1, available: 7)) {
                _ = try reader.uint8(at: 8)
            }
            #expect(throws: DecodeError.outOfRange(position: -1, byteLength: 1, available: 7)) {
                _ = try reader.int8(at: -1)
            }

            // The size in a sized header, and the version after it
            let sized = TableReader(
                bytes.extracting(4..<7), layout: TableLayout(sized: true, version: 0, offsetType: .uint16))
            #expect(try sized.size == 0x1ff)
            #expect(throws: DecodeError.outOfRange(position: 2, byteLength: 2, available: 3)) {
                _ = try sized.version
            }

            // A count of 0x1ff elements, of 2 bytes each, after the count
            let short = TableReader(bytes.extracting(4..<7), layout: TableLayout(offsetType: .uint16))
            #expect(throws: DecodeError.outOfRange(position: 2, byteLength: 0x3fe, available: 3)) {
                _ = try short.uint16Array(at: 0)
            }
            #expect(throws: DecodeError.outOfRange(position: 2, byteLength: 0x1ff, available: 3)) {
                _ = try short.string(at: 0)
            }
            #expect(throws: DecodeError.outOfRange(position: 0, byteLength: 4, available: 3)) {
                _ = try short.struct(at: 0, size: 4)
            }
            #expect(throws: DecodeError.outOfRange(position: 2, byteLength: 0x1ff * 3, available: 3)) {
                _ = try short.structArray(at: 0, size: 3)
            }

            // A vector of 0x1ff items, whose ends lie past the bytes
            #expect(throws: DecodeError.outOfRange(position: 2, byteLength: 0x3fe, available: 3)) {
                _ = try short.vector(at: 0)
            }
        }
    }

    // A struct can be as large as an Int, so the byte length of an array of
    // them saturates rather than overflows
    @Test func rejectsAnArrayWhoseByteLengthOverflows() throws {
        try withSpan(of: [2, 0, 0, 0, 3, 0, 0, 0]) { bytes throws in
            let reader = TableReader(bytes)

            #expect(throws: DecodeError.outOfRange(position: 4, byteLength: 2 * UInt64(Int.max), available: 8)) {
                _ = try reader.structArray(at: 0, size: .max)
            }
            #expect(throws: DecodeError.outOfRange(position: 8, byteLength: .max, available: 8)) {
                _ = try reader.structArray(at: 4, size: .max)
            }
        }
    }

    // A vector of uint8 offsets: its count, the ends of its items, counted
    // from the vector's position, and the items
    @Test func rejectsAVectorItemThatEndsBeforeItStartsOrPastTheBytes() throws {
        let vector: [UInt8] = [3, 5, 4, 9, 0xaa]

        try withSpan(of: vector) { bytes throws in
            let read = try TableReader(bytes, layout: TableLayout(offsetType: .uint8)).vector(at: 0)
            #expect(read.count == 3)

            #expect(try read.item(at: 0).byteCount == 1)
            #expect(try read.itemInPlace(at: 0).byteCount == 1)
            #expect(throws: DecodeError.itemEndsBeforeStart(start: 5, end: 4)) {
                _ = try read.item(at: 1)
            }
            #expect(throws: DecodeError.itemEndsBeforeStart(start: 5, end: 4)) {
                _ = try read.itemInPlace(at: 1)
            }
            #expect(throws: DecodeError.outOfRange(position: 4, byteLength: 5, available: 5)) {
                _ = try read.item(at: 2)
            }
            #expect(throws: DecodeError.outOfRange(position: 4, byteLength: 5, available: 5)) {
                _ = try read.itemInPlace(at: 2)
            }
        }
    }

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
    func rejectsInvalidUTF8(_ utf8: [UInt8]) throws {
        var builder = try TableBuilder(fields: 1)
        try builder.setUInt8Array(utf8)

        try withSpan(of: builder.bytes()) { bytes throws in
            let reader = TableReader(bytes)
            #expect(throws: DecodeError.invalidUTF8(position: 4)) {
                _ = try reader.string(at: reader.rootOffset(0))
            }
        }
    }

    @Test func describesEachError() {
        #expect(
            "\(DecodeError.outOfRange(position: 4, byteLength: 8, available: 7))"
                == "Read exceeds the supplied buffer: 8 bytes at 4 of 7")
        #expect("\(DecodeError.invalidUTF8(position: 4))" == "The string at 4 isn't valid UTF-8")
        #expect(
            "\(DecodeError.itemEndsBeforeStart(start: 5, end: 4))"
                == "The vector item at 5 ends before it starts, at 4")
        #expect(
            "\(DecodeError.unsupportedVersion(version: 2, supported: 1))"
                == "Unsupported format version 2; the reader reads version 1")
    }

    @Test func stopsTheProgramForAMistakeOfTheCaller() async {
        await #expect(processExitsWith: .failure) {
            _ = try? TableReader(RawSpan()).rootOffset(-1)
        }
        await #expect(processExitsWith: .failure) {
            _ = try? TableReader(RawSpan()).version
        }
        await #expect(processExitsWith: .failure) {
            _ = try? TableReader(RawSpan()).struct(at: 0, size: -1)
        }
        await #expect(processExitsWith: .failure) {
            try? withSpan(of: [1, 7]) { bytes in
                _ = try TableReader(bytes, layout: TableLayout(offsetType: .uint8)).uint8Array(at: 0)[1]
            }
        }
        await #expect(processExitsWith: .failure) {
            try? withSpan(of: [1, 2]) { bytes in
                _ = try TableReader(bytes, layout: TableLayout(offsetType: .uint8)).vector(at: 0).item(at: 1)
            }
        }
    }
}

/// Checks that a read either succeeds or fails with a decode error, or with
/// a mismatch that bytes can cause, of a size or a number of items
func checkReadError(sourceLocation: SourceLocation = #_sourceLocation, _ read: () throws -> Void) {
    do {
        try read()
    } catch is DecodeError {
    } catch let mismatch as Mismatch where mismatch.kind != .disagreement {
    } catch {
        Issue.record("\(error)", sourceLocation: sourceLocation)
    }
}

/// The layouts of every header of an offset type
func layouts(of offsetType: OffsetType) -> [TableLayout] {
    [(false, false), (true, false), (false, true), (true, true)].map { sized, versioned in
        TableLayout(sized: sized, version: versioned ? 0 : nil, offsetType: offsetType)
    }
}

func slotValue(_ bytes: [UInt8], at position: Int, _ width: Int) -> Int {
    (0..<width).reduce(0) { $0 | Int(bytes[position + $1]) << (8 * $1) }
}

/// What reading arbitrary bytes returns: a value, a position, a count, the
/// range of bytes that a struct, an array of structs or a vector item takes,
/// an error, or a mismatch
enum Read: Hashable {
    case value(Field)
    case table(Table)
    case position(Int)
    case count(Int)
    case bytes(Range<Int>)
    case error(DecodeError)
    case mismatch(Mismatch.Kind, String)
}

/// Arbitrary bytes, the bytes around them, and what to read from them
struct ArbitraryReads: Sendable {
    var bytes: [UInt8]
    var before: [UInt8]
    var after: [UInt8]
    var byteOffset: Int
    var table: Table
    var position: Int
    var index: Int
    var size: Int

    /// Reads `bytes` with every getter, as the table, at the root offsets of
    /// every layout, at the position and field offset, and every item of a
    /// vector there
    func reads(of bytes: RawSpan) -> [Read] {
        var reads: [Read] = []

        reads.append(read { .table(try readTable(bytes, table)) })
        let root = TableReader(bytes, layout: table.layout)
        for (index, field) in table.fields.enumerated() {
            reads.append(read { .value(try readRootField(root, index, field)) })
        }

        for offsetType in OffsetType.allCases {
            for layout in layouts(of: offsetType) {
                let reader = TableReader(bytes, layout: layout)
                reads.append(read { .position(try reader.rootOffset(index)) })
                reads.append(read { .count(try reader.size) })
                if layout.isVersioned {
                    reads.append(read { .count(Int(try reader.version)) })
                }
            }

            let reader = TableReader(bytes, layout: TableLayout(offsetType: offsetType))
            reads.append(read { .position(try reader.fieldOffset(index, inTableAt: position)) })
            reads += self.reads(of: reader, within: bytes)
        }

        return reads
    }

    /// Reads every type at the position
    func reads(of reader: TableReader, within bytes: RawSpan) -> [Read] {
        let values: [Field] = [
            .int8(0), .uint8(0), .int16(0), .uint16(0), .int32(0), .uint32(0), .int64(0), .uint64(0),
            .float32(bits: 0), .float64(bits: 0), .int8Array([]), .uint8Array([]), .int16Array([]),
            .uint16Array([]), .int32Array([]), .uint32Array([]), .int64Array([]), .uint64Array([]),
            .float32Array(bits: []), .float64Array(bits: []), .string(utf8: []),
        ]

        var reads = values.map { field in read { .value(try readField(reader, at: position, field)) } }
        reads.append(read { .bytes(bytes.byteOffsets(of: try reader.struct(at: position, size: size))!) })
        reads.append(read { .bytes(bytes.byteOffsets(of: try reader.structArray(at: position, size: size))!) })

        do {
            let vector = try reader.vector(at: position)
            reads.append(.count(vector.count))
            for index in vector.indices {
                reads.append(read { .bytes(bytes.byteOffsets(of: try vector.item(at: index))!) })
                reads.append(read { .bytes(bytes.byteOffsets(of: try vector.itemInPlace(at: index))!) })
            }
        } catch {
            reads.append(.error(error))
        }

        return reads
    }
}

/// Returns what a read returns, or its error
func read(_ body: () throws -> Read) -> Read {
    do {
        return try body()
    } catch let error as DecodeError {
        return .error(error)
    } catch let mismatch as Mismatch {
        return .mismatch(mismatch.kind, mismatch.description)
    } catch {
        return .mismatch(.disagreement, "\(error)")
    }
}

extension Draw {
    /// A byte, mostly small, so that offsets, counts and lengths often lie
    /// within the bytes, and some 0xff
    mutating func byte() -> UInt8 {
        weighted([
            (4, { _ in 0 }),
            (4, { UInt8($0.int(1...15)) }),
            (1, { _ in 0xff }),
            (1, { $0.integer() }),
        ])
    }

    /// Bytes of up to `most`, mostly small
    mutating func bytes(most: Int) -> [UInt8] {
        array(most: most) { $0.byte() }
    }

    /// A position, mostly below `small`, so that it lies within small bytes
    /// or tables, and up to the largest Int, of either sign
    mutating func large(below small: Int) -> Int {
        weighted([
            (8, { $0.int(0...small - 1) }),
            (1, { $0.element(of: [.max, .min, -1]) }),
            (1, { $0.integer() }),
        ])
    }

    /// An index or size, which callers never make negative, mostly below
    /// `small`, and up to the largest Int
    mutating func count(below small: Int) -> Int {
        weighted([
            (8, { $0.int(0...small - 1) }),
            (1, { _ in .max }),
            (1, { $0.int(0...Int.max) }),
        ])
    }
}
