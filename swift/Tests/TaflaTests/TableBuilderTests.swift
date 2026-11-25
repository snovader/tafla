import Tafla
import Testing

// Builders write any table that fits in its offset type, and reject what they
// can't write with a BuildError from the setter, leaving the builder as it
// was. The examples document the errors.

@Suite("Table builder")
struct TableBuilderTests {
    // After any fields, a field as long as fits is written and reads back, and
    // a unit longer is rejected with the length that it would make the table,
    // leaving the builder to write the field that fits
    @Test func theLongestFieldThatFitsIsWrittenAndAUnitMoreIsRejected() async {
        await checkDrawn { draw in
            (table: draw.table(of: [.uint8, .uint16]), kind: draw.element(of: LongField.allCases))
        } _: { drawn in
            let table = drawn.table
            let maxLength = Int(table.layout.offsetType.maxValue)

            // The table's length with the field of no units, whose padding
            // and fixed bytes don't change with them
            var withEmpty = table
            withEmpty.fields.append(drawn.kind.field(units: 0))
            guard let emptyLength = try? withEmpty.bytes().count else {
                return
            }
            let units = (maxLength - emptyLength) / drawn.kind.unit

            var longest = table
            longest.fields.append(drawn.kind.field(units: units))
            let bytes = try longest.bytes()
            #expect(bytes.count == emptyLength + units * drawn.kind.unit)
            #expect(try withSpan(of: bytes) { try readTable($0, longest) } == longest)

            var builder = try TableBuilder(fields: table.fields.count + 1, layout: table.writtenLayout)
            for field in table.fields {
                try builder.set(field)
            }
            let tooLong = BuildError.tooLong(
                index: table.fields.count,
                length: UInt64(emptyLength + (units + 1) * drawn.kind.unit),
                offsetType: table.layout.offsetType
            )
            #expect(throws: tooLong) {
                try builder.set(drawn.kind.field(units: units + 1))
            }

            try builder.set(drawn.kind.field(units: units))
            #expect(try builder.bytes() == bytes)
        }
    }

    // A table nested from its builder writes the same parent as from its
    // bytes at the builder's alignment: the builder is written straight into
    // its parent, in its own layout
    @Test func aTableNestedFromItsBuilderWritesTheSameParentAsFromItsBytes() async {
        await checkTables { table throws in
            #expect(try fromBytes(table).bytes() == table.bytes())
        }
    }

    @Test func rejectsAFieldBeyondTheCount() throws {
        var builder = try TableBuilder(fields: 1)
        try builder.setUInt8(1)

        #expect(throws: BuildError.tooManyFields(fields: 1)) {
            try builder.setUInt8(2)
        }
        #expect(throws: BuildError.tooManyFields(fields: 1)) {
            try builder.setTable(.bytes([]))
        }
        #expect(try builder.bytes() == [4, 0, 0, 0, 1])
    }

    @Test func rejectsWritingBeforeEveryFieldIsSet() throws {
        var builder = try TableBuilder(fields: 2)
        try builder.setUInt8(1)

        #expect(throws: BuildError.incomplete(set: 1, fields: 2)) {
            _ = try builder.bytes()
        }
    }

    @Test func rejectsAFieldThatMakesTheTableTooLong() throws {
        var builder = try TableBuilder(fields: 2, layout: TableLayout(offsetType: .uint8))
        try builder.setUInt8Array(Array(repeating: 7, count: 250))

        // 2 offsets, a count and 250 bytes, and a string of a count and 2
        #expect(throws: BuildError.tooLong(index: 1, length: 256, offsetType: .uint8)) {
            try builder.setString("ab")
        }

        try builder.setString("a")
        #expect(try builder.bytes().count == 255)
    }

    @Test func rejectsBytesThatArentWholeStructs() throws {
        var builder = try TableBuilder(fields: 1)

        #expect(throws: BuildError.notWholeStructs(length: 5, size: 2)) {
            try builder.setStructArray([1, 2, 3, 4, 5], size: 2, alignment: .two)
        }
        #expect(throws: BuildError.notWholeStructs(length: 0, size: 0)) {
            try builder.setStructArray([], size: 0, alignment: .one)
        }
        #expect(throws: BuildError.notWholeStructs(length: 2, size: -1)) {
            try builder.setStructArray([1, 2], size: -1, alignment: .one)
        }
        #expect("\(BuildError.notWholeStructs(length: 0, size: 0))" == "A struct can't be 0 bytes")
    }

    @Test func rejectsAHeaderTooLong() throws {
        #expect(throws: BuildError.headerTooLong(fields: 256, offsetType: .uint8)) {
            _ = try TableBuilder(fields: 256, layout: TableLayout(offsetType: .uint8))
        }
        #expect(throws: BuildError.headerTooLong(fields: 255, offsetType: .uint8)) {
            _ = try TableBuilder(fields: 255, layout: TableLayout(sized: true, offsetType: .uint8))
        }
        #expect(throws: BuildError.headerTooLong(fields: .max, offsetType: .uint32)) {
            _ = try TableBuilder(fields: .max)
        }

        let longest = try TableBuilder(fields: 255, layout: TableLayout(offsetType: .uint8))
        #expect(throws: BuildError.incomplete(set: 0, fields: 255)) {
            _ = try longest.bytes()
        }
    }

    @Test func rejectsAVersionTooLargeForItsSlot() throws {
        let layout = TableLayout(version: 0, offsetType: .uint16)

        #expect(throws: BuildError.versionTooLarge(version: 0x1_0000, offsetType: .uint16)) {
            _ = try TableBuilder(fields: 0, layout: layout.withVersion(0x1_0000))
        }
        #expect(try TableBuilder(fields: 0, layout: layout.withVersion(0xffff)).bytes() == [0xff, 0xff])
    }

    @Test func writesTheLayoutsVersion() throws {
        let first = TableLayout(sized: true, version: 1)
        let second = first.withVersion(2)

        #expect(second.version == 2)
        #expect(second.isSized && second.isVersioned)
        #expect(TableLayout.default.version == nil)
        #expect(!TableLayout.default.isVersioned)

        // A layout without a version gains the slot for one
        let versioned = TableLayout.default.withVersion(7)
        #expect(versioned.version == 7)
        #expect(versioned.headerSize == TableLayout.default.headerSize + 4)

        let bytes = try TableBuilder(fields: 0, layout: second).bytes()

        // A reader reads a table of any version, whatever its layout's
        #expect(try withSpan(of: bytes) { try TableReader($0, layout: first).version } == 2)
        #expect(try withSpan(of: bytes) { try TableReader($0, layout: second).version } == 2)
    }

    @Test func rejectsAnIncompleteNestedBuilder() throws {
        let nested = try TableBuilder(fields: 1)
        var builder = try TableBuilder(fields: 2)

        #expect(throws: BuildError.incomplete(set: 0, fields: 1)) {
            try builder.setTable(.builder(nested))
        }
        #expect(throws: BuildError.incomplete(set: 0, fields: 1)) {
            try builder.setVector([.bytes([]), .builder(nested)])
        }

        try builder.setVector([])
        try builder.setTable(.bytes([]))
        #expect(try builder.bytes() == [8, 0, 0, 0, 12, 0, 0, 0, 0, 0, 0, 0])
    }

    // A builder is a value: a copy taken after any number of fields, as few as
    // a builder holds within itself or more, sets the rest of its own, and
    // neither changes the other
    @Test func aCopyOfABuilderSetsItsOwnFields() throws {
        let fields = 9
        for copied in 0...fields {
            var builder = try TableBuilder(fields: fields)
            for index in 0..<copied {
                try builder.setString("field \(index)")
            }

            var copy = builder
            for index in copied..<fields {
                try builder.setString("field \(index)")
                try copy.setUInt8Array([UInt8(index)])
            }

            try withSpan(of: builder.bytes()) { bytes throws in
                let reader = TableReader(bytes)
                for index in 0..<fields {
                    #expect(try reader.string(at: reader.rootOffset(index)) == "field \(index)")
                }
            }
            try withSpan(of: copy.bytes()) { bytes throws in
                let reader = TableReader(bytes)
                for index in 0..<copied {
                    #expect(try reader.string(at: reader.rootOffset(index)) == "field \(index)")
                }
                for index in copied..<fields {
                    #expect(Array(try reader.uint8Array(at: reader.rootOffset(index))) == [UInt8(index)])
                }
            }
        }
    }

    // A builder nested in another writes the bytes that it writes alone,
    // whether its fields, and its parent's with it, are as few as a builder
    // holds within itself or more, and with a builder nested in it in turn
    @Test func aNestedBuilderWritesItsOwnBytesWithAnyNumberOfFields() throws {
        var pair = try TableBuilder(fields: 2)
        try pair.setUInt8(1)
        try pair.setString("𝄞")

        for nestedFields in 1...6 {
            var nested = try TableBuilder(fields: nestedFields)
            for index in 1..<nestedFields {
                try nested.setUInt16(UInt16(index))
            }
            try nested.setTable(.builder(pair))

            for parentFields in 1...6 {
                var fromBuilder = try TableBuilder(fields: parentFields)
                var fromBytes = try TableBuilder(fields: parentFields)
                for index in 1..<parentFields {
                    try fromBuilder.setInt32(Int32(index))
                    try fromBytes.setInt32(Int32(index))
                }
                try fromBuilder.setTable(.builder(nested))
                try fromBytes.setTable(.bytes(nested.bytes()), alignment: .one)

                #expect(try fromBuilder.bytes() == fromBytes.bytes())
            }
        }
    }

    // Tables nested from builders and recorded in place, with
    // setTable(fields:_:) and setVector(count:fields:_:), write the bytes that
    // their builders write, and fail as they fail, for tables of any offset
    // type and of short ones, whose limits the fields reach. A vector whose
    // count and item ends alone make its table too long fails before its
    // items, where its builders fail for them or count them too. Most tables
    // nest only tables that are recorded in place, and others any tables, of
    // which those are recorded in place among the others.
    @Test func tablesRecordedInPlaceWriteWhatTheirBuildersWrite() async {
        await checkDrawn { draw in
            let table = draw.bool() ? draw.table() : draw.table(of: [.uint8, .uint16])
            return draw.int(0...3) > 0 ? table.recordedInPlace() : table
        } _: { table in
            let inPlace = Result { () throws(BuildError) in try table.bytesInPlace() }
            let fromBuilders = Result { () throws(BuildError) in try table.bytes() }

            #expect(inPlace == fromBuilders || failsBeforeItsItems(inPlace, ratherThan: fromBuilders))
        }
    }

    // A vector recorded in place whose count and item ends alone make its
    // table too long reports that length, since its items aren't recorded,
    // where a vector of their builders reports the length with its items
    @Test func aVectorRecordedInPlaceTooLongBeforeItsItemsReportsItsLengthWithoutThem() throws {
        var builder = try TableBuilder(fields: 2, layout: TableLayout(offsetType: .uint8))
        try builder.setUInt8Array(Array(repeating: 7, count: 249))
        var item = try TableBuilder(fields: 1, layout: TableLayout(offsetType: .uint8))
        try item.setUInt8(1)

        // 2 offsets, a count and 249 bytes, and a vector of a count and 3
        // ends, and of 3 items of an offset and a byte
        #expect(throws: BuildError.tooLong(index: 1, length: 256, offsetType: .uint8)) {
            try builder.setVector(count: 3, fields: 1) { _, item throws(BuildError) in
                try item.setUInt8(1)
            }
        }
        #expect(throws: BuildError.tooLong(index: 1, length: 262, offsetType: .uint8)) {
            try builder.setVector(Array(repeating: .builder(item), count: 3))
        }
    }

    // Whatever a table or vector recorded in place throws, from its fill or
    // for the table itself, the builder is left as it was, whether the
    // fields before it fit within the builder or not
    @Test func aTableRecordedInPlaceThatFailsLeavesTheBuilderAsItWas() throws {
        for before in 0...5 {
            var builder = try TableBuilder(fields: before + 1)
            var expected = try TableBuilder(fields: before + 1)
            for index in 0..<before {
                try builder.setString("field \(index)")
                try expected.setString("field \(index)")
            }

            #expect(throws: BuildError.incomplete(set: 5, fields: 6)) {
                try builder.setTable(fields: 6) { table throws(BuildError) in
                    for index in 0..<5 {
                        try table.setUInt16(UInt16(index))
                    }
                }
            }
            #expect(throws: BuildError.tooManyFields(fields: 1)) {
                try builder.setTable(fields: 1) { table throws(BuildError) in
                    try table.setTable(fields: 1) { nested throws(BuildError) in
                        try nested.setUInt8(1)
                    }
                    try table.setUInt8(2)
                }
            }
            #expect(throws: BuildError.tooManyFields(fields: 2)) {
                try builder.setVector(count: 3, fields: 2) { index, item throws(BuildError) in
                    try item.setUInt8(1)
                    try item.setUInt8(2)
                    if index == 2 {
                        try item.setUInt8(3)
                    }
                }
            }

            try builder.setUInt8(0xff)
            try expected.setUInt8(0xff)
            #expect(try builder.bytes() == expected.bytes())
        }
    }

    @Test func rejectsATableRecordedInPlaceThatMakesItsParentTooLong() throws {
        var builder = try TableBuilder(fields: 2, layout: TableLayout(offsetType: .uint8))
        try builder.setUInt8Array(Array(repeating: 7, count: 240))

        // 2 offsets, a count and 240 bytes, and a table of an offset and a
        // string of a count and 12 bytes
        #expect(throws: BuildError.tooLong(index: 1, length: 257, offsetType: .uint8)) {
            try builder.setTable(fields: 1) { table throws(BuildError) in
                try table.setString("twelve bytes")
            }
        }

        // A vector whose count and the ends of its items alone make the table
        // too long fails before any item is recorded
        var filled = 0
        #expect(throws: BuildError.tooLong(index: 1, length: 256, offsetType: .uint8)) {
            try builder.setVector(count: 12, fields: 0) { _, _ in
                filled += 1
            }
        }
        #expect(filled == 0)

        #expect(throws: BuildError.headerTooLong(fields: 256, offsetType: .uint8)) {
            try builder.setTable(fields: 256) { _ in }
        }

        try builder.setTable(fields: 1) { table throws(BuildError) in
            try table.setString("a")
        }
        #expect(try builder.bytes().count == 246)
    }

    // A table of 4 GiB can't be allocated in a test, but a vector that nests
    // a builder of 64 MiB 64 times is that long without allocating more
    @Test func rejectsATableOfUInt32OffsetsPast4GiBBeforeWritingIt() throws {
        var nested = try TableBuilder(fields: 1)
        try nested.setUInt8Array(Array(repeating: 0, count: 1 << 26))

        // An offset, the count and 64 MiB in each item, after the parent's
        // offset, the vector's count and its 64 ends
        var builder = try TableBuilder(fields: 1)
        let length = UInt64(4 + 4 + 64 * 4) + 64 * UInt64(4 + 4 + (1 << 26))
        #expect(throws: BuildError.tooLong(index: 0, length: length, offsetType: .uint32)) {
            try builder.setVector(Array(repeating: .builder(nested), count: 64))
        }
        #expect(throws: BuildError.incomplete(set: 0, fields: 1)) {
            _ = try builder.bytes()
        }
    }

    @Test func describesEachError() {
        #expect(
            "\(BuildError.versionTooLarge(version: 256, offsetType: .uint8))"
                == "Version 256 doesn't fit in a uint8 slot")
        #expect(
            "\(BuildError.headerTooLong(fields: 256, offsetType: .uint8))"
                == "A header of 256 fields doesn't fit in uint8 offsets")
        #expect("\(BuildError.tooManyFields(fields: 1))" == "Can't add field 2 to a table of 1")
        #expect(
            "\(BuildError.tooLong(index: 1, length: 256, offsetType: .uint8))"
                == "Field 2 makes the table 256 bytes, too long for uint8 offsets")
        #expect("\(BuildError.incomplete(set: 1, fields: 2))" == "Only 1 of the table's 2 fields were set")
        #expect(
            "\(BuildError.notWholeStructs(length: 5, size: 2))" == "5 bytes aren't a whole number of 2-byte structs")
    }

    @Test func stopsTheProgramForAMistakeOfTheCaller() async {
        await #expect(processExitsWith: .failure) {
            _ = try? TableBuilder(fields: -1)
        }
        await #expect(processExitsWith: .failure) {
            var builder = try? TableBuilder(fields: 1)
            try? builder?.setVector(count: -1, fields: 1) { _, _ in }
        }

        // A closure that uses the builder that it records a table in, rather
        // than its item, is an overlapping access to the builder, which Swift
        // rejects when it compiles a local one, and stops the program for
        // otherwise
        await #expect(processExitsWith: .failure) {
            let shared = SharedBuilder(try! TableBuilder(fields: 1))
            try? shared.builder.setTable(fields: 1) { _ throws(BuildError) in
                try shared.builder.setUInt8(1)
            }
        }
    }

    // Each setter writes its field's own type, which Swift converts to
    // explicitly; floats keep their bits, NaN payloads and signs included
    @Test func writesTheBitsOfFloats() throws {
        let payload = Float(bitPattern: 0xffc0_0001)
        var builder = try TableBuilder(fields: 3)
        try builder.setFloat32(payload)
        try builder.setFloat64(-0.0)
        try builder.setFloat32(Float(1e300))
        let bytes = try builder.bytes()

        try withSpan(of: bytes) { bytes throws in
            let reader = TableReader(bytes)
            #expect(try reader.float32(at: reader.rootOffset(0)).bitPattern == 0xffc0_0001)
            #expect(try reader.float64(at: reader.rootOffset(1)).bitPattern == (-0.0).bitPattern)
            #expect(try reader.float32(at: reader.rootOffset(2)) == .infinity)
        }
    }
}

