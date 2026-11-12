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

### Changed

- The project is named tafla rather than buffy.
- A layout's offset width is its offset type, `uint8`, `uint16` or `uint32`,
  in the API and its errors.
- `ByteBuffer` reads a vector item in place within its message, as the format
  now specifies, including an item whose offsets point past its end.
- `TableWriter::set_structs()` takes each struct by reference, so it needn't
  be `Copy`.

### Fixed

- `TableSizer` accepts a struct of 0 bytes and rejects an array of 0-byte
  structs, as the builders and writers do.

[Unreleased]: https://github.com/snovader/tafla/commits/main/rust/core
