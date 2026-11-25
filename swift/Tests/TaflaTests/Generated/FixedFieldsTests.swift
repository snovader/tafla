import GeneratedFixedFields
import Tafla
import Testing

// The code generated from schemas/fixed_fields.tafla, of fields whose sizes
// the schema fixes: scalars, structs, and tables of only such fields, in
// every offset type and header. Swift's generated code writes them field by
// field, as it writes any other. GeneratedTests.swift checks the properties
// of every message and struct of the example schemas; Overflow, which is
// always too long, has a test of its own.

let fixedFieldsMessages = [
    Message(
        "fixed_fields.Tiny",
        layout: TableLayout(sized: true, offsetType: .uint8),
        draw: { draw in TinyFields(id: draw.integer(), kind: draw.integer()) },
        read: { bytes throws(DecodeError) in
            let message = Tiny(bytes)
            return try TinyFields(id: message.id, kind: message.kind)
        }
    ),
    Message(
        "fixed_fields.Tagged",
        layout: TableLayout(sized: true, version: 0, offsetType: .uint8),
        version: 3,
        draw: { draw in TaggedFields(id: draw.integer(), position: draw.vec2(), label: draw.string(most: 80)) },
        read: { bytes throws(DecodeError) in
            let message = try Tagged(bytes)
            return try TaggedFields(id: message.id, position: message.position, label: message.label)
        },
        versionOf: { bytes throws(DecodeError) in try Tagged.version(of: bytes) }
    ),
    Message(
        "fixed_fields.Header",
        layout: TableLayout(offsetType: .uint8),
        draw: { draw in HeaderFields(kind: draw.integer(), length: draw.integer(), values: draw.integers(most: 260)) },
        read: { bytes throws(DecodeError) in
            let message = Header(bytes)
            return try HeaderFields(kind: message.kind, length: message.length, values: Array(message.values))
        }
    ),
    Message(
        "fixed_fields.Pixel",
        layout: TableLayout(sized: true, version: 0, offsetType: .uint16),
        version: 2,
        draw: { draw in PixelFields(position: draw.vec2(), color: draw.integer()) },
        read: { bytes throws(DecodeError) in
            let message = try Pixel(bytes)
            return try PixelFields(position: message.position, color: message.color)
        },
        versionOf: { bytes throws(DecodeError) in try Pixel.version(of: bytes) }
    ),
    Message(
        "fixed_fields.Strip",
        layout: TableLayout(offsetType: .uint16),
        draw: { draw in StripFields(id: draw.integer(), origin: draw.vec2(), values: draw.integers(most: 8)) },
        read: { bytes throws(DecodeError) in
            let message = Strip(bytes)
            return try StripFields(id: message.id, origin: message.origin, values: Array(message.values))
        }
    ),
    Message(
        "fixed_fields.Box",
        layout: TableLayout(version: 0),
        version: 1,
        draw: { draw in BoxFields(min: draw.vec2(), max: draw.vec2(), range: draw.range()) },
        read: { bytes throws(DecodeError) in
            let message = try Box(bytes)
            return try BoxFields(min: message.min, max: message.max, range: RangeFields(reading: message.range))
        },
        versionOf: { bytes throws(DecodeError) in try Box.version(of: bytes) }
    ),
    Message(
        "fixed_fields.Track",
        layout: .default,
        draw: { draw in
            TrackFields(
                id: draw.integer(),
                origin: draw.vec2(),
                frame: draw.frame(),
                points: draw.floats(most: 6),
                name: draw.string(most: 8)
            )
        },
        read: { bytes throws(DecodeError) in
            let message = Track(bytes)
            return try TrackFields(
                id: message.id,
                origin: message.origin,
                frame: FrameFields(reading: message.frame),
                points: Array(message.points),
                name: message.name
            )
        }
    ),
    Message(
        "fixed_fields.Label",
        layout: .default,
        draw: { draw in LabelFields(text: draw.string(most: 8)) },
        read: { bytes throws(DecodeError) in try LabelFields(text: Label(bytes).text) }
    ),
    Message(
        "fixed_fields.Span",
        layout: .default,
        draw: { draw in
            SpanFields(
                name: draw.string(most: 8),
                range: draw.range(),
                frame: draw.frame(),
                ranges: draw.array(most: 4) { $0.range() }
            )
        },
        read: { bytes throws(DecodeError) in
            let message = GeneratedFixedFields.Span(bytes)
            let view = try message.ranges

            var ranges: [RangeFields] = []
            for index in view.indices {
                ranges.append(try RangeFields(reading: view[index]))
            }

            return try SpanFields(
                name: message.name,
                range: RangeFields(reading: message.range),
                frame: FrameFields(reading: message.frame),
                ranges: ranges
            )
        }
    ),
]

let fixedFieldsStructs = [
    StructCase("fixed_fields.Vec2", Vec2.self, size: 8) { $0.vec2() },
    StructCase("fixed_fields.Quad", Quad.self, size: 64) { $0.quad() },
    StructCase("fixed_fields.Block", Block.self, size: 256) { $0.block() },
]

extension TinyFields: GeneratedFields {}
extension TaggedFields: GeneratedFields {}
extension HeaderFields: GeneratedFields {}
extension PixelFields: GeneratedFields {}
extension StripFields: GeneratedFields {}
extension BoxFields: GeneratedFields {}
extension TrackFields: GeneratedFields {}
extension LabelFields: GeneratedFields {}
extension SpanFields: GeneratedFields {}
extension Vec2: GeneratedStruct {}
extension Quad: GeneratedStruct {}
extension Block: GeneratedStruct {}

extension Draw {
    fileprivate mutating func vec2() -> Vec2 {
        Vec2(x: float(), y: float())
    }

    fileprivate mutating func quad() -> Quad {
        Quad(a: double(), b: double(), c: double(), d: double(), e: double(), f: double(), g: double(), h: double())
    }

    fileprivate mutating func block() -> Block {
        Block(first: quad(), second: quad(), third: quad(), fourth: quad())
    }

    fileprivate mutating func range() -> RangeFields {
        RangeFields(start: integer(), end: integer())
    }

    fileprivate mutating func frame() -> FrameFields {
        FrameFields(origin: vec2(), range: range(), flags: integer())
    }
}

extension RangeFields {
    init(reading range: borrowing GeneratedFixedFields.Range) throws(DecodeError) {
        try self.init(start: range.start, end: range.end)
    }
}

extension FrameFields {
    init(reading frame: borrowing Frame) throws(DecodeError) {
        try self.init(origin: frame.origin, range: RangeFields(reading: frame.range), flags: frame.flags)
    }
}

@Suite("Generated fixed fields")
struct FixedFieldsTests {
    // A message whose fixed fields alone are too long for its offset type,
    // which can never be written, fails as TableBuilder fails, naming the
    // field that makes it too long: its offsets, its id and the 256 bytes of
    // its block
    @Test func throwsForAMessageWhoseFixedFieldsAreTooLongAsTableBuilderDoes() async {
        await checkDrawn { draw in
            OverflowFields(id: draw.integer(), block: draw.block())
        } _: { fields in
            let error = BuildError.tooLong(index: 1, length: 2 + 1 + 256, offsetType: .uint8)

            #expect(throws: error) {
                try fields.serialize()
            }
            #expect(throws: error) {
                try build(fields, layout: TableLayout(offsetType: .uint8), version: nil)
            }
        }
    }
}
