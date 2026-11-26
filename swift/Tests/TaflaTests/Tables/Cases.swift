import Foundation
import Tafla

// Parses the conformance suite's cases.json, and reads the bytes of its
// cases. See conformance/README.md at the root of the repository.

/// The conformance suite, at the root of the repository
let conformance = URL(fileURLWithPath: #filePath)
    .deletingLastPathComponent()  // Tables
    .deletingLastPathComponent()  // TaflaTests
    .deletingLastPathComponent()  // Tests
    .deletingLastPathComponent()  // swift
    .deletingLastPathComponent()
    .appendingPathComponent("conformance")

/// The conformance suite's cases.json, with the bytes of each case
struct Suite: Sendable {
    var valid: [ValidCase] = []
    var equivalent: [ChangedCase] = []
    var invalid: [ChangedCase] = []

    /// The suite, loaded once
    static let shared: Suite = {
        do {
            return try Suite(directory: conformance)
        } catch {
            fatalError("Can't load the conformance suite: \(error)")
        }
    }()

    /// The cases of presence, which lie apart from the suite's others: tables
    /// with absent fields, and tables that a later revision reads
    static let presence: Suite = {
        do {
            return try Suite(directory: conformance.appendingPathComponent("presence"))
        } catch {
            fatalError("Can't load the cases of presence: \(error)")
        }
    }()

    init(directory: URL) throws {
        let contents = try Data(contentsOf: directory.appendingPathComponent("cases.json"))
        let suite = try JSONDecoder().decode(JSON.self, from: contents)

        for valid in try suite["valid"].array {
            let name = try valid["name"].string
            let table = try parseTable(valid["table"], name: name)
            self.valid.append(ValidCase(name: name, bytes: try read(valid["file"], in: directory), table: table))
        }

        for (key, path) in [("equivalent", \Suite.equivalent), ("invalid", \Suite.invalid)] {
            for changed in try suite[key].array {
                self[keyPath: path].append(
                    ChangedCase(
                        name: try changed["name"].string,
                        bytes: try read(changed["file"], in: directory),
                        validCase: try changed["case"].string,
                        field: key == "invalid" ? try changed["field"].int : 0
                    )
                )
            }
        }
    }

    /// The description of the valid case that an equivalent or invalid case
    /// names
    func table(of name: String) -> Table {
        guard let valid = valid.first(where: { $0.name == name }) else {
            preconditionFailure("There's no valid case \(name)")
        }

        return valid.table
    }
}

/// A valid table, with its description
struct ValidCase: Sendable {
    var name: String
    var bytes: [UInt8]
    var table: Table
}

/// A table whose bytes differ from those of a valid case, which describes
/// them. `field` is the root field that fails to read, for an invalid case.
struct ChangedCase: Sendable {
    var name: String
    var bytes: [UInt8]
    var validCase: String
    var field: Int
}

private func read(_ file: JSON, in directory: URL) throws -> [UInt8] {
    [UInt8](try Data(contentsOf: directory.appendingPathComponent(file.string)))
}

/// A description that cases.json doesn't hold as conformance/README.md
/// describes it
struct ParseError: Error, CustomStringConvertible {
    let description: String
}

/// A value of the parsed JSON, which throws when it isn't what a description
/// holds there.
///
/// `JSONDecoder` parses it, rather than `JSONSerialization`, which drops a
/// leading U+FEFF from strings, as the `strings` case shows, on macOS and
/// Linux alike. Both parse floats exactly.
enum JSON: Decodable {
    case null
    case bool(Bool)
    case number(Double)
    case string(String)
    case array([JSON])
    case object([String: JSON])

    init(from decoder: any Decoder) throws {
        let container = try decoder.singleValueContainer()

        if container.decodeNil() {
            self = .null
        } else if let bool = try? container.decode(Bool.self) {
            self = .bool(bool)
        } else if let number = try? container.decode(Double.self) {
            self = .number(number)
        } else if let string = try? container.decode(String.self) {
            self = .string(string)
        } else if let array = try? container.decode([JSON].self) {
            self = .array(array)
        } else {
            self = .object(try container.decode([String: JSON].self))
        }
    }

    subscript(key: String) -> JSON {
        get throws {
            guard case .object(let object) = self, let member = object[key] else {
                throw ParseError(description: "No \(key) in \(self)")
            }
            return member
        }
    }

    func has(_ key: String) -> Bool {
        guard case .object(let object) = self else {
            return false
        }
        return object[key] != nil
    }

    var array: [JSON] {
        get throws {
            guard case .array(let array) = self else {
                throw ParseError(description: "\(self) isn't an array")
            }
            return array
        }
    }

    var string: String {
        get throws {
            guard case .string(let string) = self else {
                throw ParseError(description: "\(self) isn't a string")
            }
            return string
        }
    }

    var bool: Bool {
        get throws {
            guard case .bool(let bool) = self else {
                throw ParseError(description: "\(self) isn't a bool")
            }
            return bool
        }
    }

    var int: Int {
        get throws {
            guard case .number(let number) = self, let int = Int(exactly: number) else {
                throw ParseError(description: "\(self) isn't an integer")
            }
            return int
        }
    }

    var double: Double {
        get throws {
            guard case .number(let number) = self else {
                throw ParseError(description: "\(self) isn't a number")
            }
            return number
        }
    }
}

private func parseTable(_ json: JSON, name: String) throws -> Table {
    let layout = try json["layout"]
    let offsetType: OffsetType =
        switch try layout["offsetType"].string {
        case "uint8": .uint8
        case "uint16": .uint16
        case "uint32": .uint32
        case let other: throw ParseError(description: "\(name): an offset type \(other)")
        }

    let tableLayout = TableLayout(
        sized: try layout["sized"].bool,
        version: try layout["versioned"].bool ? 0 : nil,
        offsetType: offsetType
    )

    return Table(
        layout: tableLayout,
        version: tableLayout.isVersioned ? UInt32(try json["version"].int) : nil,
        fields: try json["fields"].array.map { try parseField($0, name: name) },
        missing: json.has("missing") ? try json["missing"].int : nil
    )
}

private func parseField(_ json: JSON, name: String) throws -> Field {
    let type = try json["type"].string

    if let scalarType = scalarType(named: type) {
        return try parseScalar(json["value"], scalarType)
    }

    switch type {
    case "absent": return .absent
    case "bool": return .bool(try json["value"].bool)
    case "int8[]": return .int8Array(try json["value"].array.map { Int8(try $0.int) })
    case "uint8[]": return .uint8Array(try json["value"].array.map { UInt8(try $0.int) })
    case "int16[]": return .int16Array(try json["value"].array.map { Int16(try $0.int) })
    case "uint16[]": return .uint16Array(try json["value"].array.map { UInt16(try $0.int) })
    case "int32[]": return .int32Array(try json["value"].array.map { Int32(try $0.int) })
    case "uint32[]": return .uint32Array(try json["value"].array.map { UInt32(try $0.int) })
    case "int64[]": return .int64Array(try json["value"].array.map { try parseInteger($0) })
    case "uint64[]": return .uint64Array(try json["value"].array.map { try parseInteger($0) })
    case "float32[]": return .float32Array(bits: try json["value"].array.map { Float(try parseFloat($0)).bitPattern })
    case "float64[]": return .float64Array(bits: try json["value"].array.map { try parseFloat($0).bitPattern })
    case "string": return .string(try json["value"].string)
    case "struct":
        return .struct(try json["fields"].array.map { try parseField($0, name: name) })
    case "struct[]":
        let types = try json["fields"].array.map { field in
            guard let type = scalarType(named: try field.string) else {
                throw ParseError(description: "\(name): a struct of \(field)")
            }
            return type
        }
        let items = try json["items"].array.map { item in
            try zip(item.array, types).map { try parseScalar($0, $1) }
        }
        return .structArray(types: types, items: items)
    case "table":
        let alignment = json.has("alignment") ? Alignment(rawValue: try json["alignment"].int) : nil
        return .table(try parseNested(json, name: name), alignment: alignment)
    case "vector":
        return .vector(try json["items"].array.map { try parseNested($0, name: name) })
    default:
        throw ParseError(description: "\(name): a field of type \(type)")
    }
}

private func parseNested(_ json: JSON, name: String) throws -> Nested {
    let from: Source =
        switch try json["from"].string {
        case "builder": .builder
        case "bytes": .bytes
        case let other: throw ParseError(description: "\(name): a table from \(other)")
        }

    return Nested(from: from, table: try parseTable(json["table"], name: name))
}

private func scalarType(named name: String) -> ScalarType? {
    ScalarType.allCases.first { "\($0)" == name }
}

private func parseScalar(_ json: JSON, _ type: ScalarType) throws -> Field {
    switch type {
    case .int8: .int8(Int8(try json.int))
    case .uint8: .uint8(UInt8(try json.int))
    case .int16: .int16(Int16(try json.int))
    case .uint16: .uint16(UInt16(try json.int))
    case .int32: .int32(Int32(try json.int))
    case .uint32: .uint32(UInt32(try json.int))
    case .int64: .int64(try parseInteger(json))
    case .uint64: .uint64(try parseInteger(json))
    case .float32: .float32(Float(try parseFloat(json)))
    case .float64: .float64(try parseFloat(json))
    }
}

/// A 64-bit integer, which the cases write as a decimal string
func parseInteger<T: FixedWidthInteger>(_ json: JSON) throws -> T {
    guard let value = T(try json.string) else {
        throw ParseError(description: "\(json) isn't a \(T.self)")
    }
    return value
}

/// A float, which the cases write as a number, or by the name of a value that
/// JSON has no number for. NaN is the quiet NaN, which JavaScript writes, and
/// a float32 is the float64 that equals it, which narrows to it exactly.
func parseFloat(_ json: JSON) throws -> Double {
    guard case .string(let name) = json else {
        return try json.double
    }

    switch name {
    case "NaN": return .nan
    case "Infinity": return .infinity
    case "-Infinity": return -.infinity
    case "-0": return -0.0
    default: throw ParseError(description: "A float named \(name)")
    }
}
