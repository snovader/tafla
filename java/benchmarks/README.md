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

- **tafla** reads with `ByteBuffer`'s getters by hand: `getVectorOffset()` of
  the last item or `getVectorOffsets()` of every item, and `getStructArray()`'s
  view with `getFloat()`.
- **tafla-gen** reads with the generated getters, `items()`, which return
  views, lazy `List`s whose `get(i)` reads one item. It reads every item of
  the vector with the view's iterator, which reads each item's end once, and
  every item of the array of structs in a loop of `get(i)`.
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
| `uk.co.real-logic:sbe-tool` 1.42.0, `org.agrona:agrona` 2.7.1 | Simple Binary Encoding, for the fixed-size record and the array of structs only: fields at fixed offsets, written and read in place through flyweights, like tafla's structs. |

Each library is used as its documentation shows, and as the TypeScript
comparison uses its counterpart:

- **tafla** records nested tables in place among their parent builder's
  fields, with `setTable(fields, fill)` and `setVector(count, fields, fill)`.
  Arrays of the elements' own Java type are copied in bulk, and the uint32
  keys are narrowed and widened element by element. The array of structs is
  written into a `byte[]` through a little-endian `VarHandle` and set with
  `setStructArray()`, and read from `getStructArray()`'s little-endian view,
  as tafla-gen's code does.
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
  creating a `Triple` of each. Every suite has a copy of the schema; after
  changing them, regenerate the code as `rust/gen/README.md` describes.
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
- **The tafla codec records nested tables in place.** It records the
  vector's items and their pairs, and the document's columns, among their
  parent's fields with `setVector(int, int, ObjIntConsumer)` and
  `setTable(int, Consumer)`, without a `TableBuilder` of their own. Nesting a
  builder for each of the vector's tables instead took 14% more time, 4.43 µs
  against 3.89 µs, and allocated 29.1 KB rather than 13.0 KB, as core's
  package documentation reports.
- **The tafla-gen codec converts records both ways.** It creates the
  generated `Fields` records from the workload's records before writing, and
  the workload's records from the generated readers after reading, which an
  application that held the generated types wouldn't. That costs most for
  writing the vector, with a record for each of its 100 items and their pairs.
  The array of structs is written from a fill, without records.
- **SBE's struct read depends on how C2 compiles JMH's loop.** SBE reads
  the array of structs in 0.39 µs as the suite runs it, and in 0.33 µs with
  its `deserialize()` kept out of JMH's loop, as the results describe. tafla's
  and tafla-gen's reads take 0.20 µs either way.
- **Only tafla-gen writes into reused bytes.** `ReusedBytesBenchmark` times
  its `serializeInto()` of the large document. The other codecs, and every
  other benchmark, write each message into new bytes.
  `TableSizer.writer(byte[])`'s documentation reports what reusing bytes
  saves for the smaller messages.

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
collection, and protobuf's read of the record up to 22% faster. After the
warm-up, a fork's fastest iteration is almost always one of its first two, so
each fork measures only those. Until 2026-11-14, each fork measured 5, which
took 40% longer. In two runs of 5 on that day, the fastest of each fork's
first two iterations was within 0.8% of the fastest of all five for 90% of the
benchmarks, where the two runs differed by 4.2%.

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
  latency of each addition, about 0.88 ns here, so tafla's and FlatBuffers'
  accessors sum the large document in about the same time, though not the small
  one.

## Results

One full run on 2026-11-14, on an Apple M2 Pro with 32 GB in macOS 27.0.1,
with Oracle's JDK 25.0.1 and JMH 1.37, in three rounds of a fork per codec,
as the method describes. The suite took 45 minutes. The 1-minute load
average, which counts the benchmark's own threads, was between 2.1 and 3.8 on
the 10 cores, with a median of 2.7. One run, during which other processes
used 4.9 cores, was rerun, and they used at most 3.9 cores during the runs
kept. The mean of each codec's 6 iterations was within 5% of its fastest
iteration for 65 of the 74 benchmarks and codecs, and at most 29% above it,
where FlatBuffers wrote the record in 0.146 and 0.149 µs in two forks and in
0.254 µs in the third, while other processes used 3.8 cores. Seven of the
nine were protobuf's, whose forks differ, as below. tafla's A/A pair's
fastest iterations were within 1.5% of each other in every benchmark.

Fastest iteration in µs per operation, and its ratio to tafla's:

