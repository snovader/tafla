import Foundation
import GeneratedShapes
import Tafla
import Testing

// The code generated from schemas/shapes.tafla writes the tables of
// conformance/generated, which the generated TypeScript wrote, byte for byte,
// and reads back their values. conformance/generated/README.md describes the
// cases in shapes.json.

@Suite("Generated shapes cases")
struct ShapesCasesTests {
    @Test func writesEveryCaseByteForByte() throws {
        let cases = try loadShapeCases()
        #expect(cases.count > 0)

        for shapeCase in cases {
            #expect(try shapeCase.serialize() == shapeCase.bytes, "\(shapeCase.name)")
        }
    }

    @Test func readsTheValuesOfEveryCase() throws {
        for shapeCase in try loadShapeCases() {
            let read = try withSpan(of: shapeCase.bytes) { bytes throws in try shapeCase.read(bytes) }
            #expect(sameValues(read, shapeCase.fields), "\(shapeCase.name)")
        }
    }
}

/// A case of shapes.json: the bytes of a message or struct of the schema,
/// and the values of its fields, with how its generated code writes and reads
/// them
struct ShapeCase {
    var name: String
    var bytes: [UInt8]
    /// The struct of the fields that the case describes
    var fields: any Sendable
    var serialize: () throws -> [UInt8]
    var read: (RawSpan) throws -> any Sendable
}

func loadShapeCases() throws -> [ShapeCase] {
    let directory = conformance.appendingPathComponent("generated")
    let suite = try JSONDecoder().decode(
        JSON.self, from: Data(contentsOf: directory.appendingPathComponent("shapes.json")))
    guard try suite["schema"].string == "schemas/shapes.tafla" else {
        throw ParseError(description: "shapes.json describes another schema")
    }

    return try suite["cases"].array.map { json in
        let name = try json["name"].string
        let type = try json["type"].string
        let bytes = [UInt8](try Data(contentsOf: directory.appendingPathComponent(try json["file"].string)))
        let fields = try json["fields"]

        if type == "Sample" {
            let sample = try Sample(key: UInt32(fields["key"].int), a: float(fields["a"]), b: float(fields["b"]))
            try checkCount(of: fields, sample)
            return ShapeCase(
                name: name,
                bytes: bytes,
                fields: sample,
                serialize: { sample.serialize() },
                read: { bytes in try Sample(bytes) }
            )
        }

        guard let message = shapesMessages.first(where: { $0.name == "shapes.\(type)" }) else {
            throw ParseError(description: "\(name): shapes.tafla has no message \(type)")
        }
        let described = try messageFields(of: type, fields)
        try checkCount(of: fields, described)

        return ShapeCase(
            name: name,
            bytes: bytes,
            fields: described,
            serialize: { try message.serialize(described) },
            read: { bytes in try message.read(bytes) }
        )
    }
}

/// The struct of a message's fields that `json` describes, by each field's
/// name in the schema
private func messageFields(of type: String, _ json: JSON) throws -> any Sendable {
    switch type {
    case "Point":
        return try PointFields(
            x: parseFloat(json["x"]),
            y: parseFloat(json["y"]),
            id: parseInteger(json["id"]),
            name: json["name"].string
        )
    case "Document":
        let columns = try json["columns"]
        try checkCount(of: columns, ColumnsFields(base: 0, keys: [], a: [], b: []))
        return try DocumentFields(
            x: parseFloat(json["x"]),
            y: parseFloat(json["y"]),
            name: json["name"].string,
            columns: ColumnsFields(
                base: parseInteger(columns["base"]),
                keys: columns["keys"].array.map { UInt32(try $0.int) },
                a: columns["a"].array.map(float),
                b: columns["b"].array.map(float)
            )
        )
    case "ItemList":
        return try ItemListFields(items: json["items"].array.map(itemFields))
    case "Task":
        return try TaskFields(
            priority: Priority(rawValue: Int8(json["priority"].int)),
            history: json["history"].array.map { UInt16(try $0.int) },
            step: step(json["step"]),
            name: json["name"].string
        )
    default:
        throw ParseError(description: "shapes.tafla has no message \(type)")
    }
}

/// A step, whose enums the case holds as their integers
private func step(_ json: JSON) throws -> Step {
    let step = try Step(
        state: State(rawValue: UInt16(json["state"].int)),
        priority: Priority(rawValue: Int8(json["priority"].int)),
        seconds: UInt32(json["seconds"].int)
    )
    try checkCount(of: json, step)
    return step
}

private func itemFields(_ json: JSON) throws -> ItemFields {
    let pair = try json["pair"]
    return try ItemFields(
        value: float(json["value"]), pair: PairFields(first: float(pair["first"]), second: float(pair["second"])))
}

/// A float32, which a case writes as the float64 that equals it
private func float(_ json: JSON) throws -> Float {
    Float(try parseFloat(json))
}

/// Checks that `json` describes every field of `fields` and no other
private func checkCount(of json: JSON, _ fields: Any) throws {
    guard case .object(let object) = json, object.count == Mirror(reflecting: fields).children.count else {
        throw ParseError(description: "\(json) doesn't describe every field of \(type(of: fields))")
    }
}
