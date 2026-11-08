# Changelog

Every package that tafla publishes, in every language, shares one version and
this changelog. The format is that of
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the versions
follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

The first release. Changed, Removed and Fixed list what changed since the
repository's first commits, for code built from its source before the release.

### Added

- The format, which [SPEC.md](SPEC.md) describes: tables of scalars, strings,
  typed arrays, nested tables, vectors of tables, structs and arrays of
  structs, with a layout that sets the header and the offset type (`uint8`,
  `uint16` or `uint32`).
- Runtimes in TypeScript (`tafla` on npm), Python (`tafla` on PyPI), Java
  (`io.github.snovader.tafla:tafla` on Maven Central) and Rust (`tafla` on
  crates.io), with `TableBuilder` and `TableWriter` to write tables, and
  `ByteBuffer` to read them.
- Bounds-checked reads in every runtime, which fail on malformed input with
  the runtime's decode error (`DecodeError`, or `DecodeException` in Java).
- tafla-gen, a schema compiler written in Rust, which generates code that
  writes and reads a schema's messages, tables and structs in TypeScript,
  JavaScript, Python, Java and Rust, with lazy views of vectors and arrays of
  structs, and a WebAssembly module of it for other hosts.
- A conformance suite of golden tables, which every runtime reads and rebuilds
  byte for byte, and property tests of every runtime.
- Benchmarks in every language, compared with FlatBuffers, protobuf,
  MessagePack and others on the same workloads.
- [SPEC.md](SPEC.md) describes how a format's messages evolve: each version
  fixes its fields and keeps the same layout, and a new version appends
  fields to the previous version's.
- A `bool` scalar, one byte that writers write as 0 or 1 and readers read as
  true when it isn't 0, in tables and structs, which tafla-gen generates as
  each language's boolean. Arrays of bools aren't supported.
- A static `versionOf()` on every versioned message that tafla-gen generates
  (`version_of()` in Python and Rust), which reads the version in the header
  so that an application can choose the reader of that version.

### Changed

- The project is named tafla rather than buffy, and its npm package is `tafla`
  rather than `@buffy/core`.
- A layout's offset width is its offset type, `uint8`, `uint16` or `uint32`,
  in every schema, runtime and error. Schemas set it with
  `offset_type: uint16` rather than `offset_size: 16`.
- Generated tables are written with a sizer and a writer rather than with
  `TableBuilder`, and each generated Rust table reads its layout as a constant.
- Every implementation reads a vector item in place within its message, as
  the format now specifies, including an item whose offsets point past its
  end.
- TypeScript's `ByteBuffer.getVectorOffsets()` returns a `number[]` rather
  than a `Uint32Array`, so that positions past 4 GiB are exact.
- Generated fields that clash with the generated code's members, or that are
  named `then` in TypeScript, are renamed.
- tafla-gen exits with 2 on wrong arguments, and 1 on a schema's errors, and
  rejects an unknown option rather than reading it as a schema. It has
  `--help`, `--version` and `--`, takes options anywhere, and reports every
  schema it can't read or write rather than stopping at the first.

### Removed

- TypeScript's `ByteBuffer` only reads tables, and its setters and `reset()`
  are removed. Tables are written with `TableBuilder`, or `TableSizer` and
  `TableWriter`, and structs and other records at fixed offsets with
  `StructWriter`.

### Fixed

- `ByteBuffer`'s writes in Python write nothing when a value doesn't fit, as
  documented.
- A TypeScript struct writer whose write threw no longer breaks later writes.
- TypeScript's `TableWriter` writes padding as zeros and never writes past its
  bytes, and generated structs are written into bytes of their own.
- TypeScript's `ByteBuffer` fails with `DecodeError` on a detached or shrunk
  buffer, and Python's on huge positions and non-contiguous buffers.
- Python's `get_vector_offsets()` returns positions past 4 GiB rather than
  overflowing.
- A scalar setter of Python's `TableWriter` that raises leaves the table as
  it was, rather than pointing the field's offset at the next field's value.
- TypeScript and Python reject an alignment other than 1, 2, 4 or 8, as Java
  does, rather than padding to it or dividing by zero.
- Every `TableSizer` accepts a struct of 0 bytes and rejects an array of
  0-byte structs, as the builders and writers do.
- Python raises `ValueError` for a float32 too large for its type, and
  generated Python structs reject negative offsets and out-of-range values.
- tafla-gen rejects fields whose names are the same in camelCase, names that
  clash with generated code, definitions too wide for a Java record, and
  definitions that nest too deeply, rather than generating code that doesn't
  compile or overflowing its stack. It checks for cycles in linear time, keeps
  generated files within `--out`, and lines up the carets of its errors under
  tabs.
- tafla-gen skips a byte-order mark at the start of a schema, points at the
  first bytes of a schema that aren't UTF-8, and rejects doc comments that
  hold control characters, rather than generating Rust that doesn't compile.
  Javadoc writes `&`, `<`, `>` and `@` from doc comments as HTML entities,
  and `JSDoc` writes `@` as one, so that a line such as `@deprecated` can't
  become a tag, and a schema's path in a generated header can't end its
  comment or hold a Unicode escape that Java reads.

[Unreleased]: https://github.com/snovader/tafla/commits/main
