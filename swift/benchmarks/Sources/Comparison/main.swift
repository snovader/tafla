// Times tafla's codecs and the libraries that it's compared with on every
// workload, interleaved, and prints each benchmark's fastest call, its ratio
// to tafla's, the median call and what a call allocates. README.md describes
// the method.
//
//     swift run -c release Comparison [--run regex] [--rounds n] [--warmup n] [--sample ms] [--quick]

import Codecs
import Reporting

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
    fail(error)
}

try run(benchmarks.all, arguments: arguments, reference: "tafla")
