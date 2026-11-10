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
and of the array of structs, read in three ways: the last item alone, which a
reader can find without reading the others, and every item. Each way sums the
three float32s of the items it reads as doubles, rather than creating objects
of its own, and `ItemAccesses` holds them:

- **tafla** reads with `ByteBuffer`'s getters by hand: `getVectorOffset()` of
  the last item or `getVectorOffsets()` of every item, and `getStructArray()`'s
  view with `getFloat()`.
- **tafla-gen** reads with the generated getters, `items()`, which read every
  item into a `List` or an array first.
- **tafla-gen view** reads with the generated views, `itemsView()`, lazy
  `List`s whose `get(i)` reads one item, in a loop of `get(i)` for every item.
- **tafla-gen view A/A** reads with the views again, an A/A pair that shows
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

- **tafla** nests tables from their builders. Arrays of the elements' own Java
  type are copied in bulk, and the uint32 keys are narrowed and widened element
  by element. The array of structs is written into a `byte[]` through a
  little-endian `VarHandle` and set with `setStructArray()`, and read from
  `getStructArray()`'s little-endian view, as tafla-gen's code does.
- **tafla-gen** is the code that tafla-gen generates from
  `src/main/tafla/workloads.tafla`, in the package
  `io.github.snovader.tafla.benchmarks.generated.tafla`. A message's
  `serialize()` counts the table's length with a `TableSizer`, and then
  writes every field, nested tables included, into an array of that length
  with a `TableWriter`. Its readers read each field with a getter. Its
  getters of arrays return copies, so it sums a column through the column's
  view, `aView()`, which reads the elements in place, as the other
  languages' generated getters return them. It
  takes the workloads' records, as the other codecs do, so it creates the
  generated `Fields` records, and the `Triple` and `Sample` records, from them
  in each call, as the protobuf codec builds its messages, and creates the
  workloads' records from the generated readers. Every suite has a copy of the
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

### What the codecs don't do alike

- **msgpack's vector items are flat.** Each item packs as one map of its
  three floats, as msgpackr packs it, where tafla, FlatBuffers and protobuf
  nest the item's pair as a table or message of its own. Its packer also
  reuses its buffer from message to message, where FlatBuffers starts a new
  builder. The flat items make msgpack's vector cheaper to write and read,
  and the reused buffer cheaper to write.
- **The tafla-gen codec converts records both ways.** It creates the
  generated `Fields` records from the workload's records before writing, and
  the workload's records from the generated readers after reading, which an
  application that held the generated types wouldn't. That costs most for the
  vector and the array of structs, with a record for each of their 100 items.
- **The struct reads depend on how C2 compiles JMH's loop.** tafla-gen
  reads the array of structs in 0.53 µs as the suite runs it, against SBE's
  0.39 µs. With each codec's `deserialize()` kept out of JMH's loop, they
  took 0.38 and 0.33 µs, so about two thirds of that gap comes from the loop,
  as the results describe.
- **The suite doesn't time `serializeInto()`.** The tafla-gen codec writes
  each message into new bytes with `serialize()`, as the other codecs write
  theirs. Writing into bytes that the caller reuses, with the generated
  `serializeInto()`, took 34% less time for the large document, 27% for the
  small one and 17% for the record, as `TableSizer.writer(byte[])`'s
  documentation reports.

## Method

JMH 1.37 times each benchmark in average-time mode. Every codec of every
benchmark runs in 3 forked JVMs, each of 5 warm-up and 5 measured iterations
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
another's. Three forks spread each codec over about half a minute, which makes
that less likely, and a fork's fastest iteration is unaffected unless the load
lasts the whole fork. Check `uptime` before a run, and compare the fastest
iterations with JMH's means.

The results below interleave the codecs. Each fork ran on its own, from the
jar that `./gradlew :benchmarks:jmhJar` builds, with the options above and
`-f 1 -p codec=<codec>` for one benchmark class, in three rounds. In each
round, every class ran one fork of each codec, in the opposite order to the
round before, and two of tafla's, first and last, as an A/A pair that shows
how far two forks of the same code differ. A fork started only once other
processes used less than 3 of the 10 cores, and was rerun when they used
more than 4 during it.

## Running

```sh
uptime
./gradlew :benchmarks:jmh
```

The full run takes about 45 minutes. `--jmhArgs` passes JMH's own options,
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

- `./gradlew :benchmarks:jmh` doesn't interleave the codecs, as above. The
  results below do, by running each fork on its own.
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

