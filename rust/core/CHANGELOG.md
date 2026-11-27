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
  `TableWriter` write tables, and `TableReader` reads them.
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
- Vectors of strings, which `SPEC.md` adds to the format: a count, the end
  of each string and the strings' UTF-8, one after another.
  `TableReader::get_str_vector()` returns a `StringVector`, which reads one
  string from its ends with `get()`, each in turn with `iter()`, and every
  string into a `Vec<&str>` with `to_vec()`, borrowed from the bytes.
  `to_vec()` checks the strings' run of bytes as UTF-8 at once, and read
  lists of ASCII in 0.34–0.92 times the time of reading each string.
  `TableReader::get_str_vector_item()` reads one string from a count that
  the caller keeps. Each string is checked as UTF-8 on its own, so an end
  that splits a character fails both strings around it.
  `TableSizer::add_str_vector()`, `TableWriter::set_str_vector()` and
  `TableBuilder::set_str_vector()` write them from a list of strings.
- `TableSizer::add_table_bytes()`, a public `TableWriter::copy_table()`, and
  `TableWriter::table_bytes()` and `versioned_table_bytes()`, for the code
  that tafla-gen generates: they add and copy whole a table whose length its
  schema fixes, and start a message's bytes with its header, so that a
  message of such fields is written at constant positions. They're outside
  the compatibility promise of the rest of the API, as Go's
  `AddTableBytes()` and `AppendTable()` are.

### Changed

- A layout holds its table's version, `TableLayout::DEFAULT.with_version(1)`,
  rather than only whether the header has a slot for one, and `versioned()`
  is gone. `TableBuilder::new()`, `TableSizer::new()` and
  `TableWriter::table_bytes()` take a layout with or without a version and
  write the one it has, in place of `TableBuilder::versioned()`,
  `TableSizer::versioned()` and `TableWriter::versioned_table_bytes()`. They
  return `BuildError::VersionTooLarge` for a version that doesn't fit the
  layout's offset type, as before. The layout's type parameter still says
  whether it has a version, so `TableLayout::version()` and
  `TableReader::version()` exist only where there is one, and readers read
  tables of any version, as before. A reader keeps what it reads with, not
  the layout, so `TableReader::is_sized()` and `offset_type()` replace its
  `layout()`.
- The project is named tafla rather than buffy.
- The type that reads a table is `TableReader` rather than `ByteBuffer`.
- A layout's offset width is its offset type, `uint8`, `uint16` or `uint32`,
  in the API and its errors.
- `TableReader` reads a vector item in place within its message, as the format
  now specifies, including an item whose offsets point past its end.
- `TableWriter::set_structs()` takes each struct by reference, so it needn't
  be `Copy`.
- `TableReader::get_str()` checks whether a string is ASCII, a word at a time
  at any address, before it checks its UTF-8 with `str::from_utf8()`, which
  checks a string's bytes one at a time but for whole pairs of aligned words.
  The benchmarks' record, whose 16-byte name isn't aligned, read in 6.3 ns
  rather than 11.8 ns, and a 1 KiB ASCII string in 11 ns rather than 26 ns.
  A string that's ASCII until near its end is checked twice, up to the first
  other character: 1 KiB of it took 37 ns rather than 29 ns. Its values and
  errors are unchanged.
- `TableReader::get_str()` is inlined into its callers. With several of
  them, as a schema's generated getters are, LLVM called it out of line,
  which `#[inline]` didn't change. tafla-gen's record was read in 4.9 ns
  rather than 6.7 ns, and its document of 100 rows in 15.3 ns rather than
  16.5 ns. A table of 16 strings of 8 ASCII characters was read in 51 ns
  rather than 76 ns. Its values and errors are unchanged.
- `TableBuilder` makes room to record at least 8 fields when it starts, its
  own and those of the tables that it records in place, rather than only its
  own. Recording a table in place grew a small table's record by a
  reallocation, which took 40 ns more on macOS than room for 8 from the
  start. The benchmarks' document of 100 rows, whose columns are recorded
  in place, was written in 149 ns rather than 188 ns, in 1,680 bytes in 2
  allocations rather than 1,872 bytes in 3. The record was written in the
  same time, in 444 bytes rather than 252.

### Fixed

- `TableSizer` accepts a struct of 0 bytes and rejects an array of 0-byte
  structs, as the builders and writers do.

[Unreleased]: https://github.com/snovader/tafla/commits/main/rust/core
