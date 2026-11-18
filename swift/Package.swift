// swift-tools-version:6.2

import PackageDescription

// The views of a table's bytes, `ByteBuffer`, `ArrayView` and `Vector`, are
// nonescapable, and their lifetimes depend on the bytes', which needs the
// experimental feature Lifetimes in Swift 6.2 to 6.4: README.md explains why.
let lifetimes: SwiftSetting = .enableExperimentalFeature("Lifetimes")

let package = Package(
    name: "tafla",
    platforms: [.macOS(.v12), .iOS(.v15), .tvOS(.v15), .watchOS(.v9), .visionOS(.v1)],
    products: [
        .library(name: "Tafla", targets: ["Tafla"])
    ],
    dependencies: [
        // The property tests' library, which only the tests depend on
        .package(url: "https://github.com/x-sheep/swift-property-based.git", from: "2.0.1")
    ],
    targets: [
        .target(name: "Tafla", swiftSettings: [lifetimes]),
        .testTarget(
            name: "TaflaTests",
            dependencies: [
                "Tafla",
                .product(name: "PropertyBased", package: "swift-property-based"),
            ],
            swiftSettings: [lifetimes]
        ),
    ]
)
