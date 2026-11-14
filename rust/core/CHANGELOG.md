# Changelog

The changes in each release of `tafla` on crates.io, tafla's runtime for
Rust. Each of tafla's packages has a version and a changelog of its own, as
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
- Bounds-checked reads, which fail on malformed input with `DecodeError`.
- The runtime reads and rebuilds the golden tables of tafla's conformance
  suite byte for byte, as every implementation of the format does.
- `TableBuilder::set_table_with()` and `set_vector_with()` record a nested
  table, or each item of a vector of them, in place among the builder's own
  fields, rather than from a builder of its own: a closure sets the fields on
  an `InPlaceTable`, whose setters are the builder's. They write the bytes
  that nested builders write, and leave the builder as it was if the closure
  returns an error, leaves fields unset or panics. The benchmarks' vector of
  100 nested tables took 4.4 µs so, rather than 8.8 µs from a builder per
  item, in 4 allocations rather than 208.

### Changed

- The project is named tafla rather than buffy.
- A layout's offset width is its offset type, `uint8`, `uint16` or `uint32`,
  in the API and its errors.
- `ByteBuffer` reads a vector item in place within its message, as the format
  now specifies, including an item whose offsets point past its end.
- `TableWriter::set_structs()` takes each struct by reference, so it needn't
  be `Copy`.
- `ByteBuffer::get_str()` checks whether a string is ASCII, a word at a time
  at any address, before it checks its UTF-8 with `str::from_utf8()`, which
  checks a string's bytes one at a time but for whole pairs of aligned words.
  The benchmarks' record, whose 16-byte name isn't aligned, read in 6.3 ns
  rather than 11.8 ns, and a 1 KiB ASCII string in 11 ns rather than 26 ns.
  A string that's ASCII until near its end is checked twice, up to the first
  other character: 1 KiB of it took 37 ns rather than 29 ns. Its values and
  errors are unchanged.

### Fixed

- `TableSizer` accepts a struct of 0 bytes and rejects an array of 0-byte
  structs, as the builders and writers do.

[Unreleased]: https://github.com/snovader/tafla/commits/main/rust/core
