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

            var builder = try TableBuilder(fields: table.fields.count + 1, layout: table.layout, version: table.version)
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
        let layout = TableLayout(versioned: true, offsetType: .uint16)

        #expect(throws: BuildError.versionTooLarge(version: 0x1_0000, offsetType: .uint16)) {
            _ = try TableBuilder(fields: 0, layout: layout, version: 0x1_0000)
        }
        #expect(try TableBuilder(fields: 0, layout: layout, version: 0xffff).bytes() == [0xff, 0xff])
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
            _ = try? TableBuilder(fields: 0, layout: TableLayout(versioned: true))
        }
        await #expect(processExitsWith: .failure) {
            _ = try? TableBuilder(fields: 0, version: 1)
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
            let buffer = ByteBuffer(bytes)
            #expect(try buffer.float32(at: buffer.rootOffset(0)).bitPattern == 0xffc0_0001)
            #expect(try buffer.float64(at: buffer.rootOffset(1)).bitPattern == (-0.0).bitPattern)
            #expect(try buffer.float32(at: buffer.rootOffset(2)) == .infinity)
        }
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
