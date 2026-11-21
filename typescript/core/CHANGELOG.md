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
  `TableWriter` write tables, and `TableReader` reads them.
- Bounds-checked reads, which fail on malformed input with `DecodeError`.
- The runtime reads and rebuilds the golden tables of tafla's conformance
  suite byte for byte, as every implementation of the format does.
- `TableReader.getVectorItemOffset()`, which finds a vector item from the count
  that `getVectorLength()` read, so that a reader of several items reads and
  checks the count once.
- `TableSizer.writer(bytes)`, which writes a table into the start of the
  caller's bytes rather than new ones.
- API for the code that tafla-gen generates, which lays out the fields whose
  sizes the schema fixes itself, and may change with it:
  `TableSizer.addTableBytes()`, which adds a nested table or a vector's items
  of fixed length in one call; `TableWriter.reserveTableBytes()`,
  `reserveTableVector()` and `reserveStructArray()`, which write a field's
  offset, and a vector's count and item ends or an array's count, and return
  where in `target()`, the writer's bytes, the caller writes the table, the
  items or the structs; `TableWriter.tableBytes()` and
  `versionedTableBytes()`, the bytes of a message written without a sizer and
  a writer, with its header, which check it as a sizer does; `utf8Length()`
  and `writeUtf8()`, for a string after such fields; and `viewOf()`, a
  `DataView` of a range of bytes, for a long run of structs or tables.
- `TableBuilder.setTableInPlace(fields, fill)` and
  `setVectorInPlace(count, fields, fill)`, which write a nested table, or
  each item of a vector, among the builder's own fields, through the
  `TableItem` that `fill` sets its fields on, rather than from a builder of
  its own. They write the bytes that nested builders write, and leave the
  builder as it was if `fill` throws or leaves fields unset. They're the way
  to nest tables that a program builds as it writes their parent: in Node
  26, the comparison suite's vector of 100 nested tables took 5.4 µs so,
  against 21 µs from a builder per item and 29 µs from the bytes of each,
  and allocated 16 KB on V8's heap rather than 94 KB and 114 KB.

### Changed

- The project is named tafla rather than buffy, and the package is `tafla`
  rather than `@buffy/core`.
- The class that reads a table is `TableReader` rather than `ByteBuffer`.
- A layout's offset width is its offset type, `uint8`, `uint16` or `uint32`,
  in the API and its errors.
- `TableReader` reads a vector item in place within its message, as the format
  now specifies, including an item whose offsets point past its end.
- `TableReader.getVectorOffsets()` returns a `number[]` rather than a
  `Uint32Array`, so that positions past 4 GiB are exact.
- `TableReader` checks each read against its bytes' length at the time of the
  read, so bytes made over a resizable buffer without a length follow the
  buffer as it's resized. A read past them still throws a `DecodeError`.
- `TableReader` reads offsets, field offsets and vector items in less
  bytecode, with their failures out of line, so that V8 inlines them into
  their callers: a loop that reads a vector's nested tables through a
  generated view's `get(i)` inlines every read. It throws the same errors. In
  the comparison suite in Node 27, `TableReader`'s getters summed the vector of
  100 nested tables in 0.75 µs rather than 1.19, and tafla-gen's view in
  0.91 µs rather than 1.85. `getVectorLength()` and `getVectorItemOffset()`
  check a vector's ends with one comparison, which takes the vector's position
  to be an integer, as `rootOffset()` and `fieldOffset()` find it.
- `TableReader.getString()` reads a string of up to 32 bytes of ASCII a byte
  at a time, without a view of its bytes or a `TextDecoder`, and any other
  string with the fatal decoder as before, so it reads the same strings and
  throws the same `DecodeError`. The comparison suite read its record, whose
  name is 16 bytes, in 0.116 µs rather than 0.159 in Node 26, and 0.108 µs
  rather than 0.158 in Node 27, and the document of 100 rows in 0.222 µs
  rather than 0.261 in Node 26. A string of 32 bytes that ends in a character
  outside ASCII takes about 37 ns longer, and one that starts with one 8 ns.