| Benchmark                                          |  tafla |    flatbuffers |        protobuf |         msgpack |            sbe |
| -------------------------------------------------- | -----: | -------------: | --------------: | --------------: | -------------: |
| record: serialize                                  | 0.0380 |  0.146 (3.84×) |  0.0298 (0.78×) |  0.0872 (2.29×) |                |
| record: deserialize                                | 0.0141 | 0.0198 (1.41×) |  0.0338 (2.41×) |   0.135 (9.61×) |                |
| document of 100 rows: serialize                    |  0.146 |  0.828 (5.67×) |   1.280 (8.77×) |   0.838 (5.74×) |                |
| document of 100 rows: deserialize                  | 0.0750 | 0.0783 (1.04×) |  0.824 (10.99×) |  1.393 (18.57×) |                |
| document of 100 rows: sum a column per element     | 0.0421 | 0.0572 (1.36×) |  0.837 (19.89×) |  0.775 (18.42×) |                |
| document of 100,000 rows: serialize                |   39.9 | 762.8 (19.11×) | 1793.4 (44.92×) |  704.3 (17.64×) |                |
| document of 100,000 rows: deserialize              |   47.7 |   48.1 (1.01×) | 1027.9 (21.56×) | 1081.2 (22.67×) |                |
| document of 100,000 rows: sum a column per element |   88.0 |   88.1 (1.00×) | 1045.0 (11.88×) |   630.4 (7.16×) |                |
| vector of 100 nested tables: serialize             |  3.874 |  4.950 (1.28×) |   1.678 (0.43×) |   3.423 (0.88×) |                |
| vector of 100 nested tables: deserialize           |  0.544 |  0.881 (1.62×) |   3.882 (7.13×) |  7.268 (13.36×) |                |
| array of 100 structs: serialize                    |  0.141 |  0.484 (3.43×) |   0.931 (6.60×) |   0.699 (4.95×) |  0.222 (1.57×) |
| array of 100 structs: deserialize                  |  0.203 |  0.225 (1.11×) |   1.986 (9.78×) |   1.539 (7.57×) |  0.385 (1.90×) |
| fixed-size record: serialize                       | 0.0020 | 0.132 (64.97×) |  0.0049 (2.43×) | 0.0727 (35.75×) | 0.0063 (3.11×) |
| fixed-size record: deserialize                     | 0.0024 | 0.0035 (1.46×) |  0.0179 (7.46×) | 0.0903 (37.60×) | 0.0060 (2.48×) |

- tafla reads every shape fastest. FlatBuffers reads documents within 5% of
  it, since both copy the float columns in bulk and widen the keys one by
  one.
- tafla writes documents 5.7 to 45 times faster than the others, whose
  columns are written element by element.
- protobuf and msgpack write the vector of 100 nested tables faster than
  tafla, in 1.7 and 3.4 µs against 3.9 µs, and protobuf writes the record
  faster too, in 0.030 µs against 0.038 µs. msgpack's items are flat maps, as
  above.
- The sums of 100,000 floats take 88 µs through both tafla's and
  FlatBuffers' accessors, bound by the additions; the 100-row sums differ.
- tafla writes the 100 structs in 0.141 µs, against 3.9 µs for the same items
  as nested tables, and reads them in 0.203 µs, against 0.544 µs. Reading
  creates the 100 `ItemInput` records, which every codec does.
- FlatBuffers reads its vector of structs nearly as fast, in 0.225 µs, and
  writes it in 0.484 µs. SBE writes its repeating group in 0.222 µs and reads
  it in 0.385 µs. protobuf and msgpack write and parse each struct's fields:
  0.7 to 0.9 µs to write and 1.5 to 2.0 µs to read.
- The tafla codec writes the fixed-size record through `VarHandle`s in 2.0 ns
  and reads it with `ByteBuffer`'s getters in 2.4 ns, against 6 ns with SBE,
  which writes and checks its 8-byte header too.

The fastest iteration hides differences between forks, most of all
protobuf's. It read the small document in 0.82 and 0.86 µs in two forks and
in 1.01 µs in the third, and the array of structs in 1.99 µs in one fork and
in 2.52 µs in the other two, each steady within its fork. Its forks' fastest
reads of the fixed-size record took 17.9, 21.5 and 17.9 ns, and writes of the
vector 1.79, 1.90 and 1.68 µs.

Fastest iteration in µs per operation, and tafla-gen's ratio to tafla's:

