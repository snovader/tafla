# Benchmarks

`swift run -c release Comparison` compares tafla's Swift runtime, the `Tafla`
library of `..`, with other Swift libraries that encode small tables and
numeric columns. It runs the workloads of the TypeScript comparison in
`typescript/benchmarks/`, with the same values, so that the languages'
numbers compare, and `swift run -c release Lists` how tafla-gen's code
writes and reads lists of strings, as [Lists of strings](#lists-of-strings)
describes. It's a package of its own, which depends on the runtime's
package at `..`, so that the runtime's package depends on none of the
libraries. Run the commands below from this directory.

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

Each shape is serialized into new bytes and deserialized into the types of
`Sources/Workloads`, which own their strings and columns. Column `a` of each
document is also summed through each library's per-element accessor, which
reads tafla's column in place. `Sources/Workloads` ports the TypeScript
generator, so the inputs are the same values, which its tests check against
values that the TypeScript generator printed.

Four more groups time only tafla's bytes of the vector of nested tables and
of the array of structs, read in two ways: **read the last item**, which a
reader can find without reading the others, and **sum every item**. Each way
sums the three float32s of the items it reads as `Double`s, rather than
collecting items of its own, and `Sources/Codecs/ItemAccess.swift` holds
them:

- **tafla** reads with `TableReader`'s getters by hand: each item in place,
  from `Vector.itemInPlace(at:)`, and the structs from the bytes of
  `structArray(at:size:)`, with `Float(littleEndianBytes:at:)`.
- **tafla-gen** reads with the generated getters, `items`, which return views
  of the vector and of the array, whose subscripts read one item.
- **tafla-gen A/A** reads with the generated getters again, an A/A pair that
  shows the harness's noise.

`swift run -c release Encoding` prints each library's encoded sizes and what
it does with a truncated document, as the other comparisons do. On macOS, the
comparison prints what each benchmark's call allocates too.

### What differs from TypeScript

- There are no `number[]` inputs. Swift's columns are typed already.
- The workloads' types own their columns, so every codec's read of a
  document copies them out of the bytes, and tafla's isn't the constant time
  of the other languages' views. tafla's `ArrayView`, like the standard
  library's `Span`, is nonescapable: it can't outlive the call that read it,
  nor be stored in a type that can, so a type that holds columns holds
  arrays. tafla copies each column with `Array(_:)` of its `ArrayView`, and
  FlatBuffers with its `withUnsafePointerTo…()`, each in one copy, protobuf's
  decoded arrays move into the output, and MessagePack decodes each element.
  Summing column `a` reads it in place, through `ArrayView`'s subscript.
- Strings are copies in every library, since a `String` owns its bytes: the
  record's 16-byte name, one byte longer than a `String` holds within itself,
  allocates 64 bytes in each read.
- tafla's Swift runtime has no `StructWriter` or `readUint32()`. The tafla
  codec writes the fixed-size record with the standard library's
  `storeBytes(of:toByteOffset:as:)`, as tafla's README recommends for such
  records, and reads it with `TableReader`'s getters, which check each read's
  range. tafla-gen's struct writes it with the generated `serialize()`, which
  appends each scalar's bytes, and reads it with its `init(_:)`, which loads
  them at fixed offsets.
- There's no row of a document serialized into reused bytes, as the Go and
  TypeScript suites have: tafla-gen's Swift writes into new bytes only.
- Each codec returns the bytes of its library's own type: `[UInt8]`, but
  `Data` for MessagePack, whose encoder and decoder take nothing else.
- `yarn bench`'s benchmarks of each getter and setter aren't ported.

## Libraries

| Library                            | Why it's comparable                                                                                        |
| ---------------------------------- | ---------------------------------------------------------------------------------------------------------- |
| `Tafla` (`..`)                     | The reference that the others are compared with.                                                           |
| tafla-gen's code                   | tafla's tables, written and read by the code that tafla-gen generates.                                     |
| `google/flatbuffers` 25.12.19      | Zero-copy tables with offsets and vectors, like tafla.                                                     |
| `apple/swift-protobuf` 1.41.0      | Swift's protobuf, the most used schema-based codec, with packed repeated floats.                           |
| `nnabeyang/swift-msgpack` 1.2.1    | MessagePack through `Codable`: a schemaless format, whose tables are maps with their keys.                 |
| `RawSpan` (the standard library's) | For the fixed-size record only: fields at fixed offsets, which tafla's README recommends for such records. |

Swift has no MessagePack library as widely used as Go's or Rust's. The most
starred, `a2/MessagePack.swift`, was last released in 2020, and
`Flight-School/MessagePack` is archived. `hirotakan/MessagePacker` and
`fumoboy007/msgpack-swift`, which encode through `Codable` too, were last
released in 2023 and 2024. swift-msgpack is the one that's maintained, with
releases through 2026, and it packs the workloads' own `Codable` types, as
Go's msgpack packs them through reflection.

Each library is used as its documentation shows, and as the other
comparisons use their counterparts:

- **tafla** writes with `TableBuilder`, recording the document's columns
  and the vector's items, with their pairs, in place among their parent's
  fields, with `setTable(fields:_:)` and `setVector(count:fields:_:)`, as
  Go's `SetTableFunc()` and C#'s `SetTable(fields, fill)` are given closures,
  and each array in bulk, into a table of its exact size. A builder records
  its first four fields within itself, and more in an array, until `bytes()`
  writes them. Readers check the document's version, and copy columns out of
  their `ArrayView`s with `Array(_:)`. The array of structs is written with
  `storeBytes(of:toByteOffset:as:)` into an array, which the builder keeps
  with `setStructArray(_:size:alignment:)`, and read from the bytes of
  `structArray(at:size:)` with `Float(littleEndianBytes:at:)`.
- **tafla-gen** is the code that tafla-gen generates from `workloads.tafla`,
  in `Sources/GeneratedTafla`. A message's fields' `serialize()` counts the
  table's length with a `TableSizer`, and then writes every field, nested
  tables included, into bytes of that length with a `TableWriter`, in one
  allocation. Its readers are nonescapable views of the bytes, whose getters
  read each field when it's asked for. It takes the workloads' types, as the
  other codecs do, so it collects the generated fields of the vector's items
  and the array's structs into an array in each call, as the protobuf codec
  collects its messages. Every suite has a copy of the schema; after changing
  them, regenerate the code as `rust/gen/README.md` describes.
- **FlatBuffers** builds each message with a new `FlatBufferBuilder` of the
  default 1 KiB, which grows by doubling, and each table with the functions
  that `flatc --swift` generates from `schemas/workloads.fbs`, in
  `Sources/GeneratedFlatBuffers`. Columns are written in bulk with
  `createVector(_:)`, and the vector of structs with
  `createVector(ofStructs:)` from an array of flatc's structs. A message is
  the builder's `sizedByteArray`, a copy of its bytes. Readers read the bytes
  through `ByteBuffer(bytes:)`, which shares the array, verify each message
  with `getCheckedRoot(byteBuffer:)`, as the Rust and TypeScript comparisons
  verify it, and copy columns with `withUnsafePointerTo…()`. That buffer's
  `count` is the column's count of elements rather than of bytes, so the
  codec copies the elements' bytes from its start, which the verifier
  checked.
- **protobuf** builds the messages that protoc-gen-swift generates from
  `schemas/messages.proto`, in `Sources/GeneratedProtobuf`, from the input,
  and writes them with `serializedBytes()`, which sizes the bytes first, and
  reads them with `init(serializedBytes:)`. Its messages hold their strings
  and repeated fields as Swift's values, so building one shares the input's
  columns, and the decoded columns move into the output. protobuf has no
  structs, so the array of them is a repeated message of the three floats,
  which protobuf writes as fixed32s.
- **msgpack** packs the workloads' own `Codable` types with
  `MsgPackEncoder`, which packs a struct as a map with its field names, as
  msgpackr packs an object, and reads them with `MsgPackDecoder`, both kept
  by the codec. The array of structs packs each struct, and the table that
  holds them, as an array of its fields in order, without their names, as
  every language's MessagePack codec packs that fixed shape. It packs each
  integer in the fewest bytes that hold it, as rmp-serde does, so its record
  and documents are a few bytes shorter than Go's msgpack's.
- **RawSpan** writes the fixed-size record into 12 new bytes with
  `UnsafeMutableRawBufferPointer.storeBytes(of:toByteOffset:as:)`, and reads
  it with `RawSpan.unsafeLoadUnaligned(fromByteOffset:as:)`, after checking
  that the bytes hold it, since a load past them would stop the program.

A field that FlatBuffers or protobuf bytes leave out reads as a default, or
`nil`, which their codecs throw as an error, since every workload has it.

## Method

`Comparison` times each benchmark's variants, one per library, interleaved,
in `Sources/Harness`, as the Go harness does: every round runs each variant
once, in alternating order, and the fastest round counts. Load from other
processes then slows every variant alike, and only for some rounds. A round
of a variant repeats its call for about 20 ms, counted from its first calls
and then again from its fastest of 3 warm-up rounds, and the report takes
the fastest of 30 rounds, per call. It prints the median round too, which
load moves away from the fastest.

- Every variant has a timing loop of its own. The comparison makes each
  codec's variants with generic functions that it calls with the codec's own
  type, which the compiler specializes for it, so that each loop is code of
  its own that calls the library directly, rather than through a protocol's
  witness table. Swift has no JIT to share feedback between them. The A/A
  pair is one specialization run twice, so it shows the harness's noise
  rather than the effects of where code lies.
- Every result is passed to `keep(_:)`, a function of another module that is
  neither inlined nor optimized, as package-benchmark's `blackHole(_:)`, so
  that the optimizer can't leave out the work that made it. The allocation
  counts show it: every write allocates its bytes.
- Swift frees what a call allocated when the last reference to it goes,
  within the call or when `keep(_:)`'s argument is released, so no variant's
  garbage is collected in another's round.
- After the rounds, on macOS, the harness counts what each variant's calls
  allocate, through `malloc_logger`, the hook that libmalloc calls with
  every allocation of every zone, which malloc's stack logging and
  Instruments set: the allocations and reallocations, and the bytes they ask
  for, over the same number of calls as a round, the fewest of three
  samples, divided among them. Swift's arrays, strings and class instances
  are allocated through malloc, as `Data` and the libraries' own buffers
  are, and their bytes include the header that Swift's runtime adds to each
  object, 32 bytes for an array. Linux's glibc has no such hook, so there
  the harness counts nothing.
- Before timing, every codec is checked to read back exactly what it wrote,
  with floats compared by their bits, and to sum column `a` to the same bits,
  and each way of reading tafla's items to sum them to the same bits.
  `swift test` runs the same checks, in `Tests/CodecsTests`, with the
  workloads' values.
- The suite is built with `-c release`, SwiftPM's optimized build, which
  compiles each module with `-O` and whole-module optimization, and the
  runtime as a package dependency, as an application's release build would.

Before running the comparison, check the machine's load with `uptime`; the
comparison prints the load averages when it starts and ends.

## Running

```sh
uptime
swift run -c release Comparison
swift run -c release Encoding
```

The full run takes about a minute and a half. `--run` takes a regular
expression of the benchmarks to run, `--rounds`, `--warmup` and `--sample`,
in milliseconds, change the method, and `--quick` runs every benchmark
briefly, to check that it works.

```sh
swift run -c release Comparison --run 'document of 100,000 rows'
```

`swift test -c release` checks that every codec reads back what it wrote, as
CI does on Linux.

After changing `schemas/`, regenerate the libraries' code, with `flatc` of
the FlatBuffers version that `Package.resolved` pins, and `protoc` with the
protoc-gen-swift of the resolved swift-protobuf, which SwiftPM builds from
its checkout. Dependabot's updates of either library need the same, since
each generator's code checks its library's version.

```sh
flatc --swift -o Sources/GeneratedFlatBuffers schemas/workloads.fbs
swift build -c release --package-path .build/checkouts/swift-protobuf \
    --scratch-path .build/protoc-gen-swift --product protoc-gen-swift
protoc --plugin=protoc-gen-swift=.build/protoc-gen-swift/release/protoc-gen-swift \
    --proto_path=schemas --swift_out=Sources/GeneratedProtobuf \
    --swift_opt=Visibility=Public schemas/messages.proto
```

## Code checks

`swift format` checks the package's own code, as it checks the runtime's,
with the runtime's `.swift-format`. The code that flatc and protoc-gen-swift
generate keeps their generators' style, which they rather than this package
would change:

```sh
swift format lint --strict --recursive Package.swift Tests Sources/Workloads Sources/Codecs \
    Sources/Harness Sources/Reporting Sources/Comparison Sources/Lists Sources/Encoding \
    Sources/GeneratedTafla Sources/GeneratedLists
```

## Limitations

- Only a little-endian host was timed. On a big-endian host, tafla reads and
  copies its columns element by element, swapping their bytes, and
  FlatBuffers' Swift builder stops the program.
- Only macOS was timed, on one machine. Linux was only built and checked,
  in CI and in Docker, where the harness counts no allocations, and CI's
  Linux job doesn't compile the macOS harness's counting either.
- Every Swift array that a codec allocates costs about 70 ns on this
  machine, as [Results](#results) shows, which is most of a small write's
  time in every library. Another system's allocator and Swift runtime weigh
  it differently: in Docker's Linux on the same Mac, a quick pass wrote the
  fixed-size record in 38 to 50 ns rather than 71 ns.
- The deserialize rows include copying columns out of the bytes, since the
  workloads' types own them. A reader that reads in place, as the rows that
  sum a column and that read items do, copies nothing.
- FlatBuffers' builder starts at 1 KiB and doubles by copying. A builder of
  the message's capacity, or one reused with `clear(keepingCapacity:)`,
  would make writing cheaper, but the other comparisons don't reuse their
  builders either.
- swift-msgpack decodes through `Codable`, which is slower than code
  generated for the types, as Go's `tinylib/msgp` generates it; Swift has no
  maintained library that does that.
- The code that tafla-gen generates is compiled as SwiftPM compiles an
  application's target in a release build. A project with other settings,
  such as Xcode's, or aggressive cross-module optimization, may inline more of
  it, as [Reading items through views](#reading-items-through-views) shows.

## Results

One full run on 2026-11-18, on an Apple M2 Pro with 32 GB in macOS 27.0.1,
with Xcode 27.2's Swift 6.4.2, FlatBuffers 25.12.19, swift-protobuf 1.41.0
and swift-msgpack 1.2.1. It took 1 minute 18 seconds, after the quick pass.
Other processes kept the 1-minute load average at 2.99 when it started and
2.60 when it ended, on the 10 cores. The median round was within 5% of the
fastest for 78 of the 84 benchmarks' variants, and within 10% for 80, with a
median of 1.5% and a largest of 29.3%, FlatBuffers' write of the document of
100,000 rows: the rows of that document allocate 1.2 to 4.9 MB in each call.
The A/A pair agreed within 1.1%. A first run, before the code took its final
form, agreed with these figures within a few percent.

The three rows that read a string field, the record's and the documents'
deserialize, are from a full run on 2026-11-27, with the same toolchain and
libraries, after `TableReader.string(at:)` came to validate each string as a
`UTF8Span` and copy it with `String(copying:)`. It ran through the machine's
benchmark lock, while the 1-minute load average was 2.26 when it started and
2.23 when it ended. The median rounds of the record and the document of 100
rows were within 2.6% of the fastest, and the document of 100,000 rows'
within 7.3%. Three such runs alternated with three of the build before, in
which tafla and tafla-gen read the record in 111 to 115 ns and the document
of 100 rows in 0.37 to 0.38 µs: they now take 0.57 to 0.58 of the time for
the record and 0.88 for the document. Every other row of tafla and tafla-gen
stayed within 1.5% of the build before.

Fastest round in µs per call, and its ratio to tafla's:

| Benchmark                                          | tafla   | tafla-gen        | flatbuffers     | protobuf        | msgpack             | RawSpan          |
| -------------------------------------------------- | ------: | ---------------: | --------------: | --------------: | ------------------: | ---------------: |
| record: serialize                                  |   0.221 |   0.101 (0.458×) |  0.313 (1.421×) |  0.131 (0.595×) |      3.530 (16.01×) |                  |
| record: deserialize                                |  0.0644 |  0.0642 (0.996×) |  0.791 (12.27×) |  0.119 (1.852×) |      2.283 (35.43×) |                  |
| document of 100 rows: serialize                    |   0.554 |   0.174 (0.314×) |  0.499 (0.900×) |  0.572 (1.032×) |      189.2 (341.4×) |                  |
| document of 100 rows: deserialize                  |   0.322 |   0.342 (1.060×) |  1.876 (5.821×) |  1.136 (3.525×) |      150.5 (467.0×) |                  |
| document of 100 rows: sum a column per element     |  0.0465 |  0.0525 (1.130×) |  2.434 (52.39×) |  1.159 (24.95×) |     147.2 (3168.3×) |                  |
| document of 100,000 rows: serialize                |   17.17 |   16.86 (0.982×) |  76.14 (4.435×) |  426.6 (24.85×) | 186014.7 (10834.5×) |                  |
| document of 100,000 rows: deserialize              |   16.53 |   16.66 (1.008×) |  18.27 (1.105×) |  751.2 (45.44×) |  144493.7 (8740.7×) |                  |
| document of 100,000 rows: sum a column per element |   86.50 |   86.34 (0.998×) | 1357.4 (15.69×) |  820.5 (9.486×) |  145362.6 (1680.6×) |                  |
| vector of 100 nested tables: serialize             |   20.66 |  1.406 (0.0680×) |  6.140 (0.297×) |  15.35 (0.743×) |      287.1 (13.90×) |                  |
| vector of 100 nested tables: deserialize           |   1.271 |   2.339 (1.839×) |  58.75 (46.21×) |  19.59 (15.41×) |      197.5 (155.3×) |                  |
| array of 100 structs: serialize                    |   0.254 |   0.263 (1.035×) |  0.490 (1.929×) |  12.22 (48.13×) |      99.22 (390.8×) |                  |
| array of 100 structs: deserialize                  |   0.222 |   0.244 (1.101×) |  3.227 (14.55×) |  10.09 (45.49×) |      79.79 (359.7×) |                  |
| fixed-size record: serialize                       |  0.0721 |  0.0710 (0.984×) |  0.300 (4.160×) |  0.114 (1.574×) |      2.867 (39.74×) |  0.0724 (1.003×) |
| fixed-size record: deserialize                     | 0.00319 | 0.00322 (1.010×) |  0.505 (158.5×) | 0.0556 (17.42×) |      1.826 (572.6×) | 0.00234 (0.735×) |

The other languages' figures below come from their suites' READMEs, which
were measured on the same machine.

- Every array that a codec allocates costs about 70 ns: tafla, tafla-gen
  and the standard library write the 12 bytes of the fixed-size record in
  71 to 72 ns alike, against 8.8 ns in Go. Its `malloc()` and `free()` take
  1.2 ns. `sample` of a loop that allocates 12-byte arrays puts about half of
  each allocation in `_ContiguousArrayBuffer.init`'s lookup of
  `_ContiguousArrayStorage<UInt8>`'s generic metadata in the Swift runtime's
  cache, which it repeats for every array, and the rest in the system's
  `malloc()` and `malloc_size()`. So each allocation weighs more here than in
  the other languages, as the vector's builders show below.
- tafla reads a document of 100 rows in 0.32 µs and one of 100,000 in
  16.5 µs: four allocations, of the name and the three columns, and copies of
  their bytes. FlatBuffers copies its columns in bulk too, in 18.3 µs, and
  protobuf and msgpack decode every element, in 0.75 and 144 ms. In place,
  summing the 100,000 floats takes tafla and tafla-gen 86.5 µs, bound by
  the additions, as in Go, 88.7 µs, and C++, 86.9 µs. FlatBuffers'
  `FlatbufferVector` takes 1.36 ms, 13.6 ns for each element that its
  subscript reads through the message's `ByteBuffer`.
- tafla writes the large document in 17.2 µs and tafla-gen in 16.9 µs, both
  copying the columns whole into a table of its exact length, as C++ does in
  16.5 µs and Rust in 18 µs. FlatBuffers takes 76 µs, allocating 4.9 MB as
  its builder doubles, protobuf 0.43 ms and msgpack 186 ms.
- tafla-gen writes the record in 101 ns, in one allocation, protobuf in
  131 ns, and tafla's builder in 221 ns, in two: the array in which the
  builder records its fields, and the bytes. Reading the record takes tafla
  and tafla-gen 64 ns, and protobuf 119 ns, most of it the copy of its name,
  whose 16 bytes a `String` allocates, where Go's tafla shares them with the
  bytes in 27 ns. tafla validates the name as a `UTF8Span` before copying it,
  in about half the time that `String(validating:as:)` took, with which
  tafla read the record in 117 ns.
- The vector of 100 nested tables is the shape that tafla's builders handle
  worst, as in the other languages, and more so here: 20.7 µs, in 203
  allocations, an array of fields for each of the 200 nested builders, which
  at about 70 ns each would take some 14 µs of it. That's an estimate from
  the cost of an allocation, rather than a measurement of the builder
  without them. Go's builders take 9.7 µs, Rust's 9.0 µs and C++'s, which
  record a small table's fields within the builder, 7.9 µs. tafla-gen writes
  the vector in 1.41 µs, in 2 allocations, FlatBuffers in 6.1 µs and protobuf
  in 15.4 µs. tafla reads it in 1.27 µs, and tafla-gen in 2.34 µs, as the
  next section explains. FlatBuffers takes 58.8 µs, of which its verifier
  takes about 36 µs: without it, a shorter run read the vector in 21.6 µs.
- tafla writes the 100 structs in 0.25 µs, in three allocations, and reads
  them in 0.22 µs, and tafla-gen in 0.26 and 0.24 µs. FlatBuffers writes them in 0.49 µs and reads
  them in 3.2 µs, and protobuf writes and reads a message for each, in 12.2
  and 10.1 µs.
- Reading the fixed-size record takes the standard library's
  `unsafeLoadUnaligned(fromByteOffset:as:)` 2.3 ns, and tafla's and
  tafla-gen's checked reads 3.2 ns, about as in Go.
- msgpack's `Codable` packing takes about 1.9 µs a row, in 18 allocations,
  so its large document takes 0.19 s to write and 0.14 s to read.

Bytes allocated per call, and the allocations that ask for them:

| Benchmark                                          | tafla         | tafla-gen     | flatbuffers    | protobuf      | msgpack               | RawSpan |
| -------------------------------------------------- | ------------: | ------------: | -------------: | ------------: | --------------------: | ------: |
| record: serialize                                  |       316 (2) |        92 (1) |      1,301 (7) |        79 (1) |            2,844 (46) |         |
| record: deserialize                                |        64 (1) |        64 (1) |        185 (3) |        64 (1) |            1,777 (29) |         |
| document of 100 rows: serialize                    |     1,776 (3) |     1,328 (1) |      4,605 (8) |     1,275 (1) |        121,169 (1919) |         |
| document of 100 rows: deserialize                  |     1,360 (4) |     1,360 (4) |      1,481 (6) |     1,360 (4) |        104,403 (1577) |         |
| document of 100 rows: sum a column per element     |         0 (0) |         0 (0) |        121 (2) |     1,360 (4) |        104,403 (1577) |         |
| document of 100,000 rows: serialize                | 1,200,576 (3) | 1,200,128 (1) | 4,871,373 (10) | 1,187,535 (1) | 117,653,271 (1800187) |         |
| document of 100,000 rows: deserialize              | 1,200,160 (4) | 1,200,160 (4) |  1,200,281 (6) | 1,200,160 (4) | 101,870,175 (1500107) |         |
| document of 100,000 rows: sum a column per element |         0 (0) |         0 (0) |        121 (2) | 1,200,160 (4) | 101,870,175 (1500107) |         |
| vector of 100 nested tables: serialize             |  32,952 (203) |     4,472 (2) |    10,621 (10) |  15,564 (202) |        260,025 (3541) |         |
| vector of 100 nested tables: deserialize           |     1,232 (1) |     1,232 (1) |      1,353 (3) |    15,264 (9) |        149,518 (2328) |         |
| array of 100 structs: serialize                    |     2,552 (3) |     2,472 (2) |      5,709 (9) |  12,164 (202) |         83,388 (1633) |         |
| array of 100 structs: deserialize                  |     1,232 (1) |     1,232 (1) |      1,353 (3) |    11,312 (9) |         80,948 (1518) |         |
| fixed-size record: serialize                       |        44 (1) |        44 (1) |      1,253 (7) |        48 (1) |            2,405 (37) |  44 (1) |
| fixed-size record: deserialize                     |         0 (0) |         0 (0) |        121 (2) |         0 (0) |            1,264 (22) |   0 (0) |

The `tafla` codec's builders allocate the array of each table's fields besides
the table's bytes. tafla-gen allocates only the table's bytes, and the arrays
of generated fields or structs that its codec collects from the workloads'
types. Every codec's read allocates the record's name and the document's
columns, which the workloads' types own, and the arrays of items. Neither
tafla nor tafla-gen allocates to sum a column, or to read items.

`swift run -c release Encoding` printed:

```
Encoded size (bytes)
                        record     doc 100  doc 100000      vector     structs       fixed
  tafla                     60        1296     1200096        3208        1208          12
  tafla-gen                 60        1296     1200096        3208        1208          12
  flatbuffers               72        1328     1200128        2832        1224          32
  protobuf                  47        1243     1187503        1900        1700          16
  msgpack                   53        1573     1499239        3510        1604          24
  RawSpan                                                                               12

Truncated document of 100000 rows: deserialize
  tafla             600048 B  threw: Read exceeds the supplied buffer: 400000 bytes at 400092 of 600048
  tafla                 24 B  threw: Read exceeds the supplied buffer: 8 bytes at 24 of 24
  tafla-gen         600048 B  threw: Read exceeds the supplied buffer: 400000 bytes at 400092 of 600048
  tafla-gen             24 B  threw: Read exceeds the supplied buffer: 8 bytes at 24 of 24
  flatbuffers       600064 B  threw: outOfBounds(position: 800128, end: 600064)
  flatbuffers           24 B  threw: outOfBounds(position: 48, end: 24)
  protobuf          593751 B  threw: truncated
  protobuf              24 B  threw: truncated
  msgpack           749619 B  trapped: the process stopped with signal 5
  msgpack               24 B  trapped: the process stopped with signal 5
```

The sizes of tafla, FlatBuffers and protobuf are the other languages'.
msgpack packs each integer in the fewest bytes that hold it, so its record
and documents are a few bytes shorter than Go's msgpack's. tafla, FlatBuffers'
verifier and protobuf reject the truncated document with an error.
swift-msgpack's decoder stops the program instead, on both: its scanner
slices the `Data` it reads, with `subdata(in:)`, to an end past the bytes,
which untrusted input could cause. `Encoding` reads each truncated document
in a process of its own, so that it can report that.

### Reading items through views

The four groups that read tafla's items, from the same run.

Fastest round in µs, and its ratio to `TableReader`'s by hand:

| Benchmark                                       | tafla   | tafla-gen       | tafla-gen A/A   |
| ----------------------------------------------- | ------: | --------------: | --------------: |
| vector of 100 nested tables: read the last item |  0.0218 | 0.0351 (1.608×) | 0.0351 (1.607×) |
| vector of 100 nested tables: sum every item     |   0.459 |  2.235 (4.872×) |  2.260 (4.926×) |
| array of 100 structs: read the last item        | 0.00796 | 0.0103 (1.293×) | 0.0103 (1.298×) |
| array of 100 structs: sum every item            |  0.0613 |  0.182 (2.961×) |  0.182 (2.969×) |

Nothing allocates.

- By hand, `TableReader`'s getters, which are `@inlinable`, inline into the
  codec's loop: the last item of the vector takes 21.8 ns and every item
  0.46 µs, and the array's last struct 8.0 ns and every struct 61 ns, against
  28 ns, 1.45 µs, 9.7 ns and 117 ns in Go.
- tafla-gen's views take 35.1 ns, 2.24 µs, 10.3 ns and 182 ns. Its readers
  are structs whose getters read their internal `_reader`, so SwiftPM's
  release build, `-O` with whole-module optimization of each module, can't
  inline them into the module that calls them: each getter of each item is a
  call. Built with `-cross-module-optimization` on the generated module
  alone, two runs of 20 rounds alternating with two of the build above
  took 27 rather than 35 ns for the vector's last item, 1.17 rather than
  2.25 µs for every item, 1.25 rather than 2.34 µs for the codec's read of
  the vector, as fast as tafla's by hand, and 120 rather than 126 ns for the
  record, whose name was then validated with `String(validating:as:)`. The
  array of structs didn't change, 184 ns for every struct, nor did the
  remaining gap of every item of the vector, 1.17 against 0.46 µs, which lie
  in the generated views' own code rather than in calls to it.

## Lists of strings

`swift run -c release Lists` times how the code that tafla-gen generates
writes and reads lists of 10, 100 and 10,000 distinct strings of 16 and 256
characters of ASCII and of 8 characters of 3 bytes in UTF-8, nine lists in
all. They're the lists of three of the eight kinds of the TypeScript
suite's `yarn bench:lists`, with the same strings, which
`Sources/Workloads/ListShapes.swift` makes with the TypeScript suite's
generator and seeds, and which `swift test` checks against strings that the
TypeScript suite printed.

tafla-gen generates the code from `lists.tafla`, a copy of the TypeScript
suite's schema, into `Sources/GeneratedLists`, and
`Sources/Codecs/ListCodecs.swift` writes and reads each list in two ways:

- **tafla-gen [string]**, the vector of strings, `StringList`. Its fields
  struct holds the array of strings, which a `TableSizer` sizes and a
  `TableWriter` writes, in one allocation, and its getter returns a
  `StringVector`, whose `Array(_:)` reads every string and whose subscript
  reads the last from its two ends. The suite checks that it writes the
  bytes that `TableBuilder.setStringVector(_:)` writes.
- **tafla-gen [Name]**, a vector of tables of one string each, `NameList`,
  which is how a schema holds a list of strings without the vector of
  strings. Its codec creates the generated fields of each table from the
  input in each call, as the comparison's tafla-gen codec creates its
  items', and reads each string through the view of its item, in place.
- **tafla-gen [string] A/A**, the vector of strings again, an A/A pair
  that shows the harness's noise.

Each list is serialized, deserialized into an array of its strings, and
read for its last string, and each row's ratios are to the vector of
strings'. Before timing, every variant must read back every string of
every list, and its last string, which `swift test -c release` checks too.
The variants are timed as [Method](#method) describes, interleaved, and
the suite prints the lists' encoded sizes after its report. The full run
takes about a minute. `--run`, `--rounds`, `--warmup`, `--sample` and
`--quick` work as they do for the comparison.

```sh
uptime
swift run -c release Lists
swift run -c release Lists --run '10,000 ×'
```

What differs from the TypeScript suite:

- Only tafla-gen's code is timed, for three kinds of string: not
  `TableBuilder` by hand, nor FlatBuffers, protobuf or MessagePack, nor the
  TypeScript suite's other five kinds.
- A `String` owns its bytes, so every string that a list reads is a copy,
  validated as UTF-8 as it's copied, and each of the strings here, of more
  than the 15 bytes that a `String` holds within itself, is an allocation.
  TypeScript's `getStringVector()` decodes runs of ASCII strings at once
  and slices them; Swift's `Array(_:)` reads each string from its ends, as
  the subscript does.

### Results

One full run on 2026-11-27, on an Apple M2 Pro with 32 GB in macOS 27.0.1,
built by Apple Swift 6.4.2, through the machine's benchmark lock. It took a
minute, while other processes kept the 1-minute load average at 2.23 when it
started and 2.42 when it ended, on the 10 cores. The median call was within
3% of the fastest for 79 of the 81 variants' runs, and within 3.6% for all
of them. The A/A pair agreed within 1.4%.

Fastest call in µs, and its ratio to the vector of strings':

| Benchmark | tafla-gen [string] | tafla-gen [Name] | tafla-gen [string] A/A |
| --- | ---: | ---: | ---: |
| 10 × 16 chars of ASCII: serialize | 0.237 | 0.413 (1.745×) | 0.237 (1.001×) |
| 10 × 16 chars of ASCII: deserialize | 0.618 | 0.634 (1.026×) | 0.622 (1.005×) |
| 10 × 16 chars of ASCII: read the last string | 0.0619 | 0.0647 (1.045×) | 0.0620 (1.002×) |
| 10 × 256 chars of ASCII: serialize | 0.252 | 0.439 (1.740×) | 0.254 (1.008×) |
| 10 × 256 chars of ASCII: deserialize | 0.791 | 0.802 (1.013×) | 0.789 (0.997×) |
| 10 × 256 chars of ASCII: read the last string | 0.0803 | 0.0832 (1.035×) | 0.0802 (0.998×) |
| 10 × 8 chars of 3 bytes: serialize | 0.241 | 0.422 (1.751×) | 0.242 (1.002×) |
| 10 × 8 chars of 3 bytes: deserialize | 0.771 | 0.794 (1.030×) | 0.766 (0.994×) |
| 10 × 8 chars of 3 bytes: read the last string | 0.0771 | 0.0794 (1.031×) | 0.0771 (1.001×) |
| 100 × 16 chars of ASCII: serialize | 1.537 | 2.689 (1.750×) | 1.541 (1.003×) |
| 100 × 16 chars of ASCII: deserialize | 5.511 | 5.811 (1.054×) | 5.546 (1.006×) |
| 100 × 16 chars of ASCII: read the last string | 0.0619 | 0.0646 (1.043×) | 0.0619 (1.000×) |
| 100 × 256 chars of ASCII: serialize | 1.675 | 2.980 (1.779×) | 1.669 (0.996×) |
| 100 × 256 chars of ASCII: deserialize | 7.233 | 7.427 (1.027×) | 7.298 (1.009×) |
| 100 × 256 chars of ASCII: read the last string | 0.0795 | 0.0828 (1.041×) | 0.0801 (1.008×) |
| 100 × 8 chars of 3 bytes: serialize | 1.585 | 2.737 (1.727×) | 1.585 (1.000×) |
| 100 × 8 chars of 3 bytes: deserialize | 7.185 | 7.410 (1.031×) | 7.189 (1.000×) |
| 100 × 8 chars of 3 bytes: read the last string | 0.0773 | 0.0798 (1.032×) | 0.0769 (0.994×) |
| 10,000 × 16 chars of ASCII: serialize | 145.6 | 255.6 (1.755×) | 145.0 (0.996×) |
| 10,000 × 16 chars of ASCII: deserialize | 553.3 | 579.2 (1.047×) | 545.6 (0.986×) |
| 10,000 × 16 chars of ASCII: read the last string | 0.0618 | 0.0645 (1.043×) | 0.0617 (0.998×) |
| 10,000 × 256 chars of ASCII: serialize | 157.2 | 290.7 (1.849×) | 158.7 (1.010×) |
| 10,000 × 256 chars of ASCII: deserialize | 721.2 | 739.5 (1.025×) | 722.1 (1.001×) |
| 10,000 × 256 chars of ASCII: read the last string | 0.0796 | 0.0823 (1.034×) | 0.0799 (1.004×) |
| 10,000 × 8 chars of 3 bytes: serialize | 149.5 | 259.2 (1.733×) | 148.6 (0.994×) |
| 10,000 × 8 chars of 3 bytes: deserialize | 701.2 | 726.0 (1.035×) | 698.4 (0.996×) |
| 10,000 × 8 chars of 3 bytes: read the last string | 0.0771 | 0.0794 (1.029×) | 0.0773 (1.002×) |

- The vector of strings is written in 0.54–0.58 times the time of the
  vector of tables, in one allocation, the table's, where the vector of
  tables also allocates an array of the tables' fields.
- Every string is read in 0.95–0.99 times the time, and the last string
  found in 0.96–0.97 times. Both read each string alike, validated as a
  `UTF8Span` and copied into a `String`, which takes most of the time, so
  what's left is the layout's: the vector of strings finds a string from two
  ends, where the vector of tables finds its item's table, and then the
  string's field within it, through the generated view's getter.
- Before `TableReader.string(at:)`, which reads a string field, validated
  each string as a `UTF8Span`, it validated with `String(validating:as:)`,
  which takes about twice as long for each string, and the vector of tables
  took 1.68–1.96 times as long as the vector of strings to read. This run
  is one of three that alternated with three of that build: the vector of
  tables now reads every string in 0.52–0.62 of the time, and finds the last
  in 0.56–0.62, and every other row, and the A/A pair's, stayed within 3.2%.