One full run on 2026-11-10, on an Apple M2 Pro with 32 GB in macOS 27.0.1,
with Oracle's JDK 25.0.1 and JMH 1.37, in three rounds of a fork per codec,
as the method describes. The suite took 63 minutes. Other processes kept the
1-minute load average between 1.2 and 3.9 on the 10 cores, with a median of
1.8. Four runs, during which another application used 6 cores for a few
seconds, were rerun, and other processes used at most 2.9 cores during the
runs kept. The mean of each codec's 15 iterations was within 5% of its
fastest iteration for 80 of the 90 benchmarks, and at most 18% above it,
where msgpack wrote the fixed-size record in 77 to 88 ns in each fork's first
measured iteration and in 85 to 108 ns after it. tafla's A/A pair's fastest
iterations were within 1.2% of each other in every benchmark.

Fastest iteration in µs per operation, and its ratio to tafla's:

| Benchmark                                          |  tafla |    flatbuffers |        protobuf |         msgpack |            sbe |
| -------------------------------------------------- | -----: | -------------: | --------------: | --------------: | -------------: |
| record: serialize                                  | 0.0375 |  0.144 (3.85×) |  0.0296 (0.79×) |  0.0804 (2.15×) |                |
| record: deserialize                                | 0.0138 | 0.0193 (1.40×) |  0.0310 (2.24×) |   0.136 (9.79×) |                |
| document of 100 rows: serialize                    |  0.155 |  0.818 (5.28×) |   1.191 (7.69×) |   0.821 (5.30×) |                |
| document of 100 rows: deserialize                  | 0.0752 | 0.0813 (1.08×) |  0.848 (11.27×) |  1.360 (18.09×) |                |
| document of 100 rows: sum a column per element     | 0.0408 | 0.0567 (1.39×) |  0.860 (21.08×) |  0.732 (17.95×) |                |
| document of 100,000 rows: serialize                |   41.3 | 757.5 (18.33×) | 1751.8 (42.39×) |  694.0 (16.80×) |                |
| document of 100,000 rows: deserialize              |   47.8 |   49.6 (1.04×) | 1012.9 (21.18×) | 1054.0 (22.04×) |                |
| document of 100,000 rows: sum a column per element |   85.9 |   85.9 (1.00×) | 1020.2 (11.87×) |   611.9 (7.12×) |                |
| vector of 100 nested tables: serialize             |  4.551 |  4.876 (1.07×) |   1.657 (0.36×) |   3.416 (0.75×) |                |
| vector of 100 nested tables: deserialize           |  0.536 |  0.890 (1.66×) |   3.859 (7.20×) |  7.258 (13.53×) |                |
| array of 100 structs: serialize                    |  0.145 |  0.483 (3.34×) |   0.921 (6.37×) |   0.691 (4.78×) |  0.217 (1.50×) |
| array of 100 structs: deserialize                  |  0.202 |  0.223 (1.11×) |  2.483 (12.30×) |   1.490 (7.38×) |  0.387 (1.92×) |
| fixed-size record: serialize                       | 0.0019 | 0.130 (67.38×) |  0.0049 (2.52×) | 0.0775 (40.16×) | 0.0062 (3.23×) |
| fixed-size record: deserialize                     | 0.0024 | 0.0035 (1.46×) |  0.0134 (5.64×) | 0.0911 (38.44×) | 0.0058 (2.47×) |

- tafla reads every shape fastest. FlatBuffers reads documents within 10% of
  it, since both copy the float columns in bulk and widen the keys one by
  one.
- tafla writes documents 5.3 to 42 times faster than the others, whose
  columns are written element by element.
- protobuf and msgpack write the vector of 100 nested tables faster than
  tafla, in 1.7 and 3.4 µs against 4.6 µs, and protobuf writes the record
  faster too, in 0.030 µs against 0.038 µs. msgpack's items are flat maps, as
  above.
- The sums of 100,000 floats take 86 µs through tafla's and FlatBuffers'
  accessors alike, bound by the additions; the 100-row sums differ.
- tafla writes the 100 structs in 0.145 µs, against 4.6 µs for the same items
  as nested tables, and reads them in 0.202 µs, against 0.536 µs. Reading
  creates the 100 `ItemInput` records, which every codec does.
- FlatBuffers reads its vector of structs nearly as fast, in 0.223 µs, and
  writes it in 0.483 µs. SBE writes its repeating group in 0.217 µs and reads
  it in 0.387 µs. protobuf and msgpack write and parse each struct's fields:
  0.7 to 0.9 µs to write and 1.5 to 2.5 µs to read.
- The tafla codec writes the fixed-size record through `VarHandle`s in 1.9 ns
  and reads it with `ByteBuffer`'s getters in 2.4 ns, against 6 ns with SBE,
  which writes and checks its 8-byte header too.