| Benchmark                                          |  tafla |      tafla-gen |
| -------------------------------------------------- | -----: | -------------: |
| record: serialize                                  | 0.0380 | 0.0176 (0.46×) |
| record: deserialize                                | 0.0141 | 0.0144 (1.03×) |
| document of 100 rows: serialize                    |  0.146 | 0.0800 (0.55×) |
| document of 100 rows: deserialize                  | 0.0750 | 0.0746 (0.99×) |
| document of 100 rows: sum a column per element     | 0.0421 | 0.0419 (0.99×) |
| document of 100,000 rows: serialize                |   39.9 |   42.2 (1.06×) |
| document of 100,000 rows: deserialize              |   47.7 |   47.6 (1.00×) |
| document of 100,000 rows: sum a column per element |   88.0 |   87.5 (0.99×) |
| vector of 100 nested tables: serialize             |  3.874 |  0.894 (0.23×) |
| vector of 100 nested tables: deserialize           |  0.544 |  0.647 (1.19×) |
| array of 100 structs: serialize                    |  0.141 |  0.109 (0.77×) |
| array of 100 structs: deserialize                  |  0.203 |  0.203 (1.00×) |
| fixed-size record: serialize                       | 0.0020 | 0.0021 (1.05×) |
| fixed-size record: deserialize                     | 0.0024 | 0.0019 (0.80×) |

Bytes allocated per operation, the mean of the measured iterations:

| Benchmark                                          |     tafla | tafla-gen | flatbuffers |  protobuf |   msgpack |   sbe |
| -------------------------------------------------- | --------: | --------: | ----------: | --------: | --------: | ----: |
| record: serialize                                  |       248 |        80 |       1,360 |       136 |       335 |       |
| record: deserialize                                |        96 |        96 |         144 |        96 |       320 |       |
| document of 100 rows: serialize                    |     1,928 |     1,392 |       4,736 |     6,347 |     3,908 |       |
| document of 100 rows: deserialize                  |     1,760 |     1,760 |       1,808 |     4,576 |     2,128 |       |
| document of 100 rows: sum a column per element     |         0 |         0 |           0 |     2,872 |       320 |       |
| document of 100,000 rows: serialize                | 1,200,504 | 1,200,112 |   5,394,470 | 5,954,413 | 3,013,985 |       |
| document of 100,000 rows: deserialize              | 1,600,160 | 1,600,160 |   1,600,208 | 6,367,050 | 1,600,536 |       |
| document of 100,000 rows: sum a column per element |        89 |       201 |         121 | 4,766,946 |       324 |       |
| vector of 100 nested tables: serialize             |    13,048 |     8,464 |      10,816 |    16,787 |    14,920 |       |
| vector of 100 nested tables: deserialize           |     3,272 |     2,856 |       2,856 |    26,480 |    17,352 |       |
| array of 100 structs: serialize                    |     2,440 |     1,224 |       4,624 |     7,736 |     3,740 | 1,232 |
| array of 100 structs: deserialize                  |     2,856 |     2,856 |       2,856 |    12,101 |     2,888 | 2,856 |
| fixed-size record: serialize                       |        32 |        32 |       1,320 |        32 |       208 |    40 |
| fixed-size record: deserialize                     |        32 |        32 |          32 |        96 |       208 |    32 |

- tafla-gen writes the vector of 100 nested tables in 0.89 µs, against
  3.87 µs with tafla's builders recording it in place, allocating 8.5 KB
  against 13.0 KB. By their sizes, about 5 KB of that are the 200 generated
  `Fields` records and their list, which the codec creates from the
  workload's items, and 3.2 KB the table. It writes the record and the small
  document about twice as fast, in 18 and 80 ns against 38 and 146 ns. The
  record allocates only the table's bytes. The small document allocates
  80 bytes more than its table in all three forks, the size of its two
  `Fields` records, which escape analysis removed in earlier runs.
- The large document takes 42.2 µs against 39.9 µs, both copying the
  columns' 1.2 MB into the table. tafla-gen's three forks took 42.2 to
  44.6 µs, and tafla's six, with its A/A pair, 39.9 to 41.0 µs. Written into
  reused bytes with `serializeInto()`, it takes 29.1 µs, 31% less than into
  new bytes, and allocates 48 bytes. Its three forks took 29.1 to 30.4 µs.
