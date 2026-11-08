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

One full run on 2026-11-08, on an Apple M2 Pro with 32 GB in macOS 27.0.1,
with Oracle's JDK 25.0.1 and JMH 1.37. It took 46 minutes. Other processes
kept the 1-minute load average between 1.3 and 3.9 on the 10 cores, with a
median of 2.0, except for a burst of about a minute, 39 minutes in, that
reached 9.2. JMH's mean was within 5% of the fastest iteration for 80 of the
90 benchmarks, and at most 16% above it, where one of protobuf's three forks
read the small document in 0.829 µs and the others in 1.01 to 1.02 µs.

Fastest iteration in µs per operation, and its ratio to tafla's:

| Benchmark                                          |  tafla |    flatbuffers |        protobuf |         msgpack |            sbe |
| -------------------------------------------------- | -----: | -------------: | --------------: | --------------: | -------------: |
| record: serialize                                  | 0.0377 |  0.145 (3.86×) |  0.0300 (0.80×) |  0.0808 (2.14×) |                |
| record: deserialize                                | 0.0138 | 0.0195 (1.41×) |  0.0334 (2.42×) |   0.136 (9.82×) |                |
| document of 100 rows: serialize                    |  0.155 |  0.803 (5.18×) |   1.358 (8.76×) |   0.831 (5.36×) |                |
| document of 100 rows: deserialize                  | 0.0733 | 0.0806 (1.10×) |  0.829 (11.31×) |  1.456 (19.87×) |                |
| document of 100 rows: sum a column per element     | 0.0410 | 0.0555 (1.36×) |  0.821 (20.06×) |  0.728 (17.78×) |                |
| document of 100,000 rows: serialize                |   40.8 | 792.2 (19.44×) | 1752.5 (43.00×) |  693.8 (17.02×) |                |
| document of 100,000 rows: deserialize              |   47.8 |   47.9 (1.00×) | 1029.6 (21.56×) |  937.8 (19.63×) |                |
| document of 100,000 rows: sum a column per element |   86.1 |   86.3 (1.00×) | 1029.0 (11.95×) |   617.3 (7.17×) |                |
| vector of 100 nested tables: serialize             |  4.588 |  4.895 (1.07×) |   1.789 (0.39×) |   3.404 (0.74×) |                |
| vector of 100 nested tables: deserialize           |  0.538 |  0.885 (1.64×) |   3.592 (6.67×) |  7.563 (14.05×) |                |
| array of 100 structs: serialize                    |  0.148 |  0.483 (3.26×) |   0.924 (6.23×) |   0.693 (4.68×) |  0.220 (1.49×) |
| array of 100 structs: deserialize                  |  0.202 |  0.223 (1.10×) |  2.498 (12.36×) |   1.496 (7.40×) |  0.388 (1.92×) |
| fixed-size record: serialize                       | 0.0020 | 0.130 (66.55×) |  0.0049 (2.49×) | 0.0696 (35.59×) | 0.0063 (3.20×) |
| fixed-size record: deserialize                     | 0.0024 | 0.0035 (1.44×) |  0.0131 (5.44×) | 0.0927 (38.50×) | 0.0059 (2.45×) |

- tafla reads every shape fastest. FlatBuffers reads documents within 10% of
  it, since both copy the float columns in bulk and widen the keys one by
  one.
- tafla writes documents 5.2 to 43 times faster than the others, whose
  columns are written element by element.
- protobuf and msgpack write the vector of 100 nested tables faster than
  tafla, in 1.8 and 3.4 µs against 4.6 µs, and protobuf writes the record
  faster too, in 0.030 µs against 0.038 µs.
- The sums of 100,000 floats take 86 µs through tafla's and FlatBuffers'
  accessors alike, bound by the additions; the 100-row sums differ.
- tafla writes the 100 structs in 0.148 µs, against 4.6 µs for the same items
  as nested tables, and reads them in 0.202 µs, against 0.538 µs. Reading
  creates the 100 `ItemInput` records, which every codec does.
- FlatBuffers reads its vector of structs nearly as fast, in 0.223 µs, and
  writes it in 0.483 µs. SBE writes its repeating group in 0.220 µs and reads
  it in 0.388 µs. protobuf and msgpack write and parse each struct's fields:
  0.7 to 0.9 µs to write and 1.5 to 2.5 µs to read.
- The tafla codec writes the fixed-size record through `VarHandle`s in 2.0 ns
  and reads it with `ByteBuffer`'s getters in 2.4 ns, against 6 ns with SBE,
  which writes and checks its 8-byte header too.

