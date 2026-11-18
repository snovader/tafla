// Times tafla's codecs and the libraries that it's compared with on every
// workload, interleaved, and prints each benchmark's fastest call, its ratio
// to tafla's, the median call and what a call allocates. README.md describes
// the method.
//
//     swift run -c release Comparison [--run regex] [--rounds n] [--warmup n] [--sample ms] [--quick]

import Codecs
import Foundation
import Harness
import Workloads

let arguments = try Arguments(CommandLine.arguments.dropFirst())

var benchmarks = Benchmarks()
do {
    try benchmarks.addSuite(TaflaCodec())
    try benchmarks.addSuite(TaflaGenCodec())
    try benchmarks.addSuite(FlatBuffersCodec())
    try benchmarks.addSuite(ProtobufCodec())
    try benchmarks.addSuite(MessagePackCodec())
    try benchmarks.addSample(RawSpanCodec())

    // tafla-gen's twice, an A/A pair that shows the harness's noise
    try benchmarks.addItemAccess(TaflaCodec(), name: "tafla")
    try benchmarks.addItemAccess(TaflaGenCodec(), name: "tafla-gen")
    try benchmarks.addItemAccess(TaflaGenCodec(), name: "tafla-gen A/A")
} catch {
    FileHandle.standardError.write("\(error)\n".data(using: .utf8)!)
    exit(1)
}

let selected = benchmarks.all.filter { benchmark in
    arguments.run.map { benchmark.name.contains($0) } ?? true
}

print("\(platform()), \(ProcessInfo.processInfo.activeProcessorCount) CPUs")
print("Start: \(loadAverages())")
let clock = ContinuousClock()
let started = clock.now

var results: [BenchmarkResult] = []
for benchmark in selected {
    let measurements = try measure(benchmark.variants, options: arguments.options)
    results.append(BenchmarkResult(name: benchmark.name, measurements: measurements))
    FileHandle.standardError.write("\(benchmark.name): \(summary(measurements))\n".data(using: .utf8)!)
}

print("End: \(loadAverages())")
let took = started.duration(to: clock.now)
let options = arguments.options
print(
    "Took \(took.formatted(.units(allowed: [.minutes, .seconds], width: .wide))), \(options.rounds) rounds of "
        + "\(options.sampleTime) after \(options.warmupRounds) warm-up rounds\n"
)
report(results)

/// The command's options
struct Arguments {
    /// A regular expression of the benchmarks to run
    var run: Regex<AnyRegexOutput>?
    var options = Options(rounds: 30, warmupRounds: 3, sampleTime: .milliseconds(20))

    init(_ arguments: ArraySlice<String>) throws {
        var arguments = arguments[...]
        while let argument = arguments.popFirst() {
            switch argument {
            case "--quick":
                // Every benchmark briefly, to check that it works
                options = Options(rounds: 2, warmupRounds: 0, sampleTime: .milliseconds(1))
            case "--run":
                run = try Regex(try Self.value(of: argument, in: &arguments))
            case "--rounds":
                options.rounds = try Self.count(of: argument, in: &arguments)
            case "--warmup":
                options.warmupRounds = try Self.count(of: argument, in: &arguments)
            case "--sample":
                options.sampleTime = .milliseconds(try Self.count(of: argument, in: &arguments))
            default:
                throw UsageError(description: "Unknown argument \(argument)")
            }
        }
    }

    static func value(of option: String, in arguments: inout ArraySlice<String>) throws -> String {
        guard let value = arguments.popFirst() else {
            throw UsageError(description: "\(option) needs a value")
        }
        return value
    }

    static func count(of option: String, in arguments: inout ArraySlice<String>) throws -> Int {
        let value = try value(of: option, in: &arguments)
        guard let count = Int(value), count > 0 else {
            throw UsageError(description: "\(option) needs a positive integer, not \(value)")
        }
        return count
    }
}

struct UsageError: Error, CustomStringConvertible {
    let description: String
}
