// The command line that every suite's executable takes, and the run of its
// benchmarks, which times each one's variants interleaved with the harness
// and prints the report.

import Foundation
import Harness

/// A row of the report: an operation, and each variant of it
public struct Benchmark {
    public let name: String
    public var variants: [Variant]
}

extension [Benchmark] {
    /// Adds a variant to the benchmark of that name, which the first variant
    /// of the name appends
    public mutating func add(_ benchmark: String, _ variant: String, _ loop: @escaping (Int) throws -> Void) {
        let variant = Variant(name: variant, loop: loop)
        if let index = firstIndex(where: { $0.name == benchmark }) {
            self[index].variants.append(variant)
        } else {
            append(Benchmark(name: benchmark, variants: [variant]))
        }
    }
}

/// Times each benchmark that the arguments select, in order, and prints the
/// report, whose ratios are to each benchmark's first variant, which
/// `reference` names, with the machine's load when it started and ended
public func run(_ benchmarks: [Benchmark], arguments: Arguments, reference: String) throws {
    let selected = benchmarks.filter { benchmark in
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
    report(results, reference: reference)
}

/// Writes an error of the command to stderr, and exits with a failure
public func fail(_ error: any Error) -> Never {
    FileHandle.standardError.write("\(error)\n".data(using: .utf8)!)
    exit(1)
}

/// The command's options
public struct Arguments {
    /// A regular expression of the benchmarks to run
    public var run: Regex<AnyRegexOutput>?
    public var options = Options(rounds: 30, warmupRounds: 3, sampleTime: .milliseconds(20))

    public init(_ arguments: ArraySlice<String>) throws {
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
