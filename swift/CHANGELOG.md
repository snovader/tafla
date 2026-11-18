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
- Bounds-checked reads, which throw a `DecodeError` on malformed input, with
  typed throws, and builders that throw a `BuildError` for a table that they
  can't write.
- The runtime reads and rebuilds the golden tables of tafla's conformance
  suite byte for byte, as every implementation of the format does.
- SwiftPM resolves the package from the repository's URL,
  `https://github.com/snovader/tafla.git`, and its releases from tags such as
  `0.1.0`.

[Unreleased]: https://github.com/snovader/tafla/commits/main/swift
