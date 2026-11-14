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
`./gradlew :benchmarks:encodingReport` prints each library's encoded sizes and
what it does with a truncated document, as the TypeScript comparison does.

`ItemAccessBenchmark` times only tafla's bytes of the vector of nested tables
and of the array of structs, read in two ways: the last item alone, which a
reader can find without reading the others, and every item. Each way sums the
three float32s of the items it reads as doubles, rather than creating objects
of its own, and `ItemAccesses` holds them:

- **tafla** reads with `ByteBuffer`'s getters by hand: `getVectorOffset()` of
  the last item or `getVectorOffsets()` of every item, and `getStructArray()`'s
  view with `getFloat()`.
- **tafla-gen** reads with the generated getters, `items()`, which return
  views, lazy `List`s whose `get(i)` reads one item, in a loop of `get(i)` for
  every item.
- **tafla-gen A/A** reads with the getters again, an A/A pair that shows
  JMH's noise.

### What differs from TypeScript

- There are no `number[]` inputs. Java's arrays are typed, and its closest
  counterpart, a `List<Float>` of boxed values, isn't what a codec is given.
- Java's tafla has no `StructWriter`, and its package documentation leaves
  records at fixed offsets to `MethodHandles.byteArrayViewVarHandle`. So the
  tafla codec writes the fixed-size record through little-endian `VarHandle`s,
  as tafla-gen's `Sample` does, and reads it with `ByteBuffer`'s getters at
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
  `getStructArray()`'s little-endian view, as tafla-gen's code does.
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

The full run takes about 33 minutes. `--jmhArgs` passes JMH's own options,
which override those in `build.gradle.kts`: a regular expression selects
benchmarks, and `-p` sets a parameter.

```sh
./gradlew :benchmarks:jmh --jmhArgs="Document -p codec=tafla"
```

A quick run of one fork, with one warm-up and one measured iteration of
100 ms, checks that every benchmark works in about 30 seconds. CI runs it after
the round-trip test, on Java 25. Its numbers mean little.

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
- The fixed-size record takes 2 to 3 ns with tafla's `ByteBuffer`, and 6 ns
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
  2.1 and 1.8 ns, against 2.2 and 2.4 ns with `ByteBuffer`'s setters, which
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

Fastest iteration in µs per operation, and its ratio to `ByteBuffer`'s by
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

- The views read the vector's last item as fast as `ByteBuffer` by hand,
  0.0061 µs against 0.0062 µs, and the array's in 0.0042 µs against
  0.0034 µs, 64 and 45 times faster than the getters, which read all 100
  items first.
- Through the views, nothing is allocated: C2's escape analysis removes each
  item's reader or record, which a view returns to the loop rather than
  stores, as the getters do in their `List` or array. Summing the vector takes
  0.52 µs against 0.89 µs through the getter and 0.40 µs by hand, and the
  array 0.070 µs against 0.242 µs and 0.066 µs.
