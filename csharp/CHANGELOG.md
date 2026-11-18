# Changelog

The changes in each release of `Tafla` on NuGet, tafla's runtime for .NET.
Each of tafla's packages has a version and a changelog of its own, as
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
  and `TableReader` reads them, with arrays as `ReadOnlySpan<T>`s of the bytes
  on a little-endian host, and structs and vector items as spans of the
  bytes. `TableReader` and `Vector` are `ref struct`s over a
  `ReadOnlySpan<byte>` of any memory, such as an array's, a slice's, or stack
  or native memory, so that no read converts the bytes to a span.
- The package, which is trimmable and compatible with Native AOT, with its
  symbols in a `.snupkg`, whose Source Link finds the sources of its commit
  on GitHub.
- `TableSizer` and `TableWriter`, which write a table whose fields are known
  in advance into bytes of its exact length, or into the start of bytes that
  the caller reuses, with nested tables, vector items and structs written in
  place, as the other runtimes' generated code writes tables. The bytes are
  those that `TableBuilder` writes.
- `TableBuilder.SetTable(fields, fill)` and `SetVector(count, fields, fill)`,
  which record a nested table, or each item of a vector, in place among the
  builder's own bytes through a `TableBuilder.Item`, without a builder of its
  own for each, as the other runtimes' builders do, and write the bytes that
  nested builders write. Their overloads that take a `state` pass it to a
  `static` lambda, so that recording them allocates nothing.
- Bounds-checked reads, which fail on malformed input with a
  `DecodeException`, and builders that reject what they can't write with a
  `BuildException`. A `TableWriter` whose fields differ from those that its
  `TableSizer` counted throws one of kind `NotAsSized`, as Go's, Rust's and
  C++'s writers report it, and never writes past the table, even into
  reused bytes longer than it.
- The runtime reads and rebuilds the golden tables of tafla's conformance
  suite byte for byte, as every implementation of the format does.

[Unreleased]: https://github.com/snovader/tafla/commits/main/csharp
