// swift-tools-version:6.2

import PackageDescription

// The package's own manifest, with its tests. Other packages resolve it from
// the manifest at the repository's root, ../Package.swift, which declares the
// same library alone, and which the release workflow checks against this one.

// The views of a table's bytes, `TableReader`, `ArrayView` and `Vector`, are
// nonescapable, and their lifetimes depend on the bytes', which needs the
// experimental feature Lifetimes in Swift 6.2 to 6.4: README.md explains why.
// The code that tafla-gen generates declares views of its own, which need it
// too.
let lifetimes: SwiftSetting = .enableExperimentalFeature("Lifetimes")

/// The code that tafla-gen generates from each example schema in `schemas/`
/// at the root of the repository, and from the schema of optional fields and
/// versions, `typescript/core/test/presence.tafla`, a module of each, which
/// the generator's tests compare with what it generates, and which the tests
/// in `Tests/TaflaTests/Generated` run
let generated = [
    (module: "GeneratedAllTypes", schema: "all_types"),
    (module: "GeneratedFixedFields", schema: "fixed_fields"),
    (module: "GeneratedLayouts", schema: "layouts"),
    (module: "GeneratedPresence", schema: "presence"),
    (module: "GeneratedShapes", schema: "shapes"),
]

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
            ] + generated.map { .target(name: $0.module) },
            swiftSettings: [lifetimes]
        ),
    ]
        + generated.map { module, schema in
            .target(
                name: module,
                dependencies: ["Tafla"],
                path: "Tests/Generated/\(schema)",
                swiftSettings: [lifetimes]
            )
        }
)
