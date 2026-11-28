import GeneratedAllTypes
import Tafla
import Testing

// The code generated from schemas/all_types.tafla, of every type of field:
// how the tests draw and read its messages and structs, fields named after
// keywords, trees of tables that hold vectors of themselves, enums, whose
// fields hold any integer of their types, named or not, and vectors of
// strings, a string of which fails to read on its own.
// GeneratedTests.swift checks the properties of every message and struct of
// the example schemas.

let allTypesMessages = [
    Message(
        "all_types.Scalars",
        layout: .default,
        draw: { $0.scalars() },
        read: { bytes throws(DecodeError) in try ScalarsFields(reading: Scalars(bytes)) }
    ),
    Message(
        "all_types.Arrays",
        layout: .default,
        draw: { $0.arrays() },
        read: { bytes throws(DecodeError) in try ArraysFields(reading: Arrays(bytes)) }
    ),
    Message(
        "all_types.Nested",
        layout: .default,
        draw: { $0.nested() },
        read: { bytes throws(DecodeError) in try NestedFields(reading: GeneratedAllTypes.Nested(bytes)) }
    ),
    Message(
        "all_types.Reserved",
        layout: .default,
        draw: { $0.reserved() },
        read: { bytes throws(DecodeError) in try ReservedFields(reading: Reserved(bytes)) }
    ),
    Message(
        "all_types.Structs",
        layout: .default,
        draw: { $0.structs() },
        read: { bytes throws(DecodeError) in try StructsFields(reading: Structs(bytes)) }
    ),
    Message(
        "all_types.Enums",
        layout: .default,
        draw: { $0.enums() },
        read: { bytes throws(DecodeError) in try EnumsFields(reading: Enums(bytes)) }
    ),
    Message(
        "all_types.StringVectors",
        layout: .default,
        draw: { $0.stringVectors() },
        read: { bytes throws(DecodeError) in try StringVectorsFields(reading: StringVectors(bytes)) }
    ),
    Message(
        "all_types.NarrowStringVectors",
        layout: TableLayout(offsetType: .uint16),
        draw: { $0.narrowStringVectors() },
        read: { bytes throws(DecodeError) in try NarrowStringVectorsFields(reading: NarrowStringVectors(bytes)) }
    ),
]

let allTypesStructs = [
    StructCase("all_types.Segment", Segment.self, size: 16) { $0.segment() },
    StructCase("all_types.Point", Point.self, size: 8) { $0.point() },
    StructCase("all_types.Packed", Packed.self, size: 43) { $0.packed() },
    StructCase("all_types.Documented", Documented.self, size: 7) { $0.documented() },
    StructCase("all_types.Mark", Mark.self, size: 24) { $0.mark() },
    StructCase("all_types.Turn", Turn.self, size: 9) { $0.turn() },
]

extension ScalarsFields: GeneratedFields {}
extension ArraysFields: GeneratedFields {}
extension NestedFields: GeneratedFields {}
extension ReservedFields: GeneratedFields {}
extension StructsFields: GeneratedFields {}
extension EnumsFields: GeneratedFields {}
extension StringVectorsFields: GeneratedFields {}
extension NarrowStringVectorsFields: GeneratedFields {}
extension Segment: GeneratedStruct {}
extension Point: GeneratedStruct {}
extension Packed: GeneratedStruct {}
extension Documented: GeneratedStruct {}
extension Mark: GeneratedStruct {}
extension Turn: GeneratedStruct {}

// The values that each enum names, in the order that the schema declares them

extension Level: GeneratedEnum {
    static let named: [Level] = [.low, .medium, .high]
}

extension Direction: GeneratedEnum {
    static let named: [Direction] = [.backward, .none, .forward, .furthestBack]
}

extension Unit: GeneratedEnum {
    static let named: [Unit] = [.last, .first, .v2]
}

extension Change: GeneratedEnum {
    static let named: [Change] = [.least, .greatest]
}

extension Status: GeneratedEnum {
    static let named: [Status] = [.ok, .notFound, .greatest]
}

extension Sign: GeneratedEnum {
    static let named: [Sign] = [.least, .greatest]
}

