import GeneratedPresence
import Tafla
import Testing

// The code generated from typescript/core/test/presence.tafla: optional
// fields, which its writers leave out with the offset 0, and fields that a
// later version of a message or a later revision of a table appends, which
// its readers find missing from the tables that don't hold them, as `nil`,
// or as a scalar's default, which `has<Field>` tells apart. The messages and
// tables named `V1` are the first version or revision of the others.
// GeneratedTests.swift checks the properties that every message has.

let presenceMessages = [
    Message(
        "presence.Record",
        layout: TableLayout(sized: true, version: 0),
        version: 2,
        firstVersion: 1,
        draw: { $0.presenceRecord() },
        read: { bytes throws(DecodeError) in try RecordFields(reading: Record(bytes)) },
        versionOf: { bytes throws(DecodeError) in try Record.version(of: bytes) }
    ),
    Message(
        "presence.RecordV1",
        layout: TableLayout(sized: true, version: 0),
        version: 1,
        draw: { $0.presenceRecord().firstVersion },
        read: { bytes throws(DecodeError) in try RecordV1Fields(reading: RecordV1(bytes)) },
        versionOf: { bytes throws(DecodeError) in try RecordV1.version(of: bytes) }
    ),
    Message(
        "presence.Sample",
        layout: .default,
        draw: { SampleFields(key: $0.integer(), count: $0.integer()) },
        read: { bytes throws(DecodeError) in
            let sample = Sample(bytes)
            return try SampleFields(key: sample.key, count: sample.count)
        }
    ),
    Message(
        "presence.SampleV1",
        layout: .default,
        draw: { SampleV1Fields(key: $0.integer()) },
        read: { bytes throws(DecodeError) in try SampleV1Fields(key: SampleV1(bytes).key) }
    ),
]

let presenceStructs = [StructCase("presence.Extent", Extent.self, size: 8) { $0.presenceExtent() }]

extension RecordFields: GeneratedFields {}
extension RecordV1Fields: GeneratedFields {}
extension SampleFields: GeneratedFields {}
extension SampleV1Fields: GeneratedFields {}
extension Extent: GeneratedStruct {}

// The defaults that the schema gives the scalars that may be missing
let flagDefault = true
let totalDefault: Int64 = -1
let zDefault = -0.5
let weightDefault: Float = 0.1
let countDefault: UInt16 = 7

extension Draw {
    /// A value, or `nil` as often as not
    mutating func optional<T>(_ value: (inout Draw) -> T) -> T? {
        bool() ? value(&self) : nil
    }

    mutating func presenceRecord() -> RecordFields {
        RecordFields(
            id: integer(),
            name: optional { $0.string(most: 8) },
            values: optional { $0.doubles(most: 4) },
            origin: optional { PointFields(x: $0.double(), y: $0.double(), z: $0.double()) },
            flag: optional { $0.bool() },
            entries: optional { $0.array(most: 3) { $0.presenceEntry() } },
            total: optional { $0.integer() },
            label: string(most: 8),
            extent: optional { $0.presenceExtent() }
        )
    }

    mutating func presenceEntry() -> EntryFields {
        EntryFields(key: integer(), note: optional { $0.string(most: 6) }, weight: float())
    }

    mutating func presenceExtent() -> Extent {
        Extent(width: float(), height: float())
    }
}

extension RecordFields {
    /// Reads every field through the getters, an absent one as `nil`, as
    /// `has<Field>` finds a scalar
    init(reading record: borrowing Record) throws(DecodeError) {
        var values: [Double]?
        if let view = try record.values {
            values = Array(view)
        }
        var origin: PointFields?
        if let point = try record.origin {
            origin = try PointFields(x: point.x, y: point.y, z: point.z)
        }
        var entries: [EntryFields]?
        if let view = try record.entries {
            var items: [EntryFields] = []
            for index in view.indices {
                let entry = try view[index]
                items.append(try EntryFields(key: entry.key, note: entry.note, weight: entry.weight))
            }
            entries = items
        }

        try self.init(
            id: record.id,
            name: record.name,
            values: values,
            origin: origin,
            flag: record.hasFlag ? record.flag : nil,
            entries: entries,
            total: record.hasTotal ? record.total : nil,
            // Which a record of the first version doesn't hold
            label: record.label ?? "",
            extent: record.extent
        )
    }

    /// The fields of the first version, without those that the second added
    var firstVersion: RecordV1Fields {
        RecordV1Fields(
            id: id,
            name: name,
            values: values,
            origin: origin.map { PointV1Fields(x: $0.x, y: $0.y) },
            flag: flag,
            entries: entries?.map { EntryV1Fields(key: $0.key, note: $0.note) },
            total: total
        )
    }
}

