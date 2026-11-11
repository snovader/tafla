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

Four more groups time only tafla's bytes of the vector of nested tables and
of the array of structs, read in three ways: **read the last item**, which a
reader can find without reading the others, and **sum every item**. Each way
sums the three float32s of the items it reads as doubles, rather than
collecting items of its own, and `src/item_access.hpp` holds them:

- **tafla** reads with `ByteBuffer`'s getters by hand: each item in place,
  from `Vector::get_in_place()` of the last item or `items_in_place()`, and
  `get_struct_array()`'s bytes with `load_le()`.
- **tafla-gen** reads with the generated getters, `items()`, which read every
  item into a `std::vector` first.
- **tafla-gen view** reads with the generated views, `items_view()`, whose
  `get(i)` or `at(i)` reads one item and whose iterators read every item.
- **tafla-gen view A/A** reads with the views again, an A/A pair that shows
  the harness's noise.

`tafla_encoding` prints each library's encoded sizes and what it does with a
truncated document, as the TypeScript comparison does, and
`tafla_allocation` prints the allocations and bytes of each benchmark's call.

### What differs from TypeScript

- There are no `number[]` inputs. C++'s columns are typed already.
- tafla's columns are read as views of the bytes, through
  `ArrayView::as_span()`, when they're aligned on a little-endian host, and so
  are FlatBuffers', through `Vector::data()`, as C++'s FlatBuffers reads them.
  protobuf's are views of the repeated fields that it decodes them into,
  which the output keeps, and MessagePack copies its into vectors. Summing a
  column shows each library's per-element access.
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

- **tafla** nests tables from their builders, which view the columns and
  write each array in bulk into a table of its exact size. A builder records
  its first fields within itself, and a builder nested in place records its
  fields among its parent's, so a small table allocates nothing until it's
  written. Readers check the document's version, and read columns as
  `ArrayView`s, whose `as_span()` views them. The vector's items are nested
  from builders, collected into a `std::vector` of `NestedTable`s first. The
  array of structs is written with `store_le()` into a vector, which the
  builder owns through `set_struct_array()`, and read from
  `get_struct_array()`'s bytes with `load_le()`.
- **tafla-gen** is the code that tafla-gen generates from `workloads.tafla`,
  in `generated/tafla/`. A message's `serialize()` counts the table's length
  with a `TableSizer`, and then writes every field, nested tables included,
  into bytes of that length with a `TableWriter`, in one allocation. Its
  readers view strings and columns, as tafla's do, and read with a
  `ByteBuffer` in the table's layout, a constant. It takes the workloads'
  types, as the other codecs do, so it collects the generated fields of the
  vector's items and the generated structs of the array into a
  `std::vector` in each call, since the generated code writes them from
  spans. The generated getter of the vector reads its items' readers into a
  `std::vector`, which the codec reads into the workload's items, and the
  array of structs is read through its view.
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
  output views and keeps the message for, as tafla's and FlatBuffers'
  outputs view their bytes, and moves its strings. protobuf has no structs,
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

The full run takes about 30 seconds. Arguments select benchmarks by a part of
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
  collects the vector's and the array's items into the generated types,
  which a caller that holds its values in them wouldn't, but views the
  columns.
- A failed read throws, which takes about 4.5 µs to unwind on macOS, so a
  stream of corrupted messages reads at about 200,000 a second a core. The
  benchmarks time valid bytes only, which reading never throws for.

## Results

Two full runs on 2026-11-11, on an Apple M2 Pro with 32 GB in macOS 27.0.1,
compiled by Apple clang 21.0.0 with Homebrew's FlatBuffers 25.12.19, protobuf
36.2 and msgpack-cxx 9.0.0. Each took 32 seconds. The 1-minute load average
was between 4.5 and 4.6 at the runs' starts and ends, on the 10 cores, where
this machine idled at about 4 with a browser and other sessions open. The
median round was within 3% of the fastest for 168 of the 176 codecs' runs of
a benchmark, and at most 7% above it. The two runs' fastest rounds were
within 2% of each other for 81 of the 88, and within 8% for all but
tafla-gen's read of the record, which took 8.5 ns in one run and 6.9 ns in
the other, steady within each. Runs of an earlier build saw the same two
speeds of that read, 9.3 and 6.7 ns, and two of tafla's write of the small
document, 0.168 µs and 0.180 to 0.184 µs, which these runs both took: they
change from one process to the next rather than from round to round.

