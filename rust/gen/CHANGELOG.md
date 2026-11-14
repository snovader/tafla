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
- A generated static `write(bytes, offset, ...)` on Java's structs, which
  writes a struct's values without creating its record, and a generated
  `serialize(count, fill)` on Java's messages of an array of structs alone,
  whose fill writes each struct straight into the table with it, from what
  the caller holds, rather than from an array of records. Java's comparison
  suite wrote its array of 100 structs so in 0.109 µs rather than 0.338 µs,
  allocating 1.2 KB rather than 4.0 KB.
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
- Generated Go declares `Fields()` on each message and table, which reads
  every field at once into the `<Name>Fields` that it's written from, nested
  tables and the items of vectors included, in one function rather than a
  getter's call for each field, except where a vector holds tables that
  allocate when they're read whole, so that what it allocates stays in
  proportion to the bytes. A field named `fields` is renamed `Fields_`.
- Generated TypeScript and JavaScript declare `fields()` on each message and
  table whose vectors hold tables of scalars, structs and such tables, which
  reads every field at once into the `<Name>Fields` that it's written from,
  nested tables and the items of vectors and arrays of structs included,
  without a reader for each, a static `read(buffer, position)` on each such
  nested table, `fields()` on the views of their vectors and of arrays of
  structs, and a static `read(bytes, offset)` on each struct, which reads its
  fields without creating it. A field named `fields` takes a `_`.
- Generated TypeScript and JavaScript views of arrays of structs have a
  getter of each field of their structs, such as `triples.value(i)`, which
  reads that field of struct `i` without creating the struct. A field named
  `length`, `get` or `fields` takes a `_` there.
- Generated Python declares `fields()` on each message and table whose
  fields are all scalars, structs and tables of them, and a string last,
  which reads every field at once into the `TypedDict` that it's written
  from, through `ByteBuffer.get_fields()`. It reads them through the getters
  when the table's offsets aren't those that its writer writes, so its values
  and errors are theirs. The getters stay lazy.
- A generated `Read<Struct>()` beside each Go struct, which reads it from
  the start of bytes and returns it, so that the compiler can keep it in
  registers, where `UnmarshalBinary()` reads into a struct in memory. The
  checker keeps declarations from taking its name. The Go comparison suite
  reads its fixed-size record with it.
- A generated Go `<Struct>Array` declares `AsSlice()` and `Slice()`, which
  return its structs at once, as `tafla.ArrayView` returns arrays:
  `AsSlice()` in place, through the Go runtime's `StructsInPlace()`, where
  the struct's Go struct lies in memory as its bytes do on every platform,
  with no bool, and the bytes are aligned to it on a little-endian host, and
  `Slice()` in place where it can, and otherwise a copy. `Fields()` reads
  arrays of structs with `Slice()`, so they share the bytes, as its arrays
  do. The Go comparison suite reads its array of 100 structs with it.
- Generated Python views of arrays of structs of scalars have
  `iter_field_values()`, which iterates over the structs' values as tuples,
  and `item_field_values(index)`, which reads one struct's, without creating
  their dataclasses. A view reads a struct by index with one unpack, whose
  check of the offset is the check of the index.

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
- A generated Rust getter of a vector or an array of structs returns the view
  of its items, a `<Table>Vector` or a `<Struct>Array`, which reads each item
  when it's asked for, rather than a `Vec` of every item's reader or an
  iterator over the structs, and the `<field>_view()` getter beside it is
  gone. An item whose bytes are corrupt fails when it's read, rather than
  when the getter is called. A caller that wants every item at once collects
  the view's `iter()`.
- Generated Rust writes a nested table whose fields' sizes the schema fixes
  whole, from the array that its `to_bytes()` returns, and sizes a vector of
  them in one call, as Go and C++ do. A message of such fields, with or
  without a string after them, is written whole too, at constant positions,
  into the bytes that the runtime's `TableWriter::table_bytes()` returns. The
  benchmarks' vector of 100 nested tables took 0.15 µs to write rather than
  0.83 µs, and the record 20 ns rather than 30 ns.
- Generated C++ readers of tables nested in a message hold the bytes from the
  table's start, 16 bytes rather than 24, and read its offsets at constant
  positions, so their decode errors report positions from the table's start,
  as a vector item's do.
- Generated Go getters of vectors and arrays of structs return the views
  that read each item when it's asked for, a `<Table>Vector` or a
  `<Struct>Array`, rather than a slice of every item, and the separate
  `<Field>View()` getters are gone. So a corrupt item fails with the decode
  error when it's read rather than when the getter is called. Collect a view
  with its `All()` for every item at once. The Go comparison suite reads its
  vector of 100 nested tables in 1 allocation rather than 2.
