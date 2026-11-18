import GeneratedAllTypes
import Tafla
import Testing

// The code generated from schemas/all_types.tafla, of every type of field:
// how the tests draw and read its messages and structs, fields named after
// keywords, and trees of tables that hold vectors of themselves.
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
]

let allTypesStructs = [
    StructCase("all_types.Segment", Segment.self, size: 16) { $0.segment() },
    StructCase("all_types.Point", Point.self, size: 8) { $0.point() },
    StructCase("all_types.Packed", Packed.self, size: 43) { $0.packed() },
    StructCase("all_types.Documented", Documented.self, size: 7) { $0.documented() },
]

extension ScalarsFields: GeneratedFields {}
extension ArraysFields: GeneratedFields {}
extension NestedFields: GeneratedFields {}
extension ReservedFields: GeneratedFields {}
extension StructsFields: GeneratedFields {}
extension Segment: GeneratedStruct {}
extension Point: GeneratedStruct {}
extension Packed: GeneratedStruct {}
extension Documented: GeneratedStruct {}

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
