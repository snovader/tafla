# Benchmarks

`./gradlew :benchmarks:jmh` compares tafla's Java runtime with other Java
libraries that encode small tables and numeric columns, in JMH benchmarks. It
runs the workloads of the TypeScript comparison in `typescript/benchmarks/`,
with the same values, so that the two languages' numbers compare. Run the
commands below from `java/`.

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

Each shape is serialized and deserialized, with columns read back as Java
arrays: the uint32 keys as a `long[]`, as tafla reads them, and the float32s
as `float[]`s. Column `a` of each document is also summed through each
library's per-element accessor. `Workloads` ports the TypeScript generator, so
the inputs are the same values, which `WorkloadsTest` checks against values
that the TypeScript generator printed.

The benchmark classes are named after the TypeScript code's workloads:
`RecordBenchmark`, `DocumentBenchmark`, `ListBenchmark` for the vector of
nested tables, `StructsBenchmark` for the array of structs and
`SampleBenchmark` for the fixed-size record.

`ReusedBytesBenchmark` times the document of 100,000 rows written into the
start of the same bytes in every call, rather than into new bytes, by the
codecs that can: tafla-gen's `Document.serializeInto()`, whose bytes the
benchmark owns and keeps. The other libraries write into new bytes, as the
document's `serialize` times them. msgpack's packer reuses its buffer, but
copies each message out of it with `toByteArray()`, and FlatBuffers'
builder could be reused with `clear()`, though the comparison doesn't, as
Limitations describes. Before timing, the check that the codec writes into
the bytes what `serialize()` writes into new ones starts from bytes of
`0xff`.
`./gradlew :benchmarks:encodingReport` prints each library's encoded sizes and
what it does with a truncated document, as the TypeScript comparison does.

`ItemAccessBenchmark` times only tafla's bytes of the vector of nested tables
and of the array of structs, read in two ways: the last item alone, which a
reader can find without reading the others, and every item. Each way sums the
three float32s of the items it reads as doubles, rather than creating objects
of its own, and `ItemAccesses` holds them:

- **tafla** reads with `TableReader`'s getters by hand: `getVectorOffset()` of
  the last item or `getVectorOffsets()` of every item, and `getStructArray()`'s
  view with `getFloat()`.
- **tafla-gen** reads with the generated getters, `items()`, which return
  views, lazy `List`s whose `get(i)` reads one item, in a loop of `get(i)` for
  every item.
- **tafla-gen A/A** reads with the getters again, an A/A pair that shows
  JMH's noise.

