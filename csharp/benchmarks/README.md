# Benchmarks

`dotnet run -c Release --project benchmarks/Tafla.Benchmarks` compares
tafla's C# runtime, `Tafla`, with other .NET libraries that encode small
tables and numeric columns. It runs the workloads of the TypeScript
comparison in `typescript/benchmarks/`, with the same values, so that the
languages' numbers compare. `Tafla.Benchmarks` and its tests,
`Tafla.Benchmarks.Tests`, are projects of their own, which reference the
runtime, so that the runtime depends on none of their packages. Run the
commands below from `csharp/`.

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

Each shape is serialized into new bytes and deserialized into the records of
`Workloads.cs`, whose strings and columns are new strings and arrays, since a
span of the bytes can't be kept in a record. Column `a` of each document is
also summed through each library's per-element accessor. `Workloads.cs`
ports the TypeScript generator, so the inputs are the same values, which
`WorkloadsTests` checks against values that the TypeScript generator printed.

The document of 100,000 rows is also **serialized into reused bytes**, by
`TableWriter`, into the start of bytes that the benchmark keeps between
calls, as long as the document. The other libraries can write into reused
bytes too, with an `IBufferWriter<byte>` or a builder that's reset, but
their codecs write into new bytes, as their documentation's examples do, and
this row times tafla's alone.

Seven more benchmarks time only tafla's reads of some of a table's fields,
which time `TableReader`'s getters more closely than reading a whole
workload, as tafla's other languages time their item reads. Each sums what it
reads as doubles, rather than collecting values of its own, and
`TaflaReads.cs` holds them:

- **record: read the scalars**: the two float64s and the int64, without the
  string.
- **document: sum a column per getter**: column `a` with `GetFloat32()` at
  each element's position, which times a getter's checks once a row, as
  Java's comparison sums it.
- **vector of 100 nested tables** and **array of 100 structs**: **read the
  last item**, which a reader can find without reading the others, from
  `Vector.GetInPlace()` or `GetStructArray()`'s bytes, and **sum every
  item**, in place, with `Vector.ItemsInPlace()` or from the structs' bytes.

`dotnet run -c Release --project benchmarks/Tafla.Benchmarks -- encoding`
prints each library's encoded sizes and what it does with a truncated
document, as the other comparisons do. The comparison prints the bytes that
each benchmark's call allocates too.

### What differs from the other languages

- There's no tafla-gen target for C# yet, so there's no codec of generated
  code. The `tafla TableWriter` codec writes tables as tafla-gen's code
  writes them in Java and Go, with a `TableSizer` and a `TableWriter`, and
  reads them with `TaflaCodec`'s reader.
- Every reader copies its columns into the document's arrays, as Java's
  comparison does, since `TableReader` returns them as spans, which can't be
  kept in a record. tafla's read of the large document is therefore a copy
  of its 1.2 MB of columns, where Go and Rust return slices of the bytes.
  Summing a column shows tafla reading it in place.
