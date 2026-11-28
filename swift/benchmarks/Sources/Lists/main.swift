// Times how the code that tafla-gen generates writes and reads lists of 10,
// 100 and 10,000 strings: a vector of strings, and a vector of tables of one
// string each, interleaved, and prints each benchmark's fastest call, its
// ratio to the vector of strings', the median call and what a call
// allocates, and then the lists' sizes. README.md describes the suite.
//
//     swift run -c release Lists [--run regex] [--rounds n] [--warmup n] [--sample ms] [--quick]

import Codecs
import Harness
import Reporting
import Workloads

let arguments = try Arguments(CommandLine.arguments.dropFirst())

let reference = TaflaGenStringList()
var benchmarks: [Benchmark] = []
do {
    try checkStringListsAsTableBuilderWritesThem()

    try addLists(reference, to: &benchmarks)
    try addLists(TaflaGenNameList(), to: &benchmarks)
    // The vector of strings twice, an A/A pair that shows the harness's noise
    try addLists(TaflaGenStringList(name: "tafla-gen [string] A/A"), to: &benchmarks)
} catch {
    fail(error)
}

try run(benchmarks, arguments: arguments, reference: reference.name)
try printSizes(of: [reference, TaflaGenNameList()])

/// Adds a codec's variants of every list's benchmarks, after checking that it
/// reads back every list. It's generic, so that the compiler specializes it
/// for each codec's type, and each variant's loop is code of its own that
/// calls the generated code directly.
func addLists<C: StringListCodec>(_ codec: C, to benchmarks: inout [Benchmark]) throws {
    try checkStringLists(codec)

    for shape in listShapes {
        let strings = shape.strings
        let bytes = try codec.serializeStrings(strings)

        benchmarks.add("\(shape.name): serialize", codec.name) { calls in
            for _ in 0..<calls {
                keep(try codec.serializeStrings(strings))
            }
        }
        benchmarks.add("\(shape.name): deserialize", codec.name) { calls in
            for _ in 0..<calls {
                keep(try codec.deserializeStrings(bytes))
            }
        }
        benchmarks.add("\(shape.name): read the last string", codec.name) { calls in
            for _ in 0..<calls {
                keep(try codec.readLastString(bytes))
            }
        }
    }
}

/// Prints the bytes of each codec's table of every list
func printSizes(of codecs: [any StringListCodec]) throws {
    print("\nEncoded size (bytes)")
    print("  " + padded("", to: -28) + codecs.map { padded($0.name, to: 24) }.joined())

    for shape in listShapes {
        var sizes = ""
        for codec in codecs {
            sizes += padded(thousands(try codec.serializeStrings(shape.strings).count), to: 24)
        }
        print("  " + padded(shape.name, to: -28) + sizes)
    }
}

/// `text` padded with spaces to `width` characters, on the left, or on the
/// right when `width` is negative
func padded(_ text: String, to width: Int) -> String {
    let padding = String(repeating: " ", count: max(abs(width) - text.count, 0))
    return width < 0 ? text + padding : padding + text
}
