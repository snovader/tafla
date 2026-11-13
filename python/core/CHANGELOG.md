# Changelog

The changes in each release of `tafla` on PyPI, tafla's runtime for Python.
Each of tafla's packages has a version and a changelog of its own, as
[RELEASING.md](https://github.com/snovader/tafla/blob/main/RELEASING.md)
describes. The format is that of
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the versions
follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

The first release. Changed, Removed and Fixed list what changed since the
repository's first commits, for code built from its source before the release.

### Added

- The runtime, which writes and reads tables in the format that
  [SPEC.md](https://github.com/snovader/tafla/blob/main/SPEC.md) describes:
  tables of scalars, strings, typed arrays, nested tables, vectors of tables,
  structs and arrays of structs, with a layout that sets the header and the
  offset type (`uint8`, `uint16` or `uint32`). `TableBuilder` and
  `TableWriter` write tables, and `ByteBuffer` reads them.
- The classes that generated code uses have a native implementation, written
  in Rust, which the wheels for Linux, macOS and Windows hold, and a
  pure-Python one, which installs on any other platform.
- Bounds-checked reads, which fail on malformed input with `DecodeError`.
- The runtime reads and rebuilds the golden tables of tafla's conformance
  suite byte for byte, as every implementation of the format does.
- `ByteBuffer.get_vector_item_offset()`, which finds a vector item from the
  count that `get_vector_length()` read, so that a reader of several items
  reads and checks the count once.
- `ByteBuffer.get_fields()`, which reads every field of a table at once, when
  its offsets are those that `TableWriter` writes, for the code that tafla-gen
  generates.
- `TableBuilder.set_table_in_place()` and `set_vector_in_place()`, which
  record a nested table, or each item of a vector, among the builder's own
  fields, from a function that sets them on the builder, rather than from a
  builder of its own. They write the bytes that nested builders write, and
  the benchmarks' vector of 100 nested tables took 28% to 30% less time so
  with the native classes, and 23% less with the pure-Python ones, holding
  half the memory at its peak.

### Changed

- The project is named tafla rather than buffy.
- A layout's offset width is its offset type, `uint8`, `uint16` or `uint32`,
  in the API and its errors.
- `ByteBuffer` reads a vector item in place within its message, as the format
  now specifies, including an item whose offsets point past its end.

### Removed

- `ByteBuffer` only reads tables, and its setters are removed, with
  `reset()`. Tables are written with `TableBuilder` or `TableWriter`, and
  structs and other records at fixed offsets with the standard library's
  `struct`.

### Fixed

- `ByteBuffer` fails with `DecodeError` on huge positions and non-contiguous
  buffers.
- `get_vector_offsets()` returns positions past 4 GiB rather than
  overflowing.
- A scalar setter of `TableWriter` that raises leaves the table as it was,
  rather than pointing the field's offset at the next field's value.
- The runtime rejects an alignment other than 1, 2, 4 or 8, as Java's does,
  rather than padding to it or dividing by zero.
- The runtime raises `ValueError` for a float32 too large for its type.

[Unreleased]: https://github.com/snovader/tafla/commits/main/python
