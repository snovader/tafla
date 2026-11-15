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
- `TableWriter.write_fields()` and `ByteBuffer.read_fields()`, which write
  a message of only the fields that a `FixedFields` describes into bytes of
  its own, and read their values back, each in one call and without a
  writer or a reader, for the code that tafla-gen generates.
- `ByteBuffer.iter_vector_fields()`, which iterates over the values of a
  vector's items as `get_fields()` reads each, reading each item's end when
  it reaches the item, so that the code that tafla-gen generates iterates
  over a vector's values without reading them all at once.
- `TableBuilder.set_table_in_place()` and `set_vector_in_place()`, which
  write a nested table, or each item of a vector, among the builder's own
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
- The native `TableWriter` converts an array from an exact `list` or `tuple`
  of `int`s, `float`s and `bool`s itself, straight into the table's bytes,
  rather than into an `array` that it copies, with the values and errors of
  `array()`, which still converts anything else. The benchmarks' documents
  written from lists took 87% less time for 100,000 rows and 69% less for
  100, with the native module built for the Python it runs on, and 55% and
  48% less with the abi3 wheels', whose stable ABI makes a call for each
  element.

- `TableBuilder` writes each field into the table's bytes when it's set,
  rather than recording it and writing every field through a `TableWriter`
  in `finalize()`, and has a native implementation, in `tafla._native`, as
  the other classes do, with the pure-Python one elsewhere. With the native
  module, the comparison's vector of 100 nested tables took 83% less time,
  the record 77% less and the 100-row document 79% less, and documents
  written from lists 85% to 88% less, since their lists are converted
  natively too. With the pure-Python classes, the vector took 36% less and
  the 100-row document 45% less. No row of the comparison took longer, but a
  table that nests a large builder does, as below. Its API and the bytes it
  writes are unchanged, and so are its errors, but for when some of them are
  raised:
  - A scalar that its type can't hold raises `ValueError` from its setter,
    rather than from `finalize()` or `bytes()`, and isn't added.
  - Arrays, structs and tables given as buffers are copied when they're set,
    so a buffer changed after its setter returns doesn't change the table,
    and one whose length changed no longer raises from `finalize()`.
  - A nested builder is copied into its parent as its bytes, so the parent
    holds and copies a large nested table twice: nesting a builder of
    1,000,000 float64s took 2.6 times as long as before, and peaked at 16 MB
    rather than 8. Written in place with `set_table_in_place()`, the same
    table takes as long as before and is held once. A builder whose layout
    has a header is no longer finalized by nesting it.
  - `finalize()` or `bytes()` within a `fill`, or nesting the builder in
    another table there, raises `ValueError` itself, rather than the
    in-place setter raising once `fill` returns.
  - A table written in place whose slots alone are longer than the offset
    type addresses raises `ValueError` before `fill` runs, as
    `TableWriter.start_table()` does.
  - The native `TableBuilder` can't be subclassed, and is for one thread at a
    time, as the native `TableWriter` is.

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
- While a view of a `TableWriter`'s bytes is held, the native writer raises
  `BufferError` only for a call that would change their length, as the
  pure-Python one does, rather than for any call that appends, even nothing:
  `start_table(0)`, an empty struct's `set_struct()`, or `set_fields()` and
  `write_table()` for no leading fields write in place. `set_fields()` and
  `write_table()` pack their values before the view fails them, so a value
  that can't be written raises the setter's error in both.
- `TableWriter.start_table()` raises `ValueError` for a nested table whose
  slots alone are longer than the offset type addresses, as the constructor
  does for the root's header, rather than `MemoryError` or `OverflowError`
  from allocating them, which differed by the count and the implementation.

[Unreleased]: https://github.com/snovader/tafla/commits/main/python