Fastest round of the two runs in µs per call, and its ratio to tafla's:

| Benchmark                                          |    tafla |        tafla-gen |     flatbuffers |       protobuf |          msgpack |         bit_cast |
| -------------------------------------------------- | -------: | ---------------: | --------------: | -------------: | ---------------: | ---------------: |
| record: serialize                                  |   0.0569 |   0.0227 (0.40×) |  0.0545 (0.96×) | 0.0712 (1.25×) |   0.0549 (0.96×) |                  |
| record: deserialize                                |  0.00629 |  0.00691 (1.10×) | 0.00858 (1.36×) | 0.0528 (8.39×) |    0.157 (25.0×) |                  |
| document of 100 rows: serialize                    |    0.180 |   0.0454 (0.25×) |   0.166 (0.92×) |  0.463 (2.57×) |    0.672 (3.73×) |                  |
| document of 100 rows: deserialize                  |   0.0103 |   0.0111 (1.08×) |  0.0208 (2.02×) |  0.673 (65.3×) |     1.797 (174×) |                  |
| document of 100 rows: sum a column per element     |   0.0355 |   0.0351 (0.99×) |  0.0585 (1.65×) |  0.656 (18.5×) |    1.841 (51.9×) |                  |
| document of 100,000 rows: serialize                |   16.523 |   16.360 (0.99×) |  45.106 (2.73×) |  347.0 (21.0×) |    639.8 (38.7×) |                  |
| document of 100,000 rows: deserialize              |   0.0103 |   0.0110 (1.07×) |  0.0207 (2.01×) | 436.8 (42408×) | 1248.2 (121184×) |                  |
| document of 100,000 rows: sum a column per element |   86.889 |   88.083 (1.01×) |  88.267 (1.02×) |  504.3 (5.80×) |   1330.6 (15.3×) |                  |
| vector of 100 nested tables: serialize             |    7.868 |    0.460 (0.06×) |   2.969 (0.38×) |  5.047 (0.64×) |    2.739 (0.35×) |                  |
| vector of 100 nested tables: deserialize           |    0.259 |    0.404 (1.56×) |   1.029 (3.97×) |  6.042 (23.3×) |    5.697 (22.0×) |                  |
| array of 100 structs: serialize                    |    0.148 |   0.0784 (0.53×) |   0.146 (0.99×) |  2.811 (19.0×) |    0.842 (5.69×) |                  |
| array of 100 structs: deserialize                  |   0.0463 |   0.0386 (0.83×) |  0.0432 (0.93×) |  3.505 (75.7×) |    2.069 (44.7×) |                  |
| fixed-size record: serialize                       | 0.000399 | 0.000401 (1.01×) |   0.0448 (112×) | 0.0113 (28.3×) |    0.0421 (106×) | 0.000400 (1.00×) |
| fixed-size record: deserialize                     | 0.000400 | 0.000399 (1.00×) | 0.00566 (14.2×) | 0.0240 (60.0×) |     0.141 (352×) | 0.000400 (1.00×) |

- tafla reads a document in 10 ns, at 100 rows and at 100,000 alike,
  checking its version and viewing its columns, and tafla-gen in 11 ns.
  FlatBuffers views its columns too, in 21 ns, which include its
  `Verifier`'s check of the message. protobuf and msgpack-cxx copy every
  element, in 0.44 and 1.25 ms for 100,000 rows.
- tafla and tafla-gen write the large document in 16.5 and 16.4 µs, copying
  its columns whole into a table of its exact length. FlatBuffers copies them
  whole too, in 45 µs, into a builder that grows by copying, through 3.5 MB
  in 4 allocations. protobuf and msgpack-cxx write each element, in 347 and
  640 µs.
