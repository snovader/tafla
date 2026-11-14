# Benchmarks

`tafla_comparison` compares tafla's C++ runtime with other C++ libraries that
encode small tables and numeric columns. It runs the workloads of the
TypeScript comparison in `typescript/benchmarks/`, with the same values, so
that the languages' numbers compare. Build it from `cpp/` with
`-DTAFLA_BUILD_BENCHMARKS=ON`, as [Running](#running) shows.

## Workloads

- **record**: one table of two float64s, an int64 and a 16-byte string.
- **document of 100 and 100,000 rows**: a sized, versioned table of two
  float64s and a string, with a nested table of an int64 and three columns, a
  uint32 and two float32.
- **vector of 100 nested tables**: a table holding a vector of tables, each a
  float32 and a nested table of two float32s.
- **array of 100 structs**: a table holding the same items as an array of
  structs of their three float32s, 12 bytes each.
- **fixed-size record**: a uint32 and two float32s at fixed offsets.

Each shape is serialized and deserialized into the types of
`src/workloads.hpp`, whose strings and columns are a `Text` and a `Column`:
views of the bytes where a library reads them in place, as `std::string_view`
and `std::span`, and copies otherwise. Column `a` of each document is also
summed through each library's per-element accessor. `src/workloads.hpp` ports
the TypeScript generator, so the inputs are the same values, which
`tests/benchmarks_test.cpp` checks against values that the TypeScript
generator printed. Each of these benchmarks also times tafla's codec again,
as **tafla A/A**, an A/A pair whose ratio to tafla's shows the harness's
noise.

**document of 100,000 rows: serialize into reused bytes** times tafla-gen's
`serialize_into()`, which writes the document into the start of bytes that
the caller keeps, here a `std::vector` that each call writes over, as a
caller that sends each document before writing the next would, rather than
into new bytes, as the serialize rows do. It times it again as **tafla-gen
A/A**. Its time compares with tafla-gen's in the document's serialize row. No
other codec here reuses a buffer: FlatBuffers' builder, protobuf's string and
msgpack-cxx's `sbuffer` are new for each message, though a builder's
`Clear()`, `SerializeToArray()` and an `sbuffer`'s `clear()` would reuse
theirs.

Four more groups time only tafla's bytes of the vector of nested tables and
of the array of structs, read in two ways: **read the last item**, which a
reader can find without reading the others, and **sum every item**. Each way
sums the three float32s of the items it reads as doubles, rather than
collecting items of its own, and `src/item_access.hpp` holds them:

- **tafla** reads with `ByteBuffer`'s getters by hand: each item in place,
  from `Vector::get_in_place()` of the last item or `items_in_place()`, and
  `get_struct_array()`'s bytes with `load_le()`.
- **tafla-gen** reads with the generated getters, `items()`, which return
  views: a vector's `get(i)` reads one item, an array's `operator[]` reads
  one struct without checking its index, as the read by hand does, and their
  iterators read every item. The generated readers are passed by value, as
  `ByteBuffer`'s spans are by hand.
- **tafla-gen A/A** reads with the getters again, an A/A pair that shows the
  harness's noise.

`tafla_encoding` prints each library's encoded sizes and what it does with a
truncated document, as the TypeScript comparison does, and
`tafla_allocation` prints the allocations and bytes of each benchmark's call.

### What differs from TypeScript

- There are no `number[]` inputs. C++'s columns are typed already.
- tafla's columns are read as views of the bytes, through
  `ArrayView::as_span()`, when they're aligned on a little-endian host, and so
  are FlatBuffers', through `Vector::data()`, as C++'s FlatBuffers reads them.
  protobuf's are views of the repeated fields that it decodes them into, and
  its document is returned in a `Kept` with the message, so that only its
  reads pay for keeping what they view. MessagePack copies its columns into
  vectors. Summing a column shows each library's per-element access.
- tafla's C++ runtime has no `StructWriter`. The tafla codec writes the
  fixed-size record with `store_le()` and reads it with `ByteBuffer`'s
  getters. tafla-gen's struct writes and reads it with the generated
  `to_bytes()` and `read()`, and `bit_cast` copies the record as it is, a
  struct of its three fields, with `std::bit_cast`, which C++ code would
  write such a record with.
- MessagePack's columns are arrays of float32s and uint32s, packed and
  unpacked element by element, as serde serializes a `[f32]`. msgpackr's
  typed-array extensions are msgpackr's own.
- FlatBuffers' and protobuf's code is generated at build time by `flatc` and
  `protoc`, from `schemas/`, as their documentation shows, rather than
  written by hand in the shape that they generate, as the Rust, Java and
  Python suites write it.
- `yarn bench`'s benchmarks of each getter and setter aren't ported.

## Libraries

| Library                             | Why it's comparable                                                                                                            |
| ----------------------------------- | ------------------------------------------------------------------------------------------------------------------------------ |
| tafla (`include/`)                  | The reference that the others are compared with.                                                                               |
| tafla-gen's code                    | tafla's tables, written and read by the code that tafla-gen generates.                                                         |
| FlatBuffers 25.12.19                | Zero-copy tables with offsets and vectors, like tafla.                                                                         |
| protobuf 36.2                       | The most used schema-based codec, with packed repeated floats.                                                                 |
| msgpack-cxx 9.0.0                   | MessagePack's C++ library: a schemaless format, whose tables are maps with their keys.                                         |
| `std::bit_cast`                     | For the fixed-size record only: a struct of its fields at fixed offsets, copied as it is, as C++ code writes such a record.    |

Each library is used as its documentation shows, and as the TypeScript
comparison uses its counterpart:

- **tafla** records nested tables in place among their parent builder's
  fields, with `set_table(fields, fill)` and `set_vector(count, fields,
  fill)`. The builders view the columns and write each array in bulk into a
  table of its exact size. A builder records its first fields within itself,
  so a small table allocates nothing until it's written. Readers check the
  document's version, and read columns as `ArrayView`s, whose `as_span()`
  views them. The array of structs is written with `store_le()` into a vector, which the
  builder views through `set_struct_array()` until it writes the table, and read from
  `get_struct_array()`'s bytes with `load_le()`.
- **tafla-gen** is the code that tafla-gen generates from `workloads.tafla`,
  in `generated/tafla/`. A message's `serialize()` counts the table's length
  with a `TableSizer`, and then writes every field, nested tables included,
  into bytes of that length with a `TableWriter`, in one allocation. The
  record, whose fields' sizes are fixed but for a string after them, it writes
  whole instead, each field at a constant position in bytes of its length,
  without a sizer and a writer. Its readers view strings and columns, as
  tafla's do, and read with a `ByteBuffer` in the table's layout, a constant.
  It takes the workloads' types, as the other codecs do, and writes the
  vector's items and the array's structs from `std::views::transform()` of
  the workload's items, with the generated generic fields structs, which
  convert each item as they write it. The generated getters of the vector
  and of the array of structs return views of their items, which the codec
  iterates into the workload's items, reading each item once.
- **FlatBuffers** builds each message with a new `FlatBufferBuilder` of the
  default size, which grows by doubling, and each table with flatc's
  `Create*()` functions. Columns are written from spans with
  `CreateVector()`, and the vector of structs with `CreateVectorOfStructs()`
  from a vector of flatc's structs. A message is the builder's buffer,
  released without a copy. Readers verify the message with a `Verifier`, as
  the generated `Verify*Buffer()` of a root type does, and view strings and
  columns.
- **protobuf** builds messages from the input and writes them with
  `SerializeAsString()`, which sizes the bytes first, and reads them with
  `ParseFromArray()`. Its messages own their strings and repeated fields, so
  building one copies its columns, as a caller that keeps its values would.
  Reading decodes the columns into the message's repeated fields, which the
  document views, as tafla's and FlatBuffers' documents view their bytes,
  and returns it in a `Kept` with a `shared_ptr` to the message. It moves its
  strings. protobuf has no structs,
  so the array of them is a repeated message of the three floats.
- **msgpack-cxx** packs the workloads' values with its `packer` into an
  `sbuffer`, a table as a map with its field names, as msgpackr packs an
  object, and unpacks them into an object, which the codec reads its fields
  from. Unpacking references strings in the bytes rather than copying them.
  The array of structs packs each struct, and the table that holds them, as
  an array of its fields in order, without their names, as every language
  packs that fixed shape.
- **`bit_cast`** writes the fixed-size record with
  `std::bit_cast<std::array<std::uint8_t, 12>>()` of a struct of its fields,
  and reads it with `std::bit_cast` of its first 12 bytes, which are the
  format's little-endian bytes on a little-endian host only.

Every codec collects a list's items into a `std::vector` that it sizes
first, and assigns each item, as `StructArray::to_vector()` and
`TableVector::to_vector()` do. `reserve()` and `push_back()` took five times
as long for 100 items of 12 bytes, 225 ns against 41 ns: Apple Clang
compiled libc++'s `push_back()` into a loop that copies each item through
the stack and stores the vector's end, where the assignments copy 8 items an
iteration.

A field that FlatBuffers' bytes leave out reads as null, which its codec
throws an error for, since every workload has it, as protobuf's and
MessagePack's codecs do for a message that they can't parse.

## Method

`src/harness.hpp` times each benchmark's codecs interleaved, as the
TypeScript and Java comparisons do: after calibrating each codec's calls to
rounds of about 10 ms, it runs 5 warm-up rounds and then 30 rounds of every
codec, each round running them once in alternating order, and reports the
fastest round's time per call and the median round's. Load from other
processes then slows every codec alike, and only for some rounds.

- Every codec is a type of its own, and each benchmark's loop is a lambda
  compiled for its codec's call alone, so nothing is shared between their
  loops; C++ has no JIT feedback to share.
- Inputs pass through an empty `asm` statement, so that the compiler can't
  fold the constant record and fixed-size record into the code, and each
  call's result is kept alive by another and destroyed within the loop, as a
  caller frees its bytes.
- Before timing, every benchmark checks that its codec reads back exactly
  what it wrote, with floats compared by their bits, and that it sums column
  `a` to the same bits. `ctest` runs `benchmarks_test`, which builds every
  benchmark, and so checks every codec on every workload, and runs each of
  its calls once.
- The release build compiles with `-O3` and without `-march=native`. tafla
  and tafla-gen are headers, which inline into each call; FlatBuffers and
  msgpack-cxx are headers too, and protobuf a shared library.
- The `allocation` binary counts what each benchmark's call allocates: the
  allocations and reallocations, and the bytes they ask for, over 100 calls
  after one, divided among them. It replaces the global `operator new`, which
  counts what the libraries allocate in C++, and defines `malloc()`,
  `calloc()` and `realloc()`, which count what code compiled into the program
  allocates with them, as msgpack-cxx's header-only zone and `sbuffer` do. On
  macOS, protobuf's shared library allocates its own `malloc()`s past them,
  but it allocates through `operator new`. A reallocation counts its new size,
  and freeing isn't counted.

## Running

Every timing run waits until the machine is quiet: check `uptime` first.

```sh
cmake -S . -B build -G Ninja -DCMAKE_BUILD_TYPE=Release -DTAFLA_BUILD_BENCHMARKS=ON
cmake --build build --target tafla_comparison tafla_encoding tafla_allocation benchmarks_test
uptime
build/benchmarks/tafla_comparison --markdown
build/benchmarks/tafla_encoding
build/benchmarks/tafla_allocation
```

The full run takes about 40 seconds. Arguments select benchmarks by a part of
their name, such as `'document of 100,000'` or `item`, and `--rounds`,
`--warmup-rounds` and `--sample-ms` set the method's numbers. `--quick` runs 3
rounds of 1 ms, which checks that every benchmark works in a few seconds; CI
runs it, with the encoding and allocation reports. `--markdown` prints the
table below too.

## Code checks

The benchmarks compile with the warnings of tafla's own targets, as errors in
CI, and clang-format formats them with the rest of `cpp/`. flatc's and
protoc's generated code is compiled as the libraries' own, with their
headers as system headers, and tafla-gen's is checked as the golden files of
`tests/generated` are.

## Limitations

- The Apple M2 Pro has 6 performance and 4 efficiency cores, and macOS can
  move the benchmark's thread to an efficiency core when others are busy.
- Only a little-endian host was timed. On a big-endian host, tafla's
  `as_span()` returns nothing, so its columns are copied, swapping their
  bytes, and the comparison leaves `bit_cast` out, since its record isn't
  the format's bytes there.
- Only macOS's allocator was timed, which every library's allocations depend
  on.
- FlatBuffers' builder starts at 1 KiB and doubles by copying, and
  msgpack-cxx's `sbuffer` starts at 8 KiB. A builder or buffer reused for
  each message would make writing cheaper, but the TypeScript comparison
  doesn't reuse its builder either.
- protobuf copies the input's columns into its message, which a caller that
  builds its values in protobuf's messages wouldn't. tafla-gen's codec
  converts the vector's and the array's items from the workload's as it
  writes them, and views the columns.
- A failed read throws, which takes about 4.5 µs to unwind on macOS, so a
  stream of corrupted messages reads at about 200,000 a second a core. The
  benchmarks time valid bytes only, which reading never throws for.

## Results

Two full runs, one after the other, on 2026-11-13, of main at c43286c, on an
Apple M2 Pro with 32 GB in macOS 27.0.1, compiled by Apple clang 21.0.0
(clang-2100.4.1.1) with Homebrew's FlatBuffers 25.12.19, protobuf 36.2 and
msgpack-cxx 9.0.0. Each took 37 seconds. The 1-minute load average was 5.1
at the first run's start, 20 seconds after the build, 4.4 between the runs
and 3.8 at the second's end, on the 10 cores, where this machine idled at
about 4 with a browser and other sessions open. In the second run, the
median round was within 3% of the fastest for 97 of the 102 codecs' runs of
a benchmark, and at most 5% above it. In the first, it was for 79, and up to
13% above it in the record's benchmarks, which it ran first. The two runs'
fastest rounds were within 2% of each other for 84 of the 102, within 5% for
97, and within 8% for all but msgpack-cxx's write of the fixed-size record,
42.8 ns in one and 49.8 ns in the other.

Fastest round of the two runs in µs per call, and its ratio to tafla's:

| Benchmark                                          |    tafla |        tafla-gen |     flatbuffers |       protobuf |          msgpack |         bit_cast |        tafla A/A |
| -------------------------------------------------- | -------: | ---------------: | --------------: | -------------: | ---------------: | ---------------: | ---------------: |
| record: serialize                                  |   0.0458 |   0.0225 (0.49×) |  0.0539 (1.18×) | 0.0726 (1.59×) |   0.0551 (1.20×) |                  |   0.0456 (1.00×) |
| record: deserialize                                |  0.00628 |  0.00668 (1.06×) | 0.00867 (1.38×) | 0.0534 (8.50×) |    0.155 (24.7×) |                  |  0.00638 (1.02×) |
| document of 100 rows: serialize                    |    0.149 |   0.0430 (0.29×) |   0.167 (1.12×) |  0.469 (3.15×) |    0.670 (4.50×) |                  |    0.149 (1.00×) |
| document of 100 rows: deserialize                  |   0.0117 |   0.0125 (1.07×) |  0.0225 (1.92×) |  0.644 (55.0×) |     1.793 (153×) |                  |   0.0117 (1.00×) |
| document of 100 rows: sum a column per element     |   0.0357 |   0.0353 (0.99×) |  0.0584 (1.64×) |  0.667 (18.7×) |    1.855 (52.0×) |                  |   0.0353 (0.99×) |
| document of 100,000 rows: serialize                |   16.507 |   16.389 (0.99×) |  43.786 (2.65×) |  347.7 (21.1×) |    647.5 (39.2×) |                  |   16.523 (1.00×) |
| document of 100,000 rows: deserialize              |   0.0115 |   0.0124 (1.08×) |  0.0224 (1.95×) | 413.9 (35991×) | 1213.7 (105539×) |                  |   0.0115 (1.00×) |
| document of 100,000 rows: sum a column per element |   87.368 |   85.911 (0.98×) |  88.235 (1.01×) |  504.9 (5.78×) |   1303.9 (14.9×) |                  |   88.185 (1.01×) |
| vector of 100 nested tables: serialize             |    7.143 |   0.453 (0.063×) |   3.055 (0.43×) |  5.048 (0.71×) |    2.704 (0.38×) |                  |    7.136 (1.00×) |
| vector of 100 nested tables: deserialize           |    0.260 |    0.344 (1.32×) |   1.025 (3.94×) |  6.038 (23.2×) |    5.631 (21.7×) |                  |    0.261 (1.00×) |
| array of 100 structs: serialize                    |    0.149 |   0.0787 (0.53×) |   0.148 (0.99×) |  2.823 (18.9×) |    0.841 (5.64×) |                  |    0.148 (0.99×) |
| array of 100 structs: deserialize                  |   0.0410 |   0.0386 (0.94×) |  0.0457 (1.11×) |  3.494 (85.2×) |    2.236 (54.5×) |                  |   0.0412 (1.00×) |
| fixed-size record: serialize                       | 0.000405 | 0.000403 (1.00×) |   0.0444 (110×) | 0.0112 (27.7×) |    0.0428 (106×) | 0.000398 (0.98×) | 0.000401 (0.99×) |
| fixed-size record: deserialize                     | 0.000403 | 0.000400 (0.99×) | 0.00568 (14.1×) | 0.0239 (59.3×) |     0.139 (345×) | 0.000399 (0.99×) | 0.000398 (0.99×) |

- tafla reads a document in 12 ns, at 100 rows and at 100,000 alike,
  checking its version and viewing its columns, and tafla-gen in 12.5 ns.
  FlatBuffers views its columns too, in 22 ns, which include its
  `Verifier`'s check of the message. Each includes destroying the output's
  three `Column`s, whose `std::shared_ptr` keeps protobuf's message alive
  where its codec views it: in a build without it, tafla read the document
  in 10.2 ns, tafla-gen in 10.7 ns and FlatBuffers in 20.5 ns. protobuf and
  msgpack-cxx decode every element, in 0.41 and 1.21 ms for 100,000 rows.
- tafla and tafla-gen write the large document in 16.5 and 16.4 µs, copying
  its columns whole into a table of its exact length. FlatBuffers copies them
  whole too, in 44 to 46 µs, into a builder that grows by copying, through
  3.5 MB in 4 allocations. protobuf and msgpack-cxx write each element, in
  348 and 648 µs.
- Summing 100,000 floats takes 86 to 88 µs through tafla's, tafla-gen's and
  FlatBuffers' accessors alike, bound by the additions, each of which waits
  for the one before. Summing 100 takes tafla and tafla-gen 35 to 36 ns and
  FlatBuffers 58 ns, whose extra 23 ns doesn't grow with the rows.
- tafla-gen writes the record in 23 ns and the small document in 43 to 46 ns,
  each in one allocation of the table's exact length, against 46 and 149 ns
  through tafla's builder and 54 and 167 ns through FlatBuffers'. msgpack-cxx
  writes the record in 55 ns, into a new `sbuffer` of 8 KiB, and protobuf in
  73 ns.
- tafla reads the record in 6.3 ns, tafla-gen in 6.7 ns and FlatBuffers in
  8.7 ns, against protobuf's 53 ns and msgpack-cxx's 155 ns, whose zone
  allocates 8.8 KB for it.
- The vector of 100 nested tables is the shape that tafla's builder writes
  slowest, as in the other languages: 7.1 µs, against msgpack-cxx's 2.7 µs,
  FlatBuffers' 3.1 µs and protobuf's 5.0 µs. Its 200 builders allocate
  nothing of their own, but each item's builder is 216 bytes of the codec's
  vector of `NestedTable`s, and `set_vector()` copies the records of the
  items' 500 fields into the parent's, 16 KB. Recorded in place, with
  `set_vector(count, fields, fill)`, the same bytes took 3.2 µs in a separate
  run of the harness, against 6.9 µs from item builders and 3.0 µs through
  FlatBuffers. tafla-gen writes it in 0.45 µs, the fastest, in one
  allocation of the table and one of the vector of the items' generated
  fields, which the codec collects from the workload's.
- tafla reads the vector in 0.26 µs and tafla-gen in 0.34 µs, whose getter
  reads the items' readers into a vector of 1,600 bytes first, against
  FlatBuffers' 1.03 µs and protobuf's and msgpack-cxx's 6.0 and 5.6 µs.
- tafla writes the 100 structs in 149 ns, against 7.1 µs for the same items as
  nested tables, and reads them in 41 ns, against 260 ns. tafla-gen writes
  them in 79 ns, from the generated structs that the codec collects straight
  into the table, and FlatBuffers in 148 ns. tafla, tafla-gen and
  FlatBuffers read them in 39 to 46 ns, about as long as filling the vector of
  100 items took alone, 41 ns. protobuf and msgpack-cxx write and parse each
  struct's three fields, in 0.84 to 2.8 µs and 2.2 to 3.5 µs.
- tafla, tafla-gen and `bit_cast` write and read the fixed-size record in
  0.40 ns alike, about one and a half cycles of a performance core at
  3.5 GHz, which is the harness's loop around a few loads or stores.
  FlatBuffers takes 44 ns to write it, allocating its builder's 1 KiB,
  protobuf 11 ns and msgpack-cxx 43 ns, allocating its 8 KiB `sbuffer`.
  Reading it takes FlatBuffers 5.7 ns, with its `Verifier`, protobuf 24 ns
  and msgpack-cxx 139 ns.

Bytes allocated per call, and the allocations and reallocations that ask for
them, which `tafla_allocation` counts:

| Benchmark                                          |         tafla |     tafla-gen |   flatbuffers |      protobuf |         msgpack | bit_cast |
| -------------------------------------------------- | ------------: | ------------: | ------------: | ------------: | --------------: | -------: |
| record: serialize                                  |        60 (1) |        60 (1) |     1,024 (1) |        72 (2) |       8,192 (1) |          |
| record: deserialize                                |         0 (0) |         0 (0) |         0 (0) |        24 (1) |       8,768 (4) |          |
| document of 100 rows: serialize                    |     1,552 (2) |     1,296 (1) |     2,560 (2) |     2,584 (6) |       8,192 (1) |          |
| document of 100 rows: deserialize                  |         0 (0) |         0 (0) |         0 (0) |     1,416 (6) |       9,968 (7) |          |
| document of 100 rows: sum a column per element     |         0 (0) |         0 (0) |         0 (0) |     1,336 (5) |       9,968 (7) |          |
| document of 100,000 rows: serialize                | 1,200,352 (2) | 1,200,096 (1) | 3,500,000 (4) | 2,387,640 (6) |   4,186,112 (9) |          |
| document of 100,000 rows: deserialize              |         0 (0) |         0 (0) |         0 (0) | 1,200,216 (6) | 13,791,704 (10) |          |
| document of 100,000 rows: sum a column per element |         0 (0) |         0 (0) |         0 (0) | 1,200,136 (5) | 13,791,704 (10) |          |
| vector of 100 nested tables: serialize             |    40,840 (3) |     4,408 (2) |     8,720 (5) |   9,912 (202) |       8,192 (1) |          |
| vector of 100 nested tables: deserialize           |     1,200 (1) |     2,800 (2) |     1,200 (1) |  10,416 (207) |      26,368 (7) |          |
| array of 100 structs: serialize                    |     2,480 (4) |     2,408 (2) |     4,200 (3) |   6,512 (102) |       8,192 (1) |          |
| array of 100 structs: deserialize                  |     1,200 (1) |     1,200 (1) |     1,200 (1) |   7,216 (107) |      18,168 (6) |          |
| fixed-size record: serialize                       |         0 (0) |         0 (0) |     1,024 (1) |         0 (0) |       8,192 (1) |    0 (0) |
| fixed-size record: deserialize                     |         0 (0) |         0 (0) |         0 (0) |         0 (0) |       8,768 (4) |    0 (0) |

- tafla-gen allocates each table once, at its exact length, and the codec
  allocates the vectors that it collects items into, and that it reads
  items into. tafla's builder records up to 4 fields within itself, and the
  fields of more, nested tables' included, in a vector: 256 bytes for the
  document's 8. The array of structs allocates the structs' bytes, 72 bytes
  in 2 allocations for the builder to own them, and the table.
- Reading never allocates in tafla, tafla-gen or FlatBuffers, but for the
  vectors that the codecs read items into, 1,200 bytes for 100 items, and
  tafla-gen's getter of the vector, which reads the items' readers into
  1,600 bytes first. protobuf allocates its messages, and msgpack-cxx a zone
  of at least 8 KiB for each message.

`tafla_encoding` printed:

```
Encoded size (bytes)
                    record     doc 100  doc 100000      vector     structs       fixed
  tafla                 60        1296     1200096        3208        1208          12
  tafla-gen             60        1296     1200096        3208        1208          12
  flatbuffers           72        1328     1200128        2832        1224          32
  protobuf              47        1243     1187503        1900        1700          16
  msgpack               53        1573     1499239        3510        1604          24
  bit_cast                                                                          12

Truncated document of 100000 rows: deserialize()
  tafla          600048 B  threw: Read exceeds the supplied buffer: 400000 bytes at 400092 of 600048
  tafla              24 B  threw: Read exceeds the supplied buffer: 8 bytes at 24 of 24
  tafla-gen      600048 B  threw: Read exceeds the supplied buffer: 400000 bytes at 400032 of 599988
  tafla-gen          24 B  threw: Read exceeds the supplied buffer: 8 bytes at 24 of 24
  flatbuffers    600064 B  threw: The FlatBuffers message isn't valid
  flatbuffers        24 B  threw: The FlatBuffers message isn't valid
  protobuf       593751 B  threw: The protobuf message isn't valid
  protobuf           24 B  threw: The protobuf message isn't valid
  msgpack        749619 B  threw: insufficient bytes
  msgpack            24 B  threw: insufficient bytes
```

Every library rejects the truncated document with an error. The sizes are
the other languages': tafla's, FlatBuffers' and protobuf's are the same in
every language, and msgpack-cxx's are rmp-serde's in Rust and msgpack-core's
in Java, which pack floats as float32s too. FlatBuffers' `Verifier` and
protobuf's parser return only `false`, which their codecs throw as an error.
tafla-gen's reader of the nested table of columns reads the bytes from that
table's start, 60 bytes into the document, so its error counts the position
and the length from there.

### Reading items through views

The four groups that read tafla's items, from the same two runs. Fastest
round in µs, and its ratio to `ByteBuffer`'s by hand:

| Benchmark                                       |   tafla |      tafla-gen |  tafla-gen view | tafla-gen view A/A |
| ----------------------------------------------- | ------: | -------------: | --------------: | -----------------: |
| vector of 100 nested tables: read the last item | 0.00310 |  0.103 (33.2×) | 0.00453 (1.46×) |    0.00457 (1.47×) |
| vector of 100 nested tables: sum every item     |   0.236 |  0.331 (1.40×) |   0.261 (1.11×) |      0.259 (1.10×) |
| array of 100 structs: read the last item        | 0.00112 | 0.0410 (36.6×) | 0.00160 (1.43×) |    0.00159 (1.42×) |
| array of 100 structs: sum every item            |  0.0582 |  0.117 (2.01×) |  0.0584 (1.00×) |     0.0581 (1.00×) |

Only tafla-gen's getters allocate: 1,600 bytes for the vector's 100 readers
and 1,200 for the array's 100 structs. The A/A pair's fastest rounds are
within 0.9% of each other.

- The getters read every item into a `std::vector` first, so reading the last
  item takes 103 and 41 ns, against 4.5 and 1.6 ns through the views, whose
  `get(i)` and `at(i)` read one item.
- The view sums the array of structs as fast as `load_le()` by hand, in
  58 ns. Reading the last struct takes it 1.6 ns against 1.1 ns, since
  `at()` checks its index, which the hand-written read leaves out.
- The view sums the vector's items 1.11 times as slowly as `ByteBuffer`'s
  getters by hand, and reads the last item 1.46 times as slowly. In both,
  Apple Clang calls the function that sums an item out of line rather than
  inlining it. The hand-written one takes the item's bytes, a `std::span` in
  two registers. The generated one takes the item's reader by reference, in
  memory, which holds the item's bytes from its start, 16 bytes, and reads
  its offsets at constant positions, as the hand-written one does. Reading
  the last item also calls the view's `get()` out of line, where the
  hand-written read inlines `Vector::get_in_place()`.