`StringsBenchmark` writes and reads tables of 16 strings of one shape and
nothing else, as [Strings](#strings) describes.

### What differs from TypeScript

- There are no `number[]` inputs. Java's arrays are typed, and its closest
  counterpart, a `List<Float>` of boxed values, isn't what a codec is given.
- Java's tafla has no `StructWriter`, and its package documentation leaves
  records at fixed offsets to `MethodHandles.byteArrayViewVarHandle`. So the
  tafla codec writes the fixed-size record through little-endian `VarHandle`s,
  as tafla-gen's `Sample` does, and reads it with `TableReader`'s getters at
  fixed positions.
- msgpack-core has no typed arrays, and msgpackr's extension that copies their
  bytes is msgpackr's own, so MessagePack's columns are arrays of float32s and
  uint32s, packed and unpacked element by element.
- SBE, which has no JavaScript runtime here, encodes only the fixed-size
  record and the array of structs, as a repeating group: the kinds of message
  it's designed for.
- flatbuffers-java's latest release is 25.2.10, where the TypeScript
  comparison uses flatbuffers 25.9.23.
- `yarn bench`'s benchmarks of each getter and setter aren't ported.

## Libraries

| Library                                                       | Why it's comparable                                                                                                                                            |
| ------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| tafla (`:core`)                                               | The reference that the others are compared with.                                                                                                               |
| tafla-gen's code                                              | tafla's tables, written and read by the code that tafla-gen generates.                                                                                        |
| `com.google.flatbuffers:flatbuffers-java` 25.2.10             | Zero-copy tables with offsets and vectors, like tafla.                                                                                                         |
| `com.google.protobuf:protobuf-java` 4.36.2                    | The most used schema-based codec, with packed repeated floats.                                                                                                 |
| `org.msgpack:msgpack-core` 0.9.12                             | MessagePack's Java implementation: a schemaless format, whose tables are maps with their keys.                                                                 |
| `uk.co.real-logic:sbe-tool` 1.41.0, `org.agrona:agrona` 2.7.0 | Simple Binary Encoding, for the fixed-size record and the array of structs only: fields at fixed offsets, written and read in place through flyweights, like tafla's structs. |

Each library is used as its documentation shows, and as the TypeScript
comparison uses its counterpart:

- **tafla** records nested tables in place among their parent builder's
  fields, with `setTable(fields, fill)` and `setVector(count, fields, fill)`.
  Arrays of the elements' own Java type are copied in bulk, and the uint32
  keys are narrowed and widened element by element. The array of structs is written into a `byte[]` through a
  little-endian `VarHandle` and set with `setStructArray()`, and read from
  `getStructArray()`'s little-endian view. tafla-gen's code writes each struct
  through the same `VarHandle`, but straight into the table, and reads them
  from the same view.
- **tafla-gen** is the code that tafla-gen generates from
  `src/main/tafla/workloads.tafla`, in the package
  `io.github.snovader.tafla.benchmarks.generated.tafla`. A message's
  `serialize()` counts the table's length with a `TableSizer`, and then
  writes every field, nested tables included, into an array of that length
  with a `TableWriter`. Its readers read each field with a getter. The
  getters of the vector and of the array of structs return views, which read
  each item as it's asked for. Its getters of arrays of scalars return copies,
  so it sums a column through the column's view, `aView()`, which reads the
  elements in place, as the other languages' generated getters return them. It
  takes the workloads' records, as the other codecs do, so it creates the
  generated `Fields` records, and the `Sample` record, from them in each call,
  as the protobuf codec builds its messages, and creates the workloads'
  records from the generated readers. It writes the array of structs with
  `Triples.serialize(count, fill)`, whose fill writes each item's values
  straight into the table with the static `Triple.write()`, rather than
  creating a `Triple` of each. Every suite has a copy of the
  schema; after changing them, regenerate the code as `rust/gen/README.md`
  describes.
- **FlatBuffers** builds each message with a new `FlatBufferBuilder` of the
  default size, and creates vectors element by element with the generated
  methods. Float columns are copied out of `aAsByteBuffer()`'s little-endian
  view, and keys are widened through an `IntVector`. A vector of structs is
  started, filled with `createTriple()` from its last item to its first, and
  ended, and its structs are read with the generated accessors. Its classes in
  `generated/flatbuffers/` are written by hand in the shape that
  `flatc --java` generates from `src/main/flatbuffers/workloads.fbs`, with only
  what the benchmarks use, since flatc isn't something Gradle can download.
- **protobuf** builds messages with their builders and parses them with their
  parsers. Repeated fields are added and read one element at a time, which
  keeps them unboxed, and parsed columns are copied into arrays. protobuf has
  no structs, so the array of them is a repeated message of the three floats,
  which protobuf writes as fixed32s; packed repeated floats would make a column
  of each field, which the documents time already. The protobuf
  Gradle plugin downloads protoc and generates the messages from
  `src/main/proto/messages.proto`.
- **MessagePack** packs a table as a map with its keys, and the reader matches
  the keys in whatever order they come. The array of structs packs each struct,
  and the table that holds them, as an array of its fields in order, without
  their keys, as rmp-serde's `to_vec()` packs them, and the reader checks the
  arrays' lengths. One packer and one unpacker serve every
  message, as msgpackr reuses its buffer: the packer is cleared before each
  message and `toByteArray()` copies the message out, and the unpacker is reset
  to each message's bytes. A new unpacker allocates a 16 KiB string decoder for
  its first string, which made reading the record 6 times slower in a quick
  run.
- **SBE** writes an 8-byte message header and the 12-byte record into a new
  array, and the decoder checks the header's template before reading. The array
  of structs is a repeating group, whose 4-byte header holds its entries'
  length and count, followed by an entry of 12 bytes for each struct. Its
  encoder and decoder flyweights are reused, as SBE intends. `generateSbe` runs
  sbe-tool on `src/main/sbe/workloads.xml`.

## Method

JMH 1.37 times each benchmark in average-time mode. Every codec of every
benchmark runs in 3 forked JVMs, each of 5 warm-up and 2 measured iterations
of 1 s, with a fixed 2 GiB heap.

- A fork runs one codec, so the JIT compiles its call sites without the other
  codecs' type profiles, as the TypeScript harness's separate timer loops
  ensure.
- Inputs are read from the benchmark's state rather than constants, so that
  the JIT can't fold them into the code. Results are returned to JMH, which
  consumes them, so that the JIT can't eliminate the work.
- Before timing, every benchmark checks that its codec reads back exactly what
  it wrote. `./gradlew build` runs `RoundTripTest`, which checks every codec on
  every workload.
- After JMH's own summary, the mean with its 99.9% confidence interval, the
  `jmh` task prints each benchmark's fastest iteration and its ratio to
  tafla's, as the TypeScript comparison reports the fastest round, since load
  from other processes only slows iterations. JMH's results, with every
  iteration, are saved in `build/results/jmh/results.json`.
- JMH's GC profiler counts the bytes that each operation allocates, from the
  bytes that the JVM reports the benchmark's thread allocated over each
  iteration, after escape analysis has removed the allocations that don't
  escape. The `jmh` task prints their mean over the measured iterations,
  `gc.alloc.rate.norm`, in a second table.

Unlike the TypeScript harness, JMH doesn't interleave the variants: it runs
one fork after another, so a burst of load can slow one codec's forks and not
another's. Three forks spread each codec over about 20 seconds, which makes
that less likely, and a fork's fastest iteration is unaffected unless the load
lasts both of its measured iterations. Check `uptime` before a run, and compare
the fastest iterations with JMH's means.

Some allocating codecs change speed as a forked JVM ages, so the warm-up can't
be shorter. msgpack wrote the fixed-size record in 0.058 to 0.065 µs in its
forks' second to fifth seconds, and in 0.084 to 0.095 µs from their seventh.
Iterations of 200 ms, 5 to warm up and 10 measured, end each fork after 3 s:
they timed FlatBuffers' write of the large document 12% faster, and msgpack's
write of the record 14% faster, in every iteration, with or without a garbage
collection, and protobuf's read of the record up to 22% faster. After the warm-up, a fork's fastest iteration is almost
always one of its first two, so each fork measures only those. Until
2026-11-14, each fork measured 5, which took 40% longer. In two runs of 5 on
that day, the fastest of each fork's first two iterations was within 0.8% of
the fastest of all five for 90% of the benchmarks, where the two runs differed
by 4.2%.

## Running

```sh
uptime
./gradlew :benchmarks:jmh
```

The full run takes about 2 hours and a half: 33 minutes for the workloads, 90
for the [strings](#strings), which `--jmhArgs="-e Strings"` leaves out, and
30 for the [lists of strings](#lists-of-strings), which `-e StringLists`
leaves out, and `-e String` with the strings.
`--jmhArgs` passes JMH's own options, which override those in
`build.gradle.kts`: a regular expression selects benchmarks, `-e` excludes
them, and `-p` sets a parameter.

```sh
./gradlew :benchmarks:jmh --jmhArgs="Document -p codec=tafla"
```

A quick run of one fork, with one warm-up and one measured iteration of
100 ms, checks that every benchmark works in about 30 seconds, the 240 of
the strings in another 100, and the 81 of the lists of strings in another 60.
CI runs it after the round-trip test, on Java 25,
with three of the strings' shapes, `-p shape=empty,15-one-4-byte,1024-ascii`,
since the round-trip test checks every shape. Its numbers mean little.

```sh
./gradlew :benchmarks:jmh --jmhArgs="-f 1 -wi 1 -w 100ms -i 1 -r 100ms"
```

## Code checks

The benchmarks are checked with core's Error Prone and NullAway settings,
except for:

- The `generated` packages: protoc's messages, sbe-tool's encoder and decoder,
  the FlatBuffers classes in flatc's shape, and tafla-gen's classes. Their
  style is their generators', and a generator's update can change it: protoc's
  messages raise 51 Error Prone warnings, such as reference equality and
  unused variables, and flatc's shape raises 5, for its parenthesized returns
  and a class named `Record`, which `-Werror` would fail on. sbe-tool's code
  passes, but is excluded for the same reason, and tafla-gen's declares a
  `Record` too. None of it is null-marked, so NullAway skips it too.
- JMH's generated harness, which is compiled without Error Prone.

NullAway is told that JMH sets `@Param` fields and calls `@Setup` methods
after constructing a benchmark's state.

## Limitations

- JMH doesn't interleave the codecs, as above.
- The Apple M2 Pro has 6 performance and 4 efficiency cores, and macOS can move
  the benchmark's thread to an efficiency core when others are busy.
- Only JDK 25 was timed, though tafla compiles for Java 21.
- msgpack-core uses `sun.misc.Unsafe`'s memory access, which JDK 24 and later
  warn about, and a later JDK will remove. The JMH forks allow it, so that the
  warning doesn't interrupt the output, and the tests print it.
- Agrona 2 uses the JDK's internal `Unsafe`, so the tests and JMH forks run
  with `--add-exports=java.base/jdk.internal.misc=ALL-UNNAMED`.
- FlatBuffers' builder starts at 1 KiB and doubles by copying, and protobuf's
  builders grow repeated fields as they're added, without a way to reserve
  their length. Reusing a `FlatBufferBuilder` with `clear()` would make small
  messages cheaper, but the TypeScript comparison doesn't reuse it either.
- Summing 100,000 floats one after another into a `double` is bound by the
  latency of each addition, about 0.86 ns here, so tafla's and FlatBuffers'
  accessors sum the large document in the same time, though not the small one.

## Results

One full run on 2026-11-01, on an Apple M2 Pro with 32 GB in macOS 27.0.1,
with Oracle's JDK 25.0.1 and JMH 1.37. It took 26 minutes. Other processes
kept the 1-minute load average between 1.3 and 3.8 on the 10 cores, with a
median of 2.2. JMH's mean was within 5% of the fastest iteration for 41 of the
50 benchmarks, and at most 39% above it, where one of protobuf's three forks
read the fixed-size record in 0.0135 µs and the others in 0.018–0.030 µs.

Fastest iteration in µs per operation, and its ratio to tafla's:

| Benchmark                                          | tafla  | flatbuffers    | protobuf        | msgpack         | sbe            |
| -------------------------------------------------- | -----: | -------------: | --------------: | --------------: | -------------: |
| record: serialize                                  | 0.0478 |  0.146 (3.05×) |  0.0297 (0.62×) |  0.0819 (1.71×) |                |
| record: deserialize                                | 0.0140 | 0.0195 (1.39×) |  0.0322 (2.29×) |   0.131 (9.32×) |                |
| document of 100 rows: serialize                    |  0.177 |  0.816 (4.62×) |   1.268 (7.18×) |   0.829 (4.69×) |                |
| document of 100 rows: deserialize                  | 0.0758 | 0.0797 (1.05×) |  0.816 (10.77×) |  1.457 (19.22×) |                |
| document of 100 rows: sum a column per element     | 0.0417 | 0.0555 (1.33×) |  0.826 (19.79×) |  0.730 (17.49×) |                |
| document of 100,000 rows: serialize                |   41.8 | 792.4 (18.94×) | 1757.8 (42.02×) |  695.6 (16.63×) |                |
| document of 100,000 rows: deserialize              |   48.7 |   49.5 (1.02×) | 1007.7 (20.70×) |  971.6 (19.95×) |                |
| document of 100,000 rows: sum a column per element |   86.2 |   86.2 (1.00×) | 1026.2 (11.91×) |   614.9 (7.14×) |                |
| vector of 100 nested tables: serialize             |  6.609 |  4.882 (0.74×) |   1.663 (0.25×) |   3.403 (0.51×) |                |
| vector of 100 nested tables: deserialize           |  0.566 |  0.870 (1.54×) |   3.535 (6.25×) |  7.377 (13.03×) |                |
| fixed-size record: serialize                       | 0.0021 | 0.129 (61.33×) |  0.0049 (2.31×) | 0.0759 (36.01×) | 0.0063 (2.97×) |
| fixed-size record: deserialize                     | 0.0025 | 0.0035 (1.42×) |  0.0135 (5.49×) | 0.0908 (37.04×) | 0.0059 (2.41×) |

- tafla reads every shape fastest. FlatBuffers reads documents within 5% of
  it, since both copy the float columns in bulk and widen the keys one by one.
- tafla writes documents 4.6 to 42 times faster than the others, whose
  columns are written element by element.
- The vector of 100 nested tables is the one shape that tafla writes slowest:
  6.6 µs, against protobuf's 1.7 µs. protobuf also writes the record faster.
- The sums of 100,000 floats take 86 µs through tafla's and FlatBuffers'
  accessors alike, bound by the additions; the 100-row sums differ.
- The fixed-size record takes 2 to 3 ns with tafla's `TableReader`, and 6 ns
  with SBE, which writes and checks its 8-byte header too.

An iteration can also run faster than the rest when fewer garbage collections
fall into it. msgpack's fastest read of the large document took 972 µs,
against 1,030 to 1,107 µs for its other 14 iterations.

The array of 100 structs was added after that run, and timed on the same day
with the vector of nested tables, in their benchmarks alone:
`./gradlew :benchmarks:jmh --jmhArgs="Structs|List"`. It took 9 minutes, while
another session ran benchmarks, which kept the 1-minute load average between
2.3 and 4.3, with a median of 3.0. JMH's mean was within 5% of the fastest
iteration for 15 of the 18 benchmarks, and at most 21% above it, where SBE's
first fork read the structs in 0.391 µs and the others had iterations of
0.67 to 0.83 µs. The vector's fastest iterations were within 10% of the full
run's.

| Benchmark                                | tafla |   flatbuffers |       protobuf |        msgpack |           sbe |
| ---------------------------------------- | ----: | ------------: | -------------: | -------------: | ------------: |
| vector of 100 nested tables: serialize   | 6.001 | 4.978 (0.83×) |  1.660 (0.28×) |  3.468 (0.58×) |               |
| vector of 100 nested tables: deserialize | 0.540 | 0.887 (1.64×) |  3.614 (6.69×) | 7.360 (13.63×) |               |
| array of 100 structs: serialize          | 0.135 | 0.489 (3.62×) |  0.943 (6.98×) |  0.709 (5.25×) | 0.226 (1.68×) |
| array of 100 structs: deserialize        | 0.204 | 0.226 (1.11×) | 2.543 (12.48×) |  1.564 (7.67×) | 0.391 (1.92×) |

- tafla writes the 100 structs in 0.135 µs, against 6.0 µs for the same items
  as nested tables, and reads them in 0.204 µs, against 0.540 µs. Reading
  creates the 100 `ItemInput` records, which every codec does.
- FlatBuffers reads its vector of structs nearly as fast, in 0.226 µs, and
  writes it in 0.489 µs. SBE writes its repeating group in 0.226 µs and reads
  it in 0.391 µs.
- protobuf and msgpack write and parse each struct's fields: 0.7 to 0.9 µs to
  write and 1.6 to 2.5 µs to read.

tafla-gen's codec was added after those runs, and timed on 2026-11-02 in a
full run of every codec, with JMH's GC profiler, which took 38 minutes. Other
processes, a development server and a compiler among them for part of the
run, kept the 1-minute load average between 1.5 and 18.7, with a median of
3.1. JMH's mean was within 5% of the fastest iteration for 49 of the 74
benchmarks, and at most 48% above it, for msgpack's write of the large
document. The other libraries' fastest iterations were within 14% of the runs
above, with a median difference of 2.3%, and the documents' reads within 9%.
tafla's builders wrote the record, the small document and the vector 8% to
13% faster than in the runs above, which measured an earlier version of the
runtime. A full run earlier that day, before the runtime encoded strings of 64
characters or more with `String.getBytes`, gave fastest iterations within 4%
of these for tafla and tafla-gen, and the same allocations.

Fastest iteration in µs per operation, and tafla-gen's ratio to tafla's:

| Benchmark                                          |  tafla |      tafla-gen |
| -------------------------------------------------- | -----: | -------------: |
| record: serialize                                  | 0.0415 | 0.0176 (0.42×) |
| record: deserialize                                | 0.0143 | 0.0146 (1.03×) |
| document of 100 rows: serialize                    |  0.156 | 0.0827 (0.53×) |
| document of 100 rows: deserialize                  | 0.0743 | 0.0748 (1.01×) |
| document of 100 rows: sum a column per element     | 0.0441 | 0.0594 (1.35×) |
| document of 100,000 rows: serialize                |   41.3 |   42.2 (1.02×) |
| document of 100,000 rows: deserialize              |   48.1 |   48.3 (1.00×) |
| document of 100,000 rows: sum a column per element |   89.9 |  101.7 (1.13×) |
| vector of 100 nested tables: serialize             |  6.073 |  0.889 (0.15×) |
| vector of 100 nested tables: deserialize           |  0.533 |  1.179 (2.21×) |
| array of 100 structs: serialize                    |  0.132 |  0.378 (2.86×) |
| array of 100 structs: deserialize                  |  0.200 |  0.519 (2.59×) |
| fixed-size record: serialize                       | 0.0022 | 0.0021 (0.97×) |
| fixed-size record: deserialize                     | 0.0024 | 0.0018 (0.76×) |

Bytes allocated per operation, the mean of the measured iterations:

| Benchmark                                          |     tafla | tafla-gen | flatbuffers |  protobuf |   msgpack |   sbe |
| -------------------------------------------------- | --------: | --------: | ----------: | --------: | --------: | ----: |
| record: serialize                                  |       400 |        80 |       1,360 |       136 |       335 |       |
| record: deserialize                                |        96 |        96 |         144 |        96 |       320 |       |
| document of 100 rows: serialize                    |     1,928 |     1,312 |       4,736 |     6,328 |     3,908 |       |
| document of 100 rows: deserialize                  |     1,760 |     1,760 |       1,808 |     4,595 |     2,128 |       |
| document of 100 rows: sum a column per element     |         0 |       416 |           0 |     2,872 |       320 |       |
| document of 100,000 rows: serialize                | 1,200,728 | 1,200,112 |   5,394,470 | 5,954,414 | 3,013,988 |       |
| document of 100,000 rows: deserialize              | 1,600,160 | 1,600,160 |   1,600,208 | 6,367,043 | 1,600,536 |       |
| document of 100,000 rows: sum a column per element |       105 |   400,121 |         121 | 4,766,943 |       324 |       |
| vector of 100 nested tables: serialize             |    39,184 |     8,464 |      10,816 |    16,800 |    14,920 |       |
| vector of 100 nested tables: deserialize           |     3,272 |     6,424 |       2,856 |    26,517 |    17,352 |       |
| array of 100 structs: serialize                    |     2,512 |     5,256 |       4,624 |     7,736 |     3,740 | 1,232 |
| array of 100 structs: deserialize                  |     2,856 |     5,672 |       2,856 |    13,701 |     2,888 | 2,856 |
| fixed-size record: serialize                       |        32 |        32 |       1,320 |        32 |       208 |    40 |
| fixed-size record: deserialize                     |        32 |        32 |          32 |        53 |       208 |    32 |

- tafla-gen writes the vector of 100 nested tables in 0.89 µs, against 6.07 µs
  with tafla's builders, allocating 8.5 KB against 39 KB. By their sizes,
  about 5 KB of that are the 200 generated `Fields` records and their list,
  which the codec creates from the workload's items, and 3.2 KB the table. It
  writes the record and the small document twice as fast, in 18 and 83 ns
  against 42 and 156 ns, allocating only the table's bytes.
- The large document takes 42.2 µs against 41.3 µs, both copying the columns'
  1.2 MB into the table.
- The array of structs takes 0.378 µs against 0.132 µs, and allocates 5.3 KB
  against 2.5 KB. The codec creates a `Triple` record for each item, and the
  generated code wrote them into an array of their own before copying it
  into the table. It has since written them straight into the table, with
  `TableWriter.setStructs()`, in 4,040 bytes.
- The generated readers read the record and the documents within 3% of the
  hand-written reads, but the column's getter returns a copy, so summing it
  copies the floats first: 59 ns against 44 ns for 100 rows, and 101.7 µs
  against 89.9 µs, with 400 KB, for 100,000. The reader of the vector creates
  an `Item` and a `Pair` for each item, and the reader of the array a `Triple`
  for each struct, before the codec creates the workload's records: 1.18 µs
  against 0.533 µs, and 0.519 µs against 0.200 µs, with twice the bytes.
- tafla-gen writes and reads the fixed-size record through `VarHandle`s in
  2.1 and 1.8 ns, against 2.2 and 2.4 ns with `TableReader`'s setters, which
  are gone, and getters. Both allocate only the 12-byte array, or the
  workload's record, 32 bytes, so escape analysis removes the generated
  `Sample` record.

`./gradlew :benchmarks:encodingReport` printed:

```
Encoded size (bytes)
                        record     doc 100  doc 100000      vector     structs       fixed
  tafla                     60        1296     1200096        3208        1208          12
  tafla-gen                 60        1296     1200096        3208        1208          12
  flatbuffers               72        1328     1200128        2832        1224          32
  protobuf                  47        1243     1187503        1900        1700          16
  msgpack                   53        1573     1499239        3510        1604          24
  sbe                                                                     1212          20

Truncated document of 100000 rows: deserialize()
  tafla             600048 B  threw DecodeException: Read exceeds the supplied buffer: 400000 bytes at
  tafla                 24 B  threw DecodeException: Read exceeds the supplied buffer: 8 bytes at 24 of
  tafla-gen         600048 B  threw DecodeException: Read exceeds the supplied buffer: 400000 bytes at
  tafla-gen             24 B  threw DecodeException: Read exceeds the supplied buffer: 8 bytes at 24 of
  flatbuffers       600064 B  threw IndexOutOfBoundsException
  flatbuffers           24 B  threw IndexOutOfBoundsException
  protobuf          593751 B  threw InvalidProtocolBufferException: While parsing a protocol message, the input ended
  protobuf              24 B  threw InvalidProtocolBufferException: While parsing a protocol message, the input ended
  msgpack           749619 B  threw MessageInsufficientBufferException
  msgpack               24 B  threw MessageInsufficientBufferException
```

Every library rejects the truncated document. FlatBuffers' Java runtime reads
through a `java.nio.ByteBuffer`, whose range checks throw. The array of
structs takes 1,208 bytes in tafla, 1,212 in SBE, whose message and group
headers take 12, and 1,224 in FlatBuffers.

### Reading items through views

One run of `ItemAccessBenchmark` on 2026-11-02 on the same machine and JDK, of
`./gradlew :benchmarks:jmh --jmhArgs="ItemAccess"`. It took 8 minutes. It
started as another process's burst of load ended, at a 1-minute load average
of 4.9, which was 2.8 when the Python suite started after it. JMH's mean was within 5%
of the fastest iteration for 14 of the 16 benchmarks, and at most 6.1% above
it.

Fastest iteration in µs per operation, and its ratio to `TableReader`'s by
hand:

| Benchmark                    |  tafla |      tafla-gen | tafla-gen view | tafla-gen view A/A |
| ---------------------------- | -----: | -------------: | -------------: | -----------------: |
| ItemAccess: vectorLastItem   | 0.0062 | 0.394 (63.99×) | 0.0061 (1.00×) |     0.0062 (1.00×) |
| ItemAccess: vectorEveryItem  |  0.397 |  0.885 (2.23×) |  0.518 (1.30×) |      0.519 (1.31×) |
| ItemAccess: structsLastItem  | 0.0034 | 0.189 (55.76×) | 0.0042 (1.23×) |     0.0042 (1.23×) |
| ItemAccess: structsEveryItem | 0.0664 |  0.242 (3.65×) | 0.0701 (1.06×) |     0.0699 (1.05×) |

Bytes allocated per operation:

| Benchmark                    | tafla | tafla-gen | tafla-gen view | tafla-gen view A/A |
| ---------------------------- | ----: | --------: | -------------: | -----------------: |
| ItemAccess: vectorLastItem   |     0 |     3,568 |              0 |                  0 |
| ItemAccess: vectorEveryItem  |   416 |     3,568 |              0 |                  0 |
| ItemAccess: structsLastItem  |     0 |     2,816 |              0 |                  0 |
| ItemAccess: structsEveryItem |     0 |     2,816 |              0 |                  0 |

- The views read the vector's last item as fast as `TableReader` by hand,
  0.0061 µs against 0.0062 µs, and the array's in 0.0042 µs against
  0.0034 µs, 64 and 45 times faster than the getters, which read all 100
  items first.
- Through the views, nothing is allocated: C2's escape analysis removes each
  item's reader or record, which a view returns to the loop rather than
  stores, as the getters do in their `List` or array. Summing the vector takes
  0.52 µs against 0.89 µs through the getter and 0.40 µs by hand, and the
  array 0.070 µs against 0.242 µs and 0.066 µs.

## Strings

`StringsBenchmark` compares how tafla and the other libraries write and read
strings, which the workloads above hold one of, of 16 bytes of ASCII. It's the
counterpart of the TypeScript suite in `typescript/benchmarks/strings/`. Each
table holds 16 distinct strings of one shape and nothing else, so that a call
spends most of its time on their UTF-8. The benchmark's `shape` parameter
takes the shapes' names, which give their length in code points and their
mix:

- `8-ascii`, `16-ascii`, `32-ascii`, `64-ascii`, `256-ascii` and
  `1024-ascii`: 8 to 1,024 characters of ASCII
- `8-one-2-byte`: 8 characters, one of 2 bytes, and `16-two-2-byte`: 16, two
  of 2 bytes
- `15-one-4-byte`: 15 characters, one of 4 bytes, a surrogate pair
- `8-3-byte`, `32-3-byte` and `256-3-byte`: 8, 32 and 256 characters of 3
  bytes
- `empty`: 16 empty strings, which time what a call takes without any: the
  table, its offsets and lengths, and the library's own work

These are the TypeScript suite's strings. `StringShapes` ports its generator,
seeds and alphabets, and `StringShapesTest` checks the strings against values
that the TypeScript generator printed. In place of the TypeScript suite's
shapes either side of V8's lengths, the others sit either side of the Java
libraries' own:

- `63-ascii` and `64-ascii`, `63-last-2-byte` and `64-last-2-byte`, and
  `63-last-3-byte` and `64-last-3-byte`, whose last character is of 2 or of 3
  bytes, either side of tafla's `Utf8.SHORT_LENGTH`. tafla counts a string of
  fewer than 64 `char`s and writes it a `char` at a time, in place, and
  encodes a longer one with `String.getBytes`, after a scan for lone
  surrogates, and copies it into the table.
- `42-ascii` and `43-ascii`, and `127-ascii` and `128-ascii`: protobuf leaves
  room for a string's length before it, which a varint of 1 byte holds up to
  127 bytes and of 2 up to 16,383. The string's `char`s tell the varint's
  size unless they are 43 to 127, which may take 128 bytes or more, so
  protobuf counts those strings' bytes a second time.
- `511-ascii` and `512-ascii`: msgpack-core encodes a string of fewer than 512
  `char`s with `String.getBytes`, and a longer one with a `CharsetEncoder`,
  into its buffer.

The characters of 2 bytes are Latin-1 letters, which a compact Java string
holds in a byte, as V8 does, the characters of 3 bytes CJK ideographs, and
those of 4 bytes emoji. A string with a character of 3 or 4 bytes holds every
character in two bytes, as UTF-16, so `64-last-3-byte` is a string of UTF-16
and `64-last-2-byte` one of Latin-1.

Each library writes the 16 strings as it would an array of them, except
tafla, whose string fields the suite times, rather than a vector of strings,
which [Lists of strings](#lists-of-strings) times:

- **tafla** writes a table of 16 string fields with a `TableBuilder`'s
  `setString()`, and reads them with `TableReader.getString()`, which decodes
  with `new String(bytes, UTF_8)`. That replaces an invalid sequence with
  U+FFFD, so a string with U+FFFD is decoded again with a `CharsetDecoder`,
  which throws on one.
- **tafla-gen** writes and reads the same table with the code that tafla-gen
  generates from `src/main/tafla/strings.tafla`, in the package
  `io.github.snovader.tafla.benchmarks.generated.strings`. Its `TableSizer`
  counts the strings, and keeps the bytes of those of 64 `char`s or more that
  it encodes, which its `TableWriter` copies into the table. It writes the
  `TableBuilder`'s bytes, which the round-trip test checks, and creates its
  generated `Fields` from the input in each call, as protobuf builds its
  message.
- **FlatBuffers** writes a vector of strings, with a class written by hand in
  the shape that flatc generates from `table Strings { items: [string]; }`.
  flatbuffers-java's `Utf8Safe` counts and writes each string a `char` at a
  time, and decodes it with its own loop into a `char[]`, at every length.
- **protobuf** writes a repeated string, with the builder's `addAllItems()`.
  `toByteArray()` counts each string's bytes, with a `char` loop in JDK 25 and
  with `String.encodedLength()` from JDK 27, and writes each with
  `String.getBytes` and a copy. Its parser decodes the strings as
  tafla's reader does, but encodes a string with U+FFFD again and compares the
  bytes.
- **msgpack** packs an array of strings with `packString()`, into the buffer
  that its packer reuses, and unpacks it with `unpackString()`, which decodes
  with `new String(bytes, UTF_8)` and doesn't check the bytes.

Before timing, every codec must read back every string, which the round-trip
test checks too. JMH times the suite as it times the others, as Method
describes.

```sh
uptime
./gradlew :benchmarks:jmh --jmhArgs="Strings"
```

The run takes about 90 minutes. `-p shape=64-ascii,64-last-3-byte` selects
shapes. `./gradlew :benchmarks:encodingReport` prints each library's encoded
sizes of the shapes.

### Limitations

- msgpack-core doesn't check the bytes that it decodes, and replaces an
  invalid sequence with U+FFFD, where tafla, protobuf and FlatBuffers throw.
- protobuf and msgpack decode every string as they parse the message, while
  tafla and FlatBuffers decode a string when its getter is called. Each codec
  reads every string here, so the times compare, but a reader that reads only
  some of the strings decodes only those with tafla and FlatBuffers.
- The strings have no lone surrogate, since the libraries write one
  differently: tafla as U+FFFD, as TypeScript's `TextEncoder` does,
  `String.getBytes`, which protobuf and msgpack use, as `?`, and FlatBuffers
  not at all, since `Utf8Safe` throws.
- Each codec but protobuf's returns the strings it reads in an array that
  `Arrays.asList()` wraps. protobuf's returns the parsed message's list.
- msgpack's packer reuses its buffer, and copies each message out of it with
  `toByteArray()`, as in the comparison. The other libraries write into a new
  buffer, FlatBuffers into one of 1 KiB that doubles by copying.

### Results

One run on 2026-11-25, on the Apple M2 Pro with 32 GB in macOS 27.0.1, with
Oracle's JDK 25.0.1 and JMH 1.37. It took 87 minutes. Other sessions' processes
kept the 1-minute load average between 1.6 and 10.6, with a median of 2.7. It
was above 4 three times, for up to four minutes, the longest while tafla-gen
wrote the shapes of 15 to 43 characters. The benchmarks that it slowed had a
fork outside it, whose fastest iteration is the one below. JMH's mean was
within 5% of the fastest iteration for 218 of the 240 benchmarks, and at most
76% above it, where one of tafla-gen's three forks wrote 512 characters of
ASCII in 2.65 µs and the others in 0.81 µs. A few rows differ as much between
forks without load: tafla-gen's forks read the strings of 8 characters with
one of 2 bytes in 0.75, 0.48 and 0.37 µs, where tafla's took 0.47 to
0.50 µs.

Fastest iteration in µs per operation of 16 strings, and its ratio to
tafla's. Writing:

| Shape          |  tafla |      tafla-gen |   flatbuffers |      protobuf |        msgpack |
| -------------- | -----: | -------------: | ------------: | ------------: | -------------: |
| empty          | 0.0804 | 0.0512 (0.64×) | 0.326 (4.06×) | 0.215 (2.67×) | 0.0720 (0.90×) |
| 8-ascii        |  0.207 |  0.205 (0.99×) | 0.471 (2.27×) | 0.346 (1.67×) |  0.200 (0.97×) |
| 8-one-2-byte   |  0.243 |  0.211 (0.87×) | 0.517 (2.13×) | 0.634 (2.61×) |  0.309 (1.27×) |
| 8-3-byte       |  0.374 |  0.324 (0.87×) | 0.556 (1.49×) | 0.514 (1.37×) |  0.398 (1.06×) |
| 16-two-2-byte  |  0.348 |  0.317 (0.91×) | 0.670 (1.93×) | 0.774 (2.22×) |  0.385 (1.11×) |
| 15-one-4-byte  |  0.493 |  0.465 (0.94×) | 0.615 (1.25×) | 0.641 (1.30×) |  0.443 (0.90×) |
| 16-ascii       |  0.273 |  0.266 (0.97×) | 0.534 (1.95×) | 0.441 (1.61×) |  0.208 (0.76×) |
| 32-ascii       |  0.419 |  0.384 (0.92×) | 0.664 (1.58×) | 0.612 (1.46×) |  0.264 (0.63×) |
| 32-3-byte      |  0.945 |  0.893 (0.94×) | 1.060 (1.12×) | 0.993 (1.05×) |  0.808 (0.85×) |
| 42-ascii       |  0.484 |  0.447 (0.92×) | 0.713 (1.47×) | 0.732 (1.51×) |  0.292 (0.60×) |
| 43-ascii       |  0.502 |  0.466 (0.93×) | 0.702 (1.40×) | 1.128 (2.25×) |  0.299 (0.60×) |
| 63-ascii       |  0.642 |  0.607 (0.95×) | 0.897 (1.40×) | 1.528 (2.38×) |  0.316 (0.49×) |
| 64-ascii       |  0.281 |  0.329 (1.17×) | 0.950 (3.38×) | 1.550 (5.52×) |  0.323 (1.15×) |
| 63-last-2-byte |  0.872 |  0.821 (0.94×) | 0.979 (1.12×) | 2.314 (2.65×) |  0.858 (0.98×) |
| 64-last-2-byte |  0.794 |  0.841 (1.06×) | 0.998 (1.26×) | 2.276 (2.87×) |  0.846 (1.07×) |
| 63-last-3-byte |  1.136 |  1.108 (0.98×) | 0.974 (0.86×) | 1.239 (1.09×) |  0.640 (0.56×) |
| 64-last-3-byte |  0.812 |  0.868 (1.07×) | 0.985 (1.21×) | 1.251 (1.54×) |  0.650 (0.80×) |
| 127-ascii      |  0.404 |  0.413 (1.02×) | 1.506 (3.73×) | 3.430 (8.49×) |  0.435 (1.08×) |
| 128-ascii      |  0.389 |  0.415 (1.07×) | 1.556 (4.00×) | 1.974 (5.08×) |  0.441 (1.13×) |
| 256-ascii      |  0.549 |  0.539 (0.98×) | 2.741 (4.99×) | 3.391 (6.17×) |  0.673 (1.23×) |
| 256-3-byte     |  5.132 |  5.250 (1.02×) | 6.057 (1.18×) | 5.570 (1.09×) |  5.286 (1.03×) |
| 511-ascii      |  0.798 |  0.817 (1.02×) | 5.513 (6.91×) | 6.310 (7.91×) |  1.341 (1.68×) |
| 512-ascii      |  0.799 |  0.812 (1.02×) | 5.557 (6.95×) | 6.368 (7.97×) |  11.6 (14.50×) |
| 1024-ascii     |  1.467 |  1.472 (1.00×) |  10.9 (7.41×) |  11.9 (8.12×) |  24.2 (16.52×) |

Reading:

| Shape          |  tafla |      tafla-gen |   flatbuffers |      protobuf |        msgpack |
| -------------- | -----: | -------------: | ------------: | ------------: | -------------: |
| empty          | 0.0898 | 0.0934 (1.04×) | 0.123 (1.37×) | 0.256 (2.85×) | 0.0601 (0.67×) |
| 8-ascii        |  0.169 |  0.171 (1.01×) | 0.246 (1.45×) | 0.637 (3.76×) |  0.354 (2.09×) |
| 8-one-2-byte   |  0.470 |  0.368 (0.78×) | 0.294 (0.63×) | 0.749 (1.60×) |  0.420 (0.89×) |
| 8-3-byte       |  1.195 |  0.938 (0.79×) | 0.430 (0.36×) | 1.385 (1.16×) |  1.226 (1.03×) |
| 16-two-2-byte  |  0.945 |  0.887 (0.94×) | 0.447 (0.47×) | 1.119 (1.18×) |  0.763 (0.81×) |
| 15-one-4-byte  |  0.565 |  0.553 (0.98×) | 0.423 (0.75×) | 0.765 (1.35×) |  0.545 (0.97×) |
| 16-ascii       |  0.175 |  0.173 (0.99×) | 0.289 (1.65×) | 0.645 (3.68×) |  0.362 (2.06×) |
| 32-ascii       |  0.200 |  0.193 (0.96×) | 0.360 (1.80×) | 0.560 (2.80×) |  0.248 (1.24×) |
| 32-3-byte      |  1.249 |  1.248 (1.00×) | 0.996 (0.80×) | 1.329 (1.06×) |  1.258 (1.01×) |
| 42-ascii       |  0.209 |  0.200 (0.95×) | 0.397 (1.90×) | 0.343 (1.64×) |  0.255 (1.22×) |
| 43-ascii       |  0.212 |  0.204 (0.96×) | 0.404 (1.90×) | 0.354 (1.67×) |  0.251 (1.18×) |
| 63-ascii       |  0.220 |  0.214 (0.97×) | 0.482 (2.19×) | 0.316 (1.44×) |  0.261 (1.19×) |
| 64-ascii       |  0.216 |  0.210 (0.97×) | 0.446 (2.06×) | 0.316 (1.46×) |  0.260 (1.20×) |
| 63-last-2-byte |  0.505 |  0.494 (0.98×) | 0.550 (1.09×) | 0.497 (0.98×) |  0.452 (0.90×) |
| 64-last-2-byte |  0.412 |  0.416 (1.01×) | 0.508 (1.23×) | 0.480 (1.16×) |  0.439 (1.06×) |
| 63-last-3-byte |  0.660 |  0.633 (0.96×) | 0.595 (0.90×) | 0.702 (1.06×) |  0.556 (0.84×) |
| 64-last-3-byte |  0.657 |  0.650 (0.99×) | 0.618 (0.94×) | 0.727 (1.11×) |  0.531 (0.81×) |
| 127-ascii      |  0.269 |  0.273 (1.01×) | 0.761 (2.83×) | 0.365 (1.36×) |  0.302 (1.12×) |
| 128-ascii      |  0.273 |  0.265 (0.97×) | 0.733 (2.69×) | 0.443 (1.63×) |  0.313 (1.15×) |
| 256-ascii      |  0.327 |  0.320 (0.98×) | 1.282 (3.92×) | 0.492 (1.51×) |  0.367 (1.12×) |
| 256-3-byte     |  8.782 |  8.566 (0.98×) | 6.673 (0.76×) | 9.003 (1.03×) |  8.801 (1.00×) |
| 511-ascii      |  0.475 |  0.460 (0.97×) | 2.488 (5.24×) | 0.573 (1.21×) |  0.493 (1.04×) |
| 512-ascii      |  0.470 |  0.457 (0.97×) | 2.455 (5.22×) | 0.548 (1.17×) |  0.489 (1.04×) |
| 1024-ascii     |  0.729 |  0.723 (0.99×) | 4.740 (6.51×) | 0.824 (1.13×) |  0.774 (1.06×) |

- From 64 characters, where tafla encodes with `String.getBytes`, tafla and
  tafla-gen write ASCII fastest: tafla 16 strings of 64 characters in
  0.28 µs against 0.32 µs for msgpack, which encodes them with `getBytes`
  too, and of 1,024 characters in 1.47 µs against 10.9 to 24.2 µs.
- Below 64 characters, tafla writes ASCII a `char` at a time, which takes
  longer than msgpack's `getBytes` from 16 characters on: 1.31 times as long
  for 16, 1.59 for 32, and 2.03 for 63. tafla writes 63 characters of ASCII
  in 0.642 µs, and 64 in 0.281 µs. 63 characters with the last of 3 bytes,
  a string of UTF-16, take 1.136 µs, and 64 take 0.812 µs; with the last of 2
  bytes, a string of Latin-1, 0.872 µs and 0.794 µs.
- tafla-gen writes strings below 64 characters 1% to 36% faster than the
  `TableBuilder`, the 16 empty strings in 0.051 µs against 0.080 µs. From 64
  characters, it takes 0.98 to 1.17 times as long, and keeps each string that
  its `TableSizer` encodes in a record in a list: it allocates 3,272 bytes
  against 2,784 for 64 characters of ASCII.
- protobuf counts a string's bytes a second time from 43 to 127 characters:
  1.128 µs for 43 against 0.732 µs for 42, and 3.430 µs for 127 against
  1.974 µs for 128. msgpack's `CharsetEncoder`, from 512 characters, takes
  11.6 µs against 1.34 µs for 511, and FlatBuffers, which writes a `char` at
  a time at every length, 7.4 times tafla's time for 1,024 characters.
- tafla and tafla-gen read ASCII fastest, except the empty strings, which
  msgpack reads in 0.060 µs against 0.090 µs: msgpack returns the same empty
  string for each, where `new String()` creates one for each, 136 bytes
  against 488.
- Every library but FlatBuffers decodes with `new String(bytes, UTF_8)`, and
  FlatBuffers' own loop reads the strings of 8 to 16 characters with any of 2
  to 4 bytes, and those of 3 bytes alone, 1.25 to 2.8 times as fast as tafla:
  8 characters of 3 bytes in 0.430 µs against 1.195 µs, and 256 in 6.67 µs
  against 8.78 µs. It takes 0.90 to 1.23 times tafla's time for 63 and 64
  characters with the last of 2 or 3 bytes, and 1.4 to 6.5 times for ASCII.
- msgpack, which doesn't check what it decodes, reads 64 characters with the
  last of 3 bytes in 0.531 µs against tafla's 0.657 µs, and 63 with the last
  of 2 bytes in 0.452 µs against 0.505 µs. tafla's scan for U+FFFD reads each
  string of UTF-16, but skips a string of Latin-1, which can't hold U+FFFD;
  this run doesn't tell how much of the difference it takes.
- tafla and tafla-gen read with the same `TableReader`, but the JIT compiles
  their calls differently: tafla-gen's getters read 8 characters of 3 bytes
  in 0.938 µs against 1.195 µs for tafla's loop, in each of their forks.

Bytes allocated per operation, the mean of the measured iterations, writing:

| Shape          |  tafla | tafla-gen | flatbuffers | protobuf | msgpack |
| -------------- | -----: | --------: | ----------: | -------: | ------: |
| empty          |    480 |       272 |       1,576 |      720 |     129 |
| 8-ascii        |    608 |       400 |       1,704 |    1,016 |     811 |
| 8-one-2-byte   |    624 |       416 |       1,704 |    1,712 |   1,488 |
| 8-3-byte       |    864 |       656 |       1,960 |    1,448 |   1,641 |
| 16-two-2-byte  |    768 |       560 |       1,832 |    2,240 |   2,191 |
| 15-one-4-byte  |    768 |       608 |       1,832 |    2,376 |   2,447 |
| 16-ascii       |    736 |       528 |       1,832 |    1,272 |   1,229 |
| 32-ascii       |    992 |       784 |       2,088 |    1,704 |   2,084 |
| 32-3-byte      |  2,016 |     1,808 |       5,232 |    3,752 |   5,500 |
| 42-ascii       |  1,152 |       944 |       2,216 |    2,120 |   2,728 |
| 43-ascii       |  1,168 |       960 |       2,216 |    2,136 |   2,744 |
| 63-ascii       |  1,488 |     1,280 |       4,656 |    2,712 |   3,776 |
| 64-ascii       |  2,784 |     3,272 |       4,720 |    2,728 |   3,792 |
| 63-last-2-byte |  1,504 |     1,296 |       4,720 |    5,152 |   6,096 |
| 64-last-2-byte |  5,232 |     5,720 |       4,720 |    5,269 |   6,240 |
| 63-last-3-byte |  1,520 |     1,312 |       4,720 |    6,320 |   7,264 |
| 64-last-3-byte |  6,272 |     6,760 |       4,720 |    6,336 |   7,280 |
| 127-ascii      |  4,816 |     5,304 |       9,848 |    4,867 |   7,192 |
| 128-ascii      |  4,832 |     5,320 |       9,912 |    4,912 |   7,208 |
| 256-ascii      |  8,928 |     9,336 |      20,224 |    8,995 |  12,704 |
| 256-3-byte     | 25,312 |    25,800 |      44,872 |   25,392 |  37,376 |
| 511-ascii      | 17,104 |    17,512 |      40,712 |   17,184 |  25,000 |
| 512-ascii      | 17,120 |    17,555 |      40,776 |   17,200 |  23,407 |
| 1024-ascii     | 33,504 |    33,912 |      81,808 |   33,504 |  84,744 |

Reading:

| Shape          |  tafla | tafla-gen | flatbuffers | protobuf | msgpack |
| -------------- | -----: | --------: | ----------: | -------: | ------: |
| empty          |    488 |       592 |         744 |      464 |     136 |
| 8-ascii        |    872 |       976 |       1,384 |    1,232 |     904 |
| 8-one-2-byte   |  1,384 |     1,488 |       1,512 |    1,765 |   1,416 |
| 8-3-byte       |  2,664 |     2,768 |       2,408 |    3,021 |   2,696 |
| 16-two-2-byte  |  1,640 |     1,744 |       1,896 |    2,000 |   1,672 |
| 15-one-4-byte  |  2,792 |     2,896 |       2,664 |    3,216 |   2,824 |
| 16-ascii       |  1,000 |     1,104 |       1,768 |    1,360 |   1,032 |
| 32-ascii       |  1,256 |     1,360 |       2,536 |    1,616 |   1,288 |
| 32-3-byte      |  6,888 |     6,992 |       5,864 |    7,360 |   6,920 |
| 42-ascii       |  1,512 |     1,616 |       3,176 |    1,872 |   1,544 |
| 43-ascii       |  1,512 |     1,616 |       3,176 |    1,872 |   1,544 |
| 63-ascii       |  1,768 |     1,872 |       4,072 |    2,128 |   1,800 |
| 64-ascii       |  1,768 |     1,872 |       4,072 |    2,128 |   1,800 |
| 63-last-2-byte |  3,048 |     3,152 |       4,072 |    3,504 |   3,080 |
| 64-last-2-byte |  3,176 |     3,280 |       4,200 |    3,632 |   3,208 |
| 63-last-3-byte |  6,632 |     6,736 |       6,504 |    7,040 |   6,664 |
| 64-last-3-byte |  6,632 |     6,736 |       6,504 |    7,053 |   6,664 |
| 127-ascii      |  2,792 |     2,896 |       7,144 |    3,152 |   2,824 |
| 128-ascii      |  2,792 |     2,896 |       7,144 |    3,152 |   2,824 |
| 256-ascii      |  4,840 |     4,944 |      13,288 |    5,200 |   4,872 |
| 256-3-byte     | 46,312 |    46,416 |      38,120 |   46,768 |  46,344 |
| 511-ascii      |  8,936 |     9,040 |      25,576 |    9,392 |   8,968 |
| 512-ascii      |  8,936 |     9,040 |      25,576 |    9,392 |   8,968 |
| 1024-ascii     | 17,128 |    17,232 |      50,152 |   17,584 |  17,160 |

`./gradlew :benchmarks:encodingReport` printed the encoded sizes in bytes:

| Shape          |  tafla | tafla-gen | flatbuffers | protobuf | msgpack |
| -------------- | -----: | --------: | ----------: | -------: | ------: |
| empty          |    128 |       128 |         216 |       32 |      19 |
| 8-ascii        |    256 |       256 |         344 |      160 |     147 |
| 8-one-2-byte   |    272 |       272 |         344 |      176 |     163 |
| 8-3-byte       |    512 |       512 |         600 |      416 |     403 |
| 16-two-2-byte  |    416 |       416 |         472 |      320 |     307 |
| 15-one-4-byte  |    416 |       416 |         472 |      320 |     307 |
| 16-ascii       |    384 |       384 |         472 |      288 |     275 |
| 32-ascii       |    640 |       640 |         728 |      544 |     547 |
| 32-3-byte      |  1,664 |     1,664 |       1,752 |    1,568 |   1,571 |
| 42-ascii       |    800 |       800 |         856 |      704 |     707 |
| 43-ascii       |    816 |       816 |         856 |      720 |     723 |
| 63-ascii       |  1,136 |     1,136 |       1,176 |    1,040 |   1,043 |
| 64-ascii       |  1,152 |     1,152 |       1,240 |    1,056 |   1,059 |
| 63-last-2-byte |  1,152 |     1,152 |       1,240 |    1,056 |   1,059 |
| 64-last-2-byte |  1,168 |     1,168 |       1,240 |    1,072 |   1,075 |
| 63-last-3-byte |  1,168 |     1,168 |       1,240 |    1,072 |   1,075 |
| 64-last-3-byte |  1,184 |     1,184 |       1,240 |    1,088 |   1,091 |
| 127-ascii      |  2,160 |     2,160 |       2,200 |    2,064 |   2,067 |
| 128-ascii      |  2,176 |     2,176 |       2,264 |    2,096 |   2,083 |
| 256-ascii      |  4,224 |     4,224 |       4,312 |    4,144 |   4,147 |
| 256-3-byte     | 12,416 |    12,416 |      12,504 |   12,336 |  12,339 |
| 511-ascii      |  8,304 |     8,304 |       8,344 |    8,224 |   8,227 |
| 512-ascii      |  8,320 |     8,320 |       8,408 |    8,240 |   8,243 |
| 1024-ascii     | 16,512 |    16,512 |      16,600 |   16,432 |  16,435 |

tafla's table holds each string's offset and length in 4 bytes each, protobuf
a tag and a varint of the length, and msgpack a header of 1 to 3 bytes.

## Lists of strings

`StringListsBenchmark` compares how the code that tafla-gen generates writes
and reads a list of strings as a vector of strings, `[string]`, and as a
vector of tables of one string each, `[Name]`. It's the counterpart of
`yarn bench:lists` in `typescript/benchmarks/`, with three of its eight kinds
of strings. Each table holds one list of 10, 100 or 10,000 distinct strings,
which the benchmark's `count` parameter takes, of one kind, which its `kind`
parameter takes:

- `16-ascii`: 16 characters of ASCII
- `256-ascii`: 256 characters of ASCII, which tafla encodes with
  `String.getBytes` rather than a `char` at a time, as the strings suite
  describes
- `8-3-byte`: 8 characters of 3 bytes, CJK ideographs

These are the TypeScript suite's strings of the same names, such as
`10,000 × 16 chars of ASCII`. `ListShapes` generates them with its
generator, seeds and alphabet, as `StringShapes` does the strings suite's,
and `ListShapesTest` checks them against values that the TypeScript
generator printed.

The `codec` parameter takes the variants, which the code that tafla-gen
generates from `src/main/tafla/lists.tafla`, a copy of the TypeScript
suite's schema, writes and reads, in the package
`io.github.snovader.tafla.benchmarks.generated.lists`:

- `string-vector`: `StringList`, a vector of strings, whose items are the
  strings' UTF-8. Its `TableSizer` counts the strings of fewer than 64
  `char`s and encodes the longer ones, whose bytes its `TableWriter` copies,
  as it writes string fields. It reads every string through its getter's
  `StringVector`, whose iterator reads each string's end once, and the last
  string with the view's `get()`, from its two ends. It writes the bytes of
  `TableBuilder.setStringVector()`, which the round-trip test checks.
- `name-vector`: `NameList`, a vector of tables of one string each, which is
  how a schema holds a list of strings without a vector of strings. It
  creates the generated `Name.Fields` of each string in each call, and reads
  each string through the reader of its table that the view returns.
- `string-vector-aa`: the code of `string-vector` again, an A/A pair, whose
  times differ from it by the measurement's noise. JMH runs each variant in
  forks of its own.

Each list is serialized, deserialized into an array of its strings that
`Arrays.asList()` wraps, and read for its last string, without reading the
others: the benchmarks `serialize`, `deserialize` and `readLast`. Before
timing, every variant must read back every string, and the last string,
which the round-trip test checks too. JMH times the suite as it times the
others, as Method describes.

```sh
uptime
./gradlew :benchmarks:jmh --jmhArgs="StringLists"
```

The run of its 81 benchmarks takes about 30 minutes. `-pcount=10,100`
selects counts, and `-pkind=16-ascii,8-3-byte` kinds, one or the other, since
the JMH plugin keeps only the last `-p` of `--jmhArgs`.

### Limitations

- It doesn't compare the other libraries' lists, as the TypeScript suite
  does; the strings suite compares how they write and read each string.
- Java decodes each string with `new String(bytes, UTF_8)`, on its own, so
  `TableReader.getStringVector()` reads every string as the view's iterator
  does, and there's no counterpart of the TypeScript suite's
  `tafla [string] each`. TypeScript's `getStringVector()` decodes runs of
  ASCII at once, which saves calls of its `TextDecoder`.
- JMH doesn't interleave the variants, as Method describes, so the A/A pair
  shows how much load moved the times of forks run minutes apart.

### Results

One full run on 2026-11-27, on an Apple M2 Pro with 32 GB in macOS 27.0.1,
with Oracle's JDK 25.0.1, through the machine's benchmark lock, of 3 forks
of 5 warm-up and 2 measured iterations of 1 second each. It took 29
minutes, after a 1-minute load average of 1.41 on the 10 cores when it
started. JMH's mean was within 5% of the fastest iteration for 65 of the 81
benchmarks; most of the others had a fork up to 42% slower than the rest,
as forks compile the same code differently. The A/A pair's fastest
iterations agreed within 5%, but for one read of the last of 10,000
strings, at 1.07 times.

Fastest iteration in µs per operation, and its ratio to the vector of
strings':

| Benchmark                                           |    string-vector |      name-vector | string-vector-aa |
| --------------------------------------------------- | ---------------: | ---------------: | ---------------: |
| StringLists: deserialize count=10 kind=16-ascii     |            0.117 |    0.125 (1.07×) |    0.117 (1.01×) |
| StringLists: deserialize count=10 kind=256-ascii    |            0.196 |    0.213 (1.09×) |    0.195 (0.99×) |
| StringLists: deserialize count=10 kind=8-3-byte     |            0.733 |    0.769 (1.05×) |    0.739 (1.01×) |
| StringLists: deserialize count=100 kind=16-ascii    |            0.995 |    1.083 (1.09×) |    1.001 (1.01×) |
| StringLists: deserialize count=100 kind=256-ascii   |            1.881 |    2.012 (1.07×) |    1.884 (1.00×) |
| StringLists: deserialize count=100 kind=8-3-byte    |            4.027 |    4.230 (1.05×) |    3.844 (0.95×) |
| StringLists: deserialize count=10000 kind=16-ascii  |            104.3 |    122.2 (1.17×) |    104.5 (1.00×) |
| StringLists: deserialize count=10000 kind=256-ascii |            184.9 |    198.6 (1.07×) |    183.6 (0.99×) |
| StringLists: deserialize count=10000 kind=8-3-byte  |            756.8 |    802.7 (1.06×) |    750.0 (0.99×) |
| StringLists: readLast count=10 kind=16-ascii        |           0.0118 |   0.0124 (1.05×) |   0.0119 (1.01×) |
| StringLists: readLast count=10 kind=256-ascii       |           0.0201 |   0.0210 (1.05×) |   0.0201 (1.00×) |
| StringLists: readLast count=10 kind=8-3-byte        |           0.0801 |   0.0825 (1.03×) |   0.0832 (1.04×) |
| StringLists: readLast count=100 kind=16-ascii       |           0.0117 |   0.0126 (1.08×) |   0.0119 (1.01×) |
| StringLists: readLast count=100 kind=256-ascii      |           0.0203 |   0.0216 (1.07×) |   0.0205 (1.01×) |
| StringLists: readLast count=100 kind=8-3-byte       |           0.0803 |   0.0830 (1.03×) |   0.0771 (0.96×) |
| StringLists: readLast count=10000 kind=16-ascii     |           0.0118 |   0.0124 (1.05×) |   0.0118 (1.00×) |
| StringLists: readLast count=10000 kind=256-ascii    |           0.0205 |   0.0216 (1.06×) |   0.0204 (1.00×) |
| StringLists: readLast count=10000 kind=8-3-byte     |           0.0791 |   0.0825 (1.04×) |   0.0846 (1.07×) |
| StringLists: serialize count=10 kind=16-ascii       |            0.384 |    0.463 (1.21×) |    0.390 (1.01×) |
| StringLists: serialize count=10 kind=256-ascii      |            2.317 |    3.879 (1.67×) |    2.337 (1.01×) |
| StringLists: serialize count=10 kind=8-3-byte       |            0.205 |    0.266 (1.30×) |    0.204 (1.00×) |
| StringLists: serialize count=100 kind=16-ascii      |            3.688 |    4.033 (1.09×) |    3.638 (0.99×) |
| StringLists: serialize count=100 kind=256-ascii     |             23.1 |     24.7 (1.07×) |     23.5 (1.02×) |
| StringLists: serialize count=100 kind=8-3-byte      |            1.798 |    2.512 (1.40×) |    1.808 (1.01×) |
| StringLists: serialize count=10000 kind=16-ascii    |            373.1 |    427.8 (1.15×) |    378.2 (1.01×) |
| StringLists: serialize count=10000 kind=256-ascii   |           2343.0 |   2495.2 (1.06×) |   2417.3 (1.03×) |
| StringLists: serialize count=10000 kind=8-3-byte    |            197.5 |    270.1 (1.37×) |    203.5 (1.03×) |

Bytes allocated per operation, the mean of the measured iterations:

| Benchmark                                           |    string-vector |      name-vector | string-vector-aa |
| --------------------------------------------------- | ---------------: | ---------------: | ---------------: |
| StringLists: deserialize count=10 kind=16-ascii     |              640 |              640 |              640 |
| StringLists: deserialize count=10 kind=256-ascii    |            3,040 |            3,040 |            3,040 |
| StringLists: deserialize count=10 kind=8-3-byte     |            1,680 |            1,680 |            1,699 |
| StringLists: deserialize count=100 kind=16-ascii    |            6,040 |            6,040 |            6,040 |
| StringLists: deserialize count=100 kind=256-ascii   |           30,040 |           30,040 |           30,040 |
| StringLists: deserialize count=100 kind=8-3-byte    |           16,469 |           16,469 |           16,440 |
| StringLists: deserialize count=10000 kind=16-ascii  |          600,129 |          600,129 |          600,129 |
| StringLists: deserialize count=10000 kind=256-ascii |        3,000,129 |        3,000,129 |        3,000,129 |
| StringLists: deserialize count=10000 kind=8-3-byte  |        1,640,133 |        1,640,134 |        1,640,133 |
| StringLists: readLast count=10 kind=16-ascii        |               56 |               56 |               56 |
| StringLists: readLast count=10 kind=256-ascii       |              296 |              296 |              296 |
| StringLists: readLast count=10 kind=8-3-byte        |              160 |              160 |              160 |
| StringLists: readLast count=100 kind=16-ascii       |               56 |               56 |               56 |
| StringLists: readLast count=100 kind=256-ascii      |              296 |              296 |              296 |
| StringLists: readLast count=100 kind=8-3-byte       |              160 |              160 |              160 |
| StringLists: readLast count=10000 kind=16-ascii     |               56 |               56 |               56 |
| StringLists: readLast count=10000 kind=256-ascii    |              296 |              296 |              296 |
| StringLists: readLast count=10000 kind=8-3-byte     |              160 |              160 |              160 |
| StringLists: serialize count=10 kind=16-ascii       |              224 |              544 |              224 |
| StringLists: serialize count=10 kind=256-ascii      |            5,664 |            5,984 |            5,664 |
| StringLists: serialize count=10 kind=8-3-byte       |              304 |              624 |              304 |
| StringLists: serialize count=100 kind=16-ascii      |            2,024 |            4,864 |            2,024 |
| StringLists: serialize count=100 kind=256-ascii     |           57,072 |           59,902 |           57,072 |
| StringLists: serialize count=100 kind=8-3-byte      |            2,824 |            5,664 |            2,824 |
| StringLists: serialize count=10000 kind=16-ascii    |          200,075 |          480,163 |          200,075 |
| StringLists: serialize count=10000 kind=256-ascii   |        5,729,121 |        6,009,162 |        5,729,122 |
| StringLists: serialize count=10000 kind=8-3-byte    |          280,073 |          560,162 |          280,074 |

- The vector of strings is written in 0.60–0.94 times the time of the
  vector of tables, allocating the table alone, where the vector of tables
  allocates its fields too. Strings of 64 `char`s and more are encoded
  once, before the table is written, which both layouts do, so lists of
  256 characters allocate their strings' UTF-8 twice.
- Every string is read in 0.85–0.95 times the time, each decoded on its own
  in either layout, as the JDK decodes it.
- The last string is found in 0.93–0.97 times the time.
