// Lists of strings written and read by the code that tafla-gen generates
// from `lists.tafla`, in `Sources/GeneratedLists`, which the lists suite
// times: a vector of strings, and a vector of tables of one string each,
// which is how a schema held a list of strings without one.

import GeneratedLists
import Tafla
import Workloads

/// Writes and reads a list of strings
public protocol StringListCodec: Codec where Bytes == [UInt8] {
    func serializeStrings(_ strings: [String]) throws -> [UInt8]
    /// Reads every string, in order
    func deserializeStrings(_ bytes: [UInt8]) throws -> [String]
    /// Reads the last string, without reading the others
    func readLastString(_ bytes: [UInt8]) throws -> String
}

/// A vector of strings, `[string]`, written from an array of strings with a
/// `TableSizer` and a `TableWriter`, and read with its getter's
/// `StringVector`: every string with `Array(_:)`, and the last from its ends
/// with its subscript
public struct TaflaGenStringList: StringListCodec {
    public typealias Bytes = [UInt8]

    public let name: String

    public init(name: String = "tafla-gen [string]") {
        self.name = name
    }

    public func serializeStrings(_ strings: [String]) throws -> [UInt8] {
        try StringListFields(items: strings).serialize()
    }

    public func deserializeStrings(_ bytes: [UInt8]) throws -> [String] {
        try Array(StringList(bytes.span.bytes).items)
    }

    public func readLastString(_ bytes: [UInt8]) throws -> String {
        let items = try StringList(bytes.span.bytes).items
        return try items[items.count - 1]
    }
}

/// A vector of tables of one string each, `[Name]`, whose generated fields
/// it creates from the input in each call, and whose items' strings it reads
/// through the items' views, each in place
public struct TaflaGenNameList: StringListCodec {
    public typealias Bytes = [UInt8]

    public init() {}

    public var name: String { "tafla-gen [Name]" }

    public func serializeStrings(_ strings: [String]) throws -> [UInt8] {
        try NameListFields(items: strings.map { NameFields(value: $0) }).serialize()
    }

    public func deserializeStrings(_ bytes: [UInt8]) throws -> [String] {
        let items = try NameList(bytes.span.bytes).items

        var strings: [String] = []
        strings.reserveCapacity(items.count)
        for index in items.indices {
            strings.append(try items[index].value)
        }

        return strings
    }

    public func readLastString(_ bytes: [UInt8]) throws -> String {
        let items = try NameList(bytes.span.bytes).items
        return try items[items.count - 1].value
    }
}

/// Checks that `codec` reads back every list's strings, in order, by their
/// UTF-8, and its last string
public func checkStringLists(_ codec: some StringListCodec) throws {
    for shape in listShapes {
        let bytes = try codec.serializeStrings(shape.strings)
        let read = try codec.deserializeStrings(bytes)
        let last = try codec.readLastString(bytes)

        let same = read.map { Array($0.utf8) } == shape.strings.map { Array($0.utf8) }
        guard same, last.utf8.elementsEqual(shape.strings[shape.strings.count - 1].utf8) else {
            throw ReadBackError(description: "\(codec.name) doesn't read back \(shape.name)")
        }
    }
}

/// Checks that the code that tafla-gen generates writes the vector of
/// strings of every list as `TableBuilder` writes it
public func checkStringListsAsTableBuilderWritesThem() throws {
    for shape in listShapes {
        var builder = try TableBuilder(fields: 1)
        try builder.setStringVector(shape.strings)

        guard try builder.bytes() == TaflaGenStringList().serializeStrings(shape.strings) else {
            throw ReadBackError(description: "tafla-gen doesn't write TableBuilder's \(shape.name)")
        }
    }
}
