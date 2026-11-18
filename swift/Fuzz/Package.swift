// swift-tools-version:6.2

import PackageDescription

// libFuzzer's targets for the runtime, a package of their own, so that the
// runtime's package neither builds nor vends them. They build only on Linux,
// where Swift's toolchain has libFuzzer's runtime, with
//
//     swift build -c release --sanitize=fuzzer --sanitize=address
//
// as README.md describes.
let lifetimes: SwiftSetting = .enableExperimentalFeature("Lifetimes")

let package = Package(
    name: "tafla-fuzz",
    // The runtime's, and String(validating:as:), which checks strings
    platforms: [.macOS(.v15), .iOS(.v18), .tvOS(.v18), .watchOS(.v11), .visionOS(.v2)],
    dependencies: [
        .package(name: "tafla", path: "..")
    ],
    targets: [
        // Arbitrary bytes, read with every getter at every position
        .executableTarget(
            name: "FuzzTableReader",
            dependencies: [.product(name: "Tafla", package: "tafla")],
            swiftSettings: [lifetimes]
        ),
        // The suite's tables, mutated and read as its descriptions
        .executableTarget(
            name: "FuzzConformance",
            dependencies: [.product(name: "Tafla", package: "tafla")],
            swiftSettings: [lifetimes]
        ),
    ]
)
