# Changelog

The changes in each release of tafla's runtime for .NET, in `csharp/`. Each
of tafla's packages has a version and a changelog of its own, as
[RELEASING.md](https://github.com/snovader/tafla/blob/main/RELEASING.md)
describes. The format is that of
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the versions
follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

The first release.

### Added

- The runtime, for .NET 10, which writes and reads tables in the format that
  [SPEC.md](https://github.com/snovader/tafla/blob/main/SPEC.md) describes:
  tables of scalars, strings, typed arrays, nested tables, vectors of tables,
  structs and arrays of structs, with a layout that sets the header and the
  offset type (`uint8`, `uint16` or `uint32`). `TableBuilder` writes tables,
  and `ByteBuffer` reads them, with arrays as `ReadOnlySpan<T>`s of the bytes
  on a little-endian host, and structs and vector items as spans of the
  bytes.
- Bounds-checked reads, which fail on malformed input with a
  `DecodeException`, and builders that reject what they can't write with a
  `BuildException`.
- The runtime reads and rebuilds the golden tables of tafla's conformance
  suite byte for byte, as every implementation of the format does.

[Unreleased]: https://github.com/snovader/tafla/commits/main/csharp
