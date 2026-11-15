# Changelog

The changes in each release of the module `github.com/snovader/tafla/go`,
tafla's runtime for Go, whose releases are tagged `go/v<version>`. Each of
tafla's packages has a version and a changelog of its own, as
[RELEASING.md](https://github.com/snovader/tafla/blob/main/RELEASING.md)
describes. The format is that of
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the versions
follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

The first release. Changed and Fixed list what changed since the repository's
first commits, for code built from its source before the release.

### Added

- The runtime, which writes and reads tables in the format that
  [SPEC.md](https://github.com/snovader/tafla/blob/main/SPEC.md) describes:
  tables of scalars, strings, typed arrays, nested tables, vectors of tables,
  structs and arrays of structs, with a layout that sets the header and the
  offset type (`uint8`, `uint16` or `uint32`). `TableBuilder` and
  `TableWriter` write tables, and `ByteBuffer` reads them.
- Bounds-checked reads, which fail on malformed input with a `*DecodeError`.
- The runtime reads and rebuilds the golden tables of tafla's conformance
  suite byte for byte, as every implementation of the format does.
- `TableBuilder.SetTableFunc()` and `SetVectorFunc()` record a nested table,
  or each item of a vector, in place among the builder's own fields, through
  an `Item` that their function sets the fields on, rather than from a
  builder of its own. They write the bytes that nested builders write, and
  the benchmarks' vector of 100 nested tables takes about 5.8 µs so, in 6
  allocations, rather than 10.3 µs and 204 from a builder per item.
- `Uint32FieldOffset()` reads the offset of a field of a table whose offsets
  are uint32s, as `ByteBuffer.FieldOffset()` does, in a function small enough
  to inline, for the getters that tafla-gen generates, which know each
  field's slot.
- `StructsInPlace()` returns the bytes of an array of structs as Go structs
  in place, where they're aligned to the struct in memory on a little-endian
  host, for the code that tafla-gen generates, which calls it only for
  structs that lie in memory as their bytes do.

### Changed

- The project is named tafla rather than buffy.
- A layout's offset width is its offset type, `uint8`, `uint16` or `uint32`,
  in the API and its errors.
- `ByteBuffer` reads a vector item in place within its message, as the format
  now specifies, including an item whose offsets point past its end.
- `TableWriter`'s array setters no longer move the writer to the heap where
  they're inlined into their caller. They passed the writer to a generic
  function, which Go's compiler assumes keeps it when it's called from
  another package. The code that tafla-gen generates writes the Go
  comparison suite's document in 1 allocation rather than 2.
- `ByteBuffer.RootOffset()` and `FieldOffset()` read uint32 offsets as
  `Uint32FieldOffset()` does, in one call rather than two, and leave the
  other offset types to the general case. They return the same positions
  and errors. The benchmarks' hand-written reader of the record takes about
  22.7 ns rather than 28.2, and of the vector of 100 nested tables 1.43 µs
  rather than 1.66.
- `TableBuilder` writes each field when it's set, into 128 bytes within the
  builder and then into a buffer on the heap, rather than recording the field
  and writing every field in `Bytes()`. Its API, the bytes that it writes and
  its errors are the same, a setter that fails still leaves the builder as it
  was, and `Bytes()` still returns new bytes of the table's exact length.
  Arrays, strings, arrays of structs and `TableBytes` of 256 bytes or more
  are kept until `Bytes()`, as every one was, and shorter ones are copied
  when they're set. A nested builder is copied into its parent when it's
  set. A builder that its caller doesn't keep, and calls neither
  `SetTableFunc()` nor `SetVectorFunc()` on, stays on the caller's stack. The
  benchmarks' record takes about 42 ns rather than 93, in 1 allocation
  rather than 2, the document of 100 rows 267 ns rather than 338, and the
  vector of 100 nested tables 3.27 µs rather than 5.95, in 4 allocations of
  7.8 KB rather than 6 of 17.6 KB. The document of 100,000 rows and the
  array of structs take as long, in an allocation fewer. A vector of 100
  nested builders, which the comparison doesn't time, takes 4% longer.

### Fixed

- `TableSizer` accepts a struct of 0 bytes and rejects an array of 0-byte
  structs, as the builders and writers do.

[Unreleased]: https://github.com/snovader/tafla/commits/main/go