extension Mask: GeneratedEnum {
    static let named: [Mask] = [.none, .one, .beyondFloat64, .all]
}

extension Extent: GeneratedEnum {
    static let named: [Extent] = [.least, .zero, .greatest]
}

// Keywords after a dot are members, but for `self`, which names the type,
// and `init`, which names the initializer, unless it's in backticks
extension Word: GeneratedEnum {
    static let named: [Word] = [
        .class, .default, .in, .match, .self_, .type, .true, .null, .none, .`init`, .new, .name, .value,
    ]
}

/// How deep the tests draw trees of children, and read them back: a tree's
/// children, and theirs. Corrupted bytes can make a tree hold itself, so a
/// reader bounds how deep it reads.
private let treeDepth = 2

extension Draw {
    fileprivate mutating func scalars() -> ScalarsFields {
        ScalarsFields(
            int8: integer(),
            uint8: integer(),
            int16: integer(),
            uint16: integer(),
            int32: integer(),
            uint32: integer(),
            int64: integer(),
            uint64: integer(),
            float32: float(),
            float64: double(),
            bool: bool()
        )
    }

    fileprivate mutating func arrays() -> ArraysFields {
        ArraysFields(
            int8: integers(most: 6),
            uint8: integers(most: 6),
            int16: integers(most: 6),
            uint16: integers(most: 6),
            int32: integers(most: 6),
            uint32: integers(most: 6),
            int64: integers(most: 6),
            uint64: integers(most: 6),
            float32: floats(most: 6),
            float64: doubles(most: 6)
        )
    }

    fileprivate mutating func nested() -> NestedFields {
        let outer = OuterFields(
            count: integer(),
            inner: InnerFields(name: string(most: 6), values: doubles(most: 4))
        )
        return NestedFields(label: string(most: 6), outer: outer, trees: array(most: 3) { $0.tree(depth: treeDepth) })
    }

    fileprivate mutating func tree(depth: Int) -> TreeFields {
        TreeFields(value: integer(), children: depth > 0 ? array(most: 2) { $0.tree(depth: depth - 1) } : [])
    }

    fileprivate mutating func reserved() -> ReservedFields {
        let keywords = KeywordsFields(
            class: integer(),
            default: integer(),
            in: integer(),
            match: integer(),
            self_: integer(),
            type: string(most: 6)
        )
        return ReservedFields(keywords: keywords)
    }

    fileprivate mutating func structs() -> StructsFields {
        StructsFields(
            packed: packed(),
            documented: array(most: 4) { $0.documented() },
            segment: segment(),
            segments: array(most: 4) { $0.segment() },
            path: PathFields(start: point(), points: array(most: 4) { $0.point() })
        )
    }

    fileprivate mutating func segment() -> Segment {
        Segment(start: point(), end: point())
    }

    fileprivate mutating func point() -> Point {
        Point(x: float(), y: float())
    }

    fileprivate mutating func packed() -> Packed {
        Packed(
            int8: integer(),
            uint8: integer(),
            int16: integer(),
            uint16: integer(),
            int32: integer(),
            uint32: integer(),
            int64: integer(),
            uint64: integer(),
            float32: float(),
            float64: double(),
            bool: bool()
        )
    }

    fileprivate mutating func documented() -> Documented {
        Documented(one: integer(), two: integer(), undocumented: integer())
    }

    fileprivate mutating func enums() -> EnumsFields {
        EnumsFields(
            level: value(),
            levels: integers(of: Level.self, most: 6),
            direction: value(),
            directions: integers(of: Direction.self, most: 6),
            unit: value(),
            units: integers(of: Unit.self, most: 6),
            change: value(),
            changes: integers(of: Change.self, most: 6),
            status: value(),
            statuses: integers(of: Status.self, most: 6),
            sign: value(),
            signs: integers(of: Sign.self, most: 6),
            mask: value(),
            masks: integers(of: Mask.self, most: 6),
            extent: value(),
            extents: integers(of: Extent.self, most: 6),
            word: value(),
            mark: mark(),
            marks: array(most: 4) { $0.mark() },
            steps: array(most: 3) { $0.step() }
        )
    }

