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
  `TableWriter` write tables, and `TableReader` reads them.
- Bounds-checked reads, which fail on malformed input with
  `DecodeException`.
- The runtime reads and rebuilds the golden tables of tafla's conformance
  suite byte for byte, as every implementation of the format does.
- `TableReader.getVectorItemOffset()`, which finds a vector item from the count
  that `getVectorLength()` read, so that a reader of several items reads and
  checks the count once.
- `TableReader.getVectorItemsStart()`, where a vector's items start, and
  `getVectorItemEnd()`, which reads where a vector item ends after checking it
  against where it starts, so that a reader of the items in order carries each
  item's end to the next and reads one end for each item, as
  `getVectorOffsets()` does, where `getVectorItemOffset()` reads two. The
  benchmarks' generated view summed every item of a vector of 100 tables so
  in 0.427 µs, against 0.490 µs with `getVectorItemOffset()`.
- `TableSizer.writer(byte[])`, which writes a table into the start of bytes
  that the caller owns and reuses rather than into new bytes.
- `TableWriter.setStructs(int, int, int, int, StructFill)`, which writes an
  array of a count of structs straight into the table, each with a fill that
  writes struct `index` at an offset in the table's bytes, without an array
  of the structs. The generated code of the benchmarks' array of 100
  structs wrote it so in 0.109 µs rather than 0.338 µs from records.
- `TableBuilder.setTable(int, Consumer)` and
  `setVector(int, int, ObjIntConsumer)`, which record a nested table, or each
  item of a vector, straight among the builder's own fields, through the
  `TableBuilder.Item` that the fill sets its fields on, rather than from a
  builder of its own. They write the bytes that nested builders write, and
  leave the builder as it was if the fill throws or leaves fields unset. The
  benchmarks' vector of 100 nested tables took 3.8 µs so, against 4.3 µs
  from a builder per item, and allocated 13.0 KB rather than 29.1 KB.

### Changed

- A layout holds its table's version, `TableLayout.builder().version(1)`,
  rather than only whether the header has a slot for one, and the builder's
  `versioned()` is gone. `TableBuilder` and `TableSizer` take the version
  from the layout rather than as a constructor argument, so a version
  missing from or given for a layout can no longer be written. A version
  that doesn't fit the offset type throws from the layout's `build()` and
  `withVersion()`, once, rather than from each builder. `withVersion()`
  returns a layout for another version of the same format, and `version()`
  the layout's own. Readers read tables of any version, as before.
- The project is named tafla rather than buffy.
- The class that reads a table is `TableReader` rather than `ByteBuffer`,
  which shared its name with `java.nio.ByteBuffer`.
- `TableBuilder` writes nested builders 5% faster: the benchmarks' vector of
  100 nested tables took 4.4 µs from a builder per item, against 4.6 µs.
- A layout's offset width is its offset type, `uint8`, `uint16` or `uint32`,
  in the API and its errors.
- `TableReader` reads a vector item in place within its message, as the format
  now specifies, including an item whose offsets point past its end.
- `TableReader` reads scalars, offsets and counts from its bytes through
  `VarHandle`s, rather than through a wrapped `java.nio.ByteBuffer`, with
  the same checks and `DecodeException`s. The benchmarks' hand-written read
  of the vector of 100 nested tables takes 0.48 µs rather than 0.54.
- `TableBuilder` copies arrays in bulk inline, as tafla-gen's code did.
  Before any copy compiles, the runtime loads the JDK class that C2 needs in
  order to inline NIO's bulk copies. The JDK loaded it itself after
  `TableBuilder`'s copies had compiled, in every run of the benchmarks'
  document of 100 rows, so that each copy was a call that allocated two
  views. The document took 0.128 µs to write rather than 0.146, and 1,704
  bytes rather than 1,928.

### Removed

- `TableReader` only reads tables, and its setters are removed, with
  `allocate()`. Tables are written with `TableBuilder`, or with `TableWriter`
  and `TableSizer`, and structs and other records at fixed offsets with the
  standard library's `VarHandle`s.

### Fixed

- `TableSizer` accepts a struct of 0 bytes and rejects an array of 0-byte
  structs, as the builders and writers do.

[Unreleased]: https://github.com/snovader/tafla/commits/main/java
