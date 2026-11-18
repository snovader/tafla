// swift-tools-version:6.2

import PackageDescription

// The comparison of tafla's runtime with other Swift libraries, a package of
// its own, so that the runtime's package depends on none of them, as
// README.md describes.

// tafla's views, and the code that tafla-gen generates, need the experimental
// feature Lifetimes, as ../Package.swift explains
let lifetimes: SwiftSetting = .enableExperimentalFeature("Lifetimes")

let package = Package(
    name: "tafla-benchmarks",
    // Array's span, from which tafla reads the bytes that every codec returns
    platforms: [.macOS(.v26)],
    dependencies: [
        .package(name: "tafla", path: ".."),
        .package(url: "https://github.com/google/flatbuffers.git", from: "25.12.19"),
        .package(url: "https://github.com/apple/swift-protobuf.git", from: "1.41.0"),
        .package(url: "https://github.com/nnabeyang/swift-msgpack.git", from: "1.2.1"),
    ],
    targets: [
        // The workloads' values and types
        .target(name: "Workloads"),
        // The code that each library's generator generates from the
        // workloads' schemas
        .target(
            name: "GeneratedTafla",
            dependencies: [.product(name: "Tafla", package: "tafla")],
            swiftSettings: [lifetimes]
        ),
        .target(
            name: "GeneratedFlatBuffers",
            dependencies: [.product(name: "FlatBuffers", package: "flatbuffers")]
        ),
        .target(
            name: "GeneratedProtobuf",
            dependencies: [.product(name: "SwiftProtobuf", package: "swift-protobuf")]
        ),
        // Each library's codec of the workloads, and the reads of tafla's items
        .target(
            name: "Codecs",
            dependencies: [
                "Workloads",
                "GeneratedTafla",
                "GeneratedFlatBuffers",
                "GeneratedProtobuf",
                .product(name: "Tafla", package: "tafla"),
                .product(name: "FlatBuffers", package: "flatbuffers"),
                .product(name: "SwiftProtobuf", package: "swift-protobuf"),
                .product(name: "SwiftMsgpack", package: "swift-msgpack"),
            ],
            swiftSettings: [lifetimes]
        ),
        // Times variants interleaved, and counts what they allocate
        .target(name: "Harness"),
        .executableTarget(name: "Comparison", dependencies: ["Codecs", "Harness", "Workloads"]),
        .executableTarget(name: "Encoding", dependencies: ["Codecs", "Workloads"]),
        .testTarget(name: "CodecsTests", dependencies: ["Codecs", "Workloads"]),
    ]
)
