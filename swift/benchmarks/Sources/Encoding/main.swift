// Prints each codec's encoded size of every workload, and what it does with a
// truncated document of 100,000 rows: an error, a document, or a trap, which
// stops the program, so each truncated read runs in a process of its own.
//
//     swift run -c release Encoding

import Codecs
import Foundation
import Workloads

let record = Record.workload
let small = Document(rows: documentRows[0])
let large = Document(rows: documentRows[1])
let list = List(items: listItems)
let sample = Sample.workload

/// The truncated reads, by codec, which each process runs one of
let truncatedReads: [String: (Int) -> String] = [
    TaflaCodec().name: { readTruncated(TaflaCodec(), length: $0) },
    TaflaGenCodec().name: { readTruncated(TaflaGenCodec(), length: $0) },
    FlatBuffersCodec().name: { readTruncated(FlatBuffersCodec(), length: $0) },
    ProtobufCodec().name: { readTruncated(ProtobufCodec(), length: $0) },
    MessagePackCodec().name: { readTruncated(MessagePackCodec(), length: $0) },
]

// A process that the command started to read one truncated document:
// `--read-truncated <codec> <length>`
let arguments = CommandLine.arguments.dropFirst()
if arguments.first == "--read-truncated", arguments.count == 3,
    let read = truncatedReads[arguments[arguments.startIndex + 1]],
    let length = Int(arguments[arguments.startIndex + 2])
{
    print(read(length))
    exit(0)
}

print("Encoded size (bytes)")
row("", ["record", "doc 100", "doc 100000", "vector", "structs", "fixed"])
printSizes(TaflaCodec())
printSizes(TaflaGenCodec())
printSizes(FlatBuffersCodec())
printSizes(ProtobufCodec())
printSizes(MessagePackCodec())
row(RawSpanCodec().name, ["", "", "", "", "", size { try RawSpanCodec().serializeSample(sample) }])

print("\nTruncated document of 100000 rows: deserialize")
try printTruncated(TaflaCodec())
try printTruncated(TaflaGenCodec())
try printTruncated(FlatBuffersCodec())
try printTruncated(ProtobufCodec())
try printTruncated(MessagePackCodec())

func printSizes<C: SuiteCodec>(_ codec: C) {
    row(
        codec.name,
        [
            size { try codec.serializeRecord(record) },
            size { try codec.serializeDocument(small) },
            size { try codec.serializeDocument(large) },
            size { try codec.serializeList(list) },
            size { try codec.serializeStructs(list) },
            size { try codec.serializeSample(sample) },
        ]
    )
}

/// Reads the large document cut to half its length and to 24 bytes, each in
/// a process of its own, and prints what happened
func printTruncated<C: DocumentCodec>(_ codec: C) throws {
    let length = try codec.serializeDocument(large).count

    for truncated in [length / 2, 24] {
        let outcome = try outcomeInProcess(codec: codec.name, length: truncated)
        print("  \(padded(codec.name, 16))\(padded("\(truncated)", 8, right: true)) B  \(shortened(outcome))")
    }
}

/// Runs this command to read a truncated document, and returns what it
/// printed, or how it stopped
func outcomeInProcess(codec: String, length: Int) throws -> String {
    let process = Process()
    process.executableURL = Bundle.main.executableURL
    process.arguments = ["--read-truncated", codec, "\(length)"]
    let output = Pipe()
    process.standardOutput = output
    process.standardError = FileHandle.nullDevice

    try process.run()
    let printed = output.fileHandleForReading.readDataToEndOfFile()
    process.waitUntilExit()

    guard process.terminationReason == .exit, process.terminationStatus == 0 else {
        return "trapped: the process stopped with signal \(process.terminationStatus)"
    }

    return String(decoding: printed, as: UTF8.self).trimmingCharacters(in: .newlines)
}

/// Reads the large document cut to `length` bytes, and describes what
/// happened
func readTruncated<C: DocumentCodec>(_ codec: C, length: Int) -> String {
    do {
        let bytes = try codec.serializeDocument(large).truncated(to: length)
        _ = try codec.deserializeDocument(bytes)
        return "returned a document"
    } catch {
        return "threw: \(error)"
    }
}

func size<Bytes: EncodedBytes>(_ write: () throws -> Bytes) -> String {
    (try? write()).map { "\($0.count)" } ?? "error"
}

func row(_ name: String, _ cells: [String]) {
    print("  " + padded(name, 16) + cells.map { padded($0, 12, right: true) }.joined())
}

func padded(_ text: String, _ width: Int, right: Bool = false) -> String {
    let padding = String(repeating: " ", count: max(0, width - text.count))
    return right ? padding + text : text + padding
}

func shortened(_ text: String) -> String {
    let line = text.replacing("\n", with: " ")
    return line.count > 120 ? String(line.prefix(120)) : line
}
