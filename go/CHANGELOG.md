# Changelog

The changes in each release of the module `github.com/snovader/tafla/go`,
tafla's runtime for Go, whose releases are tagged `go/v<version>`. Each of
tafla's packages has a version and a changelog of its own, as
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
- Bounds-checked reads, which fail on malformed input with a `*DecodeError`.
- The runtime reads and rebuilds the golden tables of tafla's conformance
  suite byte for byte, as every implementation of the format does.
- `TableBuilder.Reset(dst, fields, layout)` starts the next table after the
  bytes of `dst`, into its capacity while it has room, as `append` does, and
  `Bytes()` then returns `dst` with the table appended. Passing back the
  bytes that `Bytes()` returned, from their start, writes each table of a
  loop into the same bytes, and a `dst` made with a capacity gives a first
  large table its room. The zero `TableBuilder` holds a table of no fields
  until it's reset, so a builder can be a value of its own, on the stack or
  within a struct.
- `TableBuilder.SetTableFuncAligned()` and `Item.SetTableFuncAligned()`
  write a table nested in place padded for an alignment that the caller
  gives, before its fields are set, so that it isn't moved by its padding
  after them, unless its fields need more.
- `TableBuilder.SetTableFunc()` and `SetVectorFunc()` record a nested table,
  or each item of a vector, in place among the builder's own fields, through
  an `Item` that their function sets the fields on, rather than from a
  builder of its own. They write the bytes that nested builders write, and
  the benchmarks' vector of 100 nested tables takes about 5.8 µs so, in 6
  allocations, rather than 10.3 µs and 204 from a builder per item.
- `Uint32FieldOffset()` reads the offset of a field of a table whose offsets
  are uint32s, as `TableReader.FieldOffset()` does, in a function small enough
  to inline, for the getters that tafla-gen generates, which know each
  field's slot.
- `StructsInPlace()` returns the bytes of an array of structs as Go structs
  in place, where they're aligned to the struct in memory on a little-endian
  host, for the code that tafla-gen generates, which calls it only for
  structs that lie in memory as their bytes do.
- Arrays of bools, stored as `uint8` arrays of 1 and 0:
  `TableBuilder.SetBoolArray()`, `Item.SetBoolArray()`,
  `TableSizer.AddBoolArray()`, `TableWriter.SetBoolArray()` and
  `TableReader.GetBoolArray()`, which reads them into new bools, true where
  a byte isn't 0.
- Vectors of strings, which `SPEC.md` adds to the format, and which a
  schema declares as `[string]`. `TableReader.GetStringVector()` returns a
  `StringVector`, whose `Get()` reads one string from its ends without
  reading the others, and whose `Strings()` reads every string into a
  `[]string`, checking them as one run of UTF-8. Both return strings that
  share the bytes, and fail with a `*DecodeError` for a string that isn't
  valid UTF-8 on its own. `TableBuilder.SetStringVector()`,
  `Item.SetStringVector()`, `TableSizer.AddStringVector()` and
  `TableWriter.SetStringVector()` write one from a `[]string`.

### Changed

- A layout holds its table's version, `TableLayout{}.WithVersion(1)`,
  rather than only whether the header has a slot for one, and `Versioned()`
  is gone. `NewTableBuilder()`, `NewTableSizer()` and `AppendTable()` write
  the layout's version, in place of `NewVersionedTableBuilder()`,
  `NewVersionedTableSizer()` and `AppendVersionedTable()`, so a version
  missing from or given for a layout can no longer be written, and those
  panics are gone. They fail with VersionTooLarge for a version that doesn't
  fit the layout's offset type, as before. `TableLayout.Version()` returns a
  layout's version, and readers read tables of any version, as before.
- The project is named tafla rather than buffy.
- The type that reads a table is `TableReader` rather than `ByteBuffer`, and
  `NewTableReader()` creates it.
- A layout's offset width is its offset type, `uint8`, `uint16` or `uint32`,
  in the API and its errors.
- `TableReader` reads a vector item in place within its message, as the format
  now specifies, including an item whose offsets point past its end.
