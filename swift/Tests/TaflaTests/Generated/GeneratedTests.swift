import Tafla
import Testing

// The code that tafla-gen generates from the example schemas in schemas/,
// and from the schema of optional fields and versions, writes the tables
// that TableBuilder writes from the same fields, or fails as it fails, reads
// back what it writes wherever its bytes lie, rejects the versions that it
// doesn't read, and reads corrupted and arbitrary bytes as values or decode
// errors, through its getters and the views that they return, without
// stopping the program. Its structs are written at fixed offsets, and read
// back from the start of their bytes.

/// Every message of the example schemas, and of the schema of optional
/// fields and versions
let generatedMessages = shapesMessages + allTypesMessages + layoutsMessages + fixedFieldsMessages + presenceMessages

/// Every struct of the example schemas, and of the schema of optional fields
/// and versions
let generatedStructs = shapesStructs + allTypesStructs + fixedFieldsStructs + presenceStructs

@Suite("Generated code")
struct GeneratedTests {
    @Test(arguments: generatedMessages)
    func writesEveryMessageAsTableBuilderWritesItOrFailsAsItFails(message: Message) async {
        await checkDrawn { draw in
            message.draw(&draw)
        } _: { fields in
            let written = Result { () throws(BuildError) in try message.serialize(fields) }
            let built = Result { () throws(BuildError) in
                try build(fields, layout: message.layout, version: message.version)
            }

            switch (written, built) {
            case (.success(let written), .success(let built)):
                #expect(written == built)
            case (.failure(let written), .failure(let built)):
                #expect(written == built)
            default:
                Issue.record("\(written) rather than \(built)")
            }
        }
    }

    // At any byte offset from an aligned address, so that arrays are read in
    // place where they're aligned in memory, and copied where they aren't
    @Test(arguments: generatedMessages)
    func readsBackEveryMessageWhereverItsBytesLie(message: Message) async {
        await checkDrawn { draw in
            (fields: message.draw(&draw), byteOffset: draw.int(0...7))
        } _: { drawn in
            // A message too long for its offset type, which the test above
            // checks fails as TableBuilder fails
            guard let bytes = try? message.serialize(drawn.fields) else {
                return
            }

            let read = try withSpan(of: bytes, atByteOffset: drawn.byteOffset) { bytes throws in
                try message.read(bytes)
            }
            #expect(sameValues(read, drawn.fields))
        }
    }

    // A versioned message reads the version in the header of any bytes, and
    // its reader rejects every version but its own, or but those from its
    // first, whose fields' values PresenceTests.swift checks
    @Test(arguments: generatedMessages.filter { $0.version != nil })
    func rejectsTheVersionsThatItDoesntRead(message: Message) async {
        guard let version = message.version, let versionOf = message.versionOf else {
            Issue.record("\(message.name) isn't versioned")
            return
        }
        let offsetType = message.layout.offsetType

        await checkDrawn { draw in
            (fields: message.draw(&draw), other: UInt32(draw.int(0...Int(offsetType.maxValue))))
        } _: { drawn in
            guard var bytes = try? message.serialize(drawn.fields) else {
                return
            }
            #expect(try withSpan(of: bytes) { bytes throws in try versionOf(bytes) } == version)

            // The version follows the length when the layout is sized
            let slot = message.layout.isSized ? offsetType.size : 0
            bytes.replaceSubrange(
                slot..<slot + offsetType.size, with: littleEndianSlot(Int(drawn.other), offsetType.size))

            try withSpan(of: bytes) { bytes throws in
                #expect(try versionOf(bytes) == drawn.other)
                if drawn.other == version {
                    #expect(sameValues(try message.read(bytes), drawn.fields))
                } else if let firstVersion = message.firstVersion {
                    let supported = firstVersion...version
                    if supported.contains(drawn.other) {
                        _ = try message.read(bytes)
                    } else {
                        #expect(throws: DecodeError.versionOutsideRange(version: drawn.other, supported: supported)) {
                            try message.read(bytes)
                        }
                    }
                } else {
                    #expect(throws: DecodeError.unsupportedVersion(version: drawn.other, supported: version)) {
                        try message.read(bytes)
                    }
                }
            }
        }
    }

    // Bytes shorter than the header have no version to read
    @Test(arguments: generatedMessages.filter { $0.version != nil })
    func throwsForAHeaderLongerThanTheBytes(message: Message) throws {
        guard let versionOf = message.versionOf else {
            Issue.record("\(message.name) isn't versioned")
            return
        }
        let offsetType = message.layout.offsetType
        let slot = message.layout.isSized ? offsetType.size : 0

        let error = DecodeError.outOfRange(position: slot, byteLength: UInt64(offsetType.size), available: slot)
        try withSpan(of: Array(repeating: 0, count: slot)) { bytes throws in
            #expect(throws: error) {
                try versionOf(bytes)
            }
            #expect(throws: error) {
                try message.read(bytes)
            }
        }
    }

    // Overwritten bytes read as values, which may be others than those
    // written, or fail with the decode error, through the getters and their
    // views
    @Test(arguments: generatedMessages)
    func readsEveryCorruptedMessageAsValuesOrDecodeErrors(message: Message) async {
        await checkDrawn { draw in
            (fields: message.draw(&draw), overwrites: draw.array(most: 4) { ($0.int(0...255), $0.integer() as UInt8) })
        } _: { drawn in
            guard var bytes = try? message.serialize(drawn.fields) else {
                return
            }
            for (position, byte) in drawn.overwrites where !bytes.isEmpty {
                bytes[position % bytes.count] = byte
            }

            // Every error that a read throws is a decode error
            withSpan(of: bytes) { bytes in
                _ = try? message.read(bytes)
            }
        }
    }

    @Test(arguments: generatedMessages)
    func readsArbitraryBytesAsEveryMessageOrDecodeErrors(message: Message) async {
        await checkDrawn { draw in
            draw.integers(most: 64) as [UInt8]
        } _: { bytes in
            withSpan(of: bytes) { bytes in
                _ = try? message.read(bytes)
            }
        }
    }

    // A struct's scalars lie one after another, as the tests' Mirror finds
    // them, and it's read from the start of longer bytes, wherever they lie,
    // or fails for shorter bytes
    @Test(arguments: generatedStructs)
    func writesEveryStructAtFixedOffsetsAndReadsItBackFromTheStartOfItsBytes(structure: StructCase) async {
        await checkDrawn { draw in
            draw.next()
        } _: { seed in
            var draw = Draw(seed: seed)
            try structure.check(&draw)
        }
    }
}

/// Checks a generated struct's bytes, of `size`, and how it reads them back
/// from the start of longer bytes, wherever they lie, or fails for shorter
/// bytes, with another struct after it in an array
func checkStruct<S: GeneratedStruct>(_ value: S, _ other: S, size: Int, after: Int, byteOffset: Int) throws {
    let bytes = value.serialize()
    #expect(bytes == scalarBytes(of: value))
    #expect(bytes.count == size)
    #expect(S.size == size)

    let array = [value, other]
    #expect(S.serializeArray(array) == array.structBytes)
    #expect(S.serializeArray([]) == [])

    let longer = bytes + Array(repeating: 0xa5, count: after)
    let read = try withSpan(of: longer, atByteOffset: byteOffset) { bytes throws in try S(bytes) }
    #expect(sameValues(read, value))

    let tooShort = DecodeError.outOfRange(position: 0, byteLength: UInt64(size), available: size - 1)
    withSpan(of: Array(bytes.dropLast())) { bytes in
        #expect(throws: tooShort) {
            try S(bytes)
        }
        return
    }
}
