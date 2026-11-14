# Changelog

The changes in each release of `tafla` on npm, tafla's runtime for
TypeScript. Each of tafla's packages has a version and a changelog of its own,
as [RELEASING.md](https://github.com/snovader/tafla/blob/main/RELEASING.md)
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
- Bounds-checked reads, which fail on malformed input with `DecodeError`.
- The runtime reads and rebuilds the golden tables of tafla's conformance
  suite byte for byte, as every implementation of the format does.
- `ByteBuffer.getVectorItemOffset()`, which finds a vector item from the count
  that `getVectorLength()` read, so that a reader of several items reads and
  checks the count once.
- `TableSizer.writer(bytes)`, which writes a table into the start of the
  caller's bytes rather than new ones.
- `TableBuilder.setTableInPlace(fields, fill)` and
  `setVectorInPlace(count, fields, fill)`, which record a nested table, or
  each item of a vector, among the builder's own fields, through the
  `TableItem` that `fill` sets its fields on, rather than from a builder of
  its own. They write the bytes that nested builders write, and leave the
  builder as it was if `fill` throws or leaves fields unset. The comparison
  suite's vector of 100 nested tables took 11.5 µs rather than 13.7 µs, and
  allocated 31 KB on V8's heap rather than 47 KB.

### Changed

- The project is named tafla rather than buffy, and the package is `tafla`
  rather than `@buffy/core`.
- A layout's offset width is its offset type, `uint8`, `uint16` or `uint32`,
  in the API and its errors.
- `ByteBuffer` reads a vector item in place within its message, as the format
  now specifies, including an item whose offsets point past its end.
- `ByteBuffer.getVectorOffsets()` returns a `number[]` rather than a
  `Uint32Array`, so that positions past 4 GiB are exact.
- `ByteBuffer` checks each read against its bytes' length at the time of the
  read, so bytes made over a resizable buffer without a length follow the
  buffer as it's resized. A read past them still throws a `DecodeError`.
- `ByteBuffer` reads offsets, field offsets and vector items in less
  bytecode, with their failures out of line, so that V8 inlines them into
  their callers: a loop that reads a vector's nested tables through a
  generated view's `get(i)` inlines every read. It throws the same errors. In
  the comparison suite in Node 27, `ByteBuffer`'s getters summed the vector of
  100 nested tables in 0.75 µs rather than 1.19, and tafla-gen's view in
  0.91 µs rather than 1.85. `getVectorLength()` and `getVectorItemOffset()`
  check a vector's ends with one comparison, which takes the vector's position
  to be an integer, as `rootOffset()` and `fieldOffset()` find it.
- `ByteBuffer.getString()` reads a string of up to 32 bytes of ASCII a byte
  at a time, without a view of its bytes or a `TextDecoder`, and any other
  string with the fatal decoder as before, so it reads the same strings and
  throws the same `DecodeError`. The comparison suite read its record, whose
  name is 16 bytes, in 0.116 µs rather than 0.159 in Node 26, and 0.108 µs
  rather than 0.158 in Node 27, and the document of 100 rows in 0.222 µs
  rather than 0.261 in Node 26. A string of 32 bytes that ends in a character
  outside ASCII takes about 37 ns longer, and one that starts with one 8 ns.

### Removed

- `ByteBuffer` only reads tables, and its setters are removed, with
  `reset()`. Tables are written with `TableBuilder`, or with `TableWriter` and
  `TableSizer`, and structs and other records at fixed offsets with
  `StructWriter`.

### Fixed

- A struct writer whose write threw no longer breaks later writes.
- `TableWriter` writes padding as zeros and never writes past its bytes.
- `ByteBuffer` fails with `DecodeError` on a detached or shrunk buffer.
- The runtime rejects an alignment other than 1, 2, 4 or 8, as Java's does,
  rather than padding to it or dividing by zero.
- `TableSizer` accepts a struct of 0 bytes and rejects an array of 0-byte
  structs, as the builders and writers do.

[Unreleased]: https://github.com/snovader/tafla/commits/main/typescript
