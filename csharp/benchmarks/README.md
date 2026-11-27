# Benchmarks

`dotnet run -c Release --project benchmarks/Tafla.Benchmarks` compares
tafla's C# runtime, `Tafla`, with other .NET libraries that encode small
tables and numeric columns. It runs the workloads of the TypeScript
comparison in `typescript/benchmarks/`, with the same values, so that the
languages' numbers compare. `Tafla.Benchmarks` and its tests,
`Tafla.Benchmarks.Tests`, are projects of their own, which reference the
runtime, so that the runtime depends on none of their packages. Run the
commands below from `csharp/`. `-- lists` runs the suite of lists of
strings instead, which [its section](#lists-of-strings) describes.

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
tafla-gen's `SerializeInto()`, into the start of bytes that the benchmark
keeps between calls, as long as the document. The other libraries can write
into reused bytes too, with an `IBufferWriter<byte>` or a builder that's
reset, but their codecs write into new bytes, as their documentation's
examples do, and this row times tafla-gen's alone, as the other languages'
comparisons do.

Seven more benchmarks time only tafla's reads of some of a table's fields,
which time `TableReader`'s getters more closely than reading a whole
workload, as tafla's other languages time their item reads. Each sums what it
reads as doubles, rather than collecting values of its own. `TaflaReads.cs`
holds the reads with `TableReader`'s getters by hand, and `TaflaGenReads.cs`
the same reads with the generated getters and views, but for the column,
whose generated getter returns its span:

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

- Every reader copies its columns into the document's arrays, as Java's
  comparison does, since `TableReader` and the generated readers return them
  as spans, which can't be kept in a record. tafla's read of the large
  document is therefore a copy of its 1.2 MB of columns, where Go and Rust
  return slices of the bytes. Summing a column shows tafla reading it in
  place.
- FlatBuffers' C# code writes columns in bulk, with the generated
  `Create…VectorBlock()` methods, and reads them into arrays in bulk with
  `Get…Array()`, where Go's and Java's FlatBuffers write each element.
- MemoryPack writes .NET's own types, and has no counterpart in the other
  languages.
- `yarn bench`'s benchmarks of each getter and setter aren't ported.

## Libraries

| Library                        | Licence      | Why it's comparable                                                                                                                                                        |
| ------------------------------ | ------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `Tafla` (`../src/Tafla`)       | MIT          | The reference that the others are compared with.                                                                                                                         |
| tafla-gen's code               | MIT          | tafla's tables, written and read by the code that tafla-gen generates.                                                                                                   |
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

- **tafla** writes with `TableBuilder`s, recording nested tables and the
  vector's items in place among their parent's fields, with
  `SetTable(fields, fill)` and `SetVector(count, fields, fill)`, whose
  lambdas capture what they set, as Go's `SetTableFunc()` and Java's
  `setTable(fields, fill)` are given closures. The overloads that pass a
  `state` to a `static` lambda would avoid allocating the closures, which the
  other languages' codecs don't. Arrays of the elements' own type are copied
  in bulk. It reads with `TableReader`'s
  getters, after checking the document's version, as the runtime's README
  does, and reads each of the vector's items in place, with a reader of its
  bytes. The array of structs and the fixed-size record are written with
  `BinaryPrimitives`, as the runtime's README reads a struct's scalars, and
  as Go's README recommends for records at fixed offsets, and read from
  `GetStructArray()`'s bytes with `BinaryPrimitives`, and with the getters.
- **tafla-gen** is the code that tafla-gen generates from `workloads.tafla`,
  in `Tafla.Benchmarks/Generated/TaflaGen/`. A message's `…Fields` writes it
  with `Serialize()` into bytes of its exact length: the record, whose
  fields but its string the schema fixes, it lays out whole, into bytes
  from `TableWriter.CreateTableBytes()`, and the others it counts with a
  `TableSizer` and writes with a `TableWriter`, the vector's items, each a
  table of fixed size, whole, and the structs straight into the table. It
  takes the workloads' types, as the other codecs do, so it creates the
  generated fields from them in each call, the vector's items and the
  structs as arrays, as the protobuf codec builds its messages. Its readers
  are `ref struct`s over the bytes, which read each field with a getter, a
  column as its span, and the vector and the array through views. Every
  suite has a copy of the schema; after changing them, regenerate the code
  as `rust/gen/README.md` describes.
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

After changing `workloads.tafla`, regenerate tafla-gen's code, from `rust/`,
with `TAFLA_GEN_UPDATE=1 cargo test -p tafla-gen`, as `rust/gen/README.md`
describes. After changing `schemas/`, regenerate the libraries' code, with
`protoc` of protobuf 36.2, whose C# runtime is `Google.Protobuf` 3.36.2, and
`flatc`:

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
`<auto-generated>`, which they skip, and keeps their generators' style. The
code that tafla-gen generates is marked so too, but it's the generator's
golden output, so `../.editorconfig` has the build's analyzers check it, and
CI checks its whitespace with `dotnet format whitespace --folder
--include-generated`, as they check the tests' generated code.

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
on the runtime with PR 261's nested tables recorded in place and PR 263's
span `TableReader`, and PR 264's tafla-gen. Each took 1 minute 50 to 52
seconds. Other processes kept the 1-minute load average between 1.73 and
2.44, on the 10 cores. The tables give each variant's median of the three
runs' fastest rounds, as a set of runs before them did, where the run of
the lowest load had the noisiest rounds; this time every run's median round
was within 5% of its fastest for 115 of the 121 variants. The three runs'
fastest rounds were within 5% of each other for 107 of the 121 variants,
and within 10% for 112; the reads that copy the document of 100,000 rows
into new arrays varied most, as below. The medians of the A/A pairs, which
show the harness's noise but not that of compiling the code again, agreed
within 2% in 21 of the 22 benchmarks, and within 2.2% in all. Every call
allocated the same bytes in every run.

Median of the fastest rounds in µs per call, and its ratio to the row's
first variant's: tafla's, but for the write into reused bytes, which only
tafla-gen times, tafla-gen's:

| Benchmark                                             | tafla   | A/A              | tafla-gen        | FlatBuffers     | protobuf        | MessagePack     | MemoryPack       | BinaryPrimitives |
| ----------------------------------------------------- | ------: | ---------------: | ---------------: | --------------: | --------------: | --------------: | ---------------: | ---------------: |
| record: serialize                                     |  0.0364 |  0.0365 (1.002×) |  0.0199 (0.547×) |  0.130 (3.574×) | 0.0480 (1.318×) | 0.0497 (1.366×) |  0.0280 (0.769×) |                  |
| record: deserialize                                   |  0.0238 |  0.0237 (0.997×) |  0.0242 (1.020×) | 0.0932 (3.923×) | 0.0547 (2.300×) | 0.0948 (3.989×) |  0.0261 (1.098×) |                  |
| document of 100 rows: serialize                       |   0.291 |   0.292 (1.002×) |   0.125 (0.429×) |  0.466 (1.603×) |  1.938 (6.658×) |  0.302 (1.036×) |   0.105 (0.360×) |                  |
| document of 100 rows: deserialize                     |   0.108 |   0.108 (1.002×) |   0.109 (1.011×) |  0.301 (2.787×) |  1.094 (10.12×) |  1.110 (10.27×) |   0.102 (0.946×) |                  |
| document of 100 rows: sum a column per element        |  0.0572 |  0.0571 (1.000×) |  0.0570 (0.998×) |  0.540 (9.453×) |  1.087 (19.02×) |  1.185 (20.73×) |   0.176 (3.086×) |                  |
| document of 100,000 rows: serialize                   |   274.2 |   280.3 (1.022×) |   76.21 (0.278×) |  287.5 (1.049×) | 1759.1 (6.416×) |  331.8 (1.210×) |   94.35 (0.344×) |                  |
| document of 100,000 rows: deserialize                 |   58.59 |   57.74 (0.986×) |   58.99 (1.007×) |  55.77 (0.952×) | 1001.3 (17.09×) |  915.6 (15.63×) |   50.02 (0.854×) |                  |
| document of 100,000 rows: sum a column per element    |   85.70 |   85.71 (1.000×) |   85.69 (1.000×) |  399.3 (4.660×) |  997.1 (11.64×) | 1013.8 (11.83×) |   148.9 (1.737×) |                  |
| document of 100,000 rows: serialize into reused bytes |         |   16.05 (0.981×) |            16.36 |                 |                 |                 |                  |                  |
| vector of 100 nested tables: serialize                |   3.056 |   3.065 (1.003×) |   0.366 (0.120×) |  6.051 (1.980×) |  3.431 (1.123×) |  1.690 (0.553×) | 0.0887 (0.0290×) |                  |
| vector of 100 nested tables: deserialize              |   0.490 |   0.490 (0.999×) |   0.519 (1.058×) |  7.076 (14.43×) |  3.918 (7.993×) |  4.221 (8.611×) |  0.0719 (0.147×) |                  |
| array of 100 structs: serialize                       |   0.273 |   0.273 (1.002×) |   0.220 (0.806×) |  0.689 (2.528×) |  2.639 (9.677×) |  1.101 (4.036×) |  0.0881 (0.323×) |                  |
| array of 100 structs: deserialize                     |   0.120 |   0.120 (0.999×) |   0.133 (1.104×) |  1.102 (9.146×) |  2.916 (24.21×) |  1.902 (15.79×) |  0.0717 (0.595×) |                  |
| fixed-size record: serialize                          | 0.00501 | 0.00502 (1.002×) | 0.00499 (0.996×) |  0.100 (20.05×) | 0.0278 (5.559×) | 0.0309 (6.174×) | 0.00496 (0.990×) | 0.00498 (0.994×) |
| fixed-size record: deserialize                        | 0.00182 | 0.00183 (1.005×) | 0.00168 (0.923×) | 0.0490 (26.90×) | 0.0363 (19.95×) | 0.0706 (38.81×) | 0.00139 (0.764×) | 0.00190 (1.044×) |

Bytes allocated per call:

| Benchmark                                             | tafla     | A/A       | tafla-gen | FlatBuffers | protobuf  | MessagePack | MemoryPack | BinaryPrimitives |
| ----------------------------------------------------- | --------: | --------: | --------: | ----------: | --------: | ----------: | ---------: | ---------------: |
| record: serialize                                     |       248 |       248 |        88 |       1,472 |       192 |          80 |         80 |                  |
| record: deserialize                                   |       104 |       104 |       104 |         248 |       328 |         104 |        104 |                  |
| document of 100 rows: serialize                       |     5,064 |     5,064 |     1,320 |       4,800 |     2,816 |       1,600 |      1,288 |                  |
| document of 100 rows: deserialize                     |     1,400 |     1,400 |     1,400 |       1,544 |     3,736 |       1,400 |      1,400 |                  |
| document of 100 rows: sum a column per element        |         0 |         0 |         0 |         144 |     2,392 |       1,400 |      1,400 |                  |
| document of 100,000 rows: serialize                   | 4,001,064 | 4,001,064 | 1,200,120 |   5,394,048 | 2,387,872 |   2,999,336 |  1,200,088 |                  |
| document of 100,000 rows: deserialize                 | 1,200,200 | 1,200,200 | 1,200,200 |   1,200,344 | 3,049,528 |   1,200,200 |  1,200,200 |                  |
| document of 100,000 rows: sum a column per element    |         0 |         0 |         0 |         144 | 1,849,384 |   1,200,200 |  1,200,200 |                  |
| document of 100,000 rows: serialize into reused bytes |           |         0 |         0 |             |           |             |            |                  |
| vector of 100 nested tables: serialize                |    19,288 |    19,288 |     4,456 |      10,848 |    11,360 |       3,536 |      1,232 |                  |
| vector of 100 nested tables: deserialize              |     1,248 |     1,248 |     1,248 |       1,392 |    10,784 |       1,248 |      1,248 |                  |
| array of 100 structs: serialize                       |     3,840 |     3,840 |     2,456 |       4,696 |     7,960 |       2,880 |      1,232 |                  |
| array of 100 structs: deserialize                     |     1,248 |     1,248 |     1,248 |       1,392 |     7,584 |       2,496 |      1,248 |                  |
| fixed-size record: serialize                          |        40 |        40 |        40 |       1,432 |       144 |          48 |         40 |               40 |
| fixed-size record: deserialize                        |         0 |         0 |         0 |         144 |       208 |           0 |          0 |                0 |

The A/A column is the copy of each row's first variant.

- tafla reads a document in 108 ns at 100 rows and in 58.6 µs at 100,000,
  and tafla-gen in 109 ns and 59.0 µs, most of it the copy of its columns
  into new arrays, as MemoryPack's reads, in 102 ns and 50.0 µs, and
  FlatBuffers', in 301 ns and 55.8 µs, copy them. protobuf and MessagePack
  decode each element, in 1.00 and 0.92 ms. Summing the column in place,
  without a copy, takes tafla and tafla-gen 57 ns and 85.7 µs, 0.86 ns an
  element, the time of the additions that depend on each other, as in Go.
  FlatBuffers' accessor of each element takes 399 µs, and the others read
  the whole document first.
- The large document's copying reads varied from run to run more than any
  other row, for every codec that copies the columns alike: tafla's from 51
  to 64 µs in these three runs, and in all of the day's 19 runs, on every
  version of the runtime, from 41 to 91 µs, MemoryPack's from 31 to 92 µs
  and FlatBuffers' from 42 to 99 µs, which no change to their code explains.
  How the garbage collector reuses, or maps from the system, the 1.2 MB that
  each call allocates is the likely cause, which wasn't measured.
- tafla reads the record in 23.8 ns and tafla-gen in 24.2 ns, against
  MemoryPack's 26.1 ns, protobuf's 54.7 ns, FlatBuffers' 93.2 ns, which
  verifies the message first, and MessagePack's 94.8 ns. tafla-gen writes it
  in 19.9 ns, laying out its fields itself, into bytes of its 88, against
  `TableBuilder`'s 36.4 ns and 248 bytes, a builder and the table, and
  MemoryPack's 28.0 ns.
- tafla-gen writes the document of 100,000 rows in 76.2 µs, with the
  table's 1.2 MB, and in 16.4 µs into reused bytes, which allocates
  nothing, so the allocation of the 1.2 MB takes most of its write into new
  bytes. `TableBuilder` writes it in 274 µs, allocating 4.0 MB. MemoryPack
  writes it in 94.4 µs, FlatBuffers in 288 µs, MessagePack in 332 µs and
  protobuf in 1.76 ms.
- The vector of 100 nested tables is the shape that `TableBuilder` handles
  worst, as in the other languages: 3.06 µs and 19.3 KB. tafla-gen writes it
  in 0.37 µs, with the table and the array of the items' generated fields
  that the codec creates, against MessagePack's 1.69 µs, protobuf's 3.43 µs
  and FlatBuffers' 6.05 µs. MemoryPack writes it in 89 ns, and reads it in
  72 ns: one copy of 1,200 bytes, since it writes the items as an array of
  their three floats. tafla reads it in 0.49 µs, tafla-gen in 0.52 µs,
  protobuf in 3.92 µs, MessagePack in 4.22 µs and FlatBuffers in 7.08 µs.
- tafla writes the 100 structs in 273 ns, and tafla-gen in 220 ns, which
  writes them straight into the table, from the array of the generated
  struct that the codec creates, and they read them in 120 and 133 ns.
  MemoryPack writes and reads them in 88 and 72 ns, and FlatBuffers,
  MessagePack and protobuf take 0.69 to 2.9 µs.
- tafla, tafla-gen, MemoryPack and `BinaryPrimitives` write the fixed-size
  record in 5.0 ns alike, each allocating its 12 bytes, 40 with the array's
  header. MemoryPack reads it in 1.39 ns, a struct read whole from the
  bytes, tafla-gen's struct in 1.68 ns, tafla's getters, which check each
  read's range, in 1.82 ns, and `BinaryPrimitives` in 1.90 ns.
- PR 261's recording of nested tables in place took `TableBuilder` 0.75
  times the time on the vector, 3.06 µs rather than 4.08 µs, with 19.3 KB
  rather than 37.8 KB, and 0.82 and 0.90 times on the documents, with 4.0
  rather than 5.2 MB for the large one, against this suite's figures on the
  runtime before it, from earlier on 2026-11-18, where the rows that don't
  write with it moved by a median of 2.1%.

`dotnet run -c Release --project benchmarks/Tafla.Benchmarks -- encoding`
printed:

```
Encoded size (bytes)
                            record     doc 100  doc 100000      vector     structs       fixed
  tafla                         60        1296     1200096        3208        1208          12
  tafla-gen                     60        1296     1200096        3208        1208          12
  FlatBuffers                   72        1328     1200128        2832        1224          32
  protobuf                      47        1243     1187503        1900        1700          16
  MessagePack                   53        1573     1499239        3510        1604          24
  MemoryPack                    49        1261     1200061        1205        1205          12
  BinaryPrimitives                                                                          12

Truncated document of 100000 rows: deserialize
  tafla                 600048 B  threw DecodeException: Read exceeds the supplied bytes: 400000 bytes at 400092 of 600048
  tafla                     24 B  threw DecodeException: Read exceeds the supplied bytes: 8 bytes at 24 of 24
  tafla-gen             600048 B  threw DecodeException: Read exceeds the supplied bytes: 400000 bytes at 400092 of 600048
  tafla-gen                 24 B  threw DecodeException: Read exceeds the supplied bytes: 8 bytes at 24 of 24
  FlatBuffers           600064 B  threw InvalidDataException: The FlatBuffers message doesn't verify
  FlatBuffers               24 B  threw InvalidDataException: The FlatBuffers message doesn't verify
  protobuf              593751 B  threw InvalidProtocolBufferException: While parsing a protocol message, the input ended unexpectedly in the middle of a field.  This could mean either that t...
  protobuf                  24 B  threw InvalidProtocolBufferException: While parsing a protocol message, the input ended unexpectedly in the middle of a field.  This could mean either that t...
  MessagePack           749619 B  threw MessagePackSerializationException: Failed to deserialize Tafla.Benchmarks.Document value.
  MessagePack               24 B  threw MessagePackSerializationException: Failed to deserialize Tafla.Benchmarks.Document value.
  MemoryPack            600030 B  threw MemoryPackSerializationException: Sequence reached end, reader can not provide more buffer.
  MemoryPack                24 B  threw MemoryPackSerializationException: Sequence reached end, reader can not provide more buffer.
```

The sizes of tafla, tafla-gen, FlatBuffers and protobuf are the other
languages'. MessagePack packs each integer in the fewest bytes that hold it,
as rmp-serde and msgpackr do. MemoryPack writes the vector and the array of
structs alike, as 1,205 bytes. Every library rejects the truncated document
with an exception: tafla and tafla-gen with tafla's `DecodeException`, and
FlatBuffers when its verifier fails, before it reads.

### tafla's reads

The seven benchmarks of tafla's reads of some fields, from the same runs,
by hand with `TableReader` and with tafla-gen's code. None of them
allocates.

Median of the fastest rounds in µs per call, and its ratio to tafla's:

| Benchmark                                         | tafla   | A/A              | tafla-gen        |
| ------------------------------------------------- | ------: | ---------------: | ---------------: |
| record: read the scalars                          | 0.00316 | 0.00315 (0.997×) | 0.00644 (2.038×) |
| document of 100 rows: sum a column per getter     |  0.0711 |  0.0712 (1.001×) |                  |
| document of 100,000 rows: sum a column per getter |   85.74 |   85.77 (1.000×) |                  |
| vector of 100 nested tables: read the last item   |  0.0138 |  0.0136 (0.987×) |  0.0103 (0.744×) |
| vector of 100 nested tables: sum every item       |   0.423 |   0.423 (1.000×) |   0.458 (1.083×) |
| array of 100 structs: read the last item          | 0.00286 | 0.00286 (1.000×) | 0.00514 (1.797×) |
| array of 100 structs: sum every item              |  0.0824 |  0.0824 (1.001×) |  0.0770 (0.935×) |

- `TableReader`'s getters read the record's three scalars in 3.2 ns, and a
  column's elements in 0.71 ns an element at 100 rows and 0.86 ns at
  100,000, as fast as from the column's span at 100,000 rows, 0.86 ns, and
  against 0.57 ns at 100.
- tafla-gen's reader takes 6.4 ns for the record's scalars, twice as long
  as the getters by hand, which read the same fields, and its view of the
  array of structs 5.1 ns for the last struct, against 2.9 ns, where its
  indexer checks the index and the generated struct's constructor the
  struct's length, besides the reader's checks. Why the record's read takes
  twice as long wasn't investigated.
- The vector's last item takes 13.8 ns by hand and 10.3 ns through
  tafla-gen's view, and every item 0.42 and 0.46 µs, 4.2 and 4.6 ns an item.
  Every struct of the array takes 82 and 77 ns.
- PR 263's span `TableReader` took 0.30 to 0.72 times the time of the
  `ReadOnlyMemory<byte>` reader before it, on these rows, in this suite's
  runs of that reader earlier on 2026-11-18: the vector's read 0.49 µs
  rather than 1.07 µs, the record's 23.8 ns rather than 33.1 ns, its scalars
  3.2 ns rather than 6.3 ns, the fixed-size record 1.82 ns rather than
  3.64 ns, every item of the vector 0.42 µs rather than 1.43 µs, and a
  column per getter 85.7 µs rather than 137 µs at 100,000 rows.

## Lists of strings

`dotnet run -c Release --project benchmarks/Tafla.Benchmarks -- lists`
compares how the code that tafla-gen generates writes and reads lists of
10, 100 and 10,000 distinct strings of 16 or 256 characters of ASCII, or of
8 characters of 3 bytes in UTF-8, the CJK ideographs from U+4E00, which are
one UTF-16 code unit each. They're three of the eight kinds of strings of
the TypeScript suite of lists, `typescript/benchmarks/strings/lists/`, with
its values: `ListShapes.cs` makes them with the comparison's xorshift, from
the seed that TypeScript gives each shape, as `ListsTests` checks against
values that the TypeScript suite printed. tafla-gen's code of
`lists.tafla`, a copy of the TypeScript suite's schema, in
`Tafla.Benchmarks/Generated/Lists/`, writes each list in two ways:

- **tafla-gen [string]**, the vector of strings, `StringList`: its
  `…Fields` counts the strings' UTF-8 with a `TableSizer` and encodes each
  string straight into the table with a `TableWriter`, and its getter
  returns a `StringVector`, whose `ToArray()` reads every string, each
  string's end once, and whose indexer reads the last from its two ends.
  It runs again as **A/A**, a copy whose loop is instantiated with a type
  of its own, as in the comparison.
- **tafla-gen [Name]**, a vector of tables of one string each, `NameList`,
  which is how a schema held a list of strings before the vector of strings:
  the codec creates the generated fields of each table in each call, as the
  comparison's tafla-gen codec creates the vector's items, reads every
  string in order through the view's enumerator, each table's end once,
  and the last string through its indexer.

Each list is **serialized** into new bytes of its length, **deserialized**
into an array of its strings, each a new `string`, and its **last string**
read. Both write the tables that `TableBuilder` writes, which `ListsTests`
checks, and before timing, every variant must read back every string, as
the comparison's do. The suite runs with the comparison's harness and
method, interleaved, the fastest of 30 rounds after 3 warm-up rounds and 4
tiering passes, and each codec's operation is marked `NoInlining`.

```sh
uptime
dotnet run -c Release --project benchmarks/Tafla.Benchmarks -- lists
dotnet run -c Release --project benchmarks/Tafla.Benchmarks -- lists --quick
```

`--run`, `--rounds` and the comparison's other options apply to it too, and
`--quick` runs each benchmark briefly, to check that it works, as CI does.
A full run takes about 2 minutes, an estimate from the method's rounds and
pauses, and the quick run 10 seconds.

### Results

To come, from a full run on a quiet machine.
