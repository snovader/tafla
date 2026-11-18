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
  of structs in 284 ns, and tafla read the vector in 1.40 µs, where the
  harness of PR 254's investigation, whose loops call a static method, timed
  122 ns and 1.04 µs. Every variant pays for the one call.
- tafla's first variant runs twice in every benchmark, the second time as
  **A/A**, a copy whose loop is instantiated with a type of its own, which
  shows the harness's noise.
- Each round starts after a full garbage collection, so that one variant's
  garbage isn't collected in another's round, and the collections that a
  variant's allocations cause within its round count as its own. The
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
to compare runs, and `--markdown` writes the tables below.

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
  come from one run, and the two runs after it show how much they moved.
- FlatBuffers' builder starts at 1 KiB and doubles by copying. A builder
  with the message's capacity, or reused with `Clear()`, would make writing
  cheaper, but the other comparisons don't reuse their builders either.
- MessagePack's and MemoryPack's serializers write into pooled buffers and
  copy each message out into new bytes, which their `IBufferWriter<byte>`
  overloads would avoid.
- The Apple M2 Pro has 6 performance and 4 efficiency cores, and macOS can
  move the benchmark's thread to an efficiency core when others are busy.

## Results

One full run on 2026-11-18, on an Apple M2 Pro with 32 GB in macOS 27.0.1,
with .NET 10.0.15 (SDK 10.0.404), in Release. It took 1 minute 46 seconds.
Other processes kept the 1-minute load average at 3.23 when it started and
4.52 when it ended, on the 10 cores. The
median round was within 5% of the fastest for 81 of the 107 benchmarks'
variants, and within 10% for 93, with a median of 3.0% and a largest of
18.5%, MemoryPack's write of the array of structs. The A/A pairs agreed
within 2% in 20 of the 22 benchmarks, and within 3.6% in all: tafla's write
of the document of 100,000 rows, which allocates 5.2 MB, differed most. Two
more runs right after it, at loads of up to 11, were within 5% of it for 97
and 105 of the 107 variants, and every call allocated the same bytes.

Fastest round in µs per call, and its ratio to tafla's:

| Benchmark                                             | tafla   | A/A              | tafla TableWriter | FlatBuffers     | protobuf        | MessagePack     | MemoryPack       | BinaryPrimitives |
| ----------------------------------------------------- | ------: | ---------------: | ----------------: | --------------: | --------------: | --------------: | ---------------: | ---------------: |
| record: serialize                                     |  0.0379 |  0.0382 (1.008×) |   0.0298 (0.787×) |  0.135 (3.563×) | 0.0495 (1.307×) | 0.0512 (1.352×) |  0.0284 (0.751×) |                  |
| record: deserialize                                   |  0.0331 |  0.0328 (0.990×) |                   | 0.0980 (2.961×) | 0.0574 (1.735×) |  0.101 (3.066×) |  0.0271 (0.819×) |                  |
| document of 100 rows: serialize                       |   0.360 |   0.362 (1.005×) |    0.130 (0.361×) |  0.487 (1.354×) |  2.031 (5.642×) |  0.316 (0.878×) |   0.111 (0.309×) |                  |
| document of 100 rows: deserialize                     |   0.137 |   0.138 (1.006×) |                   |  0.318 (2.318×) |  1.141 (8.328×) |  1.138 (8.302×) |   0.107 (0.784×) |                  |
| document of 100 rows: sum a column per element        |  0.0620 |  0.0624 (1.006×) |                   |  0.560 (9.029×) |  1.141 (18.40×) |  1.228 (19.81×) |   0.190 (3.059×) |                  |
| document of 100,000 rows: serialize                   |   322.6 |   311.1 (0.964×) |    88.65 (0.275×) |  346.3 (1.074×) | 1827.8 (5.666×) |  324.8 (1.007×) |   99.89 (0.310×) |                  |
| document of 100,000 rows: deserialize                 |   40.75 |   40.72 (0.999×) |                   |  43.48 (1.067×) |  971.0 (23.83×) |  928.1 (22.77×) |   31.35 (0.769×) |                  |
| document of 100,000 rows: sum a column per element    |   89.43 |   88.59 (0.991×) |                   |  422.0 (4.718×) |  990.1 (11.07×) | 1009.4 (11.29×) |   129.7 (1.450×) |                  |
| document of 100,000 rows: serialize into reused bytes |         |   19.46 (0.979×) |             19.88 |                 |                 |                 |                  |                  |
| vector of 100 nested tables: serialize                |   4.184 |   4.148 (0.991×) |    0.901 (0.215×) |  6.166 (1.474×) |  3.499 (0.836×) |  1.753 (0.419×) | 0.0919 (0.0220×) |                  |
| vector of 100 nested tables: deserialize              |   1.072 |   1.075 (1.002×) |                   |  7.331 (6.835×) |  4.073 (3.798×) |  4.973 (4.637×) | 0.0763 (0.0711×) |                  |
| array of 100 structs: serialize                       |   0.285 |   0.286 (1.004×) |    0.126 (0.443×) |  0.734 (2.573×) |  2.775 (9.730×) |  1.188 (4.167×) |  0.0929 (0.326×) |                  |
| array of 100 structs: deserialize                     |   0.134 |   0.133 (0.994×) |                   |  1.150 (8.573×) |  3.012 (22.45×) |  1.974 (14.72×) |  0.0765 (0.570×) |                  |
| fixed-size record: serialize                          | 0.00527 | 0.00525 (0.997×) |                   |  0.106 (20.14×) | 0.0292 (5.537×) | 0.0327 (6.205×) | 0.00527 (0.999×) | 0.00523 (0.993×) |
| fixed-size record: deserialize                        | 0.00364 | 0.00361 (0.993×) |                   | 0.0512 (14.07×) | 0.0372 (10.22×) | 0.0717 (19.71×) | 0.00142 (0.391×) | 0.00195 (0.537×) |

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