- `TableBuilder` writes each field, and its offset, into a buffer when it's
  set, rather than recording it and writing every field through a
  `TableWriter` in `bytes()` and `finalize()`, which now copy the table out
  of the buffer into bytes of its length and leave the buffer for the next
  builder. It writes the same bytes, and its setters throw the same errors
  and leave it as it was when they throw. Arrays, arrays of structs, structs
  and tables given as bytes of 1 KiB or more are kept until the table is
  written and copied into it once; shorter values are copied when they're
  set, rather than all of them when the table is written. A string is given
  room in the buffer for its longest UTF-8 rather than counted first. So:
  - A number for an `int64` field, or a `bigint` for another, throws its
    `TypeError` from the setter, which leaves the builder as it was, rather
    than from `finalize()`, except in an array of 1 KiB or more.
  - Only an array of 1 KiB or more whose length has changed when the table
    is written makes `finalize()` and `bytes()` throw a `RangeError`, with a
    message of its own, and one whose buffer was transferred throws a
    `RangeError` rather than a `TypeError`.
  - An array-like whose length isn't a non-negative integer throws its
    `RangeError` from the setter, which leaves the builder as it was.
  - A nested builder is copied into its parent when it's nested, whatever
    its layout, rather than written into it when the parent is written,
    unless its table is 256 bytes or longer: its table is then written once,
    when the parent is, rather than copied into each table that nests it in
    turn.

  In the comparison suite in Node 26, the builder wrote the record in
  0.127 µs rather than 0.181, the document of 100 rows in 0.444 µs rather
  than 0.589, or 0.623 µs rather than 0.758 from `number[]` columns, and the
  vector of 100 nested tables, written in place, in 5.4 µs rather than
  10.9, with half the bytes on V8's heap; in Node 27, in 0.68, 0.78, 0.83
  and 0.53 times the time. The documents of 100,000 rows and the array of
  100 structs took as long as before. A string of 2,000 ASCII characters
  took 0.34 µs rather than 3.9, and one of 50,000 took 6.5 µs rather than
  103, though it's written into 150 KB outside V8's heap, room for 3 bytes
  for each character, which is then cut to its 50 KB.

  Some shapes take longer, in Node 26 and 27 alike, since a table is written
  into a buffer and then copied or kept, where it was recorded and written
  once, and only one buffer is kept between builders:
  - A vector of 100 nested tables from a builder per item took 1.4 times as
    long, 21.3 µs rather than 14.8, and one from the bytes of each 1.1 to
    1.2 times. Builders of 24 bytes, each nested in the next, took 1.6 to
    2.0 times as long, and of a 2 KiB array each 1.2 to 1.3 times.
  - A vector of 200 builders, all built before it, each of a `uint32` and a
    `Float32Array` of 128, took 3.2 to 3.4 times as long, since each builder
    grows a buffer of its own.
  - Two builders written at once took 1.4 times as long for the record,
    eight 1.8 to 1.9 times, and one after a builder that was never written
    2.0 times, since the second builder grows a buffer of its own.
  - Tables longer than 64 KiB, whose buffer isn't kept for the next builder:
    130 columns of 500 bytes took 1.3 times as long, and a vector in place
    of 1,000 tables of a `uint32` and a `Float32Array` of 128 1.3 times,
    allocating 1.45 MB outside V8's heap rather than 0.53 MB.
- The package is one module, `dist/index.js`, which rolldown bundles, with
  the value of each constant, such as `SIZEOF_64`, written where it's used.
  V8 loads an imported binding from its module at every use, even in
  optimized code. `StructWriter`'s setters write 100 scalars in 0.45–0.55×
  the time, and `TableBuilder` writes the comparison suite's vector of 100
  nested tables in 0.91–0.93×.
- `dist/index.js` leaves out the comments, which the declarations still
  carry, and has no character outside Latin-1. V8 keeps a module's source on
  its heap, at two bytes per character if one is outside Latin-1, as three en
  dashes in comments made it. Importing the package grows the heap by 184 KB
  rather than 331 KB in Node 26, and by 183 KB rather than 330 KB in Node 27.
  The module is 56,203 bytes rather than 101,602, and 9,817 gzipped rather
  than 22,384.
