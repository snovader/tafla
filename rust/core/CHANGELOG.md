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
- `TableSizer::add_table_bytes()`, a public `TableWriter::copy_table()`, and
  `TableWriter::table_bytes()` and `versioned_table_bytes()`, for the code
  that tafla-gen generates: they add and copy whole a table whose length its
  schema fixes, and start a message's bytes with its header, so that a
  message of such fields is written at constant positions. They're outside
  the compatibility promise of the rest of the API, as Go's
  `AddTableBytes()` and `AppendTable()` are.

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
- `TableBuilder` writes each field's bytes, and its offset, when it's set,
  rather than recording the field and writing every field through a
  `TableWriter` in `to_bytes()`, which now copies the bytes into the
  table's own, of its exact length. A run of 256 bytes or more, an array's
  elements, a string, a struct, an array of structs or a table nested as
  bytes, is kept, borrowed or owned as it was given, and copied into the
  table once by `to_bytes()`. A nested builder is copied in as its bytes,
  whatever its layout, with the runs that it keeps. Its API, bytes and
  errors are unchanged, and a setter that fails still leaves the builder as
  it was. The constructor allocates the builder's bytes, with room for 64
  bytes a field beyond the header, up to 64 KiB more and at least 256
  bytes, and writes the header into them, where it allocated room to record
  at least 8 fields, of 48 bytes each. The bytes grow fourfold below 64 KiB
  and twofold above. On an Apple M2 Pro, the benchmarks' vector of 100
  nested tables, written in place, took 1.35 µs rather than 4.40 µs, in
  7,696 bytes rather than 32,536; the record 52 ns rather than 78 ns; the
  document of 100 rows 122 ns rather than 157 ns; and the vector from a
  builder per table 7.5 µs rather than 8.6 µs, in 205 allocations rather
  than 203. A table of 32 strings and arrays of 128 and 192 bytes, which are
  copied when they're set and again by `to_bytes()`, took 430 ns rather
  than 491 ns, in 16,256 bytes from 3 allocations rather than 6,912 from 2.
  The document of 100,000 rows and the array of 100 structs took as long as
  before. Some tables take longer: a table of 4 strings or arrays of 255
  bytes, which grow the bytes, took 145 ns rather than 107 ns, and of
  100,000 of them 3.1 ms rather than 1.9 ms, holding 56 MB at once rather
  than 31 MB; a table of 9 arrays of 256 bytes, of which the builder keeps
  4 within itself, 205 ns rather than 182 ns; and a vector of 100,000
  builders of 4 `uint32` fields, which hold their room while they're
  nested by reference, 13.3 ms rather than 10.7 ms, holding 90 MB at once
  rather than 52 MB. A builder is 264 bytes rather than 48.

### Fixed

- `TableSizer` accepts a struct of 0 bytes and rejects an array of 0-byte
  structs, as the builders and writers do.

[Unreleased]: https://github.com/snovader/tafla/commits/main/rust/core