The A/A column is the copy of each row's first variant: tafla's, but for the
write into reused bytes, which only `TableWriter` times.

- tafla reads a document in 137 ns at 100 rows and in 40.8 µs at 100,000,
  most of it the copy of its columns into new arrays, as MemoryPack's
  reads, in 107 ns and 31.4 µs, and FlatBuffers', in 318 ns and 43.5 µs,
  copy them. protobuf and MessagePack decode each element, in 0.97 and
  0.93 ms. Summing the column in place, without a copy, takes tafla 62 ns
  and 89.4 µs, 0.89 ns an element, the time of the additions that depend on
  each other, as in Go. FlatBuffers' accessor of each element takes 422 µs,
  and the others read the whole document first.
- tafla reads the record in 33.1 ns, against MemoryPack's 27.1 ns,
  protobuf's 57.4 ns, FlatBuffers' 98.0 ns, which verifies the message
  first, and MessagePack's 101 ns. `TableBuilder` writes it in 37.9 ns, with
  240 bytes: a builder and the table. `TableWriter` writes it in 29.8 ns with
  the table's 88 bytes, beside MemoryPack's 28.4 ns.
- `TableBuilder` writes the document of 100,000 rows in 323 µs, allocating
  5.2 MB for its 1.2 MB table. `TableWriter` writes it in 88.7 µs, and in
  19.9 µs into reused bytes, which allocates nothing, so the allocation of
  the 1.2 MB takes most of its write into new bytes. MemoryPack writes it in
  99.9 µs, MessagePack in 325 µs, FlatBuffers in 346 µs and protobuf in
  1.83 ms.
- The vector of 100 nested tables is the shape that `TableBuilder` handles
  worst, as in the other languages: 4.18 µs and 37.8 KB, with a builder and
  its bytes for each of its 200 tables, which their parents copy.
  `TableWriter` writes it in 0.90 µs with the table's 3,232 bytes, against
  MessagePack's 1.75 µs, protobuf's 3.50 µs and FlatBuffers' 6.17 µs.
  MemoryPack writes it in 92 ns, and reads it in 76 ns: one copy of 1,200
  bytes, since it writes the items as an array of their three floats. tafla
  reads it in 1.07 µs, protobuf in 4.07 µs, MessagePack in 4.97 µs and
  FlatBuffers in 7.33 µs.
- tafla writes the 100 structs in 285 ns with `TableBuilder`, and in 126 ns
  with `TableWriter`, which writes them straight into the table rather than
  into an array that the builder copies, and reads them in 134 ns. MemoryPack
  writes and reads them in 93 and 77 ns, and FlatBuffers, MessagePack and
  protobuf take 0.73 to 3.0 µs.
- tafla, MemoryPack and `BinaryPrimitives` write the fixed-size record in
  5.2 to 5.3 ns alike, each allocating its 12 bytes, 40 with the array's
  header. MemoryPack reads it in 1.42 ns, a struct read whole from the
  bytes, `BinaryPrimitives` in 1.95 ns, and tafla's getters, which check each
  read's range, in 3.64 ns.

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

The seven benchmarks of tafla's reads of some fields, from the same run.
None of them allocates.

Fastest round in µs per call, and its ratio to tafla's:

| Benchmark                                         | tafla   | A/A              |
| ------------------------------------------------- | ------: | ---------------: |
| record: read the scalars                          | 0.00632 | 0.00633 (1.001×) |
| document of 100 rows: sum a column per getter     |   0.154 |   0.154 (0.998×) |
| document of 100,000 rows: sum a column per getter |   136.6 |   136.0 (0.995×) |
| vector of 100 nested tables: read the last item   |  0.0256 |  0.0256 (1.002×) |
| vector of 100 nested tables: sum every item       |   1.428 |   1.427 (1.000×) |
| array of 100 structs: read the last item          | 0.00418 | 0.00419 (1.002×) |
| array of 100 structs: sum every item              |  0.0782 |  0.0789 (1.009×) |

- The record's three scalars take 6.3 ns, and a getter of each element of a
  column 1.5 ns an element at 100 rows and 1.37 ns at 100,000, against 0.62
  and 0.89 ns an element summed from the column's span.
- The vector's last item takes 25.6 ns, and every item 1.43 µs, 14 ns an
  item. Reading every item into new items takes `DeserializeList()`
  1.07 µs, though it reads the same fields and allocates too; why the
  smaller loop that sums them is slower wasn't investigated. The array's
  last struct takes 4.2 ns, and every struct 78 ns, 0.78 ns a struct.
- `TableReader`'s getters are fast only where the JIT inlines them. In this
  suite's first version, whose loops inlined each read, the same reads took
  13.8 ns for the record's scalars and 266 µs for the column of 100,000 rows,
  and the vector's read 1.40 µs rather than 1.07 µs.
- The reads that copy the large document's columns into new arrays took 76
  to 81 µs in the first run of that version, and 31 to 44 µs in this one,
  for tafla, MemoryPack and FlatBuffers alike, which no change to their code
  explains. How the garbage collector reuses, or maps from the system, the
  1.2 MB that each call allocates is the likely cause, which wasn't
  measured.
