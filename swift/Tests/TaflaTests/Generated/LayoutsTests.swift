import GeneratedLayouts
import Tafla
import Testing

// The code generated from schemas/layouts.tafla, which has a message of the
// same fields in every layout: each offset type, with and without the
// table's length and a version in its header. GeneratedTests.swift checks
// that each writes the tables that TableBuilder writes, or fails as it fails,
// reads back what it writes, reads the version of tables of any version and
// rejects those of other versions.

/// A message of the schema, each of whose fields' structs holds an `id`, a
/// `name` and `values`
protocol LayoutFields: GeneratedFields {
    init(id: UInt32, name: String, values: [UInt16])
}

let layoutsMessages = [
    layoutMessage(Uint8OffsetsFields.self, "Uint8Offsets", .uint8) { bytes throws(DecodeError) in
        let message = Uint8Offsets(bytes)
        return try Uint8OffsetsFields(id: message.id, name: message.name, values: Array(message.values))
    },
    layoutMessage(Uint8OffsetsSizedFields.self, "Uint8OffsetsSized", .uint8, sized: true) { bytes throws(DecodeError) in
        let message = Uint8OffsetsSized(bytes)
        return try Uint8OffsetsSizedFields(id: message.id, name: message.name, values: Array(message.values))
    },
    layoutMessage(
        Uint8OffsetsVersionedFields.self,
        "Uint8OffsetsVersioned",
        .uint8,
        version: 255,
        versionOf: { bytes throws(DecodeError) in try Uint8OffsetsVersioned.version(of: bytes) }
    ) { bytes throws(DecodeError) in
        let message = try Uint8OffsetsVersioned(bytes)
        return try Uint8OffsetsVersionedFields(id: message.id, name: message.name, values: Array(message.values))
    },
    layoutMessage(
        Uint8OffsetsSizedVersionedFields.self,
        "Uint8OffsetsSizedVersioned",
        .uint8,
        sized: true,
        version: 0,
        versionOf: { bytes throws(DecodeError) in try Uint8OffsetsSizedVersioned.version(of: bytes) }
    ) { bytes throws(DecodeError) in
        let message = try Uint8OffsetsSizedVersioned(bytes)
        return try Uint8OffsetsSizedVersionedFields(id: message.id, name: message.name, values: Array(message.values))
    },
    layoutMessage(Uint16OffsetsFields.self, "Uint16Offsets", .uint16) { bytes throws(DecodeError) in
        let message = Uint16Offsets(bytes)
        return try Uint16OffsetsFields(id: message.id, name: message.name, values: Array(message.values))
    },
    layoutMessage(Uint16OffsetsSizedFields.self, "Uint16OffsetsSized", .uint16, sized: true) {
        bytes throws(DecodeError) in
        let message = Uint16OffsetsSized(bytes)
        return try Uint16OffsetsSizedFields(id: message.id, name: message.name, values: Array(message.values))
    },
    layoutMessage(
        Uint16OffsetsVersionedFields.self,
        "Uint16OffsetsVersioned",
        .uint16,
        version: 65535,
        versionOf: { bytes throws(DecodeError) in try Uint16OffsetsVersioned.version(of: bytes) }
    ) { bytes throws(DecodeError) in
        let message = try Uint16OffsetsVersioned(bytes)
        return try Uint16OffsetsVersionedFields(id: message.id, name: message.name, values: Array(message.values))
    },
    layoutMessage(
        Uint16OffsetsSizedVersionedFields.self,
        "Uint16OffsetsSizedVersioned",
        .uint16,
        sized: true,
        version: 1,
        versionOf: { bytes throws(DecodeError) in try Uint16OffsetsSizedVersioned.version(of: bytes) }
    ) { bytes throws(DecodeError) in
        let message = try Uint16OffsetsSizedVersioned(bytes)
        return try Uint16OffsetsSizedVersionedFields(id: message.id, name: message.name, values: Array(message.values))
    },
    layoutMessage(Uint32OffsetsFields.self, "Uint32Offsets", .uint32) { bytes throws(DecodeError) in
        let message = Uint32Offsets(bytes)
        return try Uint32OffsetsFields(id: message.id, name: message.name, values: Array(message.values))
    },
    layoutMessage(Uint32OffsetsSizedFields.self, "Uint32OffsetsSized", .uint32, sized: true) {
        bytes throws(DecodeError) in
        let message = Uint32OffsetsSized(bytes)
        return try Uint32OffsetsSizedFields(id: message.id, name: message.name, values: Array(message.values))
    },
    layoutMessage(
        Uint32OffsetsVersionedFields.self,
        "Uint32OffsetsVersioned",
        .uint32,
        version: 4_294_967_295,
        versionOf: { bytes throws(DecodeError) in try Uint32OffsetsVersioned.version(of: bytes) }
    ) { bytes throws(DecodeError) in
        let message = try Uint32OffsetsVersioned(bytes)
        return try Uint32OffsetsVersionedFields(id: message.id, name: message.name, values: Array(message.values))
    },
    layoutMessage(
        Uint32OffsetsSizedVersionedFields.self,
        "Uint32OffsetsSizedVersioned",
        .uint32,
        sized: true,
        version: 2,
        versionOf: { bytes throws(DecodeError) in try Uint32OffsetsSizedVersioned.version(of: bytes) }
    ) { bytes throws(DecodeError) in
        let message = try Uint32OffsetsSizedVersioned(bytes)
        return try Uint32OffsetsSizedVersionedFields(id: message.id, name: message.name, values: Array(message.values))
    },
]