The fastest iteration hides differences between forks. msgpack read the large
document in 1,090 to 1,106 µs in one fork, and in 938 to 1,012 µs in the other
two, with one or two garbage collections in every iteration.

Fastest iteration in µs per operation, and tafla-gen's ratio to tafla's:

| Benchmark                                          |  tafla |      tafla-gen |
| -------------------------------------------------- | -----: | -------------: |
| record: serialize                                  | 0.0377 | 0.0175 (0.46×) |
| record: deserialize                                | 0.0138 | 0.0143 (1.03×) |
| document of 100 rows: serialize                    |  0.155 | 0.0830 (0.53×) |
| document of 100 rows: deserialize                  | 0.0733 | 0.0751 (1.02×) |
| document of 100 rows: sum a column per element     | 0.0410 | 0.0408 (1.00×) |
| document of 100,000 rows: serialize                |   40.8 |   43.8 (1.08×) |
| document of 100,000 rows: deserialize              |   47.8 |   47.9 (1.00×) |
| document of 100,000 rows: sum a column per element |   86.1 |   86.4 (1.00×) |
| vector of 100 nested tables: serialize             |  4.588 |  0.882 (0.19×) |
| vector of 100 nested tables: deserialize           |  0.538 |  1.140 (2.12×) |
| array of 100 structs: serialize                    |  0.148 |  0.333 (2.25×) |
| array of 100 structs: deserialize                  |  0.202 |  0.527 (2.61×) |
| fixed-size record: serialize                       | 0.0020 | 0.0020 (1.05×) |
| fixed-size record: deserialize                     | 0.0024 | 0.0019 (0.78×) |

Bytes allocated per operation, the mean of the measured iterations:

| Benchmark                                          |     tafla | tafla-gen | flatbuffers |  protobuf |   msgpack |   sbe |
| -------------------------------------------------- | --------: | --------: | ----------: | --------: | --------: | ----: |
| record: serialize                                  |       248 |        80 |       1,360 |       136 |       335 |       |
| record: deserialize                                |        96 |        96 |         144 |        96 |       320 |       |
| document of 100 rows: serialize                    |     1,936 |     1,312 |       4,736 |     6,328 |     3,908 |       |
| document of 100 rows: deserialize                  |     1,760 |     1,760 |       1,808 |     4,613 |     2,128 |       |
| document of 100 rows: sum a column per element     |         0 |         0 |           0 |     2,872 |       320 |       |
| document of 100,000 rows: serialize                | 1,200,512 | 1,200,112 |   5,394,470 | 5,954,412 | 3,013,985 |       |
| document of 100,000 rows: deserialize              | 1,600,160 | 1,600,160 |   1,600,208 | 6,367,043 | 1,600,535 |       |
| document of 100,000 rows: sum a column per element |        89 |       201 |         121 | 4,766,939 |       324 |       |
| vector of 100 nested tables: serialize             |    29,056 |     8,464 |      10,816 |    16,800 |    14,920 |       |
| vector of 100 nested tables: deserialize           |     3,272 |     6,408 |       2,856 |    26,501 |    17,352 |       |
| array of 100 structs: serialize                    |     2,488 |     4,040 |       4,624 |     7,736 |     3,740 | 1,232 |
| array of 100 structs: deserialize                  |     2,856 |     5,672 |       2,856 |    13,701 |     2,888 | 2,856 |
| fixed-size record: serialize                       |        32 |        32 |       1,320 |        32 |       208 |    40 |
| fixed-size record: deserialize                     |        32 |        32 |          32 |        53 |       208 |    32 |

- tafla-gen writes the vector of 100 nested tables in 0.88 µs, against
  4.59 µs with tafla's builders, allocating 8.5 KB against 29 KB. By their
  sizes, about 5 KB of that are the 200 generated `Fields` records and their
  list, which the codec creates from the workload's items, and 3.2 KB the
  table. It writes the record and the small document twice as fast, in 18 and
  83 ns against 38 and 155 ns, allocating only the table's bytes.
- The large document takes 43.8 µs against 40.8 µs, both copying the
  columns' 1.2 MB into the table. tafla's other two forks took 42.2 and
  42.3 µs.
- The array of structs takes 0.333 µs against 0.148 µs, and allocates 4.0 KB
  against 2.5 KB. The codec creates a `Triple` record for each item, which
  the generated code writes straight into the table with
  `TableWriter.setStructs()`.