    fileprivate mutating func step() -> StepFields {
        StepFields(level: value(), mark: mark(), name: string(most: 6))
    }

    fileprivate mutating func mark() -> Mark {
        Mark(level: value(), change: value(), weight: float(), extent: value(), turn: turn())
    }

    fileprivate mutating func turn() -> Turn {
        Turn(direction: value(), mask: value())
    }

    fileprivate mutating func stringVectors() -> StringVectorsFields {
        StringVectorsFields(
            id: integer(),
            tags: strings(),
            labels: LabelsFields(values: strings()),
            entries: array(most: 3) { EntryFields(key: $0.string(most: 6), aliases: $0.strings()) }
        )
    }

    /// Names that fit in a table of uint16 offsets
    fileprivate mutating func narrowStringVectors() -> NarrowStringVectorsFields {
        NarrowStringVectorsFields(names: strings(), count: integer())
    }

    /// Up to 6 strings, empty too, and of empty strings
    fileprivate mutating func strings() -> [String] {
        array(most: 6) { $0.string(most: 8) }
    }
}

extension ScalarsFields {
    init(reading scalars: borrowing Scalars) throws(DecodeError) {
        try self.init(
            int8: scalars.int8,
            uint8: scalars.uint8,
            int16: scalars.int16,
            uint16: scalars.uint16,
            int32: scalars.int32,
            uint32: scalars.uint32,
            int64: scalars.int64,
            uint64: scalars.uint64,
            float32: scalars.float32,
            float64: scalars.float64,
            bool: scalars.bool
        )
    }
}

extension ArraysFields {
    init(reading arrays: borrowing Arrays) throws(DecodeError) {
        try self.init(
            int8: Array(arrays.int8),
            uint8: Array(arrays.uint8),
            int16: Array(arrays.int16),
            uint16: Array(arrays.uint16),
            int32: Array(arrays.int32),
            uint32: Array(arrays.uint32),
            int64: Array(arrays.int64),
            uint64: Array(arrays.uint64),
            float32: Array(arrays.float32),
            float64: Array(arrays.float64)
        )
    }
}

extension NestedFields {
    init(reading nested: borrowing GeneratedAllTypes.Nested) throws(DecodeError) {
        let outer = try nested.outer
        let inner = try outer.inner
        let outerFields = try OuterFields(
            count: outer.count,
            inner: InnerFields(name: inner.name, values: Array(inner.values))
        )

        let view = try nested.trees
        var trees: [TreeFields] = []
        for index in view.indices {
            trees.append(try TreeFields(reading: view[index], depth: treeDepth))
        }

        try self.init(label: nested.label, outer: outerFields, trees: trees)
    }
}

extension TreeFields {
    /// Reads a tree, and its children `depth` levels down
    init(reading tree: borrowing Tree, depth: Int) throws(DecodeError) {
        let view = try tree.children

        var children: [TreeFields] = []
        if depth > 0 {
            for index in view.indices {
                children.append(try TreeFields(reading: view[index], depth: depth - 1))
            }
        }

        try self.init(value: tree.value, children: children)
    }
}

extension ReservedFields {
    init(reading reserved: borrowing Reserved) throws(DecodeError) {
        let keywords = try reserved.keywords
        try self.init(
            keywords: KeywordsFields(
                class: keywords.class,
                default: keywords.default,
                in: keywords.in,
                match: keywords.match,
                self_: keywords.self_,
                type: keywords.type
            )
        )
    }
}

extension StructsFields {
    init(reading structs: borrowing Structs) throws(DecodeError) {
        let path = try structs.path
        try self.init(
            packed: structs.packed,
            documented: Array(structs.documented),
            segment: structs.segment,
            segments: Array(structs.segments),
            path: PathFields(start: path.start, points: Array(path.points))
        )
    }
}

