# Benchmarks

`cargo bench -p tafla-benchmarks` compares tafla's Rust runtime, `tafla`,
with other Rust crates that encode small tables and numeric columns, in
Criterion benchmarks. It runs the workloads of the TypeScript comparison in
`typescript/benchmarks/`, with the same values, so that the languages' numbers
compare. Run the commands below from `rust/`.

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
of the array of structs, read in three ways: **read the last item**, which a
reader can find without reading the others, and **sum every item**. Each way
sums the three float32s of the items it reads as `f64`s, rather than
collecting items of its own, and `item_access.rs` holds them:

- **tafla** reads with `ByteBuffer`'s getters by hand: each item in place,
  from `Vector::get_in_place()` of the last item or `Vector::iter_in_place()`,
  and `get_struct_array()`'s bytes with `as_chunks()`.
- **tafla-gen** reads with the generated getters, `items()`, which collect
  the vector's items into a `Vec` first, and iterate over the array's
  structs, whose `last()` the compiler finds without reading the others.
- **tafla-gen view** reads with the generated views, `items_view()`, whose
  `get(i)` reads one item and whose `iter()` reads every item.
- **tafla-gen view A/A** reads with the views again, an A/A pair that shows
  Criterion's noise.

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
  `ByteBuffer`'s getters, and tafla's README recommends the standard
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

- **tafla** nests tables from their builders, which borrow the columns and
  write each array in bulk into a table of its exact size. Readers check the
  document's version, and read columns as `ArrayView`s, whose `to_cow()`
  borrows them. The vector's items are nested from builders, collected first,
  since creating a builder can fail. The
  array of structs is written with `to_le_bytes()` into a `Vec`, which the
  builder takes with `set_struct_array()`, and read from
  `get_struct_array()`'s bytes with `as_chunks()` and `from_le_bytes()`, as
  tafla-gen's code does.
