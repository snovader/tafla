# Changelog

The changes in each release of `io.github.snovader.tafla:tafla` on Maven
Central, tafla's runtime for Java. Each of tafla's packages has a version and
a changelog of its own, as
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
- Bounds-checked reads, which fail on malformed input with
  `DecodeException`.
- The runtime reads and rebuilds the golden tables of tafla's conformance
  suite byte for byte, as every implementation of the format does.
- `ByteBuffer.getVectorItemOffset()`, which finds a vector item from the count
  that `getVectorLength()` read, so that a reader of several items reads and
  checks the count once.
- `ByteBuffer.getVectorItemsStart()`, where a vector's items start, and
  `getVectorItemEnd()`, which reads where a vector item ends after checking it
  against where it starts, so that a reader of the items in order carries each
  item's end to the next and reads one end for each item, as
  `getVectorOffsets()` does, where `getVectorItemOffset()` reads two. The
  benchmarks' generated view summed every item of a vector of 100 tables so
  in 0.427 µs, against 0.490 µs with `getVectorItemOffset()`.
- `TableSizer.writer(byte[])`, which writes a table into the start of bytes
  that the caller owns and reuses rather than into new bytes.
- `TableBuilder.setTable(int, Consumer)` and
  `setVector(int, int, ObjIntConsumer)`, which record a nested table, or each
  item of a vector, straight among the builder's own fields, through the
  `TableBuilder.Item` that the fill sets its fields on, rather than from a
  builder of its own. They write the bytes that nested builders write, and
  leave the builder as it was if the fill throws or leaves fields unset. The
  benchmarks' vector of 100 nested tables took 3.8 µs so, against 4.3 µs
  from a builder per item, and allocated 13.0 KB rather than 29.1 KB.

### Changed

- The project is named tafla rather than buffy.
- `TableBuilder` writes nested builders 5% faster: the benchmarks' vector of
  100 nested tables took 4.4 µs from a builder per item, against 4.6 µs.
- A layout's offset width is its offset type, `uint8`, `uint16` or `uint32`,
  in the API and its errors.
- `ByteBuffer` reads a vector item in place within its message, as the format
  now specifies, including an item whose offsets point past its end.

### Removed

- `ByteBuffer` only reads tables, and its setters are removed, with
  `allocate()`. Tables are written with `TableBuilder`, or with `TableWriter`
  and `TableSizer`, and structs and other records at fixed offsets with the
  standard library's `VarHandle`s.

### Fixed

- `TableSizer` accepts a struct of 0 bytes and rejects an array of 0-byte
  structs, as the builders and writers do.

[Unreleased]: https://github.com/snovader/tafla/commits/main/java