- tafla-gen writes the array of structs in 0.109 µs, faster than the tafla
  codec's 0.141 µs and SBE's 0.222 µs, and allocates only the table's
  1,224 bytes. Its fill writes each item's values straight into the table,
  where the tafla codec writes them into an array of its own, which
  `setStructArray()` copies.
- The generated readers read the record and the documents within 3% of the
  hand-written reads. They sum the column through its view, `aView()`, which
  reads the floats in place, as fast as tafla's `getFloat32()`: 41.9 ns
  against 42.1 ns for 100 rows, and 87.5 µs against 88.0 µs for 100,000.
- The getters of the vector and of the array return views, and escape
  analysis removes the `Item`, `Pair` and `Triple` readers that they return
  to the codec's loop, so both reads allocate only the workload's records,
  2,856 bytes. The vector's read takes 0.647 µs against 0.544 µs, and the
  array's 0.203 µs, as fast as the hand-written read.
- tafla-gen writes and reads the fixed-size record through `VarHandle`s in
  2.14 and 1.91 ns. tafla writes it through `VarHandle`s too, in 2.03 ns, and
  reads it with `ByteBuffer`'s getters in 2.40 ns. Both allocate only the
  12-byte array, or the workload's record, 32 bytes, so escape analysis
  removes the generated `Sample` record.

The struct reads were also timed with each codec's `deserialize()` kept out
of JMH's measurement loop, with
`-XX:CompileCommand=dontinline,*TaflaGenCodec$Structs::deserialize` and its
counterparts, in three forks of each codec:

- SBE's read took 0.334 to 0.336 µs out of line, against 0.393 to 0.395 µs
  in line, so how C2 compiles JMH's loop around the same code still changes
  it by 15%.
- tafla's and tafla-gen's reads took 0.202 to 0.204 µs either way.
  tafla-gen's took 0.53 µs in line and 0.38 µs out of line in earlier runs,
  when its getter read every struct into an array first.
- The vector's generated read took 0.647 to 0.651 µs in all three of the
  suite's forks. Its getter used to read every item into a `List` first, and
  in some forks C2 compiled that getter on its own, into too much machine code
  to inline into the codec's read, which made those forks 13% slower. None
  was slower in this run.

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

`ItemAccessBenchmark`'s results, from the same run, in which its three
variants ran in the suite's rounds as its codecs. The mean of each variant's
6 iterations was within 1% of its fastest for 10 of the 12 benchmarks, and
2.6% and 2.4% above it for the getters' sums of the vector and the array.

Fastest iteration in µs per operation, and its ratio to `ByteBuffer`'s by
hand:

| Benchmark                    |  tafla |      tafla-gen |  tafla-gen A/A |
| ---------------------------- | -----: | -------------: | -------------: |
| ItemAccess: vectorLastItem   | 0.0064 | 0.0062 (0.97×) | 0.0062 (0.97×) |
| ItemAccess: vectorEveryItem  |  0.410 |  0.423 (1.03×) |  0.436 (1.06×) |
| ItemAccess: structsLastItem  | 0.0036 | 0.0040 (1.10×) | 0.0040 (1.10×) |
| ItemAccess: structsEveryItem | 0.0663 | 0.0682 (1.03×) | 0.0678 (1.02×) |

Bytes allocated per operation:

| Benchmark                    | tafla | tafla-gen | tafla-gen A/A |
| ---------------------------- | ----: | --------: | ------------: |
| ItemAccess: vectorLastItem   |     0 |         0 |             0 |
| ItemAccess: vectorEveryItem  |   416 |         0 |             0 |
| ItemAccess: structsLastItem  |     0 |         0 |             0 |
| ItemAccess: structsEveryItem |     0 |         0 |             0 |

- The generated getters return views, which read the vector's last item as
  fast as `ByteBuffer` by hand, in 0.0062 µs against 0.0064 µs, and the
  array's in 0.0040 µs against 0.0036 µs, without reading the other 99 items.
- Through the views, nothing is allocated: C2's escape analysis removes each
  item's reader or record, which a view returns to the loop rather than
  stores. Summing the vector takes 0.42 to 0.44 µs against 0.41 µs by hand,
  and the array 0.068 µs against 0.066 µs. The vector's iterator reads each
  item's end once, as the next item's start, where `get(i)` reads two.
- The A/A pair's fastest sums of the vector differ by 3%, 0.423 and
  0.436 µs, though the medians of their forks' fastest are both 0.437 µs. The
  pair's other figures are within 0.7% of each other.
