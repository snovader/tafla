# Changelog

The changes in each release of `Tafla` on NuGet, tafla's runtime for .NET.
Each of tafla's packages has a version and a changelog of its own, as
[RELEASING.md](https://github.com/snovader/tafla/blob/main/RELEASING.md)
describes. The format is that of
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the versions
follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

The first release. Changed lists what changed since the repository's first
commits, for code built from its source before the release.

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
- A layout holds its table's version, `new TableLayout { Version = 1 }`, which
  `TableBuilder`, `TableSizer` and `TableWriter.CreateTableBytes()` write to
  the header, and throw a `BuildException` for when it doesn't fit the
  layout's offset type. `layout with { Version = 2 }` is the layout of
  another version of the same format, and `Versioned` says whether a layout
  has one. A `TableReader` reads tables of any version, whatever its
  layout's, and has the `Sized` and `OffsetType` that it reads with.
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
- `TableSizer.AddTableBytes()`, `TableWriter.ReserveTableBytes()`,
  `TableWriter.CreateTableBytes()` and `TableSizer.Utf8Length()`, with which
  code that lays out a table of fields of fixed sizes itself, such as
  tafla-gen's, writes it whole rather than field by field: a vector's items
  and nested tables in the bytes that the writer reserves for them, and a
  message of such fields and a string after them in new bytes with its
  header, without a sizer and a writer.
- A public `DecodeException.OutOfRange()`, beside `UnsupportedVersion()`,
  with which code that reads bytes without a `TableReader`, such as the
  structs that tafla-gen generates, fails as the runtime's reads do.
- The runtime reads and rebuilds the golden tables of tafla's conformance
  suite byte for byte, as every implementation of the format does.
- Vectors of strings, which `TableBuilder.SetStringVector()`,
  `TableSizer.AddStringVector()` and `TableWriter.SetStringVector()` write
  from a span of strings, and which `TableReader.GetStringVector()` reads as a
  `StringVector`, a `ref struct` over the bytes, as a `Vector` is, whose
  indexer reads one string from its two ends without reading the others, and
  whose `ToArray()` and enumerator read every string, in order. Each string
  is checked as UTF-8 on its own, so an end that splits a character fails
  both strings around it.

### Changed

- `TableReader.GetString()` decodes a string of up to 1,024 bytes without
  counting its characters first: up to 128 bytes in one pass into characters
  on the stack, and a longer one, if it's ASCII, widened into the string by
  `Encoding.Latin1`, or else decoded into 1,024 characters that each thread
  allocates once. Longer strings are counted and then decoded, as before.
  Tables of 16 strings were read in 0.68 to 0.84 of the time, but for ASCII
  strings of 8 and 42 bytes, at 0.90 to 1.00, and the benchmarks'
  record, whose 16-byte name is ASCII, in 23.4 ns rather than 24.4 ns. Its
  values and errors are unchanged.

[Unreleased]: https://github.com/snovader/tafla/commits/main/csharp