- Summing 100,000 floats takes 87 to 88 µs through tafla's, tafla-gen's and
  FlatBuffers' accessors alike, bound by the additions, each of which waits
  for the one before. Summing 100 takes tafla and tafla-gen 35 ns and
  FlatBuffers 59 ns, whose extra 23 ns doesn't grow with the rows.
- tafla-gen writes the record in 23 ns and the small document in 45 ns, each
  in one allocation of the table's exact length, against 57 and 180 ns
  through tafla's builder and 55 and 166 ns through FlatBuffers'. msgpack-cxx
  writes the record in 55 ns, into a new `sbuffer` of 8 KiB, and protobuf in
  71 ns.
- tafla reads the record in 6.3 ns, tafla-gen in 6.9 to 8.5 ns and FlatBuffers
  in 8.6 ns, against protobuf's 53 ns and msgpack-cxx's 157 ns, whose zone
  allocates 8.8 KB for it.
- The vector of 100 nested tables is the shape that tafla's builder writes
  slowest, as in the other languages: 7.9 µs, against msgpack-cxx's 2.7 µs,
  FlatBuffers' 3.0 µs and protobuf's 5.0 µs. Its 200 builders allocate
  nothing of their own, but each item's builder is 216 bytes of the codec's
  vector of `NestedTable`s, and `set_vector()` copies the records of the
  items' 500 fields into the parent's, 16 KB. tafla-gen writes it in 0.46 µs,
  the fastest, in one allocation of the table and one of the vector of the
  items' generated fields, which the codec collects from the workload's.
- tafla reads the vector in 0.26 µs and tafla-gen in 0.40 µs, whose getter
  reads the items' readers into a vector of 2,400 bytes first, against
  FlatBuffers' 1.03 µs and protobuf's and msgpack-cxx's 6.0 and 5.7 µs.
- tafla writes the 100 structs in 148 ns, against 7.9 µs for the same items as
  nested tables, and reads them in 46 ns, against 259 ns. tafla-gen writes
  them in 78 ns, from the generated structs that the codec collects straight
  into the table, and FlatBuffers in 146 ns. tafla, tafla-gen and
  FlatBuffers read them in 39 to 46 ns, about as long as filling the vector of
  100 items took alone, 41 ns. protobuf and msgpack-cxx write and parse each
  struct's three fields, in 0.84 to 2.8 µs and 2.1 to 3.5 µs.
- tafla, tafla-gen and `bit_cast` write and read the fixed-size record in
  0.40 ns alike, about one and a half cycles of a performance core at
  3.5 GHz, which is the harness's loop around a few loads or stores.
  FlatBuffers takes 45 ns to write it, allocating its builder's 1 KiB,
  protobuf 11 ns and msgpack-cxx 42 ns, allocating its 8 KiB `sbuffer`.
  Reading it takes FlatBuffers 5.7 ns, with its `Verifier`, protobuf 24 ns
  and msgpack-cxx 141 ns.

Bytes allocated per call, and the allocations and reallocations that ask for
them, which `tafla_allocation` counts:

