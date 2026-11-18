import Foundation
import Harness
import Workloads

/// A benchmark's measurements, a row of each table
struct BenchmarkResult {
    let name: String
    let measurements: [Harness.Measurement]
}

/// Prints the fastest call of each benchmark, and its ratio to the first
/// variant's, tafla's, the median call's distance from it, and then the bytes
/// and allocations of a call, as tables of Markdown
func report(_ results: [BenchmarkResult]) {
    let columns = columns(of: results)

    print("Fastest call in µs, and its ratio to tafla's:\n")
    table(results, columns) { measurement, first in
        let ratio = measurement.name == first.name ? "" : " (\(significant(measurement.fastest / first.fastest))×)"
        return micros(measurement.fastest) + ratio
    }

    print("\nMedian call's distance from the fastest:\n")
    table(results, columns) { measurement, _ in
        String(format: "%+.1f%%", 100 * (measurement.median / measurement.fastest - 1))
    }

    guard results.contains(where: { $0.measurements.contains { $0.allocated != nil } }) else {
        print("\nAllocations aren't counted on this platform.")
        return
    }

    print("\nBytes allocated per call, and the allocations that ask for them:\n")
    table(results, columns) { measurement, _ in
        guard let allocated = measurement.allocated else {
            return ""
        }
        return "\(thousands(Int(allocated.bytes.rounded()))) (\(trimmed(allocated.allocations)))"
    }
}

/// The variants' names, in their first order, of the benchmarks of codecs and
/// of the ways of reading items apart
func columns(of results: [BenchmarkResult]) -> [String] {
    var columns: [String] = []
    for result in results {
        for measurement in result.measurements where !columns.contains(measurement.name) {
            columns.append(measurement.name)
        }
    }

    return columns
}

func table(
    _ results: [BenchmarkResult], _ columns: [String], cell: (Harness.Measurement, Harness.Measurement) -> String
) {
    print("| Benchmark | \(columns.joined(separator: " | ")) |")
    print("| --- |\(String(repeating: " ---: |", count: columns.count))")
    for result in results {
        let cells = columns.map { column in
            result.measurements.first { $0.name == column }.map { cell($0, result.measurements[0]) } ?? ""
        }
        print("| \(result.name) | \(cells.joined(separator: " | ")) |")
    }
}

func summary(_ measurements: [Harness.Measurement]) -> String {
    measurements.map { "\($0.name) \(micros($0.fastest)) µs" }.joined(separator: ", ")
}

/// A time in nanoseconds in µs, to three significant digits, or more integer
/// digits
func micros(_ nanoseconds: Double) -> String {
    significant(nanoseconds / 1000)
}

func significant(_ value: Double) -> String {
    switch value {
    case 100...: String(format: "%.1f", value)
    case 10...: String(format: "%.2f", value)
    case 0.1...: String(format: "%.3f", value)
    case 0.01...: String(format: "%.4f", value)
    default: String(format: "%.5f", value)
    }
}

/// A number to two decimals, without trailing zeros
func trimmed(_ value: Double) -> String {
    var text = String(format: "%.2f", value)
    while text.hasSuffix("0") {
        text.removeLast()
    }
    if text.hasSuffix(".") {
        text.removeLast()
    }

    return text
}

/// The system and architecture that the comparison runs on
func platform() -> String {
    #if arch(arm64)
        let architecture = "arm64"
    #elseif arch(x86_64)
        let architecture = "x86_64"
    #else
        let architecture = "another architecture"
    #endif

    // macOS's version string doesn't name the system, as Linux's does
    #if os(macOS)
        let system = "macOS \(ProcessInfo.processInfo.operatingSystemVersionString)"
    #else
        let system = ProcessInfo.processInfo.operatingSystemVersionString
    #endif

    return "\(system) on \(architecture)"
}

/// The load averages of the last 1, 5 and 15 minutes, which show how busy the
/// machine was
func loadAverages() -> String {
    var averages = [Double](repeating: 0, count: 3)
    guard getloadavg(&averages, 3) == 3 else {
        return "load averages unknown"
    }

    return "load averages " + averages.map { String(format: "%.2f", $0) }.joined(separator: " ")
}
