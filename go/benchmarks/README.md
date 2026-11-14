# Benchmarks

`go run ./cmd/comparison` compares tafla's Go runtime,
`github.com/snovader/tafla/go`, with other Go libraries that encode small
tables and numeric columns. It runs the workloads of the TypeScript comparison
in `typescript/benchmarks/`, with the same values, so that the languages'
numbers compare. It's a module of its own, which requires the runtime from
`..`, so that the runtime's module needs none of the libraries. Run the
commands below from this directory.

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
`workloads/`, whose strings and columns share the bytes where a library reads
them in place, and are copies otherwise. Column `a` of each document is also
summed through each library's per-element accessor. `workloads/` ports the
TypeScript generator, so the inputs are the same values, which its test
checks against values that the TypeScript generator printed.

The document of 100,000 rows is also **serialized into reused bytes**, by
tafla-gen's `AppendBinary()`, into the capacity of bytes that the benchmark
keeps between calls, as long as the document. Its write into new bytes is
bound by the allocation of its 1.2 MB, which Go zeroes, or maps fresh from
the system, before the columns are copied into them, since Go has no public
allocation of bytes that it doesn't zero. Into reused bytes, the write is
the copy of the columns. The other libraries can write into reused bytes too, with FlatBuffers'
`Builder.Reset()`, protobuf's `MarshalOptions.MarshalAppend()` and
vtprotobuf's `MarshalToSizedBufferVT()`, but their codecs write into new
bytes, as their documentation's examples do, and this row times tafla-gen's
alone.

Four more groups time only tafla's bytes of the vector of nested tables and
of the array of structs, read in two ways: **read the last item**, which a
reader can find without reading the others, and **sum every item**. Each way
sums the three float32s of the items it reads as float64s, rather than
collecting items of its own, and `itemaccess/` holds them:

- **tafla** reads with `ByteBuffer`'s getters by hand: each item in place,
  from `Vector.GetInPlace()` of the last item or `Vector.ItemsInPlace()`, and
  `GetStructArray()`'s bytes.
- **tafla-gen** reads with the generated getters, `Items()`, which return
  views of the vector and of the array, whose `Get(i)` reads one item. It
  reads every item of the vector through its view's `All()`, each whole, with
  its generated `Fields()`, and every struct of the array at once, with its
  view's `Slice()`, which returns them in place where the bytes are aligned
  to them in memory, as they are here.
- **tafla-gen A/A** reads with the generated getters again, an A/A pair that
  shows the harness's noise.

`go run ./cmd/encoding` prints each library's encoded sizes and what it does
with a truncated document, as the other comparisons do. The comparison prints
what each benchmark's call allocates too.

### What differs from TypeScript

- There are no `number[]` inputs. Go's columns are typed already.
- Only tafla's columns are read as slices of the bytes, with
  `ArrayView.Slice()`, which returns them in place when they're aligned on a
  little-endian host. FlatBuffers' Go code has no slice of a vector's
  elements but a vector of bytes, so it copies its columns element by
  element, as the Rust comparison's FlatBuffers does. protobuf decodes its
  columns into slices that move into the output.
- tafla's Go runtime has no `StructWriter` or `readUint32()`. The tafla codec
  writes the fixed-size record with `encoding/binary`, as tafla's README
  recommends for such records, and reads it with `ByteBuffer`'s getters,
  which check each read's range. tafla-gen's struct writes and reads it with
  `encoding/binary`'s functions at fixed offsets, as the `encoding/binary`
  codec does.
- Every codec returns the bytes it wrote as a slice, which escapes to the
  heap, so even the fixed-size record allocates its 12 bytes, where Rust
  returns them in an array.
- `yarn bench`'s benchmarks of each getter and setter aren't ported. The
  tests' module has microbenchmarks of the runtime's hot paths in
  `tests/generated/bench_test.go`, which compare versions of the runtime
  with benchstat.

## Libraries