The fastest iteration hides differences between forks, most of all
protobuf's. It read the small document in 0.85 µs in one fork and in 0.99 to
1.00 µs in the other two, each steady within its fork, and its forks' fastest
writes of the vector took 1.66, 1.80 and 1.86 µs.

Fastest iteration in µs per operation, and tafla-gen's ratio to tafla's:

| Benchmark                                          |  tafla |      tafla-gen |
| -------------------------------------------------- | -----: | -------------: |
| record: serialize                                  | 0.0375 | 0.0174 (0.46×) |
| record: deserialize                                | 0.0138 | 0.0142 (1.03×) |
| document of 100 rows: serialize                    |  0.155 | 0.0812 (0.52×) |
| document of 100 rows: deserialize                  | 0.0752 | 0.0752 (1.00×) |
| document of 100 rows: sum a column per element     | 0.0408 | 0.0417 (1.02×) |
| document of 100,000 rows: serialize                |   41.3 |   43.4 (1.05×) |
| document of 100,000 rows: deserialize              |   47.8 |   47.7 (1.00×) |
| document of 100,000 rows: sum a column per element |   85.9 |   86.0 (1.00×) |
| vector of 100 nested tables: serialize             |  4.551 |  0.885 (0.19×) |
| vector of 100 nested tables: deserialize           |  0.536 |  1.147 (2.14×) |
| array of 100 structs: serialize                    |  0.145 |  0.337 (2.33×) |
| array of 100 structs: deserialize                  |  0.202 |  0.527 (2.61×) |
| fixed-size record: serialize                       | 0.0019 | 0.0020 (1.05×) |
| fixed-size record: deserialize                     | 0.0024 | 0.0018 (0.78×) |

Bytes allocated per operation, the mean of the measured iterations:

| Benchmark                                          |     tafla | tafla-gen | flatbuffers |  protobuf |   msgpack |   sbe |
| -------------------------------------------------- | --------: | --------: | ----------: | --------: | --------: | ----: |
| record: serialize                                  |       248 |        80 |       1,360 |       136 |       335 |       |
| record: deserialize                                |        96 |        96 |         144 |        96 |       320 |       |
| document of 100 rows: serialize                    |     1,936 |     1,312 |       4,736 |     6,309 |     3,908 |       |
| document of 100 rows: deserialize                  |     1,760 |     1,760 |       1,808 |     4,595 |     2,128 |       |
| document of 100 rows: sum a column per element     |         0 |         0 |           0 |     2,872 |       320 |       |
| document of 100,000 rows: serialize                | 1,200,512 | 1,200,112 |   5,394,469 | 5,954,414 | 3,013,985 |       |
| document of 100,000 rows: deserialize              | 1,600,160 | 1,600,160 |   1,600,208 | 6,367,043 | 1,600,536 |       |
| document of 100,000 rows: sum a column per element |        89 |       201 |         121 | 4,766,939 |       324 |       |
| vector of 100 nested tables: serialize             |    29,056 |     8,464 |      10,816 |    16,787 |    14,920 |       |
| vector of 100 nested tables: deserialize           |     3,272 |     6,408 |       2,856 |    26,501 |    17,352 |       |
| array of 100 structs: serialize                    |     2,488 |     4,040 |       4,624 |     7,736 |     3,740 | 1,232 |
| array of 100 structs: deserialize                  |     2,856 |     5,672 |       2,856 |    13,680 |     2,888 | 2,856 |
| fixed-size record: serialize                       |        32 |        32 |       1,320 |        32 |       208 |    40 |
| fixed-size record: deserialize                     |        32 |        32 |          32 |        53 |       208 |    32 |

- tafla-gen writes the vector of 100 nested tables in 0.89 µs, against
  4.55 µs with tafla's builders, allocating 8.5 KB against 29 KB. By their
  sizes, about 5 KB of that are the 200 generated `Fields` records and their
  list, which the codec creates from the workload's items, and 3.2 KB the
  table. It writes the record and the small document twice as fast, in 17 and
  81 ns against 37 and 155 ns, allocating only the table's bytes.
- The large document takes 43.4 µs against 41.3 µs, both copying the
  columns' 1.2 MB into the table. tafla-gen's three forks took 43.4 to
  43.8 µs, and tafla's six, with its A/A pair, 41.3 to 41.9 µs.
- The array of structs takes 0.337 µs against 0.145 µs, and allocates 4.0 KB
  against 2.5 KB. The codec creates a `Triple` record for each item, which
  the generated code writes straight into the table with
  `TableWriter.setStructs()`.