extension EnumsFields {
    init(reading enums: borrowing Enums) throws(DecodeError) {
        let view = try enums.steps
        var steps: [StepFields] = []
        for index in view.indices {
            steps.append(try StepFields(reading: view[index]))
        }

        try self.init(
            level: enums.level,
            levels: Array(enums.levels),
            direction: enums.direction,
            directions: Array(enums.directions),
            unit: enums.unit,
            units: Array(enums.units),
            change: enums.change,
            changes: Array(enums.changes),
            status: enums.status,
            statuses: Array(enums.statuses),
            sign: enums.sign,
            signs: Array(enums.signs),
            mask: enums.mask,
            masks: Array(enums.masks),
            extent: enums.extent,
            extents: Array(enums.extents),
            word: enums.word,
            mark: enums.mark,
            marks: Array(enums.marks),
            steps: steps
        )
    }
}

extension StepFields {
    init(reading step: borrowing Step) throws(DecodeError) {
        try self.init(level: step.level, mark: step.mark, name: step.name)
    }
}

extension StringVectorsFields {
    /// Reads every string of the tags and the labels at once, and each of an
    /// entry's aliases from its ends
    init(reading table: borrowing StringVectors) throws(DecodeError) {
        let view = try table.entries
        var entries: [EntryFields] = []
        for index in view.indices {
            entries.append(try EntryFields(reading: view[index]))
        }

        try self.init(
            id: table.id,
            tags: Array(table.tags),
            labels: LabelsFields(values: Array(table.labels.values)),
            entries: entries
        )
    }
}

extension EntryFields {
    init(reading entry: borrowing Entry) throws(DecodeError) {
        let view = try entry.aliases
        var aliases: [String] = []
        for index in view.indices {
            aliases.append(try view[index])
        }

        try self.init(key: entry.key, aliases: aliases)
    }
}

extension NarrowStringVectorsFields {
    init(reading table: borrowing NarrowStringVectors) throws(DecodeError) {
        try self.init(names: Array(table.names), count: table.count)
    }
}

@Suite("Generated all types")
struct AllTypesTests {
    // A struct's scalars lie in place in its bytes, those of a struct within
    // it in its place, and an array's structs one after another, which its
    // view reads each of when it's asked for
    @Test func readsTheStructsOfAnArrayEachWhenItsAskedFor() throws {
        let segments = [
            Segment(start: Point(x: 1, y: 2), end: Point(x: 3, y: 4)),
            Segment(start: Point(x: -0.0, y: .nan), end: Point(x: .infinity, y: 5)),
        ]
        let bytes = Segment.serializeArray(segments)
        #expect(bytes.count == 32)
        #expect(bytes[8..<12] == [0, 0, 0x40, 0x40])

        var fields = StructsFields.example
        fields.segments = segments
        let written = try fields.serialize()

        try withSpan(of: written) { bytes throws in
            let view = try Structs(bytes).segments
            #expect(view.count == 2)
            #expect(view.indices == 0..<2)
            #expect(view.bytes.byteCount == 32)
            #expect(sameValues(view[1], segments[1]))
            #expect(sameValues(Array(view), segments))
        }
    }

    // The view reads a struct within its bytes without checking its scalars
    // again, which its index's check keeps there
    @Test func stopsTheProgramForAnIndexOutsideAnArray() async {
        // Exit tests can't capture values before Swift 6.3
        await #expect(processExitsWith: .failure) { try Self.readSegment(-1) }
        await #expect(processExitsWith: .failure) { try Self.readSegment(2) }
        await #expect(processExitsWith: .failure) { try Self.readSegment(Int.max) }
    }

    /// Reads segment `index` of an array of two
    static func readSegment(_ index: Int) throws {
        var fields = StructsFields.example
        fields.segments = [
            Segment(start: Point(x: 1, y: 2), end: Point(x: 3, y: 4)),
            Segment(start: Point(x: 5, y: 6), end: Point(x: 7, y: 8)),
        ]
        let written = try fields.serialize()

        try withSpan(of: written) { bytes throws in
            _ = try Structs(bytes).segments[index]
        }
    }