- C#'s `TableBuilder` has no nested builders recorded in place among their
  parent's fields, as Go's `SetTableFunc()` and Java's `setTable(fields,
  fill)` are: a nested table or a vector's item is a builder of its own,
  which its parent copies.
- FlatBuffers' C# code writes columns in bulk, with the generated
  `Create…VectorBlock()` methods, and reads them into arrays in bulk with
  `Get…Array()`, where Go's and Java's FlatBuffers write each element.
- MemoryPack writes .NET's own types, and has no counterpart in the other
  languages.
- `yarn bench`'s benchmarks of each getter and setter aren't ported.

## Libraries

| Library                        | Licence      | Why it's comparable                                                                                                                                                        |
| ------------------------------ | ------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `Tafla` (`../src/Tafla`)       | MIT          | The reference that the others are compared with, with `TableBuilder`, and with `TableSizer` and `TableWriter`.                                                            |
| `Google.FlatBuffers` 25.2.10   | Apache-2.0   | Zero-copy tables with offsets and vectors, like tafla: Google's runtime, which the other languages' comparisons use.                                                      |
| `Google.Protobuf` 3.36.2       | BSD-3-Clause | The most used schema-based codec, with packed repeated floats.                                                                                                            |
| `MessagePack` 3.1.12           | MIT          | MessagePack-CSharp, the most used MessagePack for .NET: a schemaless format, whose tables are maps with their keys.                                                        |
| `MemoryPack` 1.22.0            | MIT          | Cysharp's binary serializer of .NET's own types, without names or tags, which copies arrays of unmanaged values whole: a .NET serializer designed for speed.             |
| `BinaryPrimitives` (.NET's)    | MIT          | For the fixed-size record only: fields at fixed offsets, written and read with the base class library, as the runtime's README reads a struct's scalars.                  |

Their versions are pinned in `../Directory.Packages.props` and locked in
each project's `packages.lock.json`. Google.FlatBuffers 25.2.10, of
2025-02-11, is the latest release on NuGet, though flatc has had releases
since; the other three had releases from September to November 2026.
FlatSharp, .NET's other FlatBuffers library, has had no release since
2025-05-26, and is used less, so the comparison leaves it out, as it does
protobuf-net.

Each library is used as its documentation shows, and as the other
comparisons use their counterparts:

- **tafla** writes with `TableBuilder`s, whose nested tables and vector items
  are builders of their own, which their parent copies, and arrays of the
  elements' own type, which it copies in bulk. It reads with `TableReader`'s
  getters, after checking the document's version, as the runtime's README
  does, and reads each of the vector's items in place, with a reader of its
  bytes. The array of structs and the fixed-size record are written with
  `BinaryPrimitives`, as the runtime's README reads a struct's scalars, and
  as Go's README recommends for records at fixed offsets, and read from
  `GetStructArray()`'s bytes with `BinaryPrimitives`, and with the getters.
- **tafla TableWriter** counts each table's length with a `TableSizer`, and
  then writes every field into bytes of that length with a `TableWriter`:
  nested tables and the vector's items in place, and the structs straight
  into the table with `ReserveStructArray()`.
- **FlatBuffers** builds each message with a new `FlatBufferBuilder` of
  1,024 bytes, as the C# tutorial creates one, which grows by doubling, and
  each table with the code that `flatc --csharp` generates from
  `schemas/workloads.fbs`, in `Tafla.Benchmarks/Generated/Tafla/`. A message
  is copied out of the builder with `SizedByteArray()`, since the builder's
  bytes start where it finished writing. A reader verifies each message with
  the generated verifier before reading it, as the Rust and TypeScript
  comparisons do, and reads every field with the generated accessors.
- **protobuf** builds the messages that `protoc` generates from
  `schemas/messages.proto`, in `Tafla.Benchmarks/Generated/Protobuf/`, from
  the input, writes them with `ToByteArray()`, which sizes the bytes first,
  and reads them with their parsers. Repeated fields copy the input's
  columns with `AddRange()`, and the parsed columns are copied into arrays.
  protobuf has no structs, so the array of them is a repeated message of the
  three floats, which protobuf writes as fixed32s.
- **MessagePack** packs the workloads' own records with the formatters that
  its source generator writes from their `[MessagePackObject]` and `[Key]`
  attributes, with `MessagePackSerializer`'s default options. String keys
  pack a record as a map with its field names, as msgpackr packs an object,
  and a column element by element, each integer in the fewest bytes that
  hold it. The array of structs packs each struct, and the table that holds
  them, as an array of its fields in order, without their names, as every
  language's MessagePack codec packs that fixed shape, so the codec copies
  the items into such records, and back, in each call, as Go's does.
- **MemoryPack** writes the workloads' own records with the code that its
  source generator writes from their `[MemoryPackable]` attribute: their
  members in order, and each array of unmanaged values, a column or the
  items, with one copy of its memory. It has no tables or structs of its
  own, so it writes the vector of nested tables and the array of structs
  alike, as an array of the items' three float32s.
- **BinaryPrimitives** writes the fixed-size record into 12 new bytes with
  `WriteUInt32LittleEndian()` and `WriteSingleLittleEndian()`, and reads it
  with their `Read…` counterparts.

A field that FlatBuffers or protobuf bytes leave out reads as null, which
their codecs throw for, since every workload has it.

## Method

`Harness.cs` times each benchmark's variants, one per library, interleaved,
as the other comparisons do: every round runs each variant once, in
alternating order, and the fastest round counts. Load from other processes
then slows every variant alike, and only for some rounds. A round of a
variant repeats its call for about 20 ms, counted from its first calls and
then again from its fastest of 3 warm-up rounds, and the report takes the
fastest of 30 rounds, per call. It prints the median round too, which load
moves away from the fastest.

Alternating keeps each benchmark's first and last variants at the ends of
every round, and the others in the middle, where the TypeScript comparison
found that a variant's position can move its time. Here it didn't: in two
runs of each order, interleaved, a rotating order, which puts each variant
in every position, moved the variants at the ends by a median of +0.5% and
those in the middle by −0.2%, where two runs of the same order differed by
up to 2% for 70% of the variants. The largest moves, of 21% to 34%, were
the reads that copy the document of 100,000 rows into new arrays, at the
ends and in the middle alike, which vary from run to run, as the results
describe.

- Before the rounds, 4 tiering passes call each variant 40 times, with a
  pause of 250 ms after each, so that tiered compilation promotes the loops
  and what they call through dynamic PGO's instrumented tier to tier 1
  before any are timed, as it promotes an application's hot methods.
- Every variant has a timing loop of its own. Each call is a struct, generic
  over its codec, a struct too, and the loop is a generic method
  instantiated with the call's type, which the JIT compiles for that
  instantiation alone, so that dynamic PGO profiles each loop apart from the
  others'.
- Every codec's operation is marked `NoInlining`, so that the JIT compiles
  it as a method of its own, as an application's call of a library is
  compiled, and each variant's loop calls it directly. Inlined into the small
  loop, the same code took up to 2.3 times as long, likely because the JIT's
  budget for inlining grows with the size of the method that it compiles, so
  that less of what the code calls was inlined: TableWriter wrote the array
  of structs in 284 ns, and tafla read the vector in 1.40 µs, with the
  reader before PR 263, where the harness of PR 254's investigation, whose
  loops call a static method, timed 122 ns and 1.04 µs. Every variant pays
  for the one call.
- tafla's first variant runs twice in every benchmark, the second time as
  **A/A**, a copy whose loop is instantiated with a type of its own. Both
  loops call the same compiled codec method, so the pair shows the noise of
  the timer, of the variants' positions in the rounds and of the garbage
  collector, but not the variance of compiling and placing the code a second
  time, by which two versions of the code would differ too. A comparison of
  two versions needs a copy of the code compiled apart, as a generic method
  instantiated with a struct of its own is.
- Each variant's sample, in every round, starts after a full garbage
  collection, so that one variant's garbage isn't collected in another's
  sample, and the collections that a variant's allocations cause within its
  sample count as its own. The
  project runs the workstation garbage collector without its background
  thread, so that a collection doesn't run beside the next round.
- Every call stores its result in a static field, so that the JIT can't
  leave the call out, and the last results are kept alive to the end.
- After the rounds, the harness counts the bytes that each variant's calls
  allocate on the managed heap, from `GC.GetAllocatedBytesForCurrentThread()`,
  over the same number of calls as a round, the fewest of three samples,
  divided among them. .NET counts no allocations, only their bytes.
- Before timing, every variant's call runs once, and checks what it stored:
  that its codec reads back exactly what it wrote, with floats compared by
  their bits, that it reads back every field, that it sums column `a` to
  the same bits, or that it writes into reused bytes, which start as
  `0xff`, the bytes that it writes into new ones. `dotnet test` runs
  `Tafla.Benchmarks.Tests`, which checks every codec on every workload, every
  benchmark's variants, and the workloads' values.

## Running

```sh
uptime
dotnet run -c Release --project benchmarks/Tafla.Benchmarks
dotnet run -c Release --project benchmarks/Tafla.Benchmarks -- encoding
```

The full run takes about 2 minutes. `--run` takes a regular expression of the
benchmarks to run, `--rounds`, `--warmup`, `--sample` (in milliseconds) and
`--tiering` change the method, `--quick` runs every benchmark briefly, to
check that it works, as CI does, `--tsv` writes every measurement to a file,
to compare runs, and `--markdown` writes one run's tables, in the form of
those below.

```sh
dotnet run -c Release --project benchmarks/Tafla.Benchmarks -- --run 'document of 100,000 rows'
```

After changing `schemas/`, regenerate the libraries' code, with `protoc` of
protobuf 36.2, whose C# runtime is `Google.Protobuf` 3.36.2, and `flatc`:

```sh
protoc --proto_path=benchmarks/schemas --csharp_out=benchmarks/Tafla.Benchmarks/Generated/Protobuf \
    benchmarks/schemas/messages.proto
