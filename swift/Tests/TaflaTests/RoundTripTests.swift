import Tafla
import Testing

// Tables read back the values that they were built from. The examples show
// builders keeping their values, ArrayView reading arrays in place or copying
// them, structs read from their bytes, and vector items found from their
// ends.

@Suite("Round trip")
struct RoundTripTests {
    // Floats compare by their bits, so that -0 and NaN count. The table is
    // read at any byte offset from an aligned address, so that its arrays are
    // read in place when they're aligned in memory and copied when they
    // aren't, and the reader checks that both read the same elements.
    @Test func tablesReadBackTheValuesTheyWereBuiltFrom() async {
        await checkTables { table in
            let bytes = try table.bytes()

            for byteOffset in 0..<8 {
                let read = try withSpan(of: bytes, atByteOffset: byteOffset) { try readTable($0, table) }
                #expect(read == table, "at \(byteOffset)")
            }
        }
    }

    // A builder keeps its values as values, so changing them after they're
    // set, or setting more fields on a builder after nesting it, changes
    // neither the builder nor its table
    @Test func roundTripsValuesThatChangeAfterTheyreSet() throws {
        var values: [Float] = [1.5, -0.0]
        var name = "før"
        var nested = try TableBuilder(fields: 1)
        try nested.setInt16(-2)

        var builder = try TableBuilder(fields: 3)
        try builder.setFloat32Array(values)
        try builder.setString(name)
        try builder.setTable(.builder(nested))
        values[0] = 2
        name += "e"
        nested = try TableBuilder(fields: 1)
        try nested.setInt16(7)

        let expected = Table(
            layout: .default,
            fields: [
                .float32Array(bits: [Float(1.5).bitPattern, Float(-0.0).bitPattern]),
                .string("før"),
                .table(Nested(from: .builder, table: Table(layout: .default, fields: [.int16(-2)])), alignment: nil),
            ]
        )
        #expect(try withSpan(of: builder.bytes()) { try readTable($0, expected) } == expected)
    }

    // A struct's scalars lie at fixed offsets from its start, unpadded, and an
    // array's structs one after another
    @Test func roundTripsStructs() throws {
        // A uint8, then a float64 of -0 and an int16 of -2, unaligned
        let scalars: [Field] = [.uint8(1), .float64(-0.0), .int16(-2)]
        let structs: [[Field]] = [[.uint8(3), .float64(.nan), .int16(.min)], scalars]

        var builder = try TableBuilder(fields: 2)
        try builder.setStruct(structBytes(scalars))
        try builder.setStructArray(structs.flatMap(structBytes), size: 11, alignment: .eight)
        let bytes = try builder.bytes()

        try withSpan(of: bytes) { bytes in
            let buffer = TableReader(bytes)
            let value = try buffer.struct(at: buffer.rootOffset(0), size: 11)
            #expect(value.byteCount == 11)
            #expect(try readScalars(value, [.uint8, .float64, .int16]) == scalars)

            let array = try buffer.structArray(at: buffer.rootOffset(1), size: 11)
            #expect(array.byteCount == 22)
            #expect(try readScalars(array.extracting(0..<11), [.uint8, .float64, .int16]) == structs[0])
            #expect(try readScalars(array.extracting(11..<22), [.uint8, .float64, .int16]) == structs[1])

            // Structs of no bytes take none, whatever the count
            #expect(try buffer.structArray(at: buffer.rootOffset(1), size: 0).byteCount == 0)
        }
    }

    // TableBuilder aligns arrays from the table's start, so an array is in
    // place in a table that's aligned in memory, on a little-endian host, and
    // copied in one that isn't. Bytes and empty arrays are in place at any
    // address.
    @Test(arguments: 0..<8)
    func readsArraysInPlaceWhenAligned(byteOffset: Int) throws {
        var builder = try TableBuilder(fields: 4)
        try builder.setUInt8(1)
        try builder.setFloat64Array([0.5, -0.0, .infinity])
        try builder.setUInt8Array([1, 2, 3])
        try builder.setInt32Array([])
        let bytes = try builder.bytes()

        try withSpan(of: bytes, atByteOffset: byteOffset) { bytes in
            let buffer = TableReader(bytes)
            let floats = try buffer.float64Array(at: buffer.rootOffset(1))

            #expect(floats.count == 3)
            #expect(floats[1].bitPattern == (-0.0).bitPattern)
            #expect(Array(floats).map(\.bitPattern) == [0.5, -0.0, .infinity].map(\.bitPattern))
            #expect((floats.span != nil) == (isLittleEndianHost && byteOffset == 0))
            if let span = floats.span {
                #expect(span[2] == .infinity)
            }

            let bytes = try buffer.uint8Array(at: buffer.rootOffset(2))
            #expect(bytes.span?.count == 3)

            let empty = try buffer.int32Array(at: buffer.rootOffset(3))
            #expect(empty.count == 0)
            #expect(empty.span?.isEmpty == true)
            #expect(Array(empty) == [])
        }
    }

    // A builder nested several times writes its bytes each time, in place,
    // as a field and as vector items
    @Test func nestsABuilderSeveralTimes() throws {
        var nested = try TableBuilder(fields: 2)
        try nested.setUInt16(0xbeef)
        try nested.setString("𝄞")
        let nestedBytes = try nested.bytes()

        var builder = try TableBuilder(fields: 3)
        try builder.setTable(.builder(nested))
        try builder.setTable(.builder(nested), alignment: .eight)
        try builder.setVector([.builder(nested), .bytes(nestedBytes), .builder(nested)])
        let bytes = try builder.bytes()

        try withSpan(of: bytes) { bytes in
            let buffer = TableReader(bytes)
            let vector = try buffer.vector(at: buffer.rootOffset(2))
            #expect(vector.count == 3)

            for index in vector.indices {
                let item = try vector.item(at: index)
                #expect(item.withUnsafeBytes(Array.init) == nestedBytes)
            }

            for index in 0..<2 {
                let position = try buffer.rootOffset(index)
                let item = TableReader(bytes.extracting(position..<position + nestedBytes.count))
                #expect(item.bytes.withUnsafeBytes(Array.init) == nestedBytes)
            }
        }
    }

    // A vector item read in place runs on to the end of its parent's bytes,
    // and reads its fields there from offsets counted from its start
    @Test func readsVectorItemsInPlace() throws {
        var item = try TableBuilder(fields: 1, layout: TableLayout(offsetType: .uint16))
        try item.setString("item")

        var builder = try TableBuilder(fields: 2, layout: TableLayout(sized: true, offsetType: .uint16))
        try builder.setVector([.builder(item), .builder(item)])
        try builder.setUInt8(0xff)
        let bytes = try builder.bytes()

        try withSpan(of: bytes) { bytes in
            let buffer = TableReader(bytes, layout: builder.layout)
            let vector = try buffer.vector(at: buffer.rootOffset(0))

            let inPlace = TableReader(try vector.itemInPlace(at: 1), layout: TableLayout(offsetType: .uint16))
            #expect(try inPlace.string(at: inPlace.rootOffset(0)) == "item")
            #expect(try inPlace.bytes.byteCount > vector.item(at: 1).byteCount)
        }
    }
}

let isLittleEndianHost: Bool = {
    #if _endian(little) && !TAFLA_PORTABLE
        true
    #else
        false
    #endif
}()