/// Whether a table recorded in place failed for a vector whose count and item
/// ends alone make its table too long, which fails before its items are
/// recorded: with that length, where its builders failed for the vector with
/// its items too, which is longer, or for an item of it first
func failsBeforeItsItems(
    _ inPlace: Result<[UInt8], BuildError>,
    ratherThan fromBuilders: Result<[UInt8], BuildError>
) -> Bool {
    guard case .failure(.tooLong(let index, let length, let offsetType)) = inPlace,
        case .failure(.tooLong(let builderIndex, let builderLength, let builderOffsetType)) = fromBuilders
    else {
        return false
    }

    let sameField = index == builderIndex && offsetType == builderOffsetType
    return !sameField || length < builderLength
}

/// A builder in a class, whose accesses Swift checks while the program runs
final class SharedBuilder {
    var builder: TableBuilder

    init(_ builder: TableBuilder) {
        self.builder = builder
    }
}

/// A field whose byte length grows by a unit with each of its units
enum LongField: CaseIterable, Sendable {
    case bytes
    case uint16s
    case uint64s
    case string
    case `struct`
    case structs

    /// The bytes that each unit adds
    var unit: Int {
        switch self {
        case .bytes, .string, .struct: 1
        case .uint16s: 2
        case .uint64s: 8
        case .structs: 3
        }
    }

    func field(units: Int) -> Field {
        switch self {
        case .bytes: .uint8Array(Array(repeating: 0xab, count: units))
        case .uint16s: .uint16Array(Array(repeating: 0xabcd, count: units))
        case .uint64s: .uint64Array(Array(repeating: .max, count: units))
        case .string: .string(utf8: Array(repeating: 0x61, count: units))
        case .struct: .struct(Array(repeating: .uint8(1), count: units))
        case .structs:
            .structArray(types: [.uint16, .int8], items: Array(repeating: [.uint16(1), .int8(-1)], count: units))
        }
    }
}

/// The table with each table that it nests from a builder nested from the
/// builder's bytes, at the builder's alignment, in it and in the tables
/// nested in it
func fromBytes(_ table: Table) -> Table {
    var converted = table
    converted.fields = table.fields.map { field in
        switch field {
        case .table(let nested, let alignment):
            let alignment = alignment ?? defaultAlignment(of: nested)
            return .table(Nested(from: .bytes, table: fromBytes(nested.table)), alignment: alignment)
        case .vector(let items):
            return .vector(items.map { Nested(from: .bytes, table: fromBytes($0.table)) })
        default:
            return field
        }
    }

    return converted
}