    // Fields named after keywords are escaped, and `self`, which would hide
    // the instance in an initializer, takes a `_`
    @Test func writesAndReadsFieldsNamedAfterKeywords() throws {
        let keywords = KeywordsFields(class: 1, default: 2, in: 3, match: 4, self_: 5, type: "type")
        let bytes = try ReservedFields(keywords: keywords).serialize()

        try withSpan(of: bytes) { bytes throws in
            let read = try Reserved(bytes).keywords
            #expect(try read.class == 1)
            #expect(try read.default == 2)
            #expect(try read.in == 3)
            #expect(try read.self_ == 5)
            #expect(try read.type == "type")
        }
    }

    // A tree's vector of children holds tables of its own type, each read in
    // place when it's asked for
    @Test func readsATreeOfTablesInVectorsOfThemselves() throws {
        let leaf = TreeFields(value: 3, children: [])
        let tree = TreeFields(value: 1, children: [TreeFields(value: 2, children: [leaf, leaf]), leaf])
        let fields = NestedFields(
            label: "trees",
            outer: OuterFields(count: 1, inner: InnerFields(name: "inner", values: [0.5])),
            trees: [tree]
        )

        try withSpan(of: try fields.serialize()) { bytes throws in
            let trees = try GeneratedAllTypes.Nested(bytes).trees
            let grandchild = try trees[0].children[0].children[1]
            #expect(try grandchild.value == 3)
            #expect(try grandchild.children.isEmpty == true)
            #expect(sameValues(try NestedFields(reading: GeneratedAllTypes.Nested(bytes)), fields))
        }
    }

    @Test func stopsTheProgramForAnIndexOutOfAnArrayOfStructs() async {
        await #expect(processExitsWith: .failure) {
            let bytes = try StructsFields.example.serialize()
            try withSpan(of: bytes) { bytes throws in
                let documented = try Structs(bytes).documented
                _ = documented[documented.count]
            }
        }
    }

    // An enum's static members are the integers that the schema names, the
    // least and the greatest of each integer type among them
    @Test func namesTheIntegersThatTheSchemaNames() {
        #expect(Level.named.map(\.rawValue) == [0, 1, 2])
        #expect(Direction.named.map(\.rawValue) == [-1, 0, 1, -128])
        #expect(Unit.named.map(\.rawValue) == [65_535, 0, 2])
        #expect(Change.named.map(\.rawValue) == [-32_768, 32_767])
        #expect(Status.named.map(\.rawValue) == [0, 404, 4_294_967_295])
        #expect(Sign.named.map(\.rawValue) == [-2_147_483_648, 2_147_483_647])
        #expect(Mask.named.map(\.rawValue) == [0, 1, 9_007_199_254_740_993, 18_446_744_073_709_551_615])
        #expect(Extent.named.map(\.rawValue) == [-9_223_372_036_854_775_808, 0, 9_223_372_036_854_775_807])
        #expect(Word.named.map(\.rawValue) == Array(0...12))
    }

    // A field of an enum holds any integer of its type, which reads as the
    // enum's value of it, whether the enum names it or not
    @Test func readsEveryByteOfAFieldOfAnEnumAsAValue() throws {
        var bytes = try EnumsFields.example.serialize()
        let level = try withSpan(of: bytes) { bytes throws in try TableReader(bytes).rootOffset(0) }

        for byte in UInt8.min...UInt8.max {
            bytes[level] = byte
            let read = try withSpan(of: bytes) { bytes throws in try Enums(bytes).level }
            #expect(read.rawValue == byte)
        }
        #expect(Level(rawValue: 2) == .high)
        #expect(!Level.named.contains(Level(rawValue: 3)))
    }

    // A struct writes the integers of its enums, little-endian, as it writes
    // its other scalars
    @Test func writesTheValuesThatAnEnumNamesAsTheirIntegers() throws {
        let mark = Mark(
            level: .high,
            change: .least,
            weight: 0.5,
            extent: .greatest,
            turn: Turn(direction: .backward, mask: .beyondFloat64)
        )
        let bytes = mark.serialize()

        let integers: [[UInt8]] = [
            [2], littleEndianBytes(Int16.min), littleEndianBytes(Float(0.5).bitPattern),
            littleEndianBytes(Int64.max), [0xff], littleEndianBytes(UInt64(9_007_199_254_740_993)),
        ]
        #expect(bytes == integers.flatMap { $0 })
        #expect(try withSpan(of: bytes) { bytes throws in try Mark(bytes) } == mark)
    }

    // The names' count, ends and 64 KiB of strings make the table longer
    // than uint16 offsets address, which the sizer reports for the field, as
    // TableBuilder does
    @Test func rejectsNarrowStringVectorsTooLongForUInt16Offsets() throws {
        let fields = NarrowStringVectorsFields(
            names: [String(repeating: "a", count: 0x8000), String(repeating: "b", count: 0x8000)],
            count: 0
        )
        // Two offsets, and a count, two ends and the strings
        let tooLong = BuildError.tooLong(index: 0, length: 4 + 2 + 4 + 0x1_0000, offsetType: .uint16)

        #expect(throws: tooLong) {
            try fields.serialize()
        }
        #expect(throws: tooLong) {
            try build(fields, layout: NarrowStringVectors.layout, version: nil)
        }
    }

    // A tag of invalid UTF-8 fails to read, and so does reading every tag,
    // but the tags before and after it read, as do the other fields
    @Test func readsTheOtherTagsOfATagThatIsntValidUTF8() throws {
        let fields = StringVectorsFields(
            id: 1,
            tags: ["ok", "bad", "fine"],
            labels: LabelsFields(values: ["label"]),
            entries: []
        )
        var bytes = try fields.serialize()

        // The second tag's first byte, after the count, three ends and "ok"
        let tags = try withSpan(of: bytes) { bytes throws in try TableReader(bytes).rootOffset(1) }
        let bad = tags + 4 * 4 + 2
        bytes[bad] = 0xc0

        try withSpan(of: bytes) { bytes throws in
            let table = StringVectors(bytes)
            let view = try table.tags

            #expect(try view[0] == "ok")
            #expect(throws: DecodeError.invalidUTF8(position: bad)) {
                _ = try view[1]
            }
            #expect(try view[2] == "fine")
            #expect(throws: DecodeError.invalidUTF8(position: bad)) {
                _ = try Array(view)
            }
            #expect(try table.id == 1)
            #expect(try Array(table.labels.values) == ["label"])
        }
    }
}