- Generated Go getters and views read structs by value too, a struct's
  `MarshalBinary()` allocates exactly its size rather than appending to
  empty bytes, and a `<Struct>Array`'s `All()` steps through its bytes,
  without a bounds check for each struct.
- Generated Go getters read uint32 offsets with the Go runtime's
  `Uint32FieldOffset()`, which the compiler inlines, at the slot of each
  field, rather than through `ByteBuffer.FieldOffset()` or `RootOffset()`, so
  that a getter is one call rather than two.
- The generated C++ getter of a vector or an array of structs returns its
  view, a `<Table>Vector` or a `<Struct>Array`, which reads each item when
  it's asked for, rather than a `std::vector` of every item, and
  `<field>_view()` is removed. The view's `to_vector()` reads every item, as
  the getter did. A corrupt item of a vector fails when it's read, rather than
  when the getter is called.
- Generated C++ getters are `TAFLA_ALWAYS_INLINE`, as the runtime's getters
  are, and the getter of a nested table reads it through the runtime's
  `ByteBuffer::get_table()`, so it throws a `tafla::DecodeError` when the
  table's offset leads past the bytes, rather than returning a reader whose
  reads all fail.
- Generated fields that clash with the generated code's members, or that are
  named `then` in TypeScript, are renamed.
- Generated Java getters of vectors and arrays of structs return their views,
  `<Table>Vector` and `<Struct>Array`, which read each item when it's asked
  for, rather than a `List` or an array of every item, and `<field>View()` of
  them is gone. An item whose bytes are corrupt throws a `DecodeException`
  when it's read rather than from the getter. `List.copyOf()` or `toArray()`
  reads every item at once. Java's comparison suite read its vector of 100
  tables in 0.64 µs rather than 1.13 µs, and its array of 100 structs in
  0.20 µs rather than 0.53 µs, allocating 2.9 KB rather than 6.4 KB and
  5.7 KB.
- Generated Java views of arrays of structs read a struct with its
  package-private `readUnchecked()`, after checking its index, rather than
  with `read()`, which checked its bounds again. `read()` checks them as
  before. Java's comparison suite read every struct of its array of 100
  through the view 3% faster, in 0.066 µs rather than 0.068 µs.
- Generated Python getters of vectors and arrays of structs return their
  views, `<Table>Vector` and `<Struct>Array`, which read each item when it's
  asked for, rather than a list of every item, and `<field>_view()` is gone.
  A list of the items is `list(view)`. An item whose bytes are corrupt raises
  `DecodeError` when it's read, rather than when the getter is called.
- Generated Java views of vectors iterate their items with
  `ByteBuffer.getVectorItemEnd()`, carrying each item's end to the next, so
  that a loop over a view reads one end for each item rather than two.
  Java's comparison suite summed every item of its vector of 100 tables so
  in 0.427 µs rather than 0.490 µs.
- tafla-gen exits with 2 on wrong arguments, and 1 on a schema's errors, and
  rejects an unknown option rather than reading it as a schema. It has
  `--help`, `--version` and `--`, takes options anywhere, and reports every
  schema it can't read or write rather than stopping at the first.
- Generated TypeScript and JavaScript getters of vectors and arrays of
  structs return their views, a `<Table>Vector` or a `<Struct>Array`, which
  read each item when it's asked for, rather than an array of every item, and
  the views' own getters, `<field>View()`, are gone. A corrupt item then
  fails with `DecodeError` when it's read, rather than when the getter is
  called.
- Generated TypeScript and JavaScript views of arrays of structs check an
  index only by reading within their bytes, rather than checking it in each
  getter, which took 12% longer to sum an array's structs: an index outside
  the array throws `DecodeError`, which is a `RangeError`, rather than a
  plain `RangeError`, and it must be an integer.
- Generated TypeScript and JavaScript declare each class after the classes
  that its code reads, rather than in the schema's order: a table after its
  nested tables and structs and its vectors' views and items, and a view
  after its items, except in a cycle of tables, where a vector's view comes
  before its items. V8 reads an exported class in a method declared before
  it through a check that it's initialized, which kept the method's receiver
  on the heap: summing a vector of 100 nested tables through `get(i).pair()`
  allocated each item's reader, 4,280 bytes, and took 0.908 µs in Node 27,
  against 280 bytes and 0.825 µs now, with the runtime's inlined vector item
  reads.

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