extension RecordV1Fields {
    init(reading record: borrowing RecordV1) throws(DecodeError) {
        var values: [Double]?
        if let view = try record.values {
            values = Array(view)
        }
        var origin: PointV1Fields?
        if let point = try record.origin {
            origin = try PointV1Fields(x: point.x, y: point.y)
        }
        var entries: [EntryV1Fields]?
        if let view = try record.entries {
            var items: [EntryV1Fields] = []
            for index in view.indices {
                let entry = try view[index]
                items.append(try EntryV1Fields(key: entry.key, note: entry.note))
            }
            entries = items
        }

        try self.init(
            id: record.id,
            name: record.name,
            values: values,
            origin: origin,
            flag: record.hasFlag ? record.flag : nil,
            entries: entries,
            total: record.hasTotal ? record.total : nil
        )
    }
}

/// A scalar that may be missing, as its getter reads it, and whether
/// `has<Field>` finds it in the table
struct Held<Value> {
    var value: Value
    var held: Bool
}

/// Every field of a record as the getters read it: `nil` for each that the
/// table doesn't hold, but a scalar that may be missing, which reads as its
/// default
struct ReadRecord {
    var id: UInt32
    var name: String?
    var values: [Double]?
    var origin: ReadPoint?
    var flag: Held<Bool>
    var entries: [ReadEntry]?
    var total: Held<Int64>
    var label: String?
    var extent: Extent?
}

struct ReadPoint {
    var x: Double
    var y: Double
    var z: Held<Double>
}

struct ReadEntry {
    var key: UInt16
    var note: String?
    var weight: Held<Float>
}

extension ReadRecord {
    init(reading record: borrowing Record) throws(DecodeError) {
        var values: [Double]?
        if let view = try record.values {
            values = Array(view)
        }
        var origin: ReadPoint?
        if let point = try record.origin {
            origin = try ReadPoint(x: point.x, y: point.y, z: Held(value: point.z, held: point.hasZ))
        }
        var entries: [ReadEntry]?
        if let view = try record.entries {
            var items: [ReadEntry] = []
            for index in view.indices {
                let entry = try view[index]
                let weight = try Held(value: entry.weight, held: entry.hasWeight)
                items.append(try ReadEntry(key: entry.key, note: entry.note, weight: weight))
            }
            entries = items
        }

        try self.init(
            id: record.id,
            name: record.name,
            values: values,
            origin: origin,
            flag: Held(value: record.flag, held: record.hasFlag),
            entries: entries,
            total: Held(value: record.total, held: record.hasTotal),
            label: record.label,
            extent: record.extent
        )
    }

    /// A record of `fields` as the getters read it: each absent field `nil`,
    /// or its default
    init(expected fields: RecordFields) {
        self.init(
            id: fields.id,
            name: fields.name,
            values: fields.values,
            origin: fields.origin.map { ReadPoint(x: $0.x, y: $0.y, z: Held(value: $0.z, held: true)) },
            flag: Held(value: fields.flag ?? flagDefault, held: fields.flag != nil),
            entries: fields.entries?.map {
                ReadEntry(key: $0.key, note: $0.note, weight: Held(value: $0.weight, held: true))
            },
            total: Held(value: fields.total ?? totalDefault, held: fields.total != nil),
            label: fields.label,
            extent: fields.extent
        )
    }

    /// A record of the first version's `fields` as the getters of the second
    /// read it: each field that the second version, or the second revision of
    /// a table, added missing
    init(expectedFromFirstVersion fields: RecordV1Fields) {
        self.init(
            id: fields.id,
            name: fields.name,
            values: fields.values,
            origin: fields.origin.map { ReadPoint(x: $0.x, y: $0.y, z: Held(value: zDefault, held: false)) },
            flag: Held(value: fields.flag ?? flagDefault, held: fields.flag != nil),
            entries: fields.entries?.map {
                ReadEntry(key: $0.key, note: $0.note, weight: Held(value: weightDefault, held: false))
            },
            total: Held(value: fields.total ?? totalDefault, held: fields.total != nil),
            label: nil,
            extent: nil
        )
    }
}

@Suite("Generated presence")
struct PresenceTests {
    // At any byte offset from an aligned address
    @Test func readsEveryFieldOfARecordAndNilOrTheDefaultForEachAbsentOneWhereverItsBytesLie() async {
        await checkDrawn { draw in
            (fields: draw.presenceRecord(), byteOffset: draw.int(0...7))
        } _: { drawn in
            let bytes = try drawn.fields.serialize()

            let read = try withSpan(of: bytes, atByteOffset: drawn.byteOffset) { bytes throws in
                try ReadRecord(reading: Record(bytes))
            }
            #expect(sameValues(read, ReadRecord(expected: drawn.fields)))
        }
    }

    // A record of an id and a label, the only fields that aren't optional
    @Test func writesAnAbsentFieldAsTableBuilderSetAbsentDoes() throws {
        var builder = try TableBuilder(fields: 9, layout: TableLayout(sized: true, version: Record.formatVersion))
        try builder.setUInt32(7)
        for _ in 1..<7 {
            try builder.setAbsent()
        }
        try builder.setString("a")
        try builder.setAbsent()

        #expect(try RecordFields(id: 7, label: "a").serialize() == builder.bytes())
    }