- `TableWriter.tableBytes()`, `versionedTableBytes()`, the write functions,
  `writeUtf8()` and `utf8Length()` take less bytecode on their common paths,
  so that V8 inlines a serializer that tafla-gen generates whole: TurboFan
  inlines up to 920 bytes of bytecode into a function, and a record's
  serializer and what it calls took about 2,100. Their checks are one
  condition each, which finds the error to throw out of line, a string's
  characters outside ASCII and strings past 32 characters are encoded and
  counted out of line, a table without header slots doesn't call the
  function that writes them, and a write of 8 bytes copies two 32-bit words
  in a loop that V8 unrolls. They throw the same errors, in the same order.
  In the comparison suite, with the package bundled, tafla-gen's record
  took 0.062 µs to write rather than 0.088 in Node 26, and 0.060 µs rather
  than 0.080 in Node 27. Records that aren't constants, which V8 can't fold
  into the benchmark's loop, took 0.094 µs rather than 0.112 in Node 26, and
  0.087 µs rather than 0.108 in Node 27. `writeFloat64()`, `writeInt64()`
  and `writeUint64()` took 3% to 4% less, and `TableBuilder`'s setters 4% to
  9% less.
- `readInt64()` and `readUint64()` copy a value's bytes to a scratch
  `DataView` and read it from there, as `readFloat64()` does, rather than
  adding its two 32-bit words as bigints, which allocated a bigint for each.
  They return the same values and throw the same `DecodeError`. With the
  package bundled in Node 26, 100 reads at unaligned offsets took 4.8 ns
  each rather than 26.2 for `readInt64()`, and 9.7 ns rather than 26.3 for
  `readUint64()`. `readFloat64()` took 2.75 ns, as before. The code that
  tafla-gen generates for a struct of every scalar type, which has one field
  of each, read its fields in 0.050 µs rather than 0.089.
- The write functions, `writeInt8()` to `writeFloat64()`, and `TableWriter`'s
  scalar setters, offsets and lengths write through one set of little-endian
  stores, and write the same bytes. A write that doesn't fit now throws its
  `RangeError` before converting its value, so a value of the wrong type at an
  offset outside the bytes throws a `RangeError` rather than a `TypeError`,
  and its `valueOf()` isn't called. `TableWriter`'s int64 and float setters
  copy their value's 32-bit words, as the write functions do, rather than its
  bytes. In Node 26 and 27, 100 integers of up to 32 bits took 0.55–0.89 times
  as long to write with the write functions, and 0.85–0.99 with `TableWriter`;
  100 float32s took 0.89–1.02 with `writeFloat32()`, and 100 int64s and floats
  0.95–1.02 with `TableWriter`. The 64-bit write functions check their offset
  in a function of their own, so that V8 inlines them into tafla-gen's
  serializers without counting them against its budget. In Node 26 the
  comparison suite's record then took as long to write as before, 78 ns, and
  100 64-bit writes in a loop 1.04–1.07 times as long; in Node 27 the record
  took 0.99–1.02, and the 64-bit writes as long as before. In Node 26, an
  integer held as a double could make the write functions take 3.7–4.3 times
  as long for the rest of the process, as V8 stopped inlining the `DataView`
  setter that they called; the stores call none.

### Removed

- `TableReader` only reads tables, and its setters are removed, with
  `reset()`. Tables are written with `TableBuilder`, or with `TableWriter` and
  `TableSizer`, and structs and other records at fixed offsets with
  `StructWriter`.

### Fixed

- A struct writer whose write threw no longer breaks later writes.
- `TableWriter` writes padding as zeros and never writes past its bytes.
- `TableReader` fails with `DecodeError` on a detached or shrunk buffer.
- The runtime rejects an alignment other than 1, 2, 4 or 8, as Java's does,
  rather than padding to it or dividing by zero.
- `TableSizer` accepts a struct of 0 bytes and rejects an array of 0-byte
  structs, as the builders and writers do.
- `TableWriter.setString()`, `TableBuilder.setString()` and `writeUtf8()`
  write a string that ends their bytes, as a table's last field or the end of
  a message that tafla-gen's code writes, rather than throwing `RangeError`
  for some strings of more than 256 UTF-16 code units that aren't all ASCII.
  Node 26.13 and 27's `TextEncoder.encodeInto()` stops a code unit short of
  about 1% of such strings with lone surrogates, and of some well-formed
  ones, and the rest of the string is now written after it.

[Unreleased]: https://github.com/snovader/tafla/commits/main/typescript
