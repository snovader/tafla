# Benchmarks

`tafla_comparison` compares tafla's C++ runtime with other C++ libraries that
encode small tables and numeric columns. It runs the workloads of the
TypeScript comparison in `typescript/benchmarks/`, with the same values, so
that the languages' numbers compare. Build it from `cpp/` with
`-DTAFLA_BUILD_BENCHMARKS=ON`, as [Running](#running) shows. `tafla_strings`
compares how the same libraries write and read strings, as
[Strings](#strings) describes, and `tafla_lists` times how tafla writes and
reads lists of strings, as [Lists of strings](#lists-of-strings) describes.

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

- **tafla** reads with `TableReader`'s getters by hand: each item in place,
  from `Vector::get_in_place()` of the last item or `items_in_place()`, and
  `get_struct_array()`'s bytes with `load_le()`.
- **tafla-gen** reads with the generated getters, `items()`, which return
  views: a vector's `get(i)` reads one item, an array's `operator[]` reads
  one struct without checking its index, as the read by hand does, and their
  iterators read every item. The generated readers are passed by value, as
  `TableReader`'s spans are by hand.
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
  fixed-size record with `store_le()` and reads it with `TableReader`'s
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
  tafla's do, and read with a `TableReader` in the table's layout, a constant.
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
cmake --build build --target tafla_comparison tafla_strings tafla_encoding tafla_allocation benchmarks_test
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
  converts the vector's and the array's items from the workload's as it
  writes them, and views the columns.
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
round in µs, and its ratio to `TableReader`'s by hand:

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
- The view sums the vector's items 1.29 times as slowly as `TableReader`'s
  getters by hand, and reads the last item 1.30 times as slowly. In both,
  Apple Clang calls the function that sums an item out of line rather than
  inlining it. The hand-written one takes the item's bytes, a `std::span` in
  two registers, and reads its offsets from position 0, a constant. The
  generated reader holds the item's bytes and its position, 24 bytes, which
  a call passes in memory, and its getters add that position to every
  offset, checking the sum.

## Strings

`tafla_strings` compares how tafla and the comparison's libraries write and
read strings, as the TypeScript suite's `yarn bench:strings` does. The
comparison's workloads hold one string, of 16 bytes of ASCII, which reaches
one branch of tafla's UTF-8 check. Each table here holds 16 distinct strings
of one shape and nothing else, so that a call spends most of its time on
their UTF-8. By the length of each string in bytes, they are:

- 16 empty strings, which time what a call takes without any: the table, its
  offsets and lengths, and the library's own work
- ASCII of 3, 7, 8, 16, 17, 32 and 33 bytes, which reach every branch of
  tafla's `valid_utf8()`: strings shorter than 4 bytes, of 4 to 7, of 8 to
  16 and of 17 to 32, and longer ones in blocks of 32
- ASCII of 22 and 23 bytes, either side of the longest string that libc++'s
  `std::string` holds within itself, 22 bytes, which protobuf's strings are
- 8 characters with one of 2 bytes, 16 with two of 2 bytes, 15 with one of 4
  bytes, and 8, 32 and 256 characters of 3 bytes
- 64 bytes of characters of 2, 3 and 4 bytes, and 1,024 bytes of them: 32
  and 512 characters of 2 bytes, 21 and 341 of 3 bytes, which take 63 and
  1,023 bytes, and 16 and 256 of 4 bytes
- 256 bytes of ASCII with a character of 2 bytes first, and with one last
- ASCII of 64, 256 and 1,024 bytes

The characters of 2 bytes are Latin-1 letters, those of 3 bytes CJK
ideographs, and those of 4 bytes emoji. `src/string_workloads.hpp` ports the
TypeScript suite's generator, with its seeds, its alphabets and its lengths
in characters, so that the shapes that both suites have hold the same bytes,
which `tests/benchmarks_test.cpp` checks against a hash of the bytes that the
TypeScript generator made of each shape. The TypeScript suite's 31 and 33
characters with the last of 2 bytes sit on V8's limits, and are left out.
The shapes of 64 and 1,024 bytes, and those with one character of 2 bytes
first or last, part a check of UTF-8 that reads character by character, as
tafla's does from the first byte that isn't ASCII, from one that reads
blocks of 16 bytes with SIMD, as protobuf's does.

The codecs are the comparison's, each with an overload for the table of
strings:

- tafla has no vector of strings, so **tafla** writes a table of 16 string
  fields with a `TableBuilder` and reads it with `get_string()`, and
  **tafla-gen**'s code of `strings.tafla`, in `generated/strings/`, writes
  the same table from its generated fields, which view the strings, and
  reads each with its getter. `strings.tafla` is a copy of the TypeScript
  suite's schema. Both write the same bytes, which the suite checks.
- **FlatBuffers** writes a vector of strings, of `schemas/strings.fbs`, with
  `CreateString()`, and reads it after its `Verifier` verifies the message.
- **protobuf** writes a repeated string, of `schemas/strings.proto`, with
  `add_items()` and `SerializeAsString()`, and reads it with
  `ParseFromArray()`.
- **msgpack-cxx** packs an array of strings into an `sbuffer`, and unpacks
  it, referencing the strings in the bytes.

Each read returns the 16 strings in a `std::array` of the comparison's
`Text`s. tafla, FlatBuffers and msgpack-cxx return views of the bytes, and
protobuf owned `std::string`s, the copies that it made as it parsed the
message, which the codec moves out of the message. The harness keeps every
result alive and destroys it in its loop, alike for every codec, and nothing
reads the strings' characters after a codec returns them. Before timing,
every codec must read back every string, and each benchmark times tafla's
codec again as an A/A pair.

```sh
cmake --build build --target tafla_strings tafla_allocation
uptime
build/benchmarks/tafla_strings --markdown
build/benchmarks/tafla_allocation --strings
```

`tafla_strings` takes the comparison's options, and prints each codec's
encoded sizes after the timings. `tafla_allocation --strings` counts the
allocations of the strings suite's calls rather than the comparison's.

### Limits of the comparison

- The libraries don't do the same work to read a string. tafla checks that
  each string is UTF-8 as `get_string()` reads it, and checks nothing when it
  writes one. protobuf checks every string of a message as it parses the
  message, whether it's read or not, with utf8_range, which tests 8 bytes at a
  time for ASCII and then checks ranges of 16 bytes with NEON's table lookups,
  and again as it writes one, though it writes a string that fails that check
  anyway. FlatBuffers' `Verifier` checks only that each string lies within the
  bytes and ends in its terminator, and msgpack-cxx checks nothing. So
  FlatBuffers' and msgpack-cxx's reads show what checking costs tafla rather
  than a reader that does the same work, and protobuf's include its parse, its
  check and its copies.
- protobuf copies each string, as it parses the message, into a `std::string`
  that it allocates, and from 23 bytes into a buffer of the string's too.
  The codec moves them out of the message rather than copying them again. An
  arena, which the comparison doesn't use, would allocate them in blocks.
- msgpack-cxx's zone allocates 8 KiB or more for each message it unpacks,
  and its `sbuffer` starts at 8 KiB, as in the comparison, and FlatBuffers'
  builder at 1 KiB, doubling by copying.
- tafla's builder records up to 4 fields within itself, and the records of
  more, here all 16, in a vector that it allocates, and then the table.
  tafla-gen's code counts the table's length first and allocates it alone.

### Results

Two full runs on 2026-11-25, on an Apple M2 Pro with 32 GB in macOS 27.0.1,
compiled by Apple clang 21.0.0 with `-O3 -DNDEBUG`, CMake's Release flags,
without `-march=native`, with Homebrew's FlatBuffers 25.12.19, protobuf 36.2
and msgpack-cxx 9.0.0. Each took 118 seconds. The 1-minute load average was
between 2.27 and 2.72 at the runs' starts and ends, on the 10 cores. The
median round was within 3% of the fastest for 575 of the 648 codecs' runs of
a benchmark, and more than 5% above it only for FlatBuffers' and
msgpack-cxx's writes of the longest strings, whose buffers grow by copying,
up to 45% for msgpack-cxx's, and for one read of tafla-gen's, by 6%. The two
runs' fastest rounds were within 2% of each other for 283 of the 324, and
within 5% for all but three of msgpack-cxx's, by up to 7.1%. tafla's A/A
pair was within 1.6% of tafla's in every benchmark.

Fastest round of the two runs in µs per call, and its ratio to tafla's, with
the length of each string in bytes:

| Serialize 16 strings of     | Bytes | tafla | tafla-gen      | flatbuffers   | protobuf      | msgpack        |
| --------------------------- | ----: | ----: | -------------: | ------------: | ------------: | -------------: |
| empty                       |     0 | 0.151 | 0.0403 (0.27×) | 0.159 (1.05×) | 0.519 (3.44×) | 0.0701 (0.46×) |
| 3 chars of ASCII            |     3 | 0.194 | 0.0815 (0.42×) | 0.179 (0.92×) | 0.591 (3.05×) |  0.120 (0.62×) |
| 7 chars of ASCII            |     7 | 0.213 |  0.100 (0.47×) | 0.199 (0.93×) | 0.647 (3.04×) |  0.139 (0.65×) |
| 8 chars of ASCII            |     8 | 0.179 | 0.0645 (0.36×) | 0.181 (1.01×) | 0.551 (3.08×) |  0.102 (0.57×) |
| 8 chars, one of 2 bytes     |     9 | 0.177 | 0.0655 (0.37×) | 0.164 (0.93×) | 0.606 (3.42×) | 0.0989 (0.56×) |
| 16 chars of ASCII           |    16 | 0.192 | 0.0781 (0.41×) | 0.196 (1.02×) | 0.572 (2.98×) |  0.115 (0.60×) |
| 17 chars of ASCII           |    17 | 0.196 | 0.0831 (0.42×) | 0.186 (0.95×) | 0.581 (2.96×) |  0.121 (0.62×) |
| 16 chars, two of 2 bytes    |    18 | 0.194 | 0.0788 (0.41×) | 0.178 (0.92×) | 0.611 (3.15×) |  0.116 (0.60×) |
| 15 chars, one of 4 bytes    |    18 | 0.192 | 0.0782 (0.41×) | 0.178 (0.93×) | 0.676 (3.52×) |  0.116 (0.60×) |
| 22 chars of ASCII           |    22 | 0.229 |  0.115 (0.50×) | 0.214 (0.93×) | 0.677 (2.96×) |  0.152 (0.66×) |
| 23 chars of ASCII           |    23 | 0.233 |  0.121 (0.52×) | 0.217 (0.93×) | 0.961 (4.12×) |  0.157 (0.67×) |
| 8 chars of 3 bytes          |    24 | 0.196 | 0.0832 (0.42×) | 0.199 (1.02×) | 0.943 (4.81×) |  0.120 (0.61×) |
| 32 chars of ASCII           |    32 | 0.203 | 0.0909 (0.45×) | 0.204 (1.00×) | 0.880 (4.33×) |  0.130 (0.64×) |
| 33 chars of ASCII           |    33 | 0.205 | 0.0959 (0.47×) | 0.194 (0.95×) | 0.889 (4.34×) |  0.134 (0.65×) |
| 21 chars of 3 bytes         |    63 | 0.253 |  0.140 (0.55×) | 0.280 (1.11×) | 1.179 (4.66×) |  0.184 (0.73×) |
| 64 chars of ASCII           |    64 | 0.173 | 0.0643 (0.37×) | 0.221 (1.28×) | 0.871 (5.03×) |  0.106 (0.61×) |
| 32 chars of 2 bytes         |    64 | 0.172 | 0.0643 (0.37×) | 0.221 (1.28×) | 0.945 (5.49×) |  0.105 (0.61×) |
| 16 chars of 4 bytes         |    64 | 0.173 | 0.0645 (0.37×) | 0.222 (1.28×) | 0.936 (5.41×) |  0.106 (0.61×) |
| 32 chars of 3 bytes         |    96 | 0.170 | 0.0682 (0.40×) | 0.271 (1.59×) | 1.033 (6.08×) |  0.111 (0.65×) |
| 256 chars of ASCII          |   256 | 0.227 |  0.105 (0.46×) | 0.460 (2.03×) | 1.112 (4.90×) |  0.152 (0.67×) |
| 255 chars, first of 2 bytes |   256 | 0.226 |  0.104 (0.46×) | 0.459 (2.03×) | 1.339 (5.92×) |  0.151 (0.67×) |
| 255 chars, last of 2 bytes  |   256 | 0.224 |  0.106 (0.47×) | 0.462 (2.06×) | 1.164 (5.20×) |  0.152 (0.68×) |
| 256 chars of 3 bytes        |   768 | 0.303 |  0.202 (0.67×) | 0.808 (2.67×) | 2.446 (8.07×) |  0.404 (1.33×) |
| 1,024 chars of ASCII        | 1,024 | 0.344 |  0.241 (0.70×) | 1.110 (3.23×) | 2.149 (6.25×) |  0.755 (2.19×) |
| 341 chars of 3 bytes        | 1,023 | 0.343 |  0.243 (0.71×) | 0.986 (2.87×) | 3.037 (8.85×) |  0.727 (2.12×) |
| 512 chars of 2 bytes        | 1,024 | 0.344 |  0.241 (0.70×) | 1.102 (3.20×) | 2.882 (8.38×) |  0.785 (2.28×) |
| 256 chars of 4 bytes        | 1,024 | 0.343 |  0.241 (0.70×) | 1.081 (3.15×) | 2.860 (8.34×) |  0.773 (2.25×) |

| Deserialize 16 strings of   | Bytes | tafla  | tafla-gen      | flatbuffers      | protobuf      | msgpack        |
| --------------------------- | ----: | -----: | -------------: | ---------------: | ------------: | -------------: |
| empty                       |     0 | 0.0287 | 0.0208 (0.72×) |   0.0395 (1.38×) | 0.591 (20.6×) |  0.180 (6.27×) |
| 3 chars of ASCII            |     3 | 0.0308 | 0.0244 (0.79×) |   0.0396 (1.29×) | 0.655 (21.3×) |  0.210 (6.82×) |
| 7 chars of ASCII            |     7 | 0.0286 | 0.0224 (0.78×) |   0.0394 (1.38×) | 0.687 (24.0×) |  0.211 (7.38×) |
| 8 chars of ASCII            |     8 | 0.0286 | 0.0224 (0.78×) |   0.0393 (1.37×) | 0.622 (21.7×) |  0.210 (7.34×) |
| 8 chars, one of 2 bytes     |     9 | 0.0851 | 0.0714 (0.84×) |   0.0394 (0.46×) | 0.699 (8.21×) |  0.210 (2.47×) |
| 16 chars of ASCII           |    16 | 0.0287 | 0.0226 (0.79×) |   0.0397 (1.38×) | 0.628 (21.9×) |  0.213 (7.42×) |
| 17 chars of ASCII           |    17 | 0.0305 | 0.0260 (0.85×) |   0.0395 (1.30×) | 0.627 (20.6×) |  0.213 (6.98×) |
| 16 chars, two of 2 bytes    |    18 |  0.186 |  0.182 (0.98×) |   0.0394 (0.21×) | 0.663 (3.56×) |  0.213 (1.15×) |
| 15 chars, one of 4 bytes    |    18 |  0.137 |  0.112 (0.82×) |   0.0395 (0.29×) | 0.756 (5.52×) |  0.210 (1.53×) |
| 22 chars of ASCII           |    22 | 0.0307 | 0.0258 (0.84×) |   0.0394 (1.28×) | 0.701 (22.8×) |  0.211 (6.87×) |
| 23 chars of ASCII           |    23 | 0.0305 | 0.0259 (0.85×) |   0.0394 (1.29×) | 1.026 (33.6×) |  0.210 (6.89×) |
| 8 chars of 3 bytes          |    24 |  0.243 |  0.227 (0.93×) |   0.0393 (0.16×) | 1.048 (4.31×) |  0.211 (0.87×) |
| 32 chars of ASCII           |    32 | 0.0307 | 0.0259 (0.84×) |   0.0394 (1.28×) | 0.974 (31.7×) |  0.269 (8.76×) |
| 33 chars of ASCII           |    33 | 0.0390 | 0.0349 (0.89×) |   0.0393 (1.01×) | 0.980 (25.1×) |  0.268 (6.87×) |
| 21 chars of 3 bytes         |    63 |  0.573 |  0.552 (0.96×) |  0.0395 (0.069×) | 1.249 (2.18×) |  0.268 (0.47×) |
| 64 chars of ASCII           |    64 | 0.0389 | 0.0348 (0.89×) |   0.0395 (1.02×) | 0.994 (25.6×) |  0.269 (6.92×) |
| 32 chars of 2 bytes         |    64 |  0.483 |  0.479 (0.99×) |  0.0392 (0.081×) | 1.068 (2.21×) |  0.267 (0.55×) |
| 16 chars of 4 bytes         |    64 |  0.509 |  0.502 (0.99×) |  0.0393 (0.077×) | 1.049 (2.06×) |  0.267 (0.52×) |
| 32 chars of 3 bytes         |    96 |  0.851 |  0.829 (0.97×) |  0.0395 (0.046×) | 1.137 (1.34×) |  0.267 (0.31×) |
| 256 chars of ASCII          |   256 | 0.0933 | 0.0827 (0.89×) |   0.0394 (0.42×) | 1.247 (13.4×) |  0.267 (2.86×) |
| 255 chars, first of 2 bytes |   256 |  0.252 |  0.258 (1.02×) |   0.0395 (0.16×) | 1.397 (5.54×) |  0.267 (1.06×) |
| 255 chars, last of 2 bytes  |   256 |  0.117 |  0.111 (0.95×) |   0.0393 (0.34×) | 1.272 (10.9×) |  0.267 (2.28×) |
| 256 chars of 3 bytes        |   768 |  6.590 |  6.537 (0.99×) |  0.0396 (0.006×) | 2.439 (0.37×) | 0.267 (0.041×) |
| 1,024 chars of ASCII        | 1,024 |  0.285 |  0.275 (0.96×) |   0.0399 (0.14×) | 2.177 (7.64×) |  0.267 (0.94×) |
| 341 chars of 3 bytes        | 1,023 |  8.770 |  8.661 (0.99×) | 0.0392 (0.0045×) | 3.011 (0.34×) | 0.268 (0.031×) |
| 512 chars of 2 bytes        | 1,024 |  7.217 |  7.158 (0.99×) | 0.0398 (0.0055×) | 2.864 (0.40×) | 0.268 (0.037×) |
| 256 chars of 4 bytes        | 1,024 |  7.595 |  7.709 (1.02×) | 0.0399 (0.0053×) | 2.857 (0.38×) | 0.267 (0.035×) |

Reads:

- tafla's read is slower than protobuf's only where the strings are long and
  not ASCII: 2.70 times as long for 768 bytes of characters of 3 bytes,
  6.59 µs against 2.44 µs, and 2.52 to 2.91 times for 1,024 bytes of
  characters of 2, 3 and 4 bytes, 7.2 to 8.8 µs against 2.9 to 3.0 µs. Below
  them protobuf's parse outweighs its check: it takes 0.59 µs for the 16
  empty strings, and tafla reads strings of 18 to 96 bytes that aren't ASCII
  in 0.18 to 0.75 of protobuf's time.
- What text that isn't ASCII adds to a read, over ASCII of the same length,
  is mostly what it costs to check, and it adds far more to tafla's than to
  protobuf's. At 1,024 bytes, it adds 6.9 to 8.5 µs to tafla's read and 0.68
  to 0.83 µs to protobuf's, 10 to 11 times as much: tafla's
  `valid_utf8_from()` checks 0.85 ns a character of 2 bytes, 1.56 ns of 3
  bytes and 1.78 ns of 4 bytes, about 2 GB a second, where it checks ASCII
  at 64 GB a second. At 64 bytes it adds 0.44 to 0.53 µs to tafla's and 0.06
  to 0.26 µs to protobuf's, 2.1 to 8.5 times as much, and at 24 bytes of
  characters of 3 bytes, 0.21 µs to tafla's and 0.02 µs to protobuf's, within
  protobuf's noise. 16 characters with two of 2 bytes, 18 bytes, add 0.16 µs
  to tafla's read over 17 bytes of ASCII and 0.04 µs to protobuf's, since
  `valid_utf8_from()` steps through ASCII byte by byte wherever the next 8
  bytes hold a character that isn't ASCII. One character of 4 bytes among
  15, and one of 2 bytes among 8, add about as much to both, 0.06 to
  0.13 µs.
- A character of 2 bytes first in 256 bytes of ASCII adds 0.16 µs to tafla's
  read, which then checks the rest 8 bytes at a time rather than 32, and
  0.15 µs to protobuf's. One last adds 0.02 µs to each.
- tafla reads 16 strings of ASCII in 29 to 39 ns at every length to 64 bytes,
  and 1,024 bytes in 0.29 µs. tafla-gen's getters read ASCII of up to 256
  bytes in 0.72 to 0.89 of tafla's time, unrolled 16 times where the codec by
  hand loops; at 768 bytes and more of characters that aren't ASCII the two
  take the same time. FlatBuffers takes 39 to 40 ns whatever the strings, and
  msgpack-cxx 0.18 to 0.27 µs, neither checking them. Past 22 bytes, protobuf
  allocates each string's buffer, and its read of 23 bytes of ASCII takes
  1.03 µs against 0.70 µs for 22.

Writes:

- tafla-gen writes the table in 0.27 to 0.71 of the time of tafla's
  builder, in one allocation of the table's exact length. The builder
  records only 4 fields within itself, so it also allocates a vector of 512
  bytes for the records of the 16.
- msgpack-cxx writes strings of up to 256 bytes in 0.46 to 0.73 of tafla's
  time, into the 8 KiB `sbuffer` that it allocates for each message, and
  longer ones in 1.33 to 2.28 of it, reallocating it. FlatBuffers writes in
  0.92 to 1.05 of tafla's time up to 33 bytes, and in 1.11 to 3.23 of it
  from 63 bytes, as its builder doubles from 1 KiB by copying: 67 KB in 8
  allocations for strings of 1,024 bytes. protobuf takes 3.0 to 8.9 times as
  long, copying each string into its message, in 18 allocations, or 34 past
  22 bytes, and checking it again as it writes it.
- Writes don't grow with the strings' length below 256 bytes: tafla,
  tafla-gen and msgpack-cxx write strings of 64 bytes as fast as those of 8,
  and those of 7, 22 and 23 bytes more slowly than either. This wasn't
  investigated.

`tafla_allocation --strings` counts no allocation in a read of tafla,
tafla-gen or FlatBuffers. protobuf's allocates 20 times, or 36 times past 22
bytes, and msgpack-cxx's zone 8,768 bytes in 4. A write allocates twice in
tafla, once in tafla-gen, once to 8 times in FlatBuffers, 18 or 34 times in
protobuf, and once to 3 times in msgpack-cxx. tafla writes 128 bytes for the
16 empty strings, an offset and a length of 4 bytes each for every string,
which its characters follow; FlatBuffers writes 216, protobuf 32 and
msgpack-cxx 19. For strings of 1,024 bytes the four write 16,512, 16,600,
16,432 and 16,435 bytes.

## Lists of strings

`tafla_lists` times how the code that tafla-gen generates from
`lists.tafla`, a copy of the TypeScript suite's schema, into
`generated/lists/` writes and reads lists of 10, 100 and 10,000 distinct
strings of 16 and 256 characters of ASCII and of 8 characters of 3 bytes in
UTF-8. `src/lists.hpp` makes them as the TypeScript suite of lists does, from
its xorshift with the same seeds, which `tests/benchmarks_test.cpp` checks
against values that the TypeScript suite printed. Each list is written two
ways:

- **tafla-gen [string]**, `StringList`, a vector of strings, whose items are
  the strings' UTF-8, written from a span of `std::string_view`s and read
  through its getter's `tafla::StringVector`: every string at once with
  `to_vector()`, which checks the strings' UTF-8 as one run, and the last
  string from its ends alone with `get()`. The suite checks that it writes the
  bytes that `TableBuilder::set_string_vector()` writes.
- **tafla-gen [Name]**, `NameList`, a vector of tables of one string each,
  which is how a schema holds a list of strings without vectors of strings.
  Its generic fields write a `std::views::transform()` of the strings into
  each table's fields as they're written, without allocating them, and each
  string is read through a reader of its table: every string by iterating
  the items, which reads each item's end once, and the last with `get()`.

**tafla-gen [string] A/A** times the vector of strings again, an A/A pair
whose ratio shows the harness's noise. Each list is serialized, deserialized
into a `std::vector<std::string_view>` of its strings, viewed in the bytes,
and read for its last string. Before timing, every variant must read back
every string, and the last one. The method is the comparison's, and the
program takes its arguments. After the timings, it prints each list's
encoded size: a vector of strings takes 4 bytes for each string beside the
strings' UTF-8, and 8 for the table's offset and the vector's count, and a
vector of tables of one string each takes 12 for each string.

```sh
cmake --build build --target tafla_lists
uptime
build/benchmarks/tafla_lists --markdown
```

The full run takes about 30 seconds, and `--quick` under a second.

### Results

One full run on 2026-11-27, on an Apple M2 Pro with 32 GB in macOS 27.0.1,
compiled by Apple clang 21.0.0, through the machine's benchmark lock. The
1-minute load average was 2.69 at the start and 3.04 at the end, on the 10
cores. The median round was within 3% of the fastest for 78 of the 81
variants' runs; the writes of 10,000 strings of 256 characters were 15–19%
above it in every variant, of 2.6 MB tables that each call allocates. The
A/A pair agreed within 2%.

Fastest round in µs per call, and its ratio to the vector of strings':

| Benchmark | tafla-gen [string] | tafla-gen [Name] | tafla-gen [string] A/A |
| --- | ---: | ---: | ---: |
| 10 × 16 chars of ASCII: serialize | 0.0573 | 0.0733 (1.28×) | 0.0565 (0.99×) |
| 10 × 16 chars of ASCII: deserialize | 0.0336 | 0.0354 (1.05×) | 0.0336 (1.00×) |
| 10 × 16 chars of ASCII: read the last string | 0.00209 | 0.00247 (1.18×) | 0.00208 (1.00×) |
| 10 × 256 chars of ASCII: serialize | 0.0690 | 0.0826 (1.20×) | 0.0693 (1.00×) |
| 10 × 256 chars of ASCII: deserialize | 0.0718 | 0.0751 (1.05×) | 0.0720 (1.00×) |
| 10 × 256 chars of ASCII: read the last string | 0.00576 | 0.00638 (1.11×) | 0.00574 (1.00×) |
| 10 × 8 chars of 3 bytes: serialize | 0.0608 | 0.0739 (1.21×) | 0.0611 (1.00×) |
| 10 × 8 chars of 3 bytes: deserialize | 0.168 | 0.177 (1.05×) | 0.168 (1.00×) |
| 10 × 8 chars of 3 bytes: read the last string | 0.0152 | 0.0157 (1.03×) | 0.0152 (1.00×) |
| 100 × 16 chars of ASCII: serialize | 0.400 | 0.526 (1.31×) | 0.402 (1.00×) |
| 100 × 16 chars of ASCII: deserialize | 0.125 | 0.196 (1.57×) | 0.125 (1.00×) |
| 100 × 16 chars of ASCII: read the last string | 0.00208 | 0.00247 (1.19×) | 0.00208 (1.00×) |
| 100 × 256 chars of ASCII: serialize | 0.556 | 0.674 (1.21×) | 0.556 (1.00×) |
| 100 × 256 chars of ASCII: deserialize | 0.523 | 0.591 (1.13×) | 0.522 (1.00×) |
| 100 × 256 chars of ASCII: read the last string | 0.00575 | 0.00638 (1.11×) | 0.00573 (1.00×) |
| 100 × 8 chars of 3 bytes: serialize | 0.426 | 0.553 (1.30×) | 0.432 (1.01×) |
| 100 × 8 chars of 3 bytes: deserialize | 1.374 | 1.546 (1.12×) | 1.374 (1.00×) |
| 100 × 8 chars of 3 bytes: read the last string | 0.0152 | 0.0158 (1.04×) | 0.0152 (1.00×) |
| 10,000 × 16 chars of ASCII: serialize | 38.879 | 51.133 (1.32×) | 38.934 (1.00×) |
| 10,000 × 16 chars of ASCII: deserialize | 10.727 | 16.864 (1.57×) | 10.805 (1.01×) |
| 10,000 × 16 chars of ASCII: read the last string | 0.00209 | 0.00247 (1.18×) | 0.00209 (1.00×) |
| 10,000 × 256 chars of ASCII: serialize | 82.013 | 92.187 (1.12×) | 83.579 (1.02×) |
| 10,000 × 256 chars of ASCII: deserialize | 50.991 | 56.115 (1.10×) | 51.773 (1.02×) |
| 10,000 × 256 chars of ASCII: read the last string | 0.00574 | 0.00642 (1.12×) | 0.00576 (1.00×) |
| 10,000 × 8 chars of 3 bytes: serialize | 41.605 | 54.002 (1.30×) | 41.866 (1.01×) |
| 10,000 × 8 chars of 3 bytes: deserialize | 136.3 | 153.3 (1.12×) | 137.0 (1.01×) |
| 10,000 × 8 chars of 3 bytes: read the last string | 0.0151 | 0.0157 (1.04×) | 0.0151 (1.00×) |

- The vector of strings is written in 0.76–0.89 times the time of the
  vector of tables of one string each.
- Every string is read in 0.64–0.95 times the time: 0.64 for 100 and 10,000
  strings of 16 characters of ASCII, whose run `to_vector()` checks at once,
  0.88–0.95 for strings of 3 bytes and of 256 characters, and 0.95 for any
  list of 10.
- The last string is found in 0.84–0.97 times the time, without a reader of
  a table around it.

A vector of strings takes 4 bytes for each string beside its UTF-8, and a
vector of tables 12: 208 bytes against 288 for 10 strings of 16
characters, and 200,008 against 280,008 for 10,000.
