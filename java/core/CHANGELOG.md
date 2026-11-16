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

- The project is named tafla rather than buffy.
- `TableBuilder` writes nested builders 5% faster: the benchmarks' vector of
  100 nested tables took 4.4 µs from a builder per item, against 4.6 µs.
- A layout's offset width is its offset type, `uint8`, `uint16` or `uint32`,
  in the API and its errors.
- `ByteBuffer` reads a vector item in place within its message, as the format
  now specifies, including an item whose offsets point past its end.
- `ByteBuffer` reads scalars, offsets and counts from its bytes through
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
- `TableBuilder` writes each field, and its offset, when it's set, into bytes
  that grow, and `bytes()` copies them into the table, rather than recording
  each field and writing them all in `bytes()`. The API and the bytes are the
  same. Arrays, strings and tables shorter than 64 bytes are now read when
  they're set, and longer ones still when `bytes()` runs. A nested builder is
  read when it's nested, unless its bytes apart from such arrays are 64 or
  more, when the outermost table's `bytes()` writes it. So arrays still
  mustn't change before `bytes()`. As the fastest of 3 rounds, the
  benchmarks' record took 19.6 ns rather than 37.6, the vector of 100 nested
  tables 1.75 µs rather than 3.80 and 8.0 KB rather than 13.0, and the
  document of 100 rows 0.111 µs rather than 0.128. These cases are slower,
  most of them tables of the benchmarks' new `BuilderBenchmark`:
  - a table of one array of 100 bytes, by 17%, of four, by 68% to 95%, and
    of eight arrays of 255 or 256 bytes, by 50%;
  - a table of 1,000 arrays of 255 bytes, by 42%, allocating 6% more;
  - a chain of 100 builders, each nested in the next with an array of 100
    bytes, by 2.0 times, allocating 1.8 times as much;
  - a chain of 1,000 tables written in place, each padded, by 4.6 times,
    since each table is moved by its padding with the tables in it, so that
    the time grows with the square of the depth;
  - a document of 100 rows whose columns are nested as bytes, by 13%, and as
    a builder, by 3%;
  - a table nesting a builder of three arrays of 100 bytes, by 27% in most
    runs, though 24% faster in others;
  - the array of 100 structs, by 2% to 3%, allocating 144 bytes more for the
    builder's buffer;
  - the document of 100,000 rows, by 1% to 2%, less than the 4% by which two
    copies of the recording builder differed in an earlier run.

  A vector of tables nested from a builder per table took 16% less time but
  allocated 64 KB rather than 29 KB, mostly the 128-byte buffer that each
  builder starts with. A vector in place whose fields shorter than 64 bytes
  make a table of `uint32` offsets too long may need 2 GiB of memory before
  it's rejected, and throw an `OutOfMemoryError` first, where the recording
  builder threw an `IllegalArgumentException`.
- `TableBuilder` rejects a builder nested while a fill writes a table in it
  with an `IllegalArgumentException`, rather than writing a table that the
  parent's `bytes()` then failed on. The message of `bytes()` in that state
  says that a fill writes the table, rather than records it.

### Removed

- `ByteBuffer` only reads tables, and its setters are removed, with
  `allocate()`. Tables are written with `TableBuilder`, or with `TableWriter`
  and `TableSizer`, and structs and other records at fixed offsets with the
  standard library's `VarHandle`s.

### Fixed

- `TableSizer` accepts a struct of 0 bytes and rejects an array of 0-byte
  structs, as the builders and writers do.

[Unreleased]: https://github.com/snovader/tafla/commits/main/java