    // In nested tables and vector items too, which have no version, and whose
    // first revision holds fewer fields
    @Test func readsARecordOfTheFirstVersionWithTheFieldsThatTheSecondAddedMissing() async {
        await checkDrawn { draw in
            draw.presenceRecord().firstVersion
        } _: { fields in
            let read = try withSpan(of: fields.serialize()) { bytes throws in try ReadRecord(reading: Record(bytes)) }
            #expect(sameValues(read, ReadRecord(expectedFromFirstVersion: fields)))
        }
    }

    @Test func readsTheVersionsFromTheFirstToItsOwnAndRejectsOthers() throws {
        #expect(Record.firstVersion == 1)
        #expect(Record.formatVersion == 2)

        var bytes = try RecordFields(id: 1, name: "a", label: "b").serialize()
        for version: UInt32 in [0, 1, 2, 3, .max] {
            // After the table's length
            bytes.replaceSubrange(4..<8, with: littleEndianBytes(version))

            try withSpan(of: bytes) { bytes throws in
                #expect(try Record.version(of: bytes) == version)

                if version == 1 || version == 2 {
                    let record = try Record(bytes)
                    #expect(try record.name == "a")
                    #expect(try record.label == (version == 2 ? "b" : nil))
                } else {
                    #expect(throws: DecodeError.versionOutsideRange(version: version, supported: 1...2)) {
                        _ = try Record(bytes)
                    }
                }
            }
        }
    }

    @Test func keepsRejectingOtherVersionsWithAReaderOfOneVersion() throws {
        let bytes = try RecordFields(id: 1, label: "").serialize()

        withSpan(of: bytes) { bytes in
            #expect(throws: DecodeError.unsupportedVersion(version: 2, supported: 1)) {
                _ = try RecordV1(bytes)
            }
            return
        }
    }

    // A message without a version finds the field that its second revision
    // appended missing from a table of the first, whose reader reads the
    // fields that it knows of the second's
    @Test func readsASampleOfTheFirstRevisionWithTheDefaultOfTheFieldThatTheSecondAppended() async {
        await checkDrawn { draw in
            (key: draw.integer() as UInt32, count: draw.integer() as UInt16)
        } _: { drawn in
            let first = try SampleV1Fields(key: drawn.key).serialize()
            let second = try SampleFields(key: drawn.key, count: drawn.count).serialize()

            try withSpan(of: first) { bytes throws in
                let sample = Sample(bytes)
                #expect(try sample.key == drawn.key)
                #expect(try sample.count == countDefault)
                #expect(try sample.hasCount == false)
            }
            try withSpan(of: second) { bytes throws in
                let sample = Sample(bytes)
                #expect(try sample.count == drawn.count)
                #expect(try sample.hasCount)
                #expect(try SampleV1(bytes).key == drawn.key)
            }
        }
    }

    // Through every getter and `has<Field>`, in the nested tables and vector
    // items too, whose first offsets, which decide whether they hold a later
    // revision's fields, can be overwritten or cut as well
    @Test func readsACorruptedRecordAsValuesNilDefaultsOrDecodeErrors() async {
        await checkDrawn { draw in
            (
                fields: draw.presenceRecord(),
                overwrites: draw.array(most: 4) { ($0.int(0...255), $0.integer() as UInt8) },
                cut: draw.int(0...255)
            )
        } _: { drawn in
            var bytes = try drawn.fields.serialize()
            for (position, byte) in drawn.overwrites {
                bytes[position % bytes.count] = byte
            }
            bytes.removeLast(drawn.cut % (bytes.count + 1))

            // Every error that a read throws is a decode error
            withSpan(of: bytes) { bytes in
                _ = try? ReadRecord(reading: Record(bytes))
            }
        }
    }

    // The README's example
    @Test func readsTheDefaultOfAnAbsentScalarAndNilForAnAbsentArray() throws {
        let bytes = try RecordFields(id: 7, label: "a").serialize()  // every optional field left out

        let all = bytes[...]
        let record = try Record(all.span.bytes)  // reads versions 1 and 2
        var read: [Double]?
        if let values = try record.values {  // nil, left out
            read = Array(values)
        }
        let flag = try record.flag  // true, the schema's default
        let hasFlag = try record.hasFlag  // false

        #expect(read == nil)
        #expect(flag)
        #expect(!hasFlag)
    }

    @Test func readsArbitraryBytesAsASampleWithoutAVersionAsValuesOrDecodeErrors() async {
        await checkDrawn { draw in
            draw.integers(most: 48) as [UInt8]
        } _: { bytes in
            withSpan(of: bytes) { bytes in
                let sample = Sample(bytes)
                _ = try? sample.key
                _ = try? sample.count
                _ = try? sample.hasCount
            }
        }
    }
}