| Library                                     | Why it's comparable                                                                                                                       |
| ------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------- |
| `github.com/snovader/tafla/go` (`..`)       | The reference that the others are compared with.                                                                                          |
| tafla-gen's code                            | tafla's tables, written and read by the code that tafla-gen generates.                                                                    |
| `github.com/google/flatbuffers` 25.12.19    | Zero-copy tables with offsets and vectors, like tafla.                                                                                    |
| `google.golang.org/protobuf` 1.36.12        | Go's protobuf, with packed repeated floats.                                                                                               |
| `github.com/planetscale/vtprotobuf` 0.6.0   | The same messages, written and read by generated code without reflection, which Go's projects that need speed use.                       |
| `github.com/vmihailenco/msgpack/v5` 5.4.1   | The most used MessagePack: a schemaless format, whose tables are maps with their keys.                                                    |
| `encoding/binary` (the standard library's)  | For the fixed-size record only: fields at fixed offsets, which tafla's README recommends writing and reading with the standard library. |

Each library is used as its documentation shows, and as the TypeScript
comparison uses its counterpart:

- **tafla** records nested tables in place among their parent builder's
  fields, with `SetTableFunc()` and `SetVectorFunc()`. The builders keep the
  columns and write each array in bulk into a table of its exact size.
  Readers check the document's version, return each read's error as it
  comes, as the runtime's examples do, and read columns as `ArrayView`s, whose
  `Slice()` shares the bytes. The array of structs is written with `encoding/binary` into a slice, which the
  builder takes with `SetStructArray()`, and read from `GetStructArray()`'s
  bytes with `encoding/binary`.
- **tafla-gen** is the code that tafla-gen generates from `workloads.tafla`,
  in `generated/tafla/`. A message's `MarshalBinary()` writes it into bytes
  of its exact length, in one allocation, and lays out the fields whose sizes
  the schema fixes itself, as it lays out structs. The record, whose only
  other field is a string, it writes into the bytes that
  `tafla.AppendTable()` returns, without a sizer and a writer. The others it
  counts with a `TableSizer` and writes with a `TableWriter`, which take the
  document's first scalars together and each of the vector's items, a table
  of only such fields, whole. It reads the record, the document and each of
  the vector's items whole, with their generated `Fields()`, which shares
  strings and columns with the bytes, as tafla's readers do. It takes the
  workloads' types, as the other codecs do, so it collects the generated
  fields of the vector's items into a slice in each call, as the protobuf
  codecs collect their messages. The generated getters of the vector and of
  the array of structs return views, which read each item when it's asked for.
  The codec reads every item of the vector through its view's `All()`, and the
  structs of the array at once with its view's `Slice()`, which returns them
  in place where the bytes are aligned to them in memory, as they are here.
  Every suite has a copy of the schema; after changing them, regenerate the
  code as `rust/gen/README.md` describes.
- **FlatBuffers** builds each message with a new `Builder`, which starts
  empty and doubles as it grows, and each table with the functions that
  `flatc --go` generates from `schemas/workloads.fbs`, in `generated/fbs/`.
  Columns are written element by element, back to front, as the builder
  writes vectors, and so is the vector of structs. A message is the
  builder's bytes from `FinishedBytes()`, without a copy. Readers read the
  root with `GetRootAs…()`, and copy columns through their per-element
  accessors. Go's FlatBuffers has no verifier, so it reads untrusted bytes
  unchecked, and panics where they're truncated, as `cmd/encoding` shows,
  where the Rust and TypeScript comparisons verify each message.
- **protobuf** builds the messages that `protoc-gen-go` generates from
  `schemas/messages.proto`, in `generated/pb/`, from the input, and writes
  them with `proto.Marshal()`, which sizes the bytes first, and reads them
  with `proto.Unmarshal()`. Its messages hold their strings and repeated
  fields, so building one shares the input's columns, and the decoded
  columns move into the output. protobuf has no structs, so the array of
  them is a repeated message of the three floats, which protobuf writes as
  fixed32s.
- **vtprotobuf** writes and reads the same messages with the
  `MarshalVT()` and `UnmarshalVT()` that `protoc-gen-go-vtproto` generates
  beside them.
- **msgpack** packs the workloads' own types through reflection over their
  tags, with `msgpack.Marshal()`, which packs a struct as a map with its
  field names, as msgpackr packs an object, and reads them with
  `msgpack.Unmarshal()`. The array of structs packs each struct, and the
  table that holds them, as an array of its fields in order, without their
  names, as every language's MessagePack codec packs that fixed shape. It
  packs an `int64` and a `uint32` in their full width, where rmp-serde and
  msgpackr pack the smallest integer that holds the value, so its record and
  documents are a few bytes longer.
- **encoding/binary** writes the fixed-size record into 12 new bytes with
  `binary.LittleEndian`'s `PutUint32()`, and reads it with `Uint32()`.

A field that FlatBuffers or protobuf bytes leave out reads as nil, which
their codecs return as an error, since every workload has it.

## Method

`cmd/comparison` times each benchmark's variants, one per library,
interleaved, in `harness/`, as the TypeScript harness does: every round runs
each variant once, in alternating order, and the fastest round counts. Load
from other processes then slows every variant alike, and only for some
rounds. A round of a variant repeats its call for about 20 ms, counted from
its first calls and then again from its fastest of 3 warm-up rounds, and the
report takes the fastest of 30 rounds, per call. It prints the median round
too, which load moves away from the fastest.

- Every variant has a timing loop of its own, a closure that calls its
  library's method, so that nothing is shared between their loops; Go has no
  JIT feedback to share. Each call goes through the codec's interface, an
  indirect call of about a nanosecond, which the variants share.
- Each round starts after a garbage collection, as `go test`'s benchmarks
  do, so that one variant's garbage isn't collected in another's round, and
  the collections that a variant's allocations cause within its round count
  as its own.
- Results are stored in package-level variables, so that the compiler can't
  leave a call out.
- After the rounds, the harness counts what each variant's calls allocate,
  from `runtime.MemStats`: the mallocs and the bytes they ask for, over the
  same number of calls as a round, the fewest of three samples, divided
  among them.
- Before timing, every benchmark checks that its codec reads back exactly
  what it wrote, with floats compared by their bits, and that it sums column
  `a` to the same bits. `go test ./...` runs `codecs/round_trip_test.go`,
  which checks every codec on every workload, and the workloads' and the
  item reads' tests.

`go test -bench` would run one benchmark after another, so a burst of load
could slow one library's benchmark and not another's.

## Running

```sh
uptime
go run ./cmd/comparison
go run ./cmd/encoding
```

The full run takes about a minute and a half. `-run` takes a regular expression of the
benchmarks to run, `-rounds`, `-warmup` and `-sample` change the method, and
`-quick` runs every benchmark briefly, to check that it works, as CI does.

```sh
go run ./cmd/comparison -run 'document of 100,000 rows'
```

After changing `schemas/`, regenerate the libraries' code, with `flatc` and
`protoc`, and `protoc-gen-go` and `protoc-gen-go-vtproto` from `go install`:

```sh
flatc --go -o generated schemas/workloads.fbs
protoc --proto_path=schemas --go_out=generated/pb --go_opt=paths=source_relative \
    --go-vtproto_out=generated/pb --go-vtproto_opt=paths=source_relative \
    --go-vtproto_opt=features=marshal+unmarshal+size schemas/messages.proto
```

## Code checks

`gofmt`, `go vet` and `staticcheck` check the module, as they check the
runtime's. The libraries' generated code keeps their generators' style, which
they rather than this module would change.

## Limitations

- Only a little-endian host was timed. On a big-endian host, tafla's
  `Slice()` copies its columns, swapping their bytes.
- Only macOS's memory and Go's allocator were timed, which every library's
  allocations depend on, and Go's garbage collector, whose cost each
  variant's rounds include as their allocations cause it.
- FlatBuffers' builder starts empty and doubles by copying. A builder with
  the message's capacity, or reused with `Reset()`, would make writing
  cheaper, but the TypeScript comparison doesn't reuse its builder either.
- msgpack's reflection is slower than code generated for the types, as
  `tinylib/msgp` generates it, which the comparison leaves out, as it does
  MessagePack's other libraries.

## Results

One full run on 2026-11-11, on an Apple M2 Pro with 32 GB in macOS 27.0.1,
with Go 1.27.4. It took 1 minute 17 seconds, after the quick pass. Other
processes kept the 1-minute load average at 3.98 when it started and 4.41
when it ended, on the 10 cores. The median round was within 5% of the
fastest for 60 of the 102 benchmarks' variants, and within 10% for 93, with
a median of 4.2% and a largest of 25.9%, vtprotobuf's write of the vector.
The A/A pair agreed within 0.3%.

Fastest round in µs per call, and its ratio to tafla's:

| Benchmark                                          | tafla   | tafla-gen        | flatbuffers       | protobuf         | vtprotobuf       | msgpack            | encoding/binary  |
| -------------------------------------------------- | ------: | ---------------: | ----------------: | ---------------: | ---------------: | -----------------: | ---------------: |
| record: serialize                                  |  0.0874 |  0.0555 (0.635×) |    0.211 (2.416×) |  0.0929 (1.063×) |  0.0253 (0.290×) |     0.174 (1.989×) |                  |
| record: deserialize                                |  0.0274 |  0.0300 (1.095×) |   0.0280 (1.025×) |  0.0991 (3.621×) |  0.0275 (1.005×) |     0.251 (9.171×) |                  |
| document of 100 rows: serialize                    |   0.284 |   0.202 (0.710×) |    2.078 (7.313×) |   0.803 (2.826×) |   0.683 (2.405×) |     5.888 (20.72×) |                  |
| document of 100 rows: deserialize                  |  0.0615 |  0.0663 (1.078×) |    1.472 (23.92×) |   0.975 (15.85×) |   0.968 (15.73×) |     8.404 (136.6×) |                  |
| document of 100 rows: sum a column per element     |  0.0780 |  0.0798 (1.023×) |    0.446 (5.718×) |   1.075 (13.78×) |   1.045 (13.39×) |     8.512 (109.1×) |                  |
| document of 100,000 rows: serialize                |   45.00 |   44.29 (0.984×) |   1306.7 (29.04×) |   585.3 (13.01×) |   642.2 (14.27×) |    5186.4 (115.2×) |                  |
| document of 100,000 rows: deserialize              |  0.0610 |  0.0658 (1.078×) | 1261.1 (20678.6×) | 692.2 (11350.3×) | 884.5 (14503.1×) | 7304.2 (119769.2×) |                  |
| document of 100,000 rows: sum a column per element |   88.66 |   88.87 (1.002×) |    413.8 (4.667×) |   786.7 (8.873×) |   960.6 (10.83×) |    7385.9 (83.30×) |                  |
| vector of 100 nested tables: serialize             |   9.719 |   2.630 (0.271×) |    5.807 (0.597×) |   7.257 (0.747×) |   3.520 (0.362×) |     10.46 (1.076×) |                  |
| vector of 100 nested tables: deserialize           |   1.609 |   2.127 (1.322×) |    2.286 (1.421×) |   9.006 (5.597×) |   3.888 (2.417×) |     15.14 (9.409×) |                  |
| array of 100 structs: serialize                    |   0.341 |   0.311 (0.910×) |    1.981 (5.804×) |   4.199 (12.30×) |   1.994 (5.843×) |     7.953 (23.30×) |                  |
| array of 100 structs: deserialize                  |   0.154 |   0.220 (1.423×) |    0.693 (4.492×) |   5.564 (36.04×) |   2.409 (15.61×) |     11.51 (74.55×) |                  |
| fixed-size record: serialize                       | 0.00885 | 0.00929 (1.050×) |    0.148 (16.78×) |  0.0656 (7.410×) |  0.0146 (1.647×) |     0.147 (16.63×) | 0.00879 (0.993×) |
| fixed-size record: deserialize                     | 0.00322 | 0.00289 (0.896×) |   0.0120 (3.717×) |  0.0616 (19.11×) |  0.0130 (4.030×) |     0.195 (60.57×) | 0.00194 (0.603×) |

- tafla reads a document in 61 ns, at 100 rows and at 100,000 alike, since
  its columns are slices of the bytes, and tafla-gen in 66 ns. FlatBuffers
  copies 100,000 rows' columns element by element in 1.26 ms, and protobuf,
  vtprotobuf and msgpack decode every element, in 0.69, 0.88 and 7.3 ms.
- tafla writes the large document in 45.0 µs and tafla-gen in 44.3 µs, both
  copying the columns whole into a table of its exact length, against 0.59
  to 5.2 ms for the libraries that write each element. Rust's tafla writes
  it in 18 µs. Go's allocation of the 1.2 MB, which its runtime zeroes and
  maps, took most of a CPU profile of tafla-gen's write, and copying the
  columns 8.6% of it, though macOS's profiler attributes samples coarsely.
- Summing the 100,000 floats takes tafla and tafla-gen 88.7 µs, bound by the
  additions. FlatBuffers' accessor takes 414 µs, since it looks up the
  column in the table's vtable at every element. protobuf, vtprotobuf and
  msgpack decode the whole document first.
- tafla-gen writes the record in 55.5 ns and tafla's builder in 87 ns,
  against vtprotobuf's 25 ns and protobuf's 93 ns. About 15 ns of
  tafla-gen's is the allocation of its bytes, as of vtprotobuf's. The rest
  is the sizer's and the writer's work for each field: the compiled code
  loads and stores their fields in memory at every field, rather than
  keeping them in registers. Into reused bytes, the write took 39 ns, of
  which creating the sizer and counting the fields took 11 ns. tafla reads the record in
  27 ns, tafla-gen in 30 ns, FlatBuffers in 28 ns and vtprotobuf in 28 ns,
  with an allocation of its string.
- The vector of 100 nested tables is the shape that tafla's builders handle
  worst, as in the other languages: 9.7 µs, with a builder and an allocation
  for each of its 200 tables. tafla-gen writes it fastest, in 2.6 µs and 2
  allocations: the table's bytes, and the slice of the items' generated
  fields that the codec collects from the workload's. vtprotobuf takes
  3.5 µs, FlatBuffers 5.8 µs and protobuf 7.3 µs. tafla reads it in 1.6 µs,
  against tafla-gen's 2.1 µs, whose getter collects the items' readers into
  a slice first, FlatBuffers' 2.3 µs and vtprotobuf's 3.9 µs.
- tafla writes the 100 structs in 0.34 µs and tafla-gen in 0.31 µs, against
  9.7 and 2.6 µs for the same items as nested tables, and reads them in
  0.15 and 0.22 µs. FlatBuffers writes its vector of structs in 2.0 µs and
  reads it in 0.69 µs. protobuf and vtprotobuf write and read a message for
  each struct, in 2.0 to 5.6 µs.
- tafla and `encoding/binary` write the fixed-size record in 8.8 ns alike,
  with the same code, most of it the allocation of its 12 bytes. Reading it
  takes `encoding/binary` 1.9 ns, tafla-gen 2.9 ns and tafla's `ByteBuffer`
  getters 3.2 ns, which return an error for each read.

Bytes allocated per call, and the allocations that ask for them:

| Benchmark                                          | tafla         | tafla-gen     | flatbuffers    | protobuf      | vtprotobuf    | msgpack        | encoding/binary |
| -------------------------------------------------- | ------------: | ------------: | -------------: | ------------: | ------------: | -------------: | --------------: |
| record: serialize                                  |       240 (2) |        64 (1) |        392 (8) |       128 (2) |        48 (1) |        160 (3) |                 |
| record: deserialize                                |         0 (0) |         0 (0) |         16 (1) |        96 (2) |        16 (1) |        112 (3) |                 |
| document of 100 rows: serialize                    |     1,760 (3) |     1,504 (2) |     7,160 (17) |     1,488 (3) |     1,280 (1) |      4,193 (8) |                 |
| document of 100 rows: deserialize                  |         0 (0) |         0 (0) |      1,296 (5) |     1,472 (6) |     1,392 (5) |     2,889 (18) |                 |
| document of 100 rows: sum a column per element     |         0 (0) |         0 (0) |         32 (1) |     1,472 (6) |     1,392 (5) |     2,889 (18) |                 |
| document of 100,000 rows: serialize                | 1,204,577 (3) | 1,204,320 (2) | 7,149,305 (37) | 1,188,048 (3) | 1,187,840 (1) | 4,195,381 (20) |                 |
| document of 100,000 rows: deserialize              |         0 (0) |         0 (0) |  1,204,272 (5) | 1,204,448 (6) | 1,204,368 (5) | 2,409,548 (19) |                 |
| document of 100,000 rows: sum a column per element |         0 (0) |         0 (0) |         32 (1) | 1,204,448 (6) | 1,204,368 (5) | 2,409,652 (20) |                 |
| vector of 100 nested tables: serialize             |  29,520 (204) |     4,736 (2) |    14,368 (19) |  14,208 (203) |  14,144 (202) |      8,202 (9) |                 |
| vector of 100 nested tables: deserialize           |     1,280 (1) |     4,736 (2) |      1,280 (1) |  14,712 (210) |  14,648 (209) |      2,705 (7) |                 |
| array of 100 structs: serialize                    |     2,640 (3) |     2,560 (2) |     7,040 (16) |   9,152 (103) |   9,088 (102) |      5,385 (9) |                 |
| array of 100 structs: deserialize                  |     1,280 (1) |     1,280 (1) |      1,280 (1) |   9,912 (110) |   9,848 (109) |    6,409 (109) |                 |
| fixed-size record: serialize                       |        16 (1) |        16 (1) |        200 (6) |        80 (2) |        16 (1) |        128 (3) |          16 (1) |
| fixed-size record: deserialize                     |         0 (0) |         0 (0) |          0 (0) |        64 (1) |         0 (0) |         64 (2) |           0 (0) |

The `tafla` codec's builders allocate a builder for each table besides the
table's bytes, and `SetTable()` writes a nested builder straight into its
parent's bytes. tafla-gen allocates only the table's bytes, and the slices of
generated fields or structs that its codec collects from the workloads'
types. Neither allocates to read the record or a document: strings and
columns share the bytes.

`go run ./cmd/encoding` printed:

```
Encoded size (bytes)
                        record     doc 100  doc 100000      vector     structs       fixed
  tafla                     60        1296     1200096        3208        1208          12
  tafla-gen                 60        1296     1200096        3208        1208          12
  flatbuffers               72        1328     1200128        2832        1224          32
  protobuf                  47        1243     1187503        1900        1700          16
  vtprotobuf                47        1243     1187503        1900        1700          16
  msgpack                   57        1577     1500083        3510        1604          24
  encoding/binary                                                                       12

Truncated document of 100000 rows: deserialize()
  tafla             600048 B  returned an error: tafla: read exceeds the supplied bytes: 400000 bytes at 400092 of 600048
  tafla                 24 B  returned an error: tafla: read exceeds the supplied bytes: 8 bytes at 24 of 24
  tafla-gen         600048 B  returned an error: tafla: read exceeds the supplied bytes: 400000 bytes at 400092 of 600048
  tafla-gen             24 B  returned an error: tafla: read exceeds the supplied bytes: 8 bytes at 24 of 24
  flatbuffers       600064 B  panicked: runtime error: slice bounds out of range [800100:600064]
  flatbuffers           24 B  panicked: runtime error: index out of range [3] with length 0
  protobuf          593751 B  returned an error: proto: cannot parse invalid wire-format data
  protobuf              24 B  returned an error: proto: cannot parse invalid wire-format data
  vtprotobuf        593751 B  returned an error: unexpected EOF
  vtprotobuf            24 B  returned an error: unexpected EOF
  msgpack           750041 B  returned an error: EOF
  msgpack               24 B  returned an error: EOF
```

The sizes of tafla, FlatBuffers and protobuf are the other languages'.
msgpack packs a `uint32` and an `int64` in their full width, so its documents
are a few bytes longer than rmp-serde's in Rust. tafla, protobuf, vtprotobuf
and msgpack reject the truncated document with an error. FlatBuffers' Go
module has no verifier, so its reader panics on the truncated bytes, which
untrusted input could cause.

### Reading items through views

The four groups that read tafla's items, from the same run.

Fastest round in µs, and its ratio to `ByteBuffer`'s by hand:

| Benchmark                                       | tafla   | tafla-gen      | tafla-gen view  | tafla-gen view A/A |
| ----------------------------------------------- | ------: | -------------: | --------------: | -----------------: |
| vector of 100 nested tables: read the last item |  0.0281 | 0.614 (21.87×) | 0.0345 (1.228×) |    0.0344 (1.224×) |
| vector of 100 nested tables: sum every item     |   1.454 | 2.105 (1.448×) |  1.785 (1.228×) |     1.788 (1.230×) |
| array of 100 structs: read the last item        | 0.00974 | 0.182 (18.64×) | 0.0104 (1.064×) |    0.0104 (1.063×) |
| array of 100 structs: sum every item            |   0.117 | 0.267 (2.280×) |  0.133 (1.138×) |     0.133 (1.137×) |

Only tafla-gen's getters allocate: 3,456 bytes, a slice of the 100 items'
readers, and 1,280 bytes, a slice of the 100 structs.

- `All()` of the vector's view sums every item in 1.79 µs, against 1.45 µs
  by hand, and `Get()` reads the last item in 34 ns, against 28 ns. Both of
  the view's iterators inline into the loop. Its items' getters don't: each
  reads with `FieldOffset()` and a getter, whose cost together exceeds the
  compiler's budget for inlining, so the loop calls one getter for each of
  an item's four fields, where the hand-written loop calls one function for
  each item. The getter of the vector collects every item's reader into a
  slice first, so it takes 0.61 µs for the last item, and 2.1 µs for all.
- The view of the array of structs reads the last struct in 10.4 ns, against
  9.7 ns by hand, and sums every struct in 133 ns, against 117 ns. The getter
  collects every struct into a slice first: 182 and 267 ns.