flatc --csharp -o benchmarks/Tafla.Benchmarks/Generated benchmarks/schemas/workloads.fbs
```

The code that flatc generates checks, in `ValidateVersion()`, that it runs
with the runtime of flatc's own version, which must then be
`Google.FlatBuffers`' version, 25.2.10. Homebrew's flatc is 25.12.19, whose
C# generator differs from 25.2.10's, besides the version, only in the code
of keys, which these tables don't have, so the code was generated with it
and the version set to the package's:

```sh
sed -i '' 's/FLATBUFFERS_25_12_19()/FLATBUFFERS_25_2_10()/' \
    benchmarks/Tafla.Benchmarks/Generated/Tafla/Benchmarks/FlatBuffers/*.cs
```

## Code checks

`dotnet format` and the build's analyzers check the projects, as they check
the runtime's. The code that protoc and flatc generate is marked
`<auto-generated>`, which they skip, and keeps their generators' style.

## Limitations

- Only a little-endian host was timed. On a big-endian host, tafla's reads
  of arrays copy them, swapping their bytes.
- Only macOS's memory and .NET 10's allocator and workstation garbage
  collector were timed, which every library's allocations depend on, and
  whose collections each variant's rounds include as their allocations cause
  them.
- Each run's JIT compiles every variant's loop once, and its tiering and
  inlining decisions can differ from one process to the next. The tables
  give each variant's median of three runs, and the results say how far
  the runs differed.
- FlatBuffers' builder starts at 1 KiB and doubles by copying. A builder
  with the message's capacity, or reused with `Clear()`, would make writing
  cheaper, but the other comparisons don't reuse their builders either.
- MessagePack's and MemoryPack's serializers write into pooled buffers and
  copy each message out into new bytes, which their `IBufferWriter<byte>`
  overloads would avoid.
- The Apple M2 Pro has 6 performance and 4 efficiency cores, and macOS can
  move the benchmark's thread to an efficiency core when others are busy.

## Results

Three full runs on 2026-11-18, one after the other, on an Apple M2 Pro
with 32 GB in macOS 27.0.1, with .NET 10.0.15 (SDK 10.0.404), in Release,
on the span `TableReader` of PR 263. Each took 1 minute 42 to 44 seconds.
Other processes kept the 1-minute load average between 1.87 and 2.83, on
the 10 cores. The tables give each variant's median of the three runs'
fastest rounds, rather than one run's: the run that was meant to give them,
whose load was lowest, had noisier rounds than the others, its median round
within 5% of the fastest for only 38 of the 107 variants, and taking the
median of the three picks none of them. The three runs' fastest rounds were
within 10% of each other for 88 of the 107 variants; the reads that copy
the document of 100,000 rows into new arrays varied most, as below. The
medians of the A/A pairs, which show the harness's noise but not that of
compiling the code again, agreed within 2% in 20 of the 22 benchmarks, and
within 4.9% in all. Every call allocated the same bytes in every run.

Median of the fastest rounds in µs per call, and its ratio to the row's
first variant's: tafla's, but for the write into reused bytes, which only
`TableWriter` times, `TableWriter`'s:

| Benchmark                                             | tafla   | A/A              | tafla TableWriter | FlatBuffers     | protobuf        | MessagePack     | MemoryPack       | BinaryPrimitives |
| ----------------------------------------------------- | ------: | ---------------: | ----------------: | --------------: | --------------: | --------------: | ---------------: | ---------------: |
| record: serialize                                     |  0.0379 |  0.0380 (1.004×) |   0.0296 (0.781×) |  0.134 (3.531×) | 0.0492 (1.299×) | 0.0509 (1.345×) |  0.0281 (0.743×) |                  |
| record: deserialize                                   |  0.0244 |  0.0244 (0.999×) |                   | 0.0956 (3.915×) | 0.0565 (2.312×) | 0.0980 (4.011×) |  0.0267 (1.095×) |                  |
| document of 100 rows: serialize                       |   0.354 |   0.355 (1.003×) |    0.126 (0.357×) |  0.474 (1.340×) |  1.992 (5.628×) |  0.310 (0.876×) |   0.106 (0.301×) |                  |
| document of 100 rows: deserialize                     |   0.111 |   0.111 (0.993×) |                   |  0.311 (2.796×) |  1.121 (10.07×) |  1.129 (10.14×) |   0.106 (0.949×) |                  |
| document of 100 rows: sum a column per element        |  0.0580 |  0.0572 (0.987×) |                   |  0.560 (9.656×) |  1.122 (19.35×) |  1.218 (21.00×) |   0.186 (3.199×) |                  |
| document of 100,000 rows: serialize                   |   305.2 |   298.8 (0.979×) |    85.70 (0.281×) |  349.5 (1.145×) | 1867.6 (6.119×) |  328.5 (1.076×) |   107.6 (0.353×) |                  |
| document of 100,000 rows: deserialize                 |   84.62 |   88.75 (1.049×) |                   |  93.04 (1.100×) | 1044.6 (12.35×) |  960.4 (11.35×) |   87.79 (1.037×) |                  |
| document of 100,000 rows: sum a column per element    |   86.56 |   86.85 (1.003×) |                   |  410.1 (4.738×) | 1044.8 (12.07×) | 1046.2 (12.09×) |   178.8 (2.065×) |                  |
| document of 100,000 rows: serialize into reused bytes |         |   16.42 (0.994×) |             16.51 |                 |                 |                 |                  |                  |
| vector of 100 nested tables: serialize                |   4.083 |   4.088 (1.001×) |    0.878 (0.215×) |  6.097 (1.493×) |  3.444 (0.843×) |  1.723 (0.422×) | 0.0895 (0.0219×) |                  |
| vector of 100 nested tables: deserialize              |   0.502 |   0.502 (0.999×) |                   |  7.110 (14.15×) |  3.928 (7.819×) |  4.251 (8.463×) |  0.0723 (0.144×) |                  |
| array of 100 structs: serialize                       |   0.271 |   0.271 (0.999×) |    0.122 (0.448×) |  0.692 (2.551×) |  2.664 (9.825×) |  1.134 (4.183×) |  0.0897 (0.331×) |                  |
| array of 100 structs: deserialize                     |   0.122 |   0.122 (0.997×) |                   |  1.114 (9.134×) |  2.923 (23.96×) |  1.909 (15.65×) |  0.0724 (0.593×) |                  |
| fixed-size record: serialize                          | 0.00501 | 0.00501 (1.000×) |                   |  0.101 (20.07×) | 0.0281 (5.617×) | 0.0321 (6.403×) | 0.00498 (0.994×) | 0.00501 (1.000×) |
| fixed-size record: deserialize                        | 0.00184 | 0.00184 (1.000×) |                   | 0.0503 (27.32×) | 0.0374 (20.30×) | 0.0716 (38.89×) | 0.00140 (0.761×) | 0.00191 (1.038×) |

Bytes allocated per call:

| Benchmark                                             | tafla     | A/A       | tafla TableWriter | FlatBuffers | protobuf  | MessagePack | MemoryPack | BinaryPrimitives |
| ----------------------------------------------------- | --------: | --------: | ----------------: | ----------: | --------: | ----------: | ---------: | ---------------: |
| record: serialize                                     |       240 |       240 |                88 |       1,472 |       192 |          80 |         80 |                  |
| record: deserialize                                   |       104 |       104 |                   |         248 |       328 |         104 |        104 |                  |
| document of 100 rows: serialize                       |     6,024 |     6,024 |             1,320 |       4,800 |     2,816 |       1,600 |      1,288 |                  |
| document of 100 rows: deserialize                     |     1,400 |     1,400 |                   |       1,544 |     3,736 |       1,400 |      1,400 |                  |
| document of 100 rows: sum a column per element        |         0 |         0 |                   |         144 |     2,392 |       1,400 |      1,400 |                  |
| document of 100,000 rows: serialize                   | 5,200,824 | 5,200,824 |         1,200,120 |   5,394,048 | 2,387,872 |   2,999,336 |  1,200,088 |                  |
| document of 100,000 rows: deserialize                 | 1,200,200 | 1,200,200 |                   |   1,200,344 | 3,049,528 |   1,200,200 |  1,200,200 |                  |
| document of 100,000 rows: sum a column per element    |         0 |         0 |                   |         144 | 1,849,384 |   1,200,200 |  1,200,200 |                  |
| document of 100,000 rows: serialize into reused bytes |           |         0 |                 0 |             |           |             |            |                  |
| vector of 100 nested tables: serialize                |    37,832 |    37,832 |             3,232 |      10,848 |    11,360 |       3,536 |      1,232 |                  |
| vector of 100 nested tables: deserialize              |     1,248 |     1,248 |                   |       1,392 |    10,784 |       1,248 |      1,248 |                  |
| array of 100 structs: serialize                       |     3,832 |     3,832 |             1,232 |       4,696 |     7,960 |       2,880 |      1,232 |                  |
| array of 100 structs: deserialize                     |     1,248 |     1,248 |                   |       1,392 |     7,584 |       2,496 |      1,248 |                  |
| fixed-size record: serialize                          |        40 |        40 |                   |       1,432 |       144 |          48 |         40 |               40 |
| fixed-size record: deserialize                        |         0 |         0 |                   |         144 |       208 |           0 |          0 |                0 |

The A/A column is the copy of each row's first variant.

- tafla reads a document in 111 ns at 100 rows and in 84.6 µs at 100,000,
  most of it the copy of its columns into new arrays, as MemoryPack's
  reads, in 106 ns and 87.8 µs, and FlatBuffers', in 311 ns and 93.0 µs,
  copy them. protobuf and MessagePack decode each element, in 1.04 and
  0.96 ms. Summing the column in place, without a copy, takes tafla 58 ns
  and 86.6 µs, 0.87 ns an element, the time of the additions that depend on
  each other, as in Go. FlatBuffers' accessor of each element takes 410 µs,
  and the others read the whole document first.
- The large document's copying reads varied from run to run more than any
  other row, for tafla, MemoryPack and FlatBuffers alike: tafla's from 60 to
  89 µs in these three runs, and in all of the day's 16 runs, on either
  reader, tafla's from 41 to 91 µs, MemoryPack's from 31 to 92 µs and
  FlatBuffers' from 42 to 99 µs, which no change to their code explains. How the garbage collector reuses, or maps
  from the system, the 1.2 MB that each call allocates is the likely cause,
  which wasn't measured.
- tafla reads the record in 24.4 ns, against MemoryPack's 26.7 ns,
  protobuf's 56.5 ns, FlatBuffers' 95.6 ns, which verifies the message
  first, and MessagePack's 98.0 ns. `TableBuilder` writes it in 37.9 ns, with
  240 bytes: a builder and the table. `TableWriter` writes it in 29.6 ns with
  the table's 88 bytes, beside MemoryPack's 28.1 ns.
- `TableBuilder` writes the document of 100,000 rows in 305 µs, allocating
  5.2 MB for its 1.2 MB table. `TableWriter` writes it in 85.7 µs, and in
  16.5 µs into reused bytes, which allocates nothing, so the allocation of
  the 1.2 MB takes most of its write into new bytes. MemoryPack writes it in
  108 µs, MessagePack in 329 µs, FlatBuffers in 350 µs and protobuf in
  1.87 ms.
- The vector of 100 nested tables is the shape that `TableBuilder` handles
  worst, as in the other languages: 4.08 µs and 37.8 KB, with a builder and
  its bytes for each of its 200 tables, which their parents copy.
  `TableWriter` writes it in 0.88 µs with the table's 3,232 bytes, against
  MessagePack's 1.72 µs, protobuf's 3.44 µs and FlatBuffers' 6.10 µs.
  MemoryPack writes it in 90 ns, and reads it in 72 ns: one copy of 1,200
  bytes, since it writes the items as an array of their three floats. tafla
  reads it in 0.50 µs, protobuf in 3.93 µs, MessagePack in 4.25 µs and
  FlatBuffers in 7.11 µs.
- tafla writes the 100 structs in 271 ns with `TableBuilder`, and in 122 ns
  with `TableWriter`, which writes them straight into the table rather than
  into an array that the builder copies, and reads them in 122 ns.
  MemoryPack writes and reads them in 90 and 72 ns, and FlatBuffers,
  MessagePack and protobuf take 0.69 to 2.9 µs.
- tafla, MemoryPack and `BinaryPrimitives` write the fixed-size record in
  5.0 ns alike, each allocating its 12 bytes, 40 with the array's header.
  MemoryPack reads it in 1.40 ns, a struct read whole from the bytes, and
  tafla's getters, which check each read's range, in 1.84 ns, beside
  `BinaryPrimitives`' 1.91 ns.

`dotnet run -c Release --project benchmarks/Tafla.Benchmarks -- encoding`
printed:

```
Encoded size (bytes)
                            record     doc 100  doc 100000      vector     structs       fixed
  tafla                         60        1296     1200096        3208        1208          12
  tafla TableWriter             60        1296     1200096        3208        1208
  FlatBuffers                   72        1328     1200128        2832        1224          32
  protobuf                      47        1243     1187503        1900        1700          16
  MessagePack                   53        1573     1499239        3510        1604          24
  MemoryPack                    49        1261     1200061        1205        1205          12
  BinaryPrimitives                                                                          12

Truncated document of 100000 rows: deserialize
  tafla                 600048 B  threw DecodeException: Read exceeds the supplied bytes: 400000 bytes at 400092 of 600048
  tafla                     24 B  threw DecodeException: Read exceeds the supplied bytes: 8 bytes at 24 of 24
  FlatBuffers           600064 B  threw InvalidDataException: The FlatBuffers message doesn't verify
  FlatBuffers               24 B  threw InvalidDataException: The FlatBuffers message doesn't verify
  protobuf              593751 B  threw InvalidProtocolBufferException: While parsing a protocol message, the input ended unexpectedly in the middle of a field.  This could mean either that t...
  protobuf                  24 B  threw InvalidProtocolBufferException: While parsing a protocol message, the input ended unexpectedly in the middle of a field.  This could mean either that t...
  MessagePack           749619 B  threw MessagePackSerializationException: Failed to deserialize Tafla.Benchmarks.Document value.
  MessagePack               24 B  threw MessagePackSerializationException: Failed to deserialize Tafla.Benchmarks.Document value.
  MemoryPack            600030 B  threw MemoryPackSerializationException: Sequence reached end, reader can not provide more buffer.
  MemoryPack                24 B  threw MemoryPackSerializationException: Sequence reached end, reader can not provide more buffer.
```

The sizes of tafla, FlatBuffers and protobuf are the other languages'.
MessagePack packs each integer in the fewest bytes that hold it, as
rmp-serde and msgpackr do. MemoryPack writes the vector and the array of
structs alike, as 1,205 bytes. Every library rejects the truncated document
with an exception: tafla with its `DecodeException`, and FlatBuffers when
its verifier fails, before it reads.

### tafla's reads

The seven benchmarks of tafla's reads of some fields, from the same runs.
None of them allocates.

Median of the fastest rounds in µs per call, and its ratio to tafla's:

| Benchmark                                         | tafla   | A/A              |
| ------------------------------------------------- | ------: | ---------------: |
| record: read the scalars                          | 0.00326 | 0.00327 (1.003×) |
| document of 100 rows: sum a column per getter     |  0.0742 |  0.0738 (0.994×) |
| document of 100,000 rows: sum a column per getter |   89.03 |   88.74 (0.997×) |
| vector of 100 nested tables: read the last item   |  0.0140 |  0.0142 (1.007×) |
| vector of 100 nested tables: sum every item       |   0.440 |   0.440 (1.000×) |
| array of 100 structs: read the last item          | 0.00294 | 0.00294 (1.000×) |
| array of 100 structs: sum every item              |  0.0778 |  0.0778 (1.001×) |

- The record's three scalars take 3.3 ns. A getter of each element of a
  column takes 0.74 ns an element at 100 rows and 0.89 ns at 100,000,
  against 0.58 and 0.87 ns an element summed from the column's span.
- The vector's last item takes 14.0 ns, and every item 0.44 µs, 4.4 ns an
  item, which `DeserializeList()` reads in 0.50 µs into new items. The
  array's last struct takes 2.9 ns, and every struct 78 ns, 0.78 ns a
  struct.
- PR 263's span `TableReader` took 0.31 to 0.74 times the time of the
  `ReadOnlyMemory<byte>` reader before it, on these rows, in this suite's
  run of that
  reader earlier on 2026-11-18, where the rows that don't read with it moved
  by a median of 1.9% between the two: the vector's read 0.50 µs rather than
  1.07 µs, the record's 24.4 ns rather than 33.1 ns, its scalars 3.3 ns
  rather than 6.3 ns, the fixed-size record 1.84 ns rather than 3.64 ns,
  every item of the vector 0.44 µs rather than 1.43 µs, and a column per
  getter 89.0 µs rather than 137 µs at 100,000 rows.
