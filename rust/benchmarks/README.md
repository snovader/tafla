# Benchmarks

`cargo bench -p tafla-benchmarks` compares tafla's Rust runtime, `tafla`,
with other Rust crates that encode small tables and numeric columns, in
Criterion benchmarks. It runs the workloads of the TypeScript comparison in
`typescript/benchmarks/`, with the same values, so that the languages' numbers
compare, and a comparison of how the crates write and read strings, which
[Strings](#strings) describes. Its suite of
[lists of strings](#lists-of-strings) times a vector of strings against a
vector of tables of one string each. Run the commands below from `rust/`.

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

Each shape is serialized and deserialized into the types of `workloads.rs`,
whose strings and columns are `Cow`s: slices of the bytes where a crate reads
them in place as Rust's `str` and `[T]`, and copies otherwise. Column `a` of
each document is also summed through each crate's per-element accessor.
`workloads.rs` ports the TypeScript generator, so the inputs are the same
values, which `tests/workloads.rs` checks against values that the TypeScript
generator printed.

Four more groups time only tafla's bytes of the vector of nested tables and
of the array of structs, read in two ways: **read the last item**, which a
reader can find without reading the others, and **sum every item**. Each way
sums the three float32s of the items it reads as `f64`s, rather than
collecting items of its own, and `item_access.rs` holds them:

- **tafla** reads with `TableReader`'s getters by hand: each item in place,
  from `Vector::get_in_place()` of the last item or `Vector::iter_in_place()`,
  and `get_struct_array()`'s bytes with `as_chunks()`.
- **tafla-gen** reads with the views that the generated getters, `items()`,
  return, whose `get(i)` reads one item and whose `iter()` reads every item.
- **tafla-gen A/A** reads with the views again, an A/A pair that shows
  Criterion's noise.

One more group, **vector of 100 sized tables: read the last item**, reads
the same items written as sized tables, whose headers hold their lengths.
`Vector::get()` returns such an item as a table of its own, from its start
to its end, rather than in place, and tafla reads it with `TableReader`'s
getters by hand; `sized_items.rs` holds it. The workloads' schema has no such
vector, and tafla-gen's views read only items in place, so tafla-gen and the
other languages' suites don't time it.

`cargo run -p tafla-benchmarks --bin encoding` prints each crate's encoded
sizes and what it does with a truncated document, as the TypeScript comparison
does, and `cargo run --release -p tafla-benchmarks --bin allocation` prints
the allocations and bytes of each benchmark's call.

### What differs from TypeScript

- There are no `number[]` inputs. Rust's columns are typed already, and a
  `Vec<f64>` that parsed JSON would hold isn't what a codec is given.
- Only tafla's columns are read as slices of the bytes. tafla's `ArrayView`
  returns one with `to_cow()` when the array is aligned on a little-endian
  host, as the TypeScript runtime's typed arrays view the bytes. The
  flatbuffers crate's `Vector` has no slice of its elements, unlike
  `aArray()` in TypeScript, and rkyv reads them in place as little-endian
  wrappers, `f32_le`, rather than `f32`s, so both copy their columns into
  `Vec`s. Summing a column shows them reading in place.
- tafla's Rust runtime has no `StructWriter` or `readUint32()`. The tafla
  codec writes the fixed-size record with `to_le_bytes()` and reads it with
  `TableReader`'s getters, and tafla's README recommends the standard
  library's `to_le_bytes()` and `from_le_bytes()` for such records, which are
  compared too. tafla-gen's struct writes and reads them in the same way as
  the standard library's. So tafla, tafla-gen and `to_le_bytes` write the
  record with the same code, and tafla-gen and `to_le_bytes` read it with the
  same code: A/A pairs that show the harness's noise.
- MessagePack's columns are arrays of float32s and uint32s, packed and
  unpacked element by element, as serde serializes a `[f32]`. msgpackr's
  typed-array extensions are msgpackr's own.
- rkyv, which has no counterpart in the other languages, is the closest Rust
  counterpart to tafla's reads: an archive that's read in place after it's
  validated. It archives a `Vec` of a struct inline, so its vector of nested
  tables and its array of structs are the same archive.
- flatbuffers' latest Rust release is 25.12.19, where the TypeScript
  comparison uses flatbuffers 25.9.23.
- `yarn bench`'s benchmarks of each getter and setter aren't ported.
- Criterion doesn't interleave the codecs; see [Method](#method).

## Crates

| Crate                                  | Why it's comparable                                                                                                                     |
| -------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------- |
| `tafla` (`core/`)                      | The reference that the others are compared with.                                                                                        |
| tafla-gen's code                       | tafla's tables, written and read by the code that tafla-gen generates.                                                                  |
| `flatbuffers` 25.12.19                 | Zero-copy tables with offsets and vectors, like tafla.                                                                                  |
| `prost` 0.14.4                         | The most used protobuf crate, with packed repeated floats.                                                                              |
| `rmp-serde` 1.3.1, `serde` 1.0.229     | MessagePack through serde: a schemaless format, whose tables are maps with their keys.                                                  |
| `rkyv` 0.8.18                          | Zero-copy archives of Rust types, read in place after bytecheck validates them, as tafla's reads check every range.                     |
| `to_le_bytes` (the standard library's) | For the fixed-size record only: fields at fixed offsets, which tafla's README recommends writing and reading with the standard library. |

Each crate is used as its documentation shows, and as the TypeScript
comparison uses its counterpart:

- **tafla** writes nested tables in place among their parent builder's
  fields, with `set_table_with()` and `set_vector_with()`. The builders
  borrow the columns and copy each array in bulk into a table of its exact
  size. Readers check the document's version, and read columns as
  `ArrayView`s, whose `to_cow()` borrows them. The array of structs is written with `to_le_bytes()` into a `Vec`, which the
  builder takes with `set_struct_array()`, and read from
  `get_struct_array()`'s bytes with `as_chunks()` and `from_le_bytes()`.
  tafla-gen's code converts each struct in the same way, but writes them
  straight into the table.
- **tafla-gen** is the code that tafla-gen generates from `workloads.tafla`,
  in `src/codecs/tafla_gen/`. A message's `serialize()` counts the table's
  length with a `TableSizer`, and then writes every field, nested tables
  included, into a `Vec` of that capacity with a `TableWriter`, in one
  allocation. Its readers borrow strings and columns, as tafla's do. They
  hold their bytes, and each getter reads them with a `TableReader` in the
  table's layout, a constant, as the hand-written reads make theirs, so that
  the getters compile for that layout wherever a reader is passed. It takes
  the workloads' types, as the other codecs do, so it collects the generated
  fields of the vector's items and the generated structs of the array into a
  `Vec` in each call, as the prost codec collects its messages, since the
  generated code writes them from slices. The generated getters of the
  vector and of the array return views, whose `iter()` the codec reads into
  the workload's items. Every suite has a copy of the schema; after changing
  them, regenerate the code as `gen/README.md` describes.
- **FlatBuffers** builds each message with a new `FlatBufferBuilder`, which
  starts empty and doubles as it grows, and each table with flatc's
  `create()` and its arguments. Columns are written from slices with
  `create_vector()`, which writes them element by element, and so is the
  vector of structs, from a `Vec` of flatc's structs, which took 16% less time
  than `create_vector_from_iter()` from the items. A message is the
  builder's buffer from where the builder finished it, taken with
  `collapse()` without a copy, as the TypeScript comparison takes
  `asUint8Array()`'s view. Readers verify the message with
  `flatbuffers::root()`, as flatc's `root_as_*()` functions do, and copy
  columns through their `Vector`'s iterator. The generated code in
  `src/codecs/flatbuffers/generated.rs` is written by hand in the shape that
  `flatc --rust` generates, with only what the benchmarks use, since the
  FlatBuffers project doesn't publish flatc on crates.io.
- **prost** builds messages from the input and writes them with
  `encode_to_vec()`, which sizes the bytes first, and reads them with
  `decode()`. Its messages own their strings and repeated fields, so building
  one from borrowed input copies its columns, as a caller that keeps its
  values would, and the decoded columns move into the output. protobuf has no
  structs, so the array of them is a repeated message of the three floats,
  which protobuf writes as fixed32s; packed repeated floats would make a column
  of each field, which the documents time already. The messages in
  `src/codecs/protobuf.rs` are derived with `#[derive(prost::Message)]` in the
  shape that prost-build generates, so no protoc is needed.
- **rmp-serde** packs the workloads' own types through their serde derives,
  with `to_vec_named()`, which packs a struct as a map with its field names,
  as msgpackr packs an object, and reads them with `from_slice()`, which
  borrows strings from the bytes. The array of structs packs with `to_vec()`,
  which packs each struct, and the table that holds them, as an array of its
  fields in order, without their names.
- **rkyv** archives the workloads' own types through their derives, with
  `to_bytes()` into an `AlignedVec`, and reads them with `access()`, which
  validates the whole archive with bytecheck first, as untrusted bytes need.
  Numbers are read with `to_native()`, through which columns are copied into
  `Vec`s.
- **`to_le_bytes`** writes the fixed-size record into a 12-byte array with
  `to_le_bytes()`, and reads it with `from_le_bytes()` from the array of its
  first 12 bytes.

A field that FlatBuffers or protobuf bytes leave out reads as `None`, which
their codecs return as an error, since every workload has it.

## Method

Criterion 0.8.2 times each benchmark in a loop of its own, after a warm-up of
half a second, in 50 samples over about a second. Samples grow linearly, from
about a millisecond to 40 ms, so the fastest isn't one lucky call, nor limited
by the timer, which ticks every 42 ns on Apple silicon. The slowest
benchmarks, which take more than a millisecond per call, start from one call
and take up to 2 s, as Criterion warns. The report doesn't use Criterion's
confidence intervals, so its bootstrap resamples 1,000 times rather than
100,000. The `report` binary then prints each benchmark's fastest
sample, per call, and its ratio to tafla's, as the other languages'
comparisons report the fastest round, since load from other processes only
slows samples down. It prints the median sample too, which load moves away
from the fastest.

- Every codec is a type of its own, and each benchmark's loop is compiled for
  its codec, so nothing is shared between their loops; Rust has no JIT
  feedback to share.
- The `allocation` binary counts what each benchmark's call allocates with a
  global allocator that wraps the system's: the allocations and reallocations,
  and the bytes they ask for, over 100 calls after one, divided among them. A
  reallocation counts its new size, and freeing isn't counted. Counting adds
  an atomic addition to every allocation, so it runs apart from the timings.
- Inputs pass through `black_box()`, so that the compiler can't fold the
  constant record and fixed-size record into the code, and results are
  returned to Criterion, which passes them through `black_box()` and drops
  them within the timed loop, as a caller frees its bytes.
- Before timing, every benchmark checks that its codec reads back exactly what
  it wrote, with floats compared by their bits, and that it sums column `a` to
  the same bits. `cargo test` runs `tests/round_trip.rs`, which checks every
  codec on every workload.
- `cargo bench` builds with the workspace's release profile, with fat LTO and
  one codegen unit, so that calls inline across the crates.

Criterion doesn't interleave the codecs as the TypeScript harness does: it
runs one benchmark's samples after another's, so a burst of load can slow one
codec's samples and not another's. A benchmark's fastest sample is unaffected
unless the load lasts the whole second. Check `uptime` before a run, and
compare the fastest samples with the medians, which the report summarizes.

Until 2026-11-14, the suite warmed each benchmark up for 2 s and took 100
samples over 4 s, which took four times as long. Four runs of these settings,
interleaved with five of those on the same build, differed from them by 1.5%
in the median benchmark, and by up to 4.1% for 90% of the benchmarks, as two
runs of the old settings differed from each other by 1.6% and 5.0%. Samples
must stay long, though: 100 samples over a second after the same warm-up,
from 0.2 ms, gave faster fastest samples than the old settings for 85% of the
benchmarks, by 0.8% in the median, and read the record with tafla-gen 4%
faster in every run.

The harness is Criterion rather than divan, although divan reports the
fastest sample itself: Criterion's samples last a millisecond or more, where
divan's last about 100 ticks of the timer, 4 µs on Apple silicon, and divan
keeps a sample's results alive until it ends, so that freeing them isn't
timed. Criterion's saved baselines also compare two versions of tafla, as a
performance change needs. It doesn't print the fastest sample, which the
report reads from the samples that it saves.

## Running

```sh
uptime
cargo bench -p tafla-benchmarks --bench comparison
cargo run -p tafla-benchmarks --bin report
cargo run --release -p tafla-benchmarks --bin allocation
```

The full run takes about 3 minutes, and the allocation counts about a
second once they're built, which CI runs too. `-p tafla-benchmarks` keeps Cargo from
running the other crates' benchmark harnesses, which reject Criterion's
options, and `--bench comparison` from running the strings comparison too.
Criterion's options follow `--`: a regular expression
selects benchmarks, and `--save-baseline <name>` and `--baseline <name>`
compare two versions. `report --markdown` prints the table below.

```sh
cargo bench -p tafla-benchmarks --bench comparison -- 'document of 100,000 rows'
```

A quick run of 10 samples over 50 ms per benchmark, after a 50 ms warm-up,
checks that every benchmark works once it's built, in about 12 seconds for
the comparison and 40 for the strings comparison, which it runs too. CI runs
it after the round-trip test, with the report and the encoding report. Its
numbers mean little, and replace the last run's in the report.

```sh
cargo bench -p tafla-benchmarks -- --warm-up-time 0.05 --measurement-time 0.05 --sample-size 10 --nresamples 1000
```

`cargo test -p tafla-benchmarks --benches` runs every benchmark once, without
optimizations.

## Code checks

Clippy checks the crate with core's lints, all of `clippy::all`, `pedantic`
and `nursery`, and rustfmt formats it, with the workspace's other crates.
tafla-gen's code passes clippy's pedantic lints, which tafla-gen's tests check
it with, and its module allows the nursery lints, whose suggestions the
generator doesn't follow, such as `map_or()` for a `match` of an `Option`.
FlatBuffers' generated code keeps flatc's style, which flatc rather than this
crate would change, so its module allows the nine lints that it raises: its
`_tab` and `_fbb` names, type names written in full rather than `Self` or
elided lifetimes, and tables that derive `PartialEq` without `Eq`, and a
struct's unused lifetimes, its `Default` implemented by hand, and the `as`
casts of the raw pointers that copy its scalars. prost's messages pass as they
are.

The crate builds on Rust 1.88, the workspace's `rust-version`, so
`cargo +1.88 test` runs its tests too. Cargo's resolver picks versions of its
dependencies that support 1.88, which leaves the other crates' dependencies as
they were.

## Limitations

- Criterion doesn't interleave the codecs, as above.
- The Apple M2 Pro has 6 performance and 4 efficiency cores, and macOS can move
  the benchmark's thread to an efficiency core when others are busy.
- Only a little-endian host was timed. On a big-endian host, tafla's
  `to_cow()` copies its columns, swapping their bytes, and rkyv's archives
  are little-endian too.
- Only macOS's allocator was timed, which every codec's allocations depend on.
- FlatBuffers' builder starts empty and doubles by copying, and rmp-serde's and
  rkyv's buffers grow by doubling too. A `FlatBufferBuilder` with the
  message's capacity, or reused with `reset()`, would make writing cheaper,
  but the TypeScript comparison doesn't reuse its builder either.
- prost copies the input's columns into its message, which a caller that
  builds its values in prost's messages wouldn't. tafla-gen's codec collects
  the vector's and the array's items into the generated types, which a caller
  that holds its values in them wouldn't, but borrows the columns.
- The allocation counts depend on the allocator's sizes only through
  `realloc()`'s new sizes, which each crate chooses, and not on what macOS's
  allocator rounds them to.
- rmp-serde's `to_vec()` packs structs as arrays without their field names,
  which is smaller than `to_vec_named()`'s maps, but needs the reader to know
  the fields' order, as a schema would, so the other languages' MessagePack
  codecs pack maps, but for the array of structs, whose fixed shape every
  language packs as arrays.
- Summing floats one after another into an `f64` is bound by the latency of
  each addition, about 0.86 ns here, so tafla, FlatBuffers and rkyv sum the
  large document's column in the same time. The small document's 100
  additions take less than 100 latencies per call, since the processor
  overlaps a call's additions with the next call's.
- rkyv validates the record's archive once, in `access()`, where bytecheck
  checks the name's UTF-8 with simdutf8, and then reads each field at a fixed
  offset. tafla reads each field at the offset that the table stores, and
  checks each read's range. In a separate measurement, interleaved in one
  binary, rkyv read the record in 4.1 ns and tafla-gen in 6.9 ns, and
  reading its fields at fixed positions, with the same check of the name,
  took 3.2 ns. The stored offsets and the range checks make the difference,
  rather than the fields' alignment. tafla's runtime checks the name for
  ASCII first, without a dependency, and took about 12 ns with
  `str::from_utf8()` alone.

## Results

One full run on 2026-11-01, on an Apple M2 Pro with 32 GB in macOS 27.0.1,
with rustc 1.99.1 and Criterion 0.8.2. It took 8 minutes. Other processes kept
the 1-minute load average between 1.3 and 1.9 on the 10 cores, with a median
of 1.5. The median sample was within 5% of the fastest for 59 of the 62
benchmarks, and tafla's read of the record within 7%. FlatBuffers' reads of
the record and of the fixed-size record changed speed in steps during the run:
the fixed-size record's first samples took 23 ns per call and the others 47 to
56 ns, which put its median 105% above its fastest sample, and the record's
took 42 to 90 ns, with a median 42% above. They did the same in an earlier
run under more load, with a 1-minute load average between 1.6 and 3.2, whose
fastest samples were all within 3.3% of these but rkyv's write of the record,
6.3% slower.

Fastest sample in µs per call, and its ratio to tafla's:

| Benchmark                                          |    tafla |    flatbuffers |          prost |       rmp-serde |             rkyv |      to_le_bytes |
| -------------------------------------------------- | -------: | -------------: | -------------: | --------------: | ---------------: | ---------------: |
| record: serialize                                  |   0.0785 |  0.277 (3.53×) | 0.0484 (0.62×) |   0.131 (1.67×) |   0.0620 (0.79×) |                  |
| record: deserialize                                |   0.0126 | 0.0415 (3.29×) | 0.0388 (3.08×) |  0.0454 (3.60×) |  0.00557 (0.44×) |                  |
| document of 100 rows: serialize                    |    0.164 |  0.591 (3.61×) |  0.565 (3.45×) |   1.227 (7.48×) |    0.191 (1.16×) |                  |
| document of 100 rows: deserialize                  |   0.0341 |  0.374 (11.0×) |  1.336 (39.2×) |   1.383 (40.6×) |   0.0842 (2.47×) |                  |
| document of 100 rows: sum a column per element     |   0.0356 |  0.166 (4.67×) |  1.360 (38.2×) |   1.428 (40.1×) |   0.0383 (1.08×) |                  |
| document of 100,000 rows: serialize                |     17.9 |  109.2 (6.11×) |  440.1 (24.6×) |   765.4 (42.8×) |     19.2 (1.07×) |                  |
| document of 100,000 rows: deserialize              |   0.0341 |  220.8 (6473×) | 757.5 (22203×) | 1249.3 (36620×) |      15.3 (448×) |                  |
| document of 100,000 rows: sum a column per element |     85.6 |   85.8 (1.00×) |  827.8 (9.67×) |  1341.9 (15.7×) |     85.6 (1.00×) |                  |
| vector of 100 nested tables: serialize             |    9.027 |  3.544 (0.39×) |  1.404 (0.16×) |   2.634 (0.29×) | 0.0382 (0.0042×) |                  |
| vector of 100 nested tables: deserialize           |    0.302 |  5.011 (16.6×) |  2.439 (8.07×) |   4.184 (13.8×) |   0.0306 (0.10×) |                  |
| fixed-size record: serialize                       | 0.000709 |   0.167 (236×) | 0.0232 (32.7×) |   0.0807 (114×) |   0.0198 (27.9×) | 0.000710 (1.00×) |
| fixed-size record: deserialize                     |  0.00117 | 0.0232 (19.7×) | 0.0107 (9.12×) |  0.0296 (25.2×) | 0.000992 (0.85×) | 0.000665 (0.57×) |

- tafla reads a document in 34 ns, at 100 rows and at 100,000 alike, since its
  columns are slices of the bytes. rkyv reads in place too, but copies its
  columns into `Vec`s, in 15 µs for 100,000 rows. FlatBuffers copies them
  through its iterator in 221 µs, and prost and rmp-serde parse every
  element, in 0.76 and 1.25 ms.
- tafla writes the large document in 18 µs and rkyv in 19 µs, both copying the
  columns whole. FlatBuffers, prost and rmp-serde write them element by
  element, 6 to 43 times slower. Copying the columns into prost's message took
  about 16 µs of its 440 µs, in a separate measurement.
- Summing the 100,000 floats takes 86 µs through tafla's, FlatBuffers' and
  rkyv's accessors alike, bound by the additions. Summing 100 takes tafla
  36 ns, rkyv 38 ns and FlatBuffers 166 ns. FlatBuffers' extra 130 ns doesn't
  grow with the rows, which suggests that it's opening the document, which
  `flatbuffers::root()` verifies. prost and rmp-serde decode the whole
  document first.
- rkyv reads the record in 5.6 ns, at fixed offsets once it's validated,
  against tafla's 12.6 ns and the others' 39 to 45 ns. prost writes it
  fastest, in 48 ns, then rkyv in 62 ns and tafla in 79 ns.
- The vector of 100 nested tables is the shape that tafla handles worst, as in
  the other languages: 9.0 µs to write, against prost's 1.4 µs, rmp-serde's
  2.6 µs and FlatBuffers' 3.5 µs, and each of its 200 tables is a builder with
  an allocation of its own. rkyv, whose items are 12 bytes each, inline,
  writes the vector in 38 ns and reads it in 31 ns, against tafla's 0.30 µs.
- tafla and `to_le_bytes` write the fixed-size record in 0.71 ns alike, with
  the same code. Reading it takes `from_le_bytes()` 0.67 ns, rkyv 0.99 ns and
  tafla's `TableReader` getters 1.17 ns, which check each read's range. The
  other crates allocate a `Vec` for each record, and take 20 to 167 ns to
  write it and 11 to 30 ns to read it.

The array of 100 structs was added after that run, and timed on the same day
with the vector of nested tables, in their groups alone:
`cargo bench -p tafla-benchmarks --bench comparison -- 'structs|nested tables'`.
Another session
ran benchmarks meanwhile, which kept the 1-minute load average between 3.2 and
3.8. The median sample was within 5% of the fastest for 19 of the 20
benchmarks, and at most 5% above it, and the vector's fastest samples were
within 8% of the full run's.

| Benchmark                                |  tafla |   flatbuffers |         prost |     rmp-serde |             rkyv |
| ---------------------------------------- | -----: | ------------: | ------------: | ------------: | ---------------: |
| vector of 100 nested tables: serialize   |  9.044 | 3.530 (0.39×) | 1.375 (0.15×) | 2.836 (0.31×) | 0.0405 (0.0045×) |
| vector of 100 nested tables: deserialize |  0.278 | 5.047 (18.2×) | 2.483 (8.95×) | 4.271 (15.4×) |   0.0309 (0.11×) |
| array of 100 structs: serialize          |  0.140 | 0.583 (4.17×) | 0.982 (7.01×) | 1.390 (9.93×) |   0.0404 (0.29×) |
| array of 100 structs: deserialize        | 0.0286 | 0.102 (3.56×) | 1.934 (67.7×) | 1.537 (53.8×) |   0.0307 (1.08×) |

- tafla writes the 100 structs in 140 ns, against 9.0 µs for the same items as
  nested tables, and reads them in 29 ns, against 278 ns. rkyv, whose archive
  is the same for both shapes, writes them in 40 ns and reads them in 31 ns.
- FlatBuffers writes its vector of structs in 583 ns and reads it in 102 ns,
  including `flatbuffers::root()`'s verification. prost and rmp-serde write and
  parse each struct's three fields, in 1.0 to 1.4 µs and 1.5 to 1.9 µs.

tafla-gen's codec was added after those runs, and timed on 2026-11-02 in two
runs of tafla's and tafla-gen's benchmarks, followed by the `allocation`
binary. They alternated with two runs of the code before its readers held
their bytes rather than a `TableReader`, in the order old, new, new, old. The
1-minute load average was between 2.6 and 4.5 at the runs' starts and ends.
The median sample was within 5% of the fastest for 79 of the 88 benchmarks,
and at most 12% above it. tafla's fastest samples were within 4.1% of the
run before, with a median difference of 1.0%. tafla and tafla-gen wrote the
fixed-size record, with the same code, in 0.709 and 0.710 ns.

Faster of the two runs' fastest samples in µs per call, and tafla-gen's ratio
to tafla's:

| Benchmark                                          |    tafla |        tafla-gen |
| -------------------------------------------------- | -------: | ---------------: |
| record: serialize                                  |   0.0796 |   0.0315 (0.40×) |
| record: deserialize                                |   0.0128 |   0.0116 (0.91×) |
| document of 100 rows: serialize                    |    0.162 |   0.0841 (0.52×) |
| document of 100 rows: deserialize                  |   0.0339 |   0.0214 (0.63×) |
| document of 100 rows: sum a column per element     |   0.0358 |   0.0364 (1.02×) |
| document of 100,000 rows: serialize                |     17.9 |     19.0 (1.06×) |
| document of 100,000 rows: deserialize              |   0.0339 |   0.0215 (0.63×) |
| document of 100,000 rows: sum a column per element |     85.9 |     85.9 (1.00×) |
| vector of 100 nested tables: serialize             |    9.253 |    0.978 (0.11×) |
| vector of 100 nested tables: deserialize           |    0.276 |    0.469 (1.70×) |
| array of 100 structs: serialize                    |    0.142 |    0.153 (1.08×) |
| array of 100 structs: deserialize                  |   0.0309 |   0.0310 (1.00×) |
| fixed-size record: serialize                       | 0.000709 | 0.000710 (1.00×) |
| fixed-size record: deserialize                     |  0.00118 | 0.000771 (0.65×) |

Bytes allocated per call, and the allocations and reallocations that ask for
them, which the `allocation` binary counts:

| Benchmark                                          |         tafla |     tafla-gen |    flatbuffers |          prost |      rmp-serde |          rkyv | to_le_bytes |
| -------------------------------------------------- | ------------: | ------------: | -------------: | -------------: | -------------: | ------------: | ----------: |
| record: serialize                                  |       252 (2) |        60 (1) |        296 (7) |         63 (2) |        120 (4) |        80 (2) |             |
| record: deserialize                                |         0 (0) |         0 (0) |          0 (0) |         16 (1) |          0 (0) |         0 (0) |             |
| document of 100 rows: serialize                    |     1,680 (3) |     1,296 (1) |     4,136 (11) |      2,459 (5) |      4,088 (9) |     3,600 (4) |             |
| document of 100 rows: deserialize                  |         0 (0) |         0 (0) |      1,200 (3) |     3,040 (19) |      1,200 (3) |     1,200 (3) |             |
| document of 100 rows: sum a column per element     |         0 (0) |         0 (0) |          0 (0) |     3,040 (19) |      1,200 (3) |         0 (0) |             |
| document of 100,000 rows: serialize                | 1,200,480 (3) | 1,200,096 (1) | 4,194,344 (21) |  2,387,519 (5) | 4,194,296 (19) | 3,670,032 (4) |             |
| document of 100,000 rows: deserialize              |         0 (0) |         0 (0) |  1,200,000 (3) | 3,145,696 (49) |  1,200,000 (3) | 1,200,000 (3) |             |
| document of 100,000 rows: sum a column per element |         0 (0) |         0 (0) |          0 (0) | 3,145,696 (49) |  1,200,000 (3) |         0 (0) |             |
| vector of 100 nested tables: serialize             |  32,536 (208) |     4,408 (2) |     8,632 (13) |      3,500 (2) |     8,184 (10) |     2,048 (1) |             |
| vector of 100 nested tables: deserialize           |     1,200 (1) |     3,600 (2) |      1,200 (1) |      5,232 (7) |      1,200 (1) |     1,200 (1) |             |
| array of 100 structs: serialize                    |     2,456 (3) |     3,608 (3) |     5,336 (12) |      2,900 (2) |      4,088 (9) |     2,048 (1) |             |
| array of 100 structs: deserialize                  |     1,200 (1) |     1,200 (1) |      1,200 (1) |      3,024 (6) |      1,200 (1) |     1,200 (1) |             |
| fixed-size record: serialize                       |         0 (0) |         0 (0) |        104 (5) |         16 (1) |         56 (3) |        16 (1) |       0 (0) |
| fixed-size record: deserialize                     |         0 (0) |         0 (0) |          0 (0) |          0 (0) |          0 (0) |         0 (0) |       0 (0) |

- tafla-gen writes the vector of 100 nested tables in 0.98 µs, against 9.3 µs
  with tafla's builders, in 2 allocations against 208: the table's bytes, and
  the `Vec` of the items' generated fields that the codec collects from the
  workload's. It writes the record and the small document twice as fast, in
  32 and 84 ns against 80 and 162 ns, each in one allocation of the table's
  exact length.
- The large document takes 19.0 µs against 17.9 µs, both copying the
  columns' 1.2 MB into the table, which tafla-gen allocates once and the
  builder grows into three allocations.
- The array of structs takes 153 ns against 142 ns. The codec collects the
  generated structs from the workload's items, the generated code writes them
  into a `Vec`, and the writer copies that into the table: 3,608 bytes against
  2,456. The generated code has since written the structs straight into the
  table, with `TableWriter::set_structs()`, in 2,408 bytes.
- tafla-gen reads the record's and the documents' fields 9% to 37% faster
  than the hand-written reads, and sums the column in about the same time.
  Its getter of the vector collects the items' readers into a `Vec` of 2,400
  bytes first, so the vector takes 0.47 µs against 0.28 µs. Its view reads
  the items as fast as the hand-written reads, as the next section shows.
- tafla-gen reads the fixed-size record with `from_le_bytes()`, in 0.77 ns,
  against 1.18 ns with `TableReader`'s getters, and 0.67 ns with
  `to_le_bytes`'s `from_le_bytes()` in the full run above.

`cargo run -p tafla-benchmarks --bin encoding` printed:

```
Encoded size (bytes)
                        record     doc 100  doc 100000      vector     structs       fixed
  tafla                     60        1296     1200096        3208        1208          12
  tafla-gen                 60        1296     1200096        3208        1208          12
  flatbuffers               72        1328     1200128        2832        1224          32
  prost                     47        1243     1187503        1900        1700          16
  rmp-serde                 53        1573     1499239        3510        1604          24
  rkyv                      48        1272     1200072        1208        1208          12
  to_le_bytes                                                                           12

Truncated document of 100000 rows: deserialize()
  tafla             600048 B  returned an error: Read exceeds the supplied buffer: 400000 bytes at 400092 of
  tafla                 24 B  returned an error: Read exceeds the supplied buffer: 8 bytes at 24 of 24
  tafla-gen         600048 B  returned an error: Read exceeds the supplied buffer: 400000 bytes at 400092 of
  tafla-gen             24 B  returned an error: Read exceeds the supplied buffer: 8 bytes at 24 of 24
  flatbuffers       600064 B  returned an error: Range [800124, 800128) is out of bounds.
  flatbuffers           24 B  returned an error: Range [32, 40) is out of bounds.
  prost             593751 B  returned an error: failed to decode Protobuf message: Document.columns: buffer
  prost                 24 B  returned an error: failed to decode Protobuf message: Document.name: buffer und
  rmp-serde         749619 B  returned an error: IO error while reading data: failed to fill whole buffer
  rmp-serde             24 B  returned an error: IO error while reading data: unexpected end of file
  rkyv              600036 B  returned an error: unaligned pointer: ptr 0x0000007682bbe7ac unaligned for alig
  rkyv                  24 B  returned an error: subtree pointer overran range: ptr 0x0000000102ca12c0 size 5
```

Every crate rejects the truncated document with an error, and none panics.
rkyv finds an archive's root at the end of its bytes, so a truncated archive
has another root. Its errors describe the failure only in builds with debug
assertions, as this one is; in release builds, they say only that it failed.
The sizes of tafla, FlatBuffers and protobuf are the other languages', and
rmp-serde's are msgpack-core's in Java, which packs floats as float32s too.
The array of structs takes 1,208 bytes in tafla and rkyv, 1,200 of them the
structs', and 1,224 in FlatBuffers. protobuf's 1,700 bytes give each struct a
tag and a length, and each float a tag, and rmp-serde's 1,604 an array header
for each struct and a type byte for each float.

### Reading items through views

The four groups that read tafla's items, from the two runs of tafla's and
tafla-gen's benchmarks above, of `cargo bench -p tafla-benchmarks --bench
comparison -- /tafla`, which include the groups that `-- item` runs.

Faster of the two runs' fastest samples in µs, and its ratio to
`TableReader`'s by hand:

| Benchmark                                       |   tafla |       tafla-gen |  tafla-gen view | tafla-gen view A/A |
| ----------------------------------------------- | ------: | --------------: | --------------: | -----------------: |
| vector of 100 nested tables: read the last item | 0.00794 |   0.132 (16.6×) | 0.00614 (0.77×) |    0.00617 (0.78×) |
| vector of 100 nested tables: sum every item     |   0.216 |   0.455 (2.11×) |   0.221 (1.02×) |      0.220 (1.02×) |
| array of 100 structs: read the last item        | 0.00206 | 0.00206 (1.00×) | 0.00206 (1.00×) |    0.00205 (1.00×) |
| array of 100 structs: sum every item            |  0.0592 |  0.0574 (0.97×) |  0.0575 (0.97×) |     0.0574 (0.97×) |

Only tafla-gen's getter of the vector allocates: 2,400 bytes, a `Vec` of the
100 items' readers.

- `iter()` of the vector's view sums every item in 221 ns, as `TableReader`'s
  getters by hand do in 216 ns, against 455 ns through the getter, which
  collects the items' readers into the `Vec` and reads each back from it.
  The view's loop compiles to the same instructions as the hand-written one:
  each generated getter reads with a `TableReader` in the table's layout, a
  constant. Readers that held a `TableReader` took 437 to 449 ns in the old
  code's runs: a function that takes a reader and isn't inlined, as the one
  that sums an item wasn't, read the reader's layout from memory, and
  branched on its offset size at every read.
- `get()` of the view reads the last item in 6.1 ns, against 132 ns through
  the getter, which collects every item's reader first, and 7.9 ns by hand,
  where the compiler calls `field_offset()` out of line for one of the reads.
- The getter of an array of structs already returns an iterator, and its
  `last()` takes as long as `get()` of the view, 2.1 ns, as the compiler
  reads only the last struct. Every way sums the array in 57 to 60 ns. The
  view adds `get(i)` for any other struct.

## Strings

`benches/strings.rs` compares how tafla and the comparison's other crates
write and read strings, as the TypeScript suite's `yarn bench:strings`,
Java's `StringsBenchmark` and C++'s `tafla_strings` do. The comparison's
workloads hold one string, of 16 bytes of ASCII, which reaches one branch of
tafla's UTF-8 check. Each table here holds 16 distinct strings of one shape
and nothing else, so that a call spends most of its time on their UTF-8. By
the length of each string in bytes, they are:

- 16 empty strings, which time what a call takes without any: the table, its
  offsets and lengths, and the crate's own work
- ASCII of 8, 16, 64, 256 and 1,024 bytes
- 8 characters with one of 2 bytes, 16 with two of 2 bytes, 15 with one of 4
  bytes, and 8, 32 and 256 characters of 3 bytes
- 64 bytes of characters of 2, 3 and 4 bytes, and about 1,024 bytes of them:
  32 and 512 characters of 2 bytes, 22 and 342 of 3 bytes, which take 66 and
  1,026 bytes, and 16 and 256 of 4 bytes
- 256 and 1,024 bytes of ASCII with a character of 2 bytes first, and with
  one last

The characters of 2 bytes are Latin-1 letters, those of 3 bytes CJK
ideographs, and those of 4 bytes emoji. `src/string_workloads.rs` ports the
TypeScript suite's generator, with its seeds, its alphabets and its lengths
in characters, so that the shapes that both suites have hold the same bytes,
and `tests/string_workloads.rs` checks every shape against a hash of the
bytes that the TypeScript generator made of it. The TypeScript suite's shapes
of 31 to 33 characters sit on V8's limits, and are left out. The shapes that
the C++ suite has too, such as 512 characters of 2 bytes, take its seeds, and
hold its strings, and those of 22, 342 and 1,023 characters are Rust's alone.

tafla's `get_str()` checks whether a string is ASCII, and then checks it with
`str::from_utf8()` if it isn't. It tests 8 bytes at a time below 64 bytes,
as `is_ascii()` does, and calls `is_ascii()` from 64, which on aarch64 tests
blocks of 64 bytes with NEON. `str::from_utf8()` skips ASCII two words at a
time where they're aligned and checks every other character on its own.
simdutf8, with which rkyv's
bytecheck checks strings, checks blocks of 64 bytes with NEON, and strings
shorter than 64 bytes with `str::from_utf8()`. So the shapes of 64 and 1,024
bytes that aren't ASCII part a check character by character from one with
SIMD, and those with one character of 2 bytes first or last show what a check
that skips ASCII first costs when the character comes late.

Each crate writes the 16 strings as it would an array of them, except tafla,
whose string fields the comparison times, rather than a vector of strings,
which [Lists of strings](#lists-of-strings) times:

- **tafla** writes a table of 16 string fields with a `TableBuilder`'s
  `set_str()` and reads each with `get_str()`. **tafla-gen**'s code of
  `strings.tafla`, in `src/codecs/tafla_gen_strings/`, writes the same table
  from its generated fields, which borrow the strings, with a `TableSizer`
  and a `TableWriter`, and reads each string with its getter, which calls
  `get_str()`. `strings.tafla` is a copy of the other strings suites' schema.
  Both write the same bytes, which the suite checks.
- **flatbuffers** writes a vector of strings with `create_string()`, with a
  table written by hand in the shape that flatc generates from
  `table Strings { items: [string]; }`, and reads it after
  `flatbuffers::root()` verifies the message, which checks every string with
  `str::from_utf8()`.
- **prost** writes a repeated string,
  `message Strings { repeated string items = 1; }`, with `encode_to_vec()`,
  from a message that copies the input's strings, and reads it with
  `decode()`, which copies each string into a `String` that it allocates and
  checks it with `str::from_utf8()`.
- **rmp-serde** packs an array of strings with `to_vec()`, and unpacks it
  with a visitor that borrows each string from the bytes once rmp-serde has
  checked it with `str::from_utf8()`, since serde's own array of `Cow`s would
  copy them.
- **rkyv** archives an array of 16 strings with `to_bytes()`, and reads them
  in place after `access()` validates the archive, where bytecheck checks
  every string with simdutf8.

Each read returns the 16 strings in an array of `Cow<str>`s: borrowed from
the bytes, but prost's, which moves its `String`s out of the message.
Criterion drops each result within its loop, alike for every codec, and
nothing reads the strings' characters after a codec returns them. Before
timing, every codec must read back every string, which `tests/round_trip.rs`
checks too, and each group times tafla's codec again, as an A/A pair that
shows the harness's noise between two timings of the same code. The compiler
merges the pair's write loop with tafla's, and both read loops call one
`deserialize()`, so the pair doesn't show what a loop's address adds to a
time.

```sh
uptime
cargo bench -p tafla-benchmarks --bench strings -- --save-baseline round-1
STRINGS_REVERSE=1 cargo bench -p tafla-benchmarks --bench strings -- --save-baseline round-2
cargo run -p tafla-benchmarks --bin report -- --markdown --baseline round-1 --baseline round-2
cargo run --release -p tafla-benchmarks --bin allocation -- --strings
cargo run -p tafla-benchmarks --bin encoding -- --strings
```

Criterion runs one codec's samples after another's, so a round runs each
group's codecs in the order that the results list them, or in reverse with
`STRINGS_REVERSE=1`, so that rounds alternate it. `report --baseline` reads
the rounds that `--save-baseline` saved and pools their samples, in the
order that the first round named ran them, and fails if a benchmark is in
some of the rounds that it names and not in the others. A round takes about
9 minutes. A
regular expression selects shapes, such as `-- '× 64 chars'`, and the quick
run's options above run every benchmark briefly. `allocation --strings`
counts each call's allocations, and `encoding --strings` prints each crate's
encoded sizes.

### Limits of the comparison

- The crates check UTF-8 in different ways: tafla with `is_ascii()` and then
  `str::from_utf8()`, flatbuffers, prost and rmp-serde with
  `str::from_utf8()` alone, and rkyv with simdutf8. tafla's runtime depends
  only on the standard library, which simdutf8 isn't part of.
- They check strings at different times. flatbuffers and rkyv check every
  string of a message when it's opened, prost and rmp-serde as they parse it,
  and tafla a string when its getter reads it. Each codec reads every string
  here, so the times compare, but a reader of some of the strings checks only
  those with tafla.
- prost copies each string, as it parses the message, into a `String` that it
  allocates, and its message's `Vec` of them grows by doubling. A write
  copies the input's strings into a message first, as a caller that keeps its
  values would.
- flatbuffers' builder starts empty and doubles by copying, as in the
  comparison, and so do rmp-serde's `Vec` and rkyv's `AlignedVec`.
- tafla's builder records each field in a `Vec` that it allocates, and then
  writes the table into one of its own. tafla-gen's code counts the table's
  length first and allocates it alone.
- rkyv writes a string of up to 8 bytes within its 8-byte slot of the array,
  rather than after it.
- Only aarch64 was timed, where `is_ascii()` and simdutf8 use NEON, which
  every aarch64 processor has. On x86-64, `is_ascii()` uses SSE2, but
  bytecheck builds simdutf8 without its `std` feature, which detects AVX2 and
  SSE 4.2 as it runs, so simdutf8 checks with `str::from_utf8()` there unless
  the build targets a processor that has them.

### Results

Four rounds on 2026-11-27 and 28, on the Apple M2 Pro with 32 GB in macOS
27.0.1, with rustc 1.100.0 and the workspace's release profile, Criterion
0.8.2, flatbuffers 25.12.19, prost 0.14.6, rmp-serde 1.3.1, and rkyv 0.8.18
with bytecheck 0.8.3 and simdutf8 0.1.5. Each round took 9 minutes, and they
ran each group's codecs in alternating order: forward, reverse, forward,
reverse. The six shapes that take the C++ suite's seeds, and the two of
1,023 characters, were timed again on 2026-11-28 with the strings that they
now hold, in four rounds of their own of 3 minutes each, in the same order.
The 1-minute load average was between 1.31 and 2.83 at the rounds' starts and
ends, on the 10 cores, and below 3 as each started. The four
rounds' fastest samples were within 5% of each other for 288 of the 308
benchmarks, and within 10.3% for all of them, and the median of the four
rounds' samples was within 5% of the fastest for 276, and at most 10.9% above
it. tafla's A/A pair was within 2.5% of tafla's in every benchmark, and
within 2% in 42 of the 44. Some rows moved between rounds with their A/A
pair: tafla's read of 16 characters with two of 2 bytes took 231 ns in the
first two rounds and 217 ns in the last two, which had less load, and its
A/A pair's did the same. Whether the load moved them or the modes that a
process takes as [#371](https://github.com/snovader/tafla/issues/371)
describes, which each round, a process of its own, could fall into, wasn't
measured.

Fastest sample of the four rounds in µs per call, and its ratio to tafla's,
with the length of each string in bytes:

| Serialize 16 strings of       | Bytes | tafla |      tafla-gen |   flatbuffers |          prost |     rmp-serde |           rkyv |
| ----------------------------- | ----: | ----: | -------------: | ------------: | -------------: | ------------: | -------------: |
| empty                         |     0 | 0.182 | 0.0516 (0.28×) | 0.384 (2.11×) | 0.0854 (0.47×) | 0.105 (0.58×) | 0.0569 (0.31×) |
| 8 chars of ASCII              |     8 | 0.205 | 0.0706 (0.34×) | 0.446 (2.17×) |  0.367 (1.79×) | 0.235 (1.15×) | 0.0620 (0.30×) |
| 8 chars, one of 2 bytes       |     9 | 0.200 | 0.0697 (0.35×) | 0.447 (2.23×) |  0.367 (1.83×) | 0.235 (1.17×) |  0.255 (1.27×) |
| 16 chars of ASCII             |    16 | 0.222 | 0.0867 (0.39×) | 0.441 (1.99×) |  0.376 (1.69×) | 0.270 (1.22×) |  0.267 (1.20×) |
| 16 chars, two of 2 bytes      |    18 | 0.219 | 0.0860 (0.39×) | 0.443 (2.02×) |  0.399 (1.82×) | 0.267 (1.22×) |  0.229 (1.04×) |
| 15 chars, one of 4 bytes      |    18 | 0.220 | 0.0847 (0.39×) | 0.444 (2.02×) |  0.398 (1.81×) | 0.265 (1.21×) |  0.229 (1.04×) |
| 8 chars of 3 bytes            |    24 | 0.223 | 0.0912 (0.41×) | 0.508 (2.27×) |  0.404 (1.81×) | 0.275 (1.23×) |  0.235 (1.05×) |
| 64 chars of ASCII             |    64 | 0.206 | 0.0777 (0.38×) | 0.603 (2.93×) |  0.394 (1.92×) | 0.292 (1.42×) |  0.285 (1.39×) |
| 32 chars of 2 bytes           |    64 | 0.203 | 0.0756 (0.37×) | 0.587 (2.89×) |  0.389 (1.92×) | 0.286 (1.41×) |  0.274 (1.35×) |
| 16 chars of 4 bytes           |    64 | 0.203 | 0.0756 (0.37×) | 0.591 (2.91×) |  0.392 (1.93×) | 0.283 (1.39×) |  0.278 (1.37×) |
| 22 chars of 3 bytes           |    66 | 0.209 | 0.0769 (0.37×) | 0.603 (2.89×) |  0.404 (1.93×) | 0.291 (1.39×) |  0.253 (1.21×) |
| 32 chars of 3 bytes           |    96 | 0.207 | 0.0806 (0.39×) | 0.595 (2.88×) |  0.409 (1.98×) | 0.306 (1.48×) |  0.250 (1.21×) |
| 256 chars of ASCII            |   256 | 0.258 |  0.112 (0.44×) | 0.996 (3.87×) |  0.474 (1.84×) | 0.379 (1.47×) |  0.412 (1.60×) |
| 255 chars, first of 2 bytes   |   256 | 0.261 |  0.112 (0.43×) | 0.978 (3.75×) |  0.480 (1.84×) | 0.378 (1.45×) |  0.399 (1.53×) |
| 255 chars, last of 2 bytes    |   256 | 0.257 |  0.113 (0.44×) | 0.975 (3.80×) |  0.482 (1.88×) | 0.373 (1.45×) |  0.400 (1.56×) |
| 256 chars of 3 bytes          |   768 | 0.358 |  0.199 (0.56×) | 1.534 (4.28×) |  0.785 (2.19×) | 0.613 (1.71×) |  0.582 (1.63×) |
| 1,024 chars of ASCII          | 1,024 | 0.396 |  0.259 (0.65×) | 2.330 (5.89×) |  0.930 (2.35×) | 0.715 (1.81×) |  0.933 (2.36×) |
| 1,023 chars, first of 2 bytes | 1,024 | 0.399 |  0.244 (0.61×) | 2.343 (5.87×) |  0.920 (2.30×) | 0.732 (1.83×) |  0.961 (2.41×) |
| 1,023 chars, last of 2 bytes  | 1,024 | 0.400 |  0.249 (0.62×) | 2.299 (5.75×) |  0.912 (2.28×) | 0.737 (1.84×) |  0.962 (2.41×) |
| 512 chars of 2 bytes          | 1,024 | 0.393 |  0.247 (0.63×) | 2.299 (5.86×) |  0.912 (2.32×) | 0.721 (1.84×) |  0.961 (2.45×) |
| 256 chars of 4 bytes          | 1,024 | 0.397 |  0.242 (0.61×) | 2.289 (5.76×) |  0.913 (2.30×) | 0.715 (1.80×) |  0.963 (2.43×) |
| 342 chars of 3 bytes          | 1,026 | 0.395 |  0.244 (0.62×) | 2.388 (6.04×) |  0.852 (2.15×) | 0.742 (1.88×) |  0.906 (2.29×) |

| Deserialize 16 strings of     | Bytes |  tafla |      tafla-gen |   flatbuffers |         prost |      rmp-serde |           rkyv |
| ----------------------------- | ----: | -----: | -------------: | ------------: | ------------: | -------------: | -------------: |
| empty                         |     0 | 0.0391 | 0.0465 (1.19×) | 0.151 (3.85×) | 0.180 (4.60×) | 0.0929 (2.37×) | 0.0506 (1.29×) |
| 8 chars of ASCII              |     8 | 0.0435 | 0.0439 (1.01×) | 0.217 (4.99×) | 0.579 (13.3×) |  0.151 (3.47×) | 0.0727 (1.67×) |
| 8 chars, one of 2 bytes       |     9 |  0.148 |  0.156 (1.05×) | 0.223 (1.50×) | 0.615 (4.14×) |  0.178 (1.20×) |  0.172 (1.16×) |
| 16 chars of ASCII             |    16 | 0.0479 | 0.0488 (1.02×) | 0.251 (5.23×) | 0.552 (11.5×) |  0.182 (3.80×) | 0.0737 (1.54×) |
| 16 chars, two of 2 bytes      |    18 |  0.217 |  0.244 (1.12×) | 0.295 (1.36×) | 0.748 (3.44×) |  0.270 (1.24×) |  0.261 (1.20×) |
| 15 chars, one of 4 bytes      |    18 |  0.207 |  0.214 (1.03×) | 0.316 (1.53×) | 0.718 (3.46×) |  0.252 (1.22×) |  0.253 (1.22×) |
| 8 chars of 3 bytes            |    24 |  0.257 |  0.257 (1.00×) | 0.349 (1.36×) | 0.831 (3.24×) |  0.285 (1.11×) |  0.293 (1.14×) |
| 64 chars of ASCII             |    64 | 0.0487 | 0.0534 (1.10×) | 0.273 (5.62×) | 0.585 (12.0×) |  0.221 (4.55×) | 0.0698 (1.43×) |
| 32 chars of 2 bytes           |    64 |  0.563 |  0.550 (0.98×) | 0.644 (1.14×) | 1.097 (1.95×) |  0.590 (1.05×) |  0.158 (0.28×) |
| 16 chars of 4 bytes           |    64 |  0.490 |  0.505 (1.03×) | 0.584 (1.19×) | 1.000 (2.04×) |  0.531 (1.08×) |  0.158 (0.32×) |
| 22 chars of 3 bytes           |    66 |  0.685 |  0.688 (1.00×) | 0.844 (1.23×) | 1.288 (1.88×) |  0.751 (1.10×) |  0.261 (0.38×) |
| 32 chars of 3 bytes           |    96 |  0.935 |  0.938 (1.00×) | 1.095 (1.17×) | 1.544 (1.65×) |  1.024 (1.09×) |  0.257 (0.27×) |
| 256 chars of ASCII            |   256 | 0.0769 | 0.0737 (0.96×) | 0.328 (4.27×) | 0.881 (11.5×) |  0.266 (3.46×) |  0.102 (1.32×) |
| 255 chars, first of 2 bytes   |   256 |  0.232 |  0.243 (1.05×) | 0.327 (1.41×) | 0.885 (3.81×) |  0.271 (1.17×) |  0.190 (0.82×) |
| 255 chars, last of 2 bytes    |   256 |  0.275 |  0.292 (1.06×) | 0.347 (1.27×) | 0.955 (3.48×) |  0.298 (1.08×) |  0.205 (0.75×) |
| 256 chars of 3 bytes          |   768 |  6.570 |  6.574 (1.00×) | 6.724 (1.02×) | 7.384 (1.12×) |  6.662 (1.01×) |  1.091 (0.17×) |
| 1,024 chars of ASCII          | 1,024 |  0.168 |  0.162 (0.96×) | 0.615 (3.67×) | 1.335 (7.96×) |  0.543 (3.23×) |  0.180 (1.07×) |
| 1,023 chars, first of 2 bytes | 1,024 |  0.470 |  0.471 (1.00×) | 0.613 (1.30×) | 1.371 (2.92×) |  0.545 (1.16×) |  0.315 (0.67×) |
| 1,023 chars, last of 2 bytes  | 1,024 |  0.590 |  0.589 (1.00×) | 0.629 (1.07×) | 1.370 (2.32×) |  0.553 (0.94×) |  0.383 (0.65×) |
| 512 chars of 2 bytes          | 1,024 |  8.311 |  8.314 (1.00×) | 8.486 (1.02×) | 9.206 (1.11×) |  8.406 (1.01×) |  1.429 (0.17×) |
| 256 chars of 4 bytes          | 1,024 |  7.068 |  7.081 (1.00×) | 7.165 (1.01×) | 7.847 (1.11×) |  7.127 (1.01×) |  1.429 (0.20×) |
| 342 chars of 3 bytes          | 1,026 |  8.725 |  8.730 (1.00×) | 8.885 (1.02×) | 9.551 (1.09×) |  8.821 (1.01×) |  1.531 (0.18×) |

Reads:

- rkyv reads strings of 64 bytes or more of characters of 2 to 4 bytes in
  0.17 to 0.38 of tafla's time, since simdutf8 checks them with NEON: 1.43 to
  1.53 µs for 16 strings of about 1,024 bytes, against tafla's 7.07 to 8.73
  µs. What such text adds to a read, over ASCII of the same length, is 6.9 to
  8.6 µs in tafla and 1.25 to 1.35 µs in rkyv: `str::from_utf8()` checks
  about 2 GB a second of it, and simdutf8 12 to 13. flatbuffers, prost and
  rmp-serde check with `str::from_utf8()` too, and take 1.01 to 1.12 times
  tafla's time for 768 bytes and more of such text. Below 64 bytes simdutf8
  checks with `str::from_utf8()`, and rkyv takes 1.14 to 1.22 times tafla's
  time.
- tafla reads 16 strings of ASCII in 39 to 49 ns up to 64 bytes, and of 1,024
  bytes in 168 ns, where `is_ascii()` checks more than 100 GB a second.
  rkyv reads ASCII in 1.07 to 1.67 times tafla's time, and flatbuffers,
  rmp-serde and prost in 3.2 to 13 times, since `str::from_utf8()` skips
  ASCII only two words at a time and they have more of their own work: for
  16 empty strings, tafla takes 39 ns, rkyv 51 ns, rmp-serde 93 ns,
  flatbuffers' verification 151 ns and prost 180 ns.
- A character of 2 bytes last in 1,023 bytes of ASCII adds 0.42 µs to tafla's
  read, where one first adds 0.30 µs: `is_ascii()` reads to the last
  character, and `str::from_utf8()` then checks the string from its start.
  rmp-serde, which checks it once, reads that shape in 0.94 of tafla's time.
  At 256 bytes, one last adds 43 ns more than one first.
- tafla-gen's getters read in 0.96 to 1.19 of the time that tafla's
  `get_str()` by hand takes, and more slowly for short strings: 46.5 ns
  against 39.1 ns for 16 empty strings, and 53.4 ns against 48.7 ns for 64
  bytes of ASCII. [#366](https://github.com/snovader/tafla/issues/366) tracks
  it.
- prost allocates each string, in 19 allocations a read, and takes 1.65 to 13
  times tafla's time, but 1.09 to 1.12 times for 768 bytes and more of
  characters of 2 to 4 bytes, where both spend most of the read in
  `str::from_utf8()`.

Writes:

- tafla-gen writes the table in 0.28 to 0.65 of the time of tafla's builder,
  in one allocation of the table's exact length. The builder also allocates
  a `Vec` of 768 bytes for the records of the 16 fields, and takes 182 ns
  for 16 empty strings against tafla-gen's 52 ns.
- rkyv writes strings of up to 8 bytes in 57 to 62 ns, within their slots, in
  one allocation. Longer ones follow the array, in an `AlignedVec` that grows
  in 5 or 6 allocations, and take 1.04 to 1.63 times tafla's time up to 768
  bytes and 2.29 to 2.45 times at about 1,024.
- prost copies each string into its message, in 18 allocations, and takes
  1.69 to 2.35 times tafla's time, but 0.47 times for empty strings.
  rmp-serde takes 1.15 to 1.88 times, in 6 allocations, but 0.58 times for
  empty strings, and flatbuffers 1.99 to 6.04 times, as its builder doubles
  from empty by copying: 65,576 bytes in 15 allocations for strings of 1,024
  bytes.
- What the strings hold doesn't change a write, and a longer string doesn't
  always take longer: tafla writes 16 strings of 64 bytes in 0.206 µs and of
  16 bytes in 0.222 µs, and tafla-gen in 78 and 87 ns. This wasn't
  investigated.

`allocation --strings` counts no allocation in a read but prost's, which
allocates 3 times for the 16 empty strings and 19 times for the others. A
write allocates twice in tafla, once in tafla-gen and 8 to 15 times in
flatbuffers. prost, rmp-serde and rkyv allocate 2, 3 and 1 times for the
empty strings, and 18, 6 and 5 or 6 times for the others, but for strings of
8 bytes, which rkyv writes in their slots, in one. `encoding --strings`
prints 128 bytes from tafla for the 16 empty strings, an offset and a length
of 4 bytes each for every string, which its characters follow; flatbuffers
writes 216, prost 32, rmp-serde 19 and rkyv 128. For strings of 1,024 bytes
the five write 16,512, 16,600, 16,432, 16,435 and 16,512 bytes.

## Lists of strings

`cargo bench -p tafla-benchmarks --bench lists` times how the code that
tafla-gen generates writes and reads lists of 10, 100 and 10,000 distinct
strings: 16 and 256 characters of ASCII, and 8 characters of 3 bytes in
UTF-8. Its lists are three kinds of the TypeScript suite's, in
`typescript/benchmarks/strings/lists/shapes.ts`, from the same generator and
seeds, which `tests/lists.rs` checks against strings that TypeScript printed.
tafla-gen generates `src/lists/generated/` from `lists.tafla`, a copy of the
TypeScript suite's schema, and writes and reads each list in two ways:

- **tafla-gen [string]**, the vector of strings, `StringList`, whose items
  are the strings' UTF-8. A `TableSizer` and a `TableWriter` write it, its
  getter's `StringVector::to_vec()` reads every string, checking their run
  of bytes as UTF-8 at once, and `StringVector::get()` reads the last from
  its ends. tafla-gen's code writes the bytes that `TableBuilder` writes,
  which the tests check.
- **tafla-gen [Name]**, how a schema holds a list of strings without one: a
  vector of tables of one string each, `NameList`. The generated code
  converts each `&str` into the fields of its table as it writes it, and
  reads each string through the generated getters.
- **tafla-gen [string] A/A**, the vector of strings again, after the vector
  of tables, an A/A pair that shows the harness's noise.

Each list is serialized, deserialized into a `Vec<&str>` borrowed from the
bytes, and read for its last string. Before timing, every way must read back
every string, and the last. The suite times them as the comparison does, in
Criterion's loops, one after another rather than interleaved, as
[Method](#method) describes, with the same samples and warm-up. Its results
go to `target/criterion-lists`, apart from the comparison's, which the
report reads when given the directory:

```sh
uptime
cargo bench -p tafla-benchmarks --bench lists
cargo run -p tafla-benchmarks --bin report -- target/criterion-lists
```

The full run of its 81 benchmarks takes about 3 minutes once it's built, and
a quick run, with the options of the comparison's above, about 30 seconds.
`cargo test -p tafla-benchmarks --test lists` checks the lists and that
every way reads them back.

### Results

One full run on 2026-11-27, on an Apple M2 Pro with 32 GB in macOS 27.0.1,
with Rust 1.100.0, through the machine's benchmark lock. Other processes
kept the 1-minute load average at 1.95 when it started, on the 10 cores.
Criterion's median sample was within 4% of the fastest for every benchmark,
and the A/A pair agreed within 2%.

Fastest sample in µs per call, and its ratio to the vector of strings':

| Benchmark | tafla-gen [string] | tafla-gen [Name] | tafla-gen [string] A/A |
| --- | ---: | ---: | ---: |
| 10 × 16 chars of ASCII: serialize | 0.0656 | 0.117 (1.78×) | 0.0658 (1.00×) |
| 10 × 16 chars of ASCII: deserialize | 0.0308 | 0.0575 (1.87×) | 0.0308 (1.00×) |
| 10 × 16 chars of ASCII: read the last string | 0.00381 | 0.00618 (1.62×) | 0.00381 (1.00×) |
| 10 × 256 chars of ASCII: serialize | 0.0799 | 0.129 (1.61×) | 0.0799 (1.00×) |
| 10 × 256 chars of ASCII: deserialize | 0.0484 | 0.0796 (1.64×) | 0.0484 (1.00×) |
| 10 × 256 chars of ASCII: read the last string | 0.00528 | 0.00765 (1.45×) | 0.00529 (1.00×) |
| 10 × 8 chars of 3 bytes: serialize | 0.0693 | 0.120 (1.73×) | 0.0692 (1.00×) |
| 10 × 8 chars of 3 bytes: deserialize | 0.169 | 0.192 (1.14×) | 0.171 (1.01×) |
| 10 × 8 chars of 3 bytes: read the last string | 0.0166 | 0.0194 (1.17×) | 0.0166 (1.00×) |
| 100 × 16 chars of ASCII: serialize | 0.434 | 0.852 (1.96×) | 0.429 (0.99×) |
| 100 × 16 chars of ASCII: deserialize | 0.118 | 0.399 (3.38×) | 0.117 (0.99×) |
| 100 × 16 chars of ASCII: read the last string | 0.00378 | 0.00617 (1.63×) | 0.00382 (1.01×) |
| 100 × 256 chars of ASCII: serialize | 0.594 | 1.016 (1.71×) | 0.593 (1.00×) |
| 100 × 256 chars of ASCII: deserialize | 0.302 | 0.554 (1.83×) | 0.302 (1.00×) |
| 100 × 256 chars of ASCII: read the last string | 0.00528 | 0.00765 (1.45×) | 0.00528 (1.00×) |
| 100 × 8 chars of 3 bytes: serialize | 0.463 | 0.902 (1.95×) | 0.465 (1.00×) |
| 100 × 8 chars of 3 bytes: deserialize | 1.414 | 1.671 (1.18×) | 1.424 (1.01×) |
| 100 × 8 chars of 3 bytes: read the last string | 0.0165 | 0.0190 (1.15×) | 0.0167 (1.01×) |
| 10,000 × 16 chars of ASCII: serialize | 40.3 | 83.8 (2.08×) | 40.5 (1.00×) |
| 10,000 × 16 chars of ASCII: deserialize | 19.2 | 37.6 (1.96×) | 19.3 (1.01×) |
| 10,000 × 16 chars of ASCII: read the last string | 0.00380 | 0.00617 (1.62×) | 0.00373 (0.98×) |
| 10,000 × 256 chars of ASCII: serialize | 76.8 | 101.0 (1.32×) | 76.2 (0.99×) |
| 10,000 × 256 chars of ASCII: deserialize | 45.5 | 54.3 (1.19×) | 45.6 (1.00×) |
| 10,000 × 256 chars of ASCII: read the last string | 0.00527 | 0.00763 (1.45×) | 0.00528 (1.00×) |
| 10,000 × 8 chars of 3 bytes: serialize | 43.7 | 86.4 (1.98×) | 43.3 (0.99×) |
| 10,000 × 8 chars of 3 bytes: deserialize | 143.4 | 163.8 (1.14×) | 144.2 (1.01×) |
| 10,000 × 8 chars of 3 bytes: read the last string | 0.0166 | 0.0189 (1.14×) | 0.0165 (0.99×) |

- The vector of strings is written in 0.48–0.76 times the time of the
  vector of tables, whose tables the generated code writes one by one.
- Every string is read in 0.30–0.84 times the time for lists of ASCII,
  whose run `to_vec()` checks as UTF-8 at once and slices without checking
  its ends, and in 0.85–0.88 times for strings of 3 bytes, whose ends it
  checks for character boundaries.
- The last string is found in 0.61–0.69 times the time for strings of
  ASCII, and 0.86–0.88 times for strings of 3 bytes, without a reader of a
  table around it.

The figures in `core/src/string_vector.rs` come from shorter runs of the
deserialize groups, with a way that read each string with
`StringVector::iter()`, which the suite doesn't keep: `to_vec()` read lists
of ASCII in 0.34–0.92 times its time, and lists of 3 bytes in 0.86–0.90
times.