- `TableWriter`'s array setters no longer move the writer to the heap where
  they're inlined into their caller. They passed the writer to a generic
  function, which Go's compiler assumes keeps it when it's called from
  another package. The code that tafla-gen generates writes the Go
  comparison suite's document in 1 allocation rather than 2.
- `TableReader.RootOffset()` and `FieldOffset()` read uint32 offsets as
  `Uint32FieldOffset()` does, in one call rather than two, and leave the
  other offset types to the general case. They return the same positions
  and errors. The benchmarks' hand-written reader of the record takes about
  22.7 ns rather than 28.2, and of the vector of 100 nested tables 1.43 µs
  rather than 1.66.
- `TableBuilder` copies each field into one buffer when it's set, and
  writes its offset, rather than recording the field and writing every field
  in `Bytes()`, and `Bytes()` returns that buffer rather than a copy. So it
  keeps none of its caller's values: a slice, a string, a nested builder or
  `TableBytes` can change once its setter returns. The bytes that it writes,
  its errors and what a setter that fails leaves behind are the same. A
  builder that its caller doesn't keep, and calls neither `SetTableFunc()`
  nor `SetVectorFunc()` on, stays on the caller's stack. Against the builder
  that recorded fields, a new builder takes 0.71 of the time for the
  benchmarks' record, 0.59 for the vector of 100 nested tables, 0.54-1.0 for
  vectors of 100,000 tables in place of values up to 60 bytes, and 0.74-1.04
  for tables of 20,000 strings, and a builder reused with `Reset()` takes
  0.23-0.71 of the time on every shape but nested builders.
- `Bytes()` returns the builder's buffer, with its spare capacity, after the
  bytes that `Reset()` was given, rather than new bytes of the table's exact
  length. The builder doesn't write to them again unless they're given back
  to `Reset()`, and `bytes.Clone` copies a table to keep it without the room.
- `TableBuilder` takes longer than the builder that recorded fields on some
  shapes, from a new builder:
  - Tables of values that outgrow its first 256 bytes, which it grows into
    new bytes, zeroing their room, rather than writing the table into bytes
    of its exact length once: the document of 100 rows takes 2.0 times as
    long, the document of 100,000 rows 2.5 times, tables of 5 to 33 float64
    columns of 100 rows 1.25-1.64 times, the array of 100 structs 1.17 times,
    and a table of 16 strings of 1 KiB 1.38 times. Written into bytes with
    room for it, with `Reset()`, the document of 100,000 rows takes 0.98 of
    the time, and with a builder reused, 0.54.
  - Nested builders, which it copies into their parent when they're set,
    and which their caller keeps on the heap, with their buffer, in 2
    allocations rather than 1: a vector of 100 nested builders takes 1.61
    times as long, trees of builders 1.61-1.71 times, a vector of 5,000
    builders of a string of 300 bytes 4.8 times, and a chain of builders,
    each nested in the next, time that grows with the square of its depth:
    2.4 times as long for 10 builders, 13 for 200 and 90 for 2,000.
  - Tables nested in place, each in another and each padded, which it moves
    by their padding once their fields are set, and again with each table
    that holds them: 2.2 times as long at a depth of 10, and 20 at 1,000,
    unless `SetTableFuncAligned()` pads them before their fields: then 0.70-
    0.81 of the time at any depth.
  - A vector in place of 100,000 tables of a string of 255 bytes takes 1.12
    times as long, and one that fails, too long for uint16 offsets after
    30,000 tables, 1.65 times, but allocates 230 KB rather than 28.8 MB.
- `TableBuilder` uses memory differently, since it writes into a buffer:
  - New bytes start with room for 256 bytes, or for 64 bytes a field after
    the header, up to 64 KiB, if that's more, and grow fourfold below 64 KiB
    and twofold above. A table that outgrows them allocates up to about 3
    times its length in all, and is returned with up to 4 times its length
    of capacity below 64 KiB, and up to twice above.
  - A vector of items written in place makes room for its items after the
    first, up to 64 KiB, and doesn't keep items once they make the table too
    long, so that a vector that fails holds little more than a table that
    fits.

### Fixed

- `TableSizer` accepts a struct of 0 bytes and rejects an array of 0-byte
  structs, as the builders and writers do.

[Unreleased]: https://github.com/snovader/tafla/commits/main/go