extension StructsFields {
    /// Structs of every kind, with an array of structs of 7 bytes
    static let example = StructsFields(
        packed: Packed(
            int8: -1,
            uint8: 1,
            int16: -2,
            uint16: 2,
            int32: -3,
            uint32: 3,
            int64: -4,
            uint64: 4,
            float32: 0.5,
            float64: -0.0,
            bool: true
        ),
        documented: [Documented(one: 1, two: 2, undocumented: 3)],
        segment: Segment(start: Point(x: 1, y: 2), end: Point(x: 3, y: 4)),
        segments: [],
        path: PathFields(start: Point(x: 5, y: 6), points: [Point(x: 7, y: 8)])
    )
}

extension EnumsFields {
    /// Values of every enum, named and not, and arrays of their integers
    static let example = EnumsFields(
        level: .high,
        levels: [0, 3, 255],
        direction: .furthestBack,
        directions: [-1, 100],
        unit: .last,
        units: [2, 1_000],
        change: .least,
        changes: [],
        status: .notFound,
        statuses: [4_294_967_295, 7],
        sign: Sign(rawValue: 5),
        signs: [-2_147_483_648],
        mask: .all,
        masks: [9_007_199_254_740_993],
        extent: .least,
        extents: [0, -5],
        word: .`init`,
        mark: Mark(
            level: .low,
            change: .greatest,
            weight: 1.5,
            extent: .zero,
            turn: Turn(direction: .none, mask: .one)
        ),
        marks: [],
        steps: [
            StepFields(
                level: Level(rawValue: 9),
                mark: Mark(
                    level: .medium,
                    change: Change(rawValue: 0),
                    weight: 2.5,
                    extent: Extent(rawValue: 1),
                    turn: Turn(direction: .forward, mask: Mask(rawValue: 2))
                ),
                name: "step"
            )
        ]
    )
}
