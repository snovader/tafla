// swift-tools-version:6.2

import PackageDescription

// The manifest from which other packages resolve tafla's runtime for Swift,
// whose sources lie in swift/. SwiftPM resolves a package from the manifest at
// the root of its repository, and its versions from tags such as 0.1.0, as
// RELEASING.md describes. It declares the library alone, without the tests
// and their dependency, and swift/Package.swift, from which the package is
// developed and tested, declares the same library: the release workflow
// checks that they agree.
let package = Package(
    name: "tafla",
    platforms: [.macOS(.v12), .iOS(.v15), .tvOS(.v15), .watchOS(.v9), .visionOS(.v1)],
    products: [
        .library(name: "Tafla", targets: ["Tafla"])
    ],
    targets: [
        // swift/Package.swift explains the experimental feature
        .target(
            name: "Tafla",
            path: "swift/Sources/Tafla",
            swiftSettings: [.enableExperimentalFeature("Lifetimes")]
        )
    ]
)