extension Uint8OffsetsFields: LayoutFields {}
extension Uint8OffsetsSizedFields: LayoutFields {}
extension Uint8OffsetsVersionedFields: LayoutFields {}
extension Uint8OffsetsSizedVersionedFields: LayoutFields {}
extension Uint16OffsetsFields: LayoutFields {}
extension Uint16OffsetsSizedFields: LayoutFields {}
extension Uint16OffsetsVersionedFields: LayoutFields {}
extension Uint16OffsetsSizedVersionedFields: LayoutFields {}
extension Uint32OffsetsFields: LayoutFields {}
extension Uint32OffsetsSizedFields: LayoutFields {}
extension Uint32OffsetsVersionedFields: LayoutFields {}
extension Uint32OffsetsSizedVersionedFields: LayoutFields {}

/// A message of the schema, in the layout that its attributes set, whose
/// fields are drawn long enough to make a table of `uint8` offsets too long
/// at times
private func layoutMessage<Fields: LayoutFields>(
    _ type: Fields.Type,
    _ name: String,
    _ offsetType: OffsetType,
    sized: Bool = false,
    version: UInt32? = nil,
    versionOf: (@Sendable (RawSpan) throws(DecodeError) -> UInt32)? = nil,
    read: @escaping @Sendable (RawSpan) throws(DecodeError) -> Fields
) -> Message {
    Message(
        "layouts.\(name)",
        layout: TableLayout(sized: sized, version: version, offsetType: offsetType),
        version: version,
        draw: { draw in Fields(id: draw.integer(), name: draw.string(most: 48), values: draw.integers(most: 64)) },
        read: read,
        versionOf: versionOf
    )
}

@Suite("Generated layouts")
struct LayoutsTests {
    // The length and the version in the header, as wide as the offsets, and
    // then the offsets, the id, the name's count and bytes, and the values'
    // count and elements, aligned to 2 from the table's start
    @Test func writesTheHeaderThatTheAttributesSet() throws {
        let fields = Uint8OffsetsSizedVersionedFields(id: 1, name: "a", values: [2])

        #expect(try fields.serialize() == [14, 0, 5, 9, 11, 1, 0, 0, 0, 1, 97, 1, 2, 0])
        #expect(Uint8OffsetsSizedVersioned.formatVersion == 0)
        #expect(Uint8OffsetsSizedVersioned.layout == TableLayout(sized: true, version: 0, offsetType: .uint8))
    }

    // A field that makes a table of uint8 offsets longer than 255 bytes fails
    // as TableBuilder's setter of the field fails: the values after a byte
    // of padding, their count and 8 bytes
    @Test func throwsForATableTooLongForItsOffsets() throws {
        let fields = Uint8OffsetsFields(id: 1, name: String(repeating: "a", count: 240), values: [1, 2, 3, 4])

        #expect(throws: BuildError.tooLong(index: 2, length: 3 + 4 + 1 + 240 + 1 + 1 + 8, offsetType: .uint8)) {
            try fields.serialize()
        }
    }
}