| Benchmark                                          |         tafla |     tafla-gen |   flatbuffers |      protobuf |         msgpack | bit_cast |
| -------------------------------------------------- | ------------: | ------------: | ------------: | ------------: | --------------: | -------: |
| record: serialize                                  |        60 (1) |        60 (1) |     1,024 (1) |        72 (2) |       8,192 (1) |          |
| record: deserialize                                |         0 (0) |         0 (0) |         0 (0) |        24 (1) |       8,768 (4) |          |
| document of 100 rows: serialize                    |     1,552 (2) |     1,296 (1) |     2,560 (2) |     2,584 (6) |       8,192 (1) |          |
| document of 100 rows: deserialize                  |         0 (0) |         0 (0) |         0 (0) |     2,536 (8) |       9,968 (7) |          |
| document of 100 rows: sum a column per element     |         0 (0) |         0 (0) |         0 (0) |     1,336 (5) |       9,968 (7) |          |
| document of 100,000 rows: serialize                | 1,200,352 (2) | 1,200,096 (1) | 3,500,000 (4) | 2,387,640 (6) |   4,186,112 (9) |          |
| document of 100,000 rows: deserialize              |         0 (0) |         0 (0) |         0 (0) | 2,400,136 (8) | 13,791,704 (10) |          |
| document of 100,000 rows: sum a column per element |         0 (0) |         0 (0) |         0 (0) | 1,200,136 (5) | 13,791,704 (10) |          |
| vector of 100 nested tables: serialize             |    40,840 (3) |     4,408 (2) |     8,720 (5) |   9,912 (202) |       8,192 (1) |          |
| vector of 100 nested tables: deserialize           |     1,200 (1) |     3,600 (2) |     1,200 (1) |  10,416 (207) |      26,368 (7) |          |
| array of 100 structs: serialize                    |     2,448 (4) |     2,408 (2) |     4,200 (3) |   6,512 (102) |       8,192 (1) |          |
| array of 100 structs: deserialize                  |     1,200 (1) |     1,200 (1) |     1,200 (1) |   7,216 (107) |      18,168 (6) |          |
| fixed-size record: serialize                       |         0 (0) |         0 (0) |     1,024 (1) |         0 (0) |       8,192 (1) |    0 (0) |
| fixed-size record: deserialize                     |         0 (0) |         0 (0) |         0 (0) |         0 (0) |       8,768 (4) |    0 (0) |

- tafla-gen allocates each table once, at its exact length, and the codec
  allocates the vectors that it collects items into, and that it reads
  items into. tafla's builder records up to 4 fields within itself, and the
  fields of more, nested tables' included, in a vector: 256 bytes for the
  document's 8. The array of structs allocates the structs' bytes, 40 bytes
  in 2 allocations for the builder to own them, and the table.
- Reading never allocates in tafla, tafla-gen or FlatBuffers, but for the
  vectors that the codecs read items into, 1,200 bytes for 100 items, and
  tafla-gen's getter of the vector, which reads the items' readers into
  2,400 bytes first. protobuf allocates its messages, and msgpack-cxx a zone
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
  tafla-gen      600048 B  threw: Read exceeds the supplied buffer: 400000 bytes at 400092 of 600048
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

### Reading items through views

The four groups that read tafla's items, from the same two runs. Fastest
round in µs, and its ratio to `ByteBuffer`'s by hand:

| Benchmark                                       |   tafla |      tafla-gen |  tafla-gen view | tafla-gen view A/A |
| ----------------------------------------------- | ------: | -------------: | --------------: | -----------------: |
| vector of 100 nested tables: read the last item | 0.00306 |  0.115 (37.6×) | 0.00397 (1.30×) |    0.00399 (1.30×) |
| vector of 100 nested tables: sum every item     |   0.235 |  0.372 (1.58×) |   0.304 (1.29×) |      0.303 (1.29×) |
| array of 100 structs: read the last item        | 0.00111 | 0.0408 (36.8×) | 0.00159 (1.43×) |    0.00158 (1.42×) |
| array of 100 structs: sum every item            |  0.0587 |  0.117 (1.99×) |  0.0589 (1.00×) |     0.0587 (1.00×) |

Only tafla-gen's getters allocate: 2,400 bytes for the vector's 100 readers
and 1,200 for the array's 100 structs. The A/A pair's fastest rounds are
within 0.7% of each other.

- The getters read every item into a `std::vector` first, so reading the last
  item takes 115 and 41 ns, against 4.0 and 1.6 ns through the views, whose
  `get(i)` and `at(i)` read one item.
- The view sums the array of structs as fast as `load_le()` by hand, in
  59 ns. Reading the last struct takes it 1.6 ns against 1.1 ns, since
  `at()` checks its index, which the hand-written read leaves out.
- The view sums the vector's items 1.29 times as slowly as `ByteBuffer`'s
  getters by hand, and reads the last item 1.30 times as slowly. In both,
  Apple Clang calls the function that sums an item out of line rather than
  inlining it. The hand-written one takes the item's bytes, a `std::span` in
  two registers, and reads its offsets from position 0, a constant. The
  generated reader holds the item's bytes and its position, 24 bytes, which
  a call passes in memory, and its getters add that position to every
  offset, checking the sum.
