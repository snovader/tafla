# Changelog

The changes in each release of `Tafla`, tafla's runtime for Swift, whose
releases are tagged `<version>`. Each of tafla's packages has a version and a
changelog of its own, as
[RELEASING.md](https://github.com/snovader/tafla/blob/main/RELEASING.md)
describes. The format is that of
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the versions
follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

The first release.

### Added

- The runtime, which writes and reads tables in the format that
  [SPEC.md](https://github.com/snovader/tafla/blob/main/SPEC.md) describes:
  tables of scalars, strings, typed arrays, nested tables, vectors of tables,
  structs and arrays of structs, with a layout that sets the header and the
  offset type (`uint8`, `uint16` or `uint32`). `TableBuilder` writes tables,
  and `TableReader` reads them.
- `TableReader`, `ArrayView` and `Vector` are nonescapable views of a
  `RawSpan`, such as `Data.bytes`, whose lifetimes the compiler keeps within
  the bytes'. `ArrayView.span` returns an array's elements as a `Span` in
  place, where the host is little-endian and they're aligned.
- `TableLayout.default`, which `TableReader`, `TableBuilder` and
  `TableSizer` take by default, is an inlinable computed property, so that
  a reader built with it, such as
  `TableReader(vector.itemInPlace(at: index))`, reads with a constant offset
  type in the code that its getters are inlined into.
- A layout holds its table's version, `TableLayout(sized: true, version: 1)`,
  which `TableBuilder` and `TableSizer` write to the header, and throw
  `BuildError.versionTooLarge` for when it doesn't fit the layout's offset
  type. `withVersion(_:)` returns the layout of another version of the same
  format, and `isVersioned` says whether a layout has one. A `TableReader`
  reads tables of any version, whatever its layout's.
- Bounds-checked reads, which throw a `DecodeError` on malformed input, with
  typed throws, and builders that throw a `BuildError` for a table that they
  can't write.
- The runtime reads and rebuilds the golden tables of tafla's conformance
  suite byte for byte, as every implementation of the format does.
- SwiftPM resolves the package from the repository's URL,
  `https://github.com/snovader/tafla.git`, and its releases from tags such as
  `0.1.0`.
- `TableSizer` and `TableWriter`, which write a table whose fields are known
  before it's written without recording them, as the code that tafla-gen's
  `swift` target generates writes its tables: the sizer counts the table's
  length, and `TableSizer.bytes(_:)` gives its writer to a closure that sets
  the same fields, in the same order, into new bytes of that length.
  `BuildError.notAsSized` reports fields that don't fill the table, and a
  write past it stops the program.
- `TableBuilder` records up to four fields within itself, and a builder
  nested with `setTable(_:alignment:)` records its fields among its
  parent's, so that a small table, nested or not, allocates nothing until
  `bytes()` writes it.
- `TableBuilder.setTable(fields:_:)` and `setVector(count:fields:_:)`,
  which record a nested table, or each item of a vector, in place among the
  builder's own fields through a `TableBuilder.Item`, without a builder of
  its own for each, as the other runtimes' builders do, and write the bytes
  that nested builders write. The closure throws `BuildError`, and whatever
  it throws, the builder is left as it was.
- `ArrayElement`'s `init(littleEndianBytes:at:)`, which reads a scalar at a
  fixed offset in bytes, as a struct's scalars lie, and which the generated
  structs read themselves with.
- Arrays of bools, stored as `uint8` arrays of 1 and 0:
  `TableBuilder.setBoolArray(_:)`, `TableSizer.addBoolArray(count:)`,
  `TableWriter.setBoolArray(_:_:)` and `TableReader.boolArray(at:)`, which
  reads a new `[Bool]`, true where a byte isn't 0, without reading any byte
  as a `Bool`. `uint8Array(at:)` reads the same bytes in place.

[Unreleased]: https://github.com/snovader/tafla/commits/main/swift
