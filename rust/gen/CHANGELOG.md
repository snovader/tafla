# Changelog

The changes in each release of `tafla-gen` on crates.io, tafla's schema
compiler. Each of tafla's packages has a version and a changelog of its own,
as [RELEASING.md](https://github.com/snovader/tafla/blob/main/RELEASING.md)
describes. When the code that tafla-gen generates for a language needs a
newer release of that language's runtime, its entry names the release. The
format is that of [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and the versions follow
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

The first release. Changed and Fixed list what changed since the repository's
first commits, for code built from its source before the release.

### Added

- tafla-gen, a schema compiler written in Rust, which generates code that
  writes and reads a schema's messages, tables and structs in TypeScript,
  JavaScript, Python, Java, Rust, Go and C++, with lazy views of vectors and
  arrays of structs, views in place of arrays of scalars in Java, whose
  getters copy them, and a WebAssembly module of it for other hosts.
- [SPEC.md](https://github.com/snovader/tafla/blob/main/SPEC.md#evolution)
  describes how a format's messages evolve: each version fixes its fields and
  keeps the same layout, and a new version appends fields to the previous
  version's.
- A `bool` scalar, one byte that writers write as 0 or 1 and readers read as
  true when it isn't 0, in tables and structs, which tafla-gen generates as
  each language's boolean. Arrays of bools aren't supported.
- A static `versionOf()` on every versioned message that tafla-gen generates
  (`version_of()` in Python and Rust), which reads the version in the header
  so that an application can choose the reader of that version.
- Generated views of vectors in Java, TypeScript, JavaScript and Python find
  each item with the runtime's `getVectorItemOffset()`
  (`get_vector_item_offset()` in Python), from the count that they read and
  check once.
- A generated `serializeInto(fields, bytes)` on Java's messages, which writes
  the message into the start of bytes that the caller owns and reuses, and
  returns the message's length.
- A static `serializeInto()` on every message that tafla-gen generates in
  TypeScript and JavaScript, which writes the message into the start of the
  caller's bytes and returns the view of the bytes that holds it, saving the
  allocation of new bytes for each table.
- A generated `serialize_into(bytes)` on C++'s messages, which writes the
  message into the start of bytes that the caller owns and reuses, and
  returns the message's length.
- Generated Python declares `<Struct>Fields` beside each struct, a protocol
  of its fields, which an array of the struct is written from, so that any
  objects with those attributes write it, rather than only the struct's
  dataclass.
- Generated Python declares `fields()` on each message and table whose
  fields are all scalars, structs and tables of them, and a string last,
  which reads every field at once into the `TypedDict` that it's written
  from, through `ByteBuffer.get_fields()`. It reads them through the getters
  when the table's offsets aren't those that its writer writes, so its values
  and errors are theirs. The getters stay lazy.

### Changed

- The project is named tafla rather than buffy.
- Schemas set a layout's offset type with `offset_type: uint16` rather than
  `offset_size: 16`.
- Generated tables are written with a sizer and a writer rather than with
  `TableBuilder`, and each generated Rust table reads its layout as a constant.
- Generated Rust fields of vectors and arrays of structs take a slice of any
  type whose reference converts into the item's fields or struct, through a
  type parameter that defaults to the generated type, so that an application
  writes the items it holds without collecting them first.
- Generated C++ readers of tables nested in a message hold the bytes from the
  table's start, 16 bytes rather than 24, and read its offsets at constant
  positions, so their decode errors report positions from the table's start,
  as a vector item's do.
- Generated fields that clash with the generated code's members, or that are
  named `then` in TypeScript, are renamed.
- tafla-gen exits with 2 on wrong arguments, and 1 on a schema's errors, and
  rejects an unknown option rather than reading it as a schema. It has
  `--help`, `--version` and `--`, takes options anywhere, and reports every
  schema it can't read or write rather than stopping at the first.

### Fixed

- Generated TypeScript structs are written into bytes of their own.
- Generated Python structs reject negative offsets and out-of-range values.
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

[Unreleased]: https://github.com/snovader/tafla/commits/main/rust/gen
