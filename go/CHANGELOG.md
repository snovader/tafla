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
  Arrays, strings, arrays of structs and `TableBytes` of 64 bytes or more
  are kept until `Bytes()`, as every one was, and shorter ones are copied
  when they're set. A nested builder is copied into its parent when it's
  set, with the long values that it kept, unless they'd take 128 bytes or
  more to copy: then its parent keeps a copy of the builder, and writes its
  table in `Bytes()`, so that nesting builders copies little however deep
  they're nested. A builder that its caller doesn't keep, and calls neither
  `SetTableFunc()` nor `SetVectorFunc()` on, stays on the caller's stack. The
  benchmarks' record takes about 40 ns rather than 87, in 1 allocation
  rather than 2, the document of 100 rows 204 ns rather than 284, and the
  vector of 100 nested tables 3.15 µs rather than 5.45, in 4 allocations of
  7.7 KB rather than 6 of 18 KB. The document of 100,000 rows and the array
  of structs take about as long, in an allocation fewer. A vector of 100
  nested builders, which the comparison doesn't time, takes 4-9% less time.
- `TableBuilder` takes longer than it did on some shapes, which the
  benchmarks in `tests/table_builder_bench_test.go` time, since copying
  values into a buffer that doubles, and keeping nested builders, cost more
  than recording each field did:
  - Tables of many values shorter than 64 bytes, which it copies: 20,000
    strings of 32 bytes take 20-22% longer, of 63 bytes 53-60% longer, and a
    vector of 100,000 tables of a string of 60 bytes 27-34% longer.
  - Tables of many longer values, which it keeps: 20,000 strings of 64 to
    256 bytes take 43-68% longer, a vector of 100,000 tables of a string of
    255 bytes 30-32% longer, tables of 50 strings of 100 or 200 bytes 30-38%
    longer, and a table of 50 arrays of 25 uint32s 11-14% longer.
  - Nested builders: a chain of 10 to 2,000 builders, each of a string of 100
    bytes and the builder before it, takes 1.4 to 1.6 times as long, and
    trees of builders up to 31% longer. A vector of 5,000 builders, each of a
    string of 300 bytes, takes 31-34% longer.
  - A vector that fails, too long for uint16 offsets after 30,000 tables,
    takes 73% longer, but allocates 190 KB rather than 28.8 MB.
- `TableBuilder` uses memory differently, since it writes into a buffer:
  - `NewTableBuilder()` allocates the header's offsets past the 128 bytes
    within the builder, 1 to 4 bytes per field, where it allocated a 32-byte
    record per field past the fourth. A builder of many fields still
    allocates for them when it's created.
  - Arrays, strings, arrays of structs and `TableBytes` shorter than 64
    bytes are copied when they're set. Longer ones are read when the table is
    written, so their elements mustn't change until then, as before.
  - A table longer than 128 bytes is written into a buffer on the heap that
    doubles as it grows. While it grows, and while `Bytes()` copies the table
    out of it, the builder holds up to about 3 times the table's length. A
    table of many short values allocates 3 to 5 times its length in all,
    where it allocated 1.5 to 2 times. A setter that fails leaves the
    buffer's capacity as it grew, so a builder that wrote much before a
    setter failed holds that memory until it's dropped.
  - A nested builder that its parent keeps is copied to the heap, in 296
    bytes on a 64-bit host, and holds its buffer and the long values that it
    kept until its parent is dropped. A chain of 2,000 builders allocates
    about 5 times its table's length.
  - A vector of items written in place makes room for its items after the
    first, up to 64 KiB, and doesn't keep items once they make the table too
    long, so that a vector that fails holds little more than a table that
    fits.

### Fixed

- `TableSizer` accepts a struct of 0 bytes and rejects an array of 0-byte
  structs, as the builders and writers do.

[Unreleased]: https://github.com/snovader/tafla/commits/main/go