- **tafla-gen** is the code that tafla-gen generates from `workloads.tafla`,
  in `src/codecs/tafla_gen/`. A message's `serialize()` counts the table's
  length with a `TableSizer`, and then writes every field, nested tables
  included, into a `Vec` of that capacity with a `TableWriter`, in one
  allocation. Its readers borrow strings and columns, as tafla's do. They
  hold their bytes, and each getter reads them with a `ByteBuffer` in the
  table's layout, a constant, as the hand-written reads make theirs, so that
  the getters compile for that layout wherever a reader is passed. It takes
  the workloads' types, as the other codecs do: the generated vector and
  array of structs take the workload's items as they are, and convert each,
  with `From`, into the item's generated fields or struct as they write it.
  The generated reader of the vector collects its items' readers into a
  `Vec`, which the codec reads into the workload's items. Every suite has a
  copy of the schema; after changing them, regenerate the code as
  `gen/README.md` describes.
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
2 s, in 100 samples over about 4 s. Samples grow linearly, from about a
millisecond to 80 ms, so the fastest isn't one lucky call, nor limited by the
timer, which ticks every 42 ns on Apple silicon. The slowest benchmarks, which
take a millisecond per call, start from one call and take up to 7 s, as
Criterion warns. The `report` binary then prints each benchmark's fastest
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
unless the load lasts the whole 4 s. Check `uptime` before a run, and compare
the fastest samples with the medians, which the report summarizes.

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
cargo bench -p tafla-benchmarks
cargo run -p tafla-benchmarks --bin report
cargo run --release -p tafla-benchmarks --bin allocation
```

The full run takes about 12 minutes, and the allocation counts about a
second once they're built, which CI runs too. `-p tafla-benchmarks` keeps Cargo from
running the other crates' benchmark harnesses, which reject Criterion's
options. Criterion's options follow `--`: a regular expression
selects benchmarks, and `--save-baseline <name>` and `--baseline <name>`
compare two versions. `report --markdown` prints the table below.

```sh
cargo bench -p tafla-benchmarks -- 'document of 100,000 rows'
```

A quick run of 10 samples over 50 ms per benchmark, after a 50 ms warm-up,
checks that every benchmark works in about 12 seconds once it's built. CI runs
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
- Rebuilding alone moves the smallest workloads. In a harness that linked
  two identical copies of tafla's and tafla-gen's codecs into one binary, the
  copies differed by up to 13.7% on tafla-gen's write of the record and 22.6%
  on its read of the array of structs, by an amount that stayed the same
  within a build and changed from one build to the next, as the code landed
  at other addresses. The documents and the vector moved by less than 1.5%.
  The A/A pairs here show one build's noise only, so a difference of less
  than about 10% on the record or the array of structs, between two codecs or
  two builds, may be placement alone.
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
  builds its values in prost's messages wouldn't.
- rmp-serde packs each item of the vector as one flat map of its three
  floats, from the workloads' flat `Item`, as msgpackr does in TypeScript.
  tafla, tafla-gen, FlatBuffers and prost nest a table or message of `first`
  and `second` in each item.
- rkyv reads only aligned bytes: `access()` returns an error for an archive
  that isn't aligned, so the harness keeps its bytes in the `AlignedVec` that
  `to_bytes()` returns. tafla reads bytes at any address, and `to_cow()`
  copies an array that isn't aligned.
- rkyv's vector of nested tables is an array of structs, the same archive as
  its array of structs, so tafla's array of structs is the like-for-like
  comparison with it.
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

## Results

One full run on 2026-11-10, of `main` at 8ded793, on an Apple M2 Pro with
32 GB in macOS 27.0.1, with rustc 1.99.1 and Criterion 0.8.2. Each group of
benchmarks ran in a `cargo bench` of its own, in the order of a whole run,
once other processes used fewer than 3 of the 10 cores, and would have run
again had they passed 4. They used at most 3.3, and the run took 13 minutes,
with the 1-minute load average between 1.6 and 3.6 and a median of 2.5. The
median sample was within 5% of the fastest for 97 of the 102 benchmarks, and
at most 7.6% above it but for rkyv's read of the large document, 20% above,
and tafla's sum of the small document's column, 16% above.

A second run right after it, an A/A pair of the whole run, agreed within 5%
for 101 of the 102 fastest samples, and within 8.2% for tafla's read of the
array of structs. Its write of the fixed-size record ran again, after other
processes passed 4 cores.

Fastest sample in µs per call, and its ratio to tafla's:

| Benchmark                                          |    tafla |    flatbuffers |          prost |       rmp-serde |             rkyv |      to_le_bytes |
| -------------------------------------------------- | -------: | -------------: | -------------: | --------------: | ---------------: | ---------------: |
| record: serialize                                  |   0.0777 |  0.283 (3.64×) | 0.0498 (0.64×) |   0.136 (1.74×) |   0.0636 (0.82×) |                  |
| record: deserialize                                |   0.0132 | 0.0417 (3.15×) | 0.0414 (3.13×) |  0.0463 (3.50×) |  0.00558 (0.42×) |                  |
| document of 100 rows: serialize                    |    0.169 |  0.591 (3.49×) |  0.569 (3.36×) |   1.206 (7.12×) |    0.189 (1.12×) |                  |
| document of 100 rows: deserialize                  |   0.0340 |  0.367 (10.8×) |  1.340 (39.4×) |   1.385 (40.7×) |   0.0809 (2.38×) |                  |
| document of 100 rows: sum a column per element     |   0.0356 |  0.167 (4.70×) |  1.397 (39.2×) |   1.432 (40.2×) |   0.0384 (1.08×) |                  |
| document of 100,000 rows: serialize                |     19.1 |  112.2 (5.89×) |  447.1 (23.5×) |   765.4 (40.2×) |     19.2 (1.01×) |                  |
| document of 100,000 rows: deserialize              |   0.0338 |  212.8 (6296×) | 768.4 (22739×) | 1269.0 (37555×) |      15.4 (456×) |                  |
| document of 100,000 rows: sum a column per element |     86.0 |   86.0 (1.00×) |  854.5 (9.93×) |  1356.0 (15.8×) |     85.5 (0.99×) |                  |
| vector of 100 nested tables: serialize             |    8.919 |  3.523 (0.39×) |  1.431 (0.16×) |   2.815 (0.32×) | 0.0403 (0.0045×) |                  |
| vector of 100 nested tables: deserialize           |    0.275 |  5.007 (18.2×) |  2.466 (8.95×) |   4.242 (15.4×) |   0.0306 (0.11×) |                  |
| array of 100 structs: serialize                    |    0.142 |  0.585 (4.11×) |  0.978 (6.88×) |   1.395 (9.81×) |   0.0403 (0.28×) |                  |
| array of 100 structs: deserialize                  |   0.0286 | 0.0984 (3.44×) |  1.915 (66.9×) |   1.550 (54.2×) |   0.0307 (1.07×) |                  |
| fixed-size record: serialize                       | 0.000709 |   0.173 (244×) | 0.0234 (33.0×) |   0.0826 (116×) |   0.0233 (32.8×) | 0.000710 (1.00×) |
| fixed-size record: deserialize                     |  0.00117 | 0.0231 (19.8×) | 0.0108 (9.18×) |  0.0317 (27.0×) | 0.000986 (0.84×) | 0.000666 (0.57×) |

- tafla reads a document in 34 ns, at 100 rows and at 100,000 alike, since its
  columns are slices of the bytes. rkyv reads in place too, but copies its
  columns into `Vec`s, in 15.4 µs for 100,000 rows. FlatBuffers copies them
  through its iterator in 213 µs, and prost and rmp-serde parse every
  element, in 0.77 and 1.27 ms.
- tafla writes the large document in 19.1 µs and rkyv in 19.2 µs, both
  copying the columns whole. FlatBuffers, prost and rmp-serde write them
  element by element, 6 to 40 times slower than tafla. Copying the columns
  into prost's message took about 16 µs of its write, in a separate
  measurement. Which of tafla and rkyv writes it faster changes from run to
  run: the second run gave 19.0 and 19.2 µs, but rkyv took 15.5 µs on
  2026-11-08, against tafla's 19.1 µs, with the same code, so a write of the
  large document can move by a fifth from one run to another.
- Summing the 100,000 floats takes 86 µs through tafla's, FlatBuffers' and
  rkyv's accessors alike, bound by the additions. Summing 100 takes tafla
  36 ns, rkyv 38 ns and FlatBuffers 167 ns. FlatBuffers' extra 130 ns doesn't
  grow with the rows, which suggests that it's opening the document, which
  `flatbuffers::root()` verifies. prost and rmp-serde decode the whole
  document first.
- rkyv reads the record in 5.6 ns, at fixed offsets once it's validated,
  against tafla's 13.2 ns and the others' 41 to 46 ns. prost writes it
  fastest, in 50 ns, then rkyv in 64 ns and tafla in 78 ns.
- The vector of 100 nested tables is the shape that tafla handles worst, as in
  the other languages: 8.9 µs to write, against prost's 1.4 µs, rmp-serde's
  2.8 µs and FlatBuffers' 3.5 µs, and each of its 200 tables is a builder with
  an allocation of its own. rkyv's vector is an array of structs, whose items
  are 12 bytes each, inline: it writes it in 40 ns and reads it in 31 ns,
  against tafla's 0.28 µs, and compares like for like with tafla's array of
  structs.
- tafla writes the 100 structs in 142 ns, against 8.9 µs for the same items as
  nested tables, and reads them in 29 ns, against 275 ns. rkyv, whose archive
  is the same for both shapes, writes them in 40 ns and reads them in 31 ns,
  and tafla-gen's code writes them in 39 ns.
- FlatBuffers writes its vector of structs in 585 ns and reads it in 98 ns,
  including `flatbuffers::root()`'s verification. prost and rmp-serde write
  and parse each struct's three fields, in 0.98 to 1.4 µs and 1.6 to 1.9 µs.
- tafla and `to_le_bytes` write the fixed-size record in 0.71 ns alike, with
  the same code. Reading it takes `from_le_bytes()` 0.67 ns, rkyv 0.99 ns and
  tafla's `ByteBuffer` getters 1.17 ns, which check each read's range. The
  other crates allocate a `Vec` for each record, and take 23 to 173 ns to
  write it and 11 to 32 ns to read it.

tafla-gen's code, in the same run, and its ratio to tafla's:

| Benchmark                                          |    tafla |        tafla-gen |
| -------------------------------------------------- | -------: | ---------------: |
| record: serialize                                  |   0.0777 |   0.0301 (0.39×) |
| record: deserialize                                |   0.0132 |   0.0119 (0.90×) |
| document of 100 rows: serialize                    |    0.169 |   0.0706 (0.42×) |
| document of 100 rows: deserialize                  |   0.0340 |   0.0214 (0.63×) |
| document of 100 rows: sum a column per element     |   0.0356 |   0.0358 (1.00×) |
| document of 100,000 rows: serialize                |     19.1 |     19.0 (1.00×) |
| document of 100,000 rows: deserialize              |   0.0338 |   0.0215 (0.64×) |
| document of 100,000 rows: sum a column per element |     86.0 |     85.7 (1.00×) |
| vector of 100 nested tables: serialize             |    8.919 |   0.815 (0.091×) |
| vector of 100 nested tables: deserialize           |    0.275 |    0.469 (1.70×) |
| array of 100 structs: serialize                    |    0.142 |   0.0387 (0.27×) |
| array of 100 structs: deserialize                  |   0.0286 |   0.0285 (1.00×) |
| fixed-size record: serialize                       | 0.000709 | 0.000709 (1.00×) |
| fixed-size record: deserialize                     |  0.00117 | 0.000770 (0.66×) |

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
| vector of 100 nested tables: serialize             |  32,536 (208) |     3,208 (1) |     8,632 (13) |      3,500 (2) |     8,184 (10) |     2,048 (1) |             |
| vector of 100 nested tables: deserialize           |     1,200 (1) |     3,600 (2) |      1,200 (1) |      5,232 (7) |      1,200 (1) |     1,200 (1) |             |
| array of 100 structs: serialize                    |     2,456 (3) |     1,208 (1) |     5,336 (12) |      2,900 (2) |      4,088 (9) |     2,048 (1) |             |
| array of 100 structs: deserialize                  |     1,200 (1) |     1,200 (1) |      1,200 (1) |      3,024 (6) |      1,200 (1) |     1,200 (1) |             |
| fixed-size record: serialize                       |         0 (0) |         0 (0) |        104 (5) |         16 (1) |         56 (3) |        16 (1) |       0 (0) |
| fixed-size record: deserialize                     |         0 (0) |         0 (0) |          0 (0) |          0 (0) |          0 (0) |         0 (0) |       0 (0) |

- tafla-gen writes the vector of 100 nested tables in 0.82 µs, against 8.9 µs
  with tafla's builders, in one allocation against 208: the table's 3,208
  bytes, into which it converts each of the workload's items as it writes
  it. It writes the record and the small document more than twice as fast,
  in 30 and 71 ns against 78 and 169 ns, each in one allocation of the
  table's exact length.
- tafla-gen and tafla's builder write the large document in the same 19 µs,
  since the builder writes through the same `TableWriter`, into a table of
  its exact length. Both copy the columns' 1.2 MB into it, and the builder's
  other two allocations are the records of the document's and its nested
  table's fields, 384 bytes.
- tafla-gen writes the array of structs in 39 ns, against 142 ns, and as fast
  as rkyv, in 40 ns. The generated code writes the structs straight into the
  table with `TableWriter::set_structs()`, converting each of the workload's
  items as it writes it: 1,208 bytes in one allocation, the table's, against
  the builder's 2,456 in 3.
- tafla-gen reads the record's and the documents' fields 10% to 37% faster
  than the hand-written reads, and sums the column in the same time. Its
  getter of the vector collects the items' readers into a `Vec` of 2,400
  bytes first, so the vector takes 0.47 µs against 0.28 µs. Its view reads
  the items as fast as the hand-written reads, as the next section shows.
- tafla, tafla-gen and `to_le_bytes` write the fixed-size record, with the
  same code, in 0.709 to 0.710 ns. tafla-gen reads it with `from_le_bytes()`,
  in 0.77 ns, against 1.17 ns with `ByteBuffer`'s getters, and 0.67 ns with
  `to_le_bytes`'s `from_le_bytes()`.

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
  rkyv              600036 B  returned an error: unaligned pointer: ptr 0x0000007629e927ac unaligned for alig
  rkyv                  24 B  returned an error: subtree pointer overran range: ptr 0x0000000103b29270 size 5
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

The four groups that read tafla's items, from the same run.
`cargo bench -p tafla-benchmarks -- item` runs them alone.

Fastest sample in µs, and its ratio to `ByteBuffer`'s by hand:

| Benchmark                                       |   tafla |       tafla-gen |  tafla-gen view | tafla-gen view A/A |
| ----------------------------------------------- | ------: | --------------: | --------------: | -----------------: |
| vector of 100 nested tables: read the last item | 0.00794 |   0.141 (17.8×) | 0.00646 (0.81×) |    0.00646 (0.81×) |
| vector of 100 nested tables: sum every item     |   0.217 |   0.465 (2.15×) |   0.217 (1.00×) |      0.217 (1.00×) |
| array of 100 structs: read the last item        | 0.00206 | 0.00206 (1.00×) | 0.00205 (1.00×) |    0.00205 (1.00×) |
| array of 100 structs: sum every item            |  0.0599 |  0.0574 (0.96×) |  0.0574 (0.96×) |     0.0575 (0.96×) |

Only tafla-gen's getter of the vector allocates: 2,400 bytes, a `Vec` of the
100 items' readers.

- `iter()` of the vector's view sums every item in 217 ns, as `ByteBuffer`'s
  getters by hand do, against 465 ns through the getter, which
  collects the items' readers into the `Vec` and reads each back from it.
  The view's loop compiles to the same instructions as the hand-written one:
  each generated getter reads with a `ByteBuffer` in the table's layout, a
  constant. Readers that held a `ByteBuffer` took 437 to 449 ns in runs of
  the code before, on 2026-11-02: a function that takes a reader and isn't
  inlined, as the one that sums an item wasn't, read the reader's layout from
  memory, and branched on its offset size at every read.
- `get()` of the view reads the last item in 6.5 ns, against 141 ns through
  the getter, which collects every item's reader first, and 7.9 ns by hand,
  where the compiler calls `field_offset()` out of line for one of the reads.
- The getter of an array of structs already returns an iterator, and its
  `last()` takes as long as `get()` of the view, 2.1 ns, as the compiler
  reads only the last struct. Every way sums the array in 57 to 60 ns. The
  view adds `get(i)` for any other struct.