- The generated readers read the record and the documents within 3% of the
  hand-written reads. They sum the column through its view, `aView()`, which
  reads the floats in place, as fast as tafla's `getFloat32()`: 40.8 ns
  against 41.0 ns for 100 rows, and 86.4 µs against 86.1 µs for 100,000.
- The reader of the vector creates an `Item` and a `Pair` for each item, and
  the reader of the array a `Triple` for each struct, before the codec creates
  the workload's records: 1.14 µs against 0.538 µs, and 0.527 µs against
  0.202 µs, with twice the bytes.
- tafla-gen writes and reads the fixed-size record through `VarHandle`s in
  2.05 and 1.87 ns. tafla writes it through `VarHandle`s too, in 1.95 ns, and
  reads it with `ByteBuffer`'s getters in 2.41 ns. Both allocate only the
  12-byte array, or the workload's record, 32 bytes, so escape analysis
  removes the generated `Sample` record.

Both generated reads that create readers, of the vector and of the array,
depend on how C2 compiles the benchmark:

- The array's read took 0.527 to 0.530 µs in every fork as the suite runs it:
  the 3 above, and 6 more in two runs of its benchmark alone. With
  `-XX:CompileCommand=dontinline,*TaflaGenCodec$Structs::deserialize`, which
  keeps C2 from inlining the codec's `deserialize()` into JMH's measurement
  loop, 5 of 6 forks took 0.376 to 0.378 µs, and one 0.539 µs. So about
  0.15 µs of its time depends on how C2 compiles JMH's loop around the same
  code, rather than on the generated reader. The other codecs weren't timed
  out of line.
- The vector's read took 1.14 µs in one fork and 1.30 µs in the other two.
  Earlier runs varied the same way, and C2's compilation log showed why: in
  the slower forks, C2 had compiled the generated getter on its own first,
  into too much machine code to inline into the codec's read.

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

`ItemAccessBenchmark`'s results, from the same run. JMH's mean was within 5%
of the fastest iteration for 15 of its 16 benchmarks, and 12.5% above it for
the view's read of the vector's last item, whose three forks' fastest
iterations took 0.0061 to 0.0062 µs.

Fastest iteration in µs per operation, and its ratio to `ByteBuffer`'s by
hand:

| Benchmark                    |  tafla |      tafla-gen | tafla-gen view | tafla-gen view A/A |
| ---------------------------- | -----: | -------------: | -------------: | -----------------: |
| ItemAccess: vectorLastItem   | 0.0065 | 0.392 (60.12×) | 0.0061 (0.94×) |     0.0061 (0.94×) |
| ItemAccess: vectorEveryItem  |  0.407 |  0.924 (2.27×) |  0.517 (1.27×) |      0.516 (1.27×) |
| ItemAccess: structsLastItem  | 0.0036 | 0.190 (52.78×) | 0.0042 (1.16×) |     0.0042 (1.16×) |
| ItemAccess: structsEveryItem | 0.0653 |  0.362 (5.54×) | 0.0695 (1.06×) |     0.0688 (1.05×) |

Bytes allocated per operation:

| Benchmark                    | tafla | tafla-gen | tafla-gen view | tafla-gen view A/A |
| ---------------------------- | ----: | --------: | -------------: | -----------------: |
| ItemAccess: vectorLastItem   |     0 |     3,552 |              0 |                  0 |
| ItemAccess: vectorEveryItem  |   416 |     3,552 |              0 |                  0 |
| ItemAccess: structsLastItem  |     0 |     2,816 |              0 |                  0 |
| ItemAccess: structsEveryItem |     0 |     2,816 |              0 |                  0 |

- The views read the vector's last item as fast as `ByteBuffer` by hand, in
  0.0061 µs against 0.0065 µs, and the array's in 0.0042 µs against
  0.0036 µs, 64 and 45 times faster than the getters, which read all 100
  items first.
- Through the views, nothing is allocated: C2's escape analysis removes each
  item's reader or record, which a view returns to the loop rather than
  stores, as the getters do in their `List` or array. Summing the vector takes
  0.52 µs against 0.92 µs through the getter and 0.41 µs by hand, and the
  array 0.070 µs against 0.362 µs and 0.065 µs.
- The getter's sum of the array took 0.362 µs in every fork of this run, and
  has taken from 0.24 to 0.40 µs in other runs, varying between forks as the
  codec's reads above do.