- The generated readers read the record and the documents within 3% of the
  hand-written reads. They sum the column through its view, `aView()`, which
  reads the floats in place, within 2% of tafla's `getFloat32()`: 41.7 ns
  against 40.8 ns for 100 rows, and 86.0 µs against 85.9 µs for 100,000.
- The reader of the vector creates an `Item` and a `Pair` for each item, and
  the reader of the array a `Triple` for each struct, before the codec creates
  the workload's records: 1.15 µs against 0.536 µs, and 0.527 µs against
  0.202 µs, with twice the bytes.
- tafla-gen writes and reads the fixed-size record through `VarHandle`s in
  2.02 and 1.85 ns. tafla writes it through `VarHandle`s too, in 1.93 ns, and
  reads it with `ByteBuffer`'s getters in 2.37 ns. Both allocate only the
  12-byte array, or the workload's record, 32 bytes, so escape analysis
  removes the generated `Sample` record.

Both generated reads that create readers, of the vector and of the array,
depend on how C2 compiles the benchmark:

- The array's read took 0.527 to 0.530 µs in all 6 forks as the suite runs
  it: the 3 above, and 3 more of its benchmark alone. With
  `-XX:CompileCommand=dontinline,*TaflaGenCodec$Structs::deserialize`, which
  keeps C2 from inlining the codec's `deserialize()` into JMH's measurement
  loop, 2 of 3 forks took 0.380 µs, and one 0.538 µs. SBE's read, kept out of
  line the same way, took 0.331 to 0.336 µs in its 3 forks, against 0.386 to
  0.389 µs in line, and tafla's took 0.203 to 0.204 µs, as in line. So about
  two thirds of tafla-gen's gap to SBE, 0.14 µs in line and 0.05 µs out of
  line, depends on how C2 compiles JMH's loop around the same code. The rest
  is tafla-gen's, whose reader creates a `Triple` for each struct before the
  codec creates the workload's records, with twice SBE's bytes.
- The vector's read took 1.15 to 1.19 µs in this run's three forks. In
  earlier runs, two of three forks took 1.30 µs, and C2's compilation log
  showed why: in the slower forks, C2 had compiled the generated getter on its
  own first, into too much machine code to inline into the codec's read.

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

`ItemAccessBenchmark`'s results, from the same run, in which its four
variants ran in the suite's rounds as its codecs. The mean of each variant's
15 iterations was within 1.2% of its fastest for all 16 benchmarks.

Fastest iteration in µs per operation, and its ratio to `ByteBuffer`'s by
hand:

| Benchmark                    |  tafla |      tafla-gen | tafla-gen view | tafla-gen view A/A |
| ---------------------------- | -----: | -------------: | -------------: | -----------------: |
| ItemAccess: vectorLastItem   | 0.0064 | 0.389 (61.17×) | 0.0061 (0.97×) |     0.0061 (0.97×) |
| ItemAccess: vectorEveryItem  |  0.405 |  0.922 (2.28×) |  0.491 (1.21×) |      0.491 (1.21×) |
| ItemAccess: structsLastItem  | 0.0036 | 0.189 (52.75×) | 0.0041 (1.15×) |     0.0041 (1.15×) |
| ItemAccess: structsEveryItem | 0.0661 |  0.362 (5.48×) | 0.0696 (1.05×) |     0.0695 (1.05×) |

Bytes allocated per operation:

| Benchmark                    | tafla | tafla-gen | tafla-gen view | tafla-gen view A/A |
| ---------------------------- | ----: | --------: | -------------: | -----------------: |
| ItemAccess: vectorLastItem   |     0 |     3,552 |              0 |                  0 |
| ItemAccess: vectorEveryItem  |   416 |     3,552 |              0 |                  0 |
| ItemAccess: structsLastItem  |     0 |     2,816 |              0 |                  0 |
| ItemAccess: structsEveryItem |     0 |     2,816 |              0 |                  0 |

- The views read the vector's last item as fast as `ByteBuffer` by hand, in
  0.0061 µs against 0.0064 µs, and the array's in 0.0041 µs against
  0.0036 µs, 64 and 46 times faster than the getters, which read all 100
  items first.
- Through the views, nothing is allocated: C2's escape analysis removes each
  item's reader or record, which a view returns to the loop rather than
  stores, as the getters do in their `List` or array. Summing the vector takes
  0.49 µs against 0.92 µs through the getter and 0.41 µs by hand, and the
  array 0.070 µs against 0.362 µs and 0.066 µs. The vector's view reads the
  vector's count once, with `getVectorItemOffset()`, rather than for each
  item.
- The getter's sum of the array took 0.362 to 0.363 µs in every fork of this
  run, and has taken from 0.24 to 0.40 µs in other runs, varying between
  forks as the codec's reads above do.
