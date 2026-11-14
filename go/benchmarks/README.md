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

One full run on 2026-11-14, of main at 0529dbd, on an Apple M2 Pro with
32 GB in macOS 27.0.1, with Go 1.27.4. It took 1 minute 14 seconds, after
the quick pass. Other processes kept the 1-minute load average at 3.11 when
it started and 3.63 when it ended, and no higher in between, on the 10
cores. The median round was within 5% of the fastest for 80 of the 99
benchmarks' variants, and within 10% for 97, with a median of 3.1% and a
largest of 13.8%, vtprotobuf's write of the vector. The A/A pair agreed
within 0.5%. A second full run, after a run of the comparison at c43286c,
is an A/A pair of the whole run. The load average fell from 2.93 to 2.18
while it ran, and it was faster for 91 of the variants, by 2.8% at the
median. Its fastest rounds were within 5% of these for 79 of the variants,
and at most 14.5% apart, vtprotobuf's read of the vector.

Fastest round in µs per call, and its ratio to tafla's:

| Benchmark                                             | tafla   | tafla-gen        | flatbuffers       | protobuf         | vtprotobuf       | msgpack            | encoding/binary  |
| ----------------------------------------------------- | ------: | ---------------: | ----------------: | ---------------: | ---------------: | -----------------: | ---------------: |
| record: serialize                                     |  0.0850 |  0.0180 (0.212×) |    0.203 (2.382×) |  0.0915 (1.076×) |  0.0246 (0.290×) |     0.169 (1.983×) |                  |
| record: deserialize                                   |  0.0279 |  0.0199 (0.713×) |   0.0271 (0.970×) |  0.0957 (3.425×) |  0.0276 (0.987×) |     0.255 (9.115×) |                  |
| document of 100 rows: serialize                       |   0.287 |   0.188 (0.654×) |    1.914 (6.658×) |   0.780 (2.713×) |   0.651 (2.265×) |     5.965 (20.75×) |                  |
| document of 100 rows: deserialize                     |  0.0610 |  0.0609 (0.998×) |    1.436 (23.55×) |   0.945 (15.49×) |   0.950 (15.58×) |     8.194 (134.4×) |                  |
| document of 100 rows: sum a column per element        |  0.0769 |  0.0746 (0.970×) |    0.445 (5.787×) |   1.035 (13.46×) |   1.011 (13.15×) |     8.464 (110.1×) |                  |
| document of 100,000 rows: serialize                   |   42.39 |   42.33 (0.999×) |   1276.5 (30.11×) |   566.0 (13.35×) |   614.4 (14.49×) |    5044.3 (119.0×) |                  |
| document of 100,000 rows: deserialize                 |  0.0622 |  0.0621 (0.999×) | 1265.9 (20362.2×) | 674.1 (10842.8×) | 864.4 (13903.5×) | 7144.8 (114922.8×) |                  |
| document of 100,000 rows: sum a column per element    |   86.38 |   86.68 (1.004×) |    413.2 (4.783×) |   748.9 (8.670×) |   944.4 (10.93×) |    7155.6 (82.84×) |                  |
| document of 100,000 rows: serialize into reused bytes |         |            16.41 |                   |                  |                  |                    |                  |
| vector of 100 nested tables: serialize                |   5.415 |  0.507 (0.0935×) |    5.808 (1.072×) |   7.435 (1.373×) |   3.769 (0.696×) |     10.52 (1.943×) |                  |
| vector of 100 nested tables: deserialize              |   1.664 |   1.077 (0.648×) |    2.266 (1.362×) |   9.078 (5.456×) |   3.949 (2.374×) |     16.02 (9.630×) |                  |
| array of 100 structs: serialize                       |   0.341 |   0.321 (0.940×) |    1.988 (5.825×) |   4.383 (12.84×) |   2.020 (5.917×) |     8.274 (24.24×) |                  |
| array of 100 structs: deserialize                     |   0.153 |   0.124 (0.805×) |    0.686 (4.472×) |   5.526 (36.01×) |   2.314 (15.08×) |     11.82 (77.03×) |                  |
| fixed-size record: serialize                          | 0.00886 | 0.00862 (0.973×) |    0.153 (17.21×) |  0.0652 (7.364×) |  0.0152 (1.715×) |     0.143 (16.16×) | 0.00881 (0.995×) |
| fixed-size record: deserialize                        | 0.00323 | 0.00204 (0.629×) |   0.0122 (3.768×) |  0.0634 (19.61×) |  0.0128 (3.971×) |     0.203 (62.65×) | 0.00197 (0.609×) |

- tafla reads a document in 61 ns, at 100 rows and at 100,000 alike, since
  its columns are slices of the bytes, and tafla-gen in 61 to 62 ns, reading
  it whole with its generated `Fields()`. FlatBuffers copies 100,000 rows'
  columns element by element in 1.27 ms, and protobuf, vtprotobuf and
  msgpack decode every element, in 0.67, 0.86 and 7.1 ms.
- tafla writes the large document in 42.4 µs and tafla-gen in 42.3 µs, both
  copying the columns whole into a table of its exact length, against 0.57
  to 5.0 ms for the libraries that write each element. Into reused bytes,
  tafla-gen writes it in 16.4 µs, without allocating, so the other 26 µs of
  its write into new bytes go to allocating the 1.2 MB, which Go zeroes or
  maps fresh from the system, and to the garbage collection that it causes.
- Summing the 100,000 floats takes tafla and tafla-gen 86 µs, bound by the
  additions. FlatBuffers' accessor takes 413 µs, since it looks up the
  column in the table's vtable at every element. protobuf, vtprotobuf and
  msgpack decode the whole document first.
- tafla-gen writes the record fastest, in 18.0 ns, against vtprotobuf's
  25 ns, tafla's builder's 85 ns and protobuf's 92 ns. It lays out the
  record's fields at positions that the generator works out, in the bytes
  that `AppendTable()` returns, without a sizer and a writer, and allocates
  only those 64 bytes, as vtprotobuf allocates its 48. It reads the record
  fastest too, in 20 ns, whole with `Fields()`, against tafla's 28 ns,
  FlatBuffers' 27 ns and vtprotobuf's 28 ns, with an allocation of its
  string.
- The vector of 100 nested tables is the shape that tafla's builders handle
  worst, as in the other languages. The `tafla` codec records the items in
  place, in 5.4 µs and 6 allocations, against 9.9 µs and 204 allocations
  from a builder for each item and each item's pair, as c43286c's
  comparison nested them, timed straight after this run. tafla-gen writes
  it fastest, in 0.51 µs and 2 allocations: the table's bytes, and the slice
  of the items' generated fields that the codec collects from the
  workload's. Each item is a table of only fixed-size fields, which the
  generated code writes whole, at positions that the generator works out.
  vtprotobuf takes 3.8 µs, FlatBuffers 5.8 µs and protobuf 7.4 µs. tafla-gen
  reads it fastest too, in 1.08 µs, each item whole through its getter's
  view, against tafla's 1.66 µs, FlatBuffers' 2.27 µs and vtprotobuf's
  3.95 µs.
- tafla writes the 100 structs in 0.34 µs and tafla-gen in 0.32 µs, against
  5.4 and 0.51 µs for the same items as nested tables. tafla reads them in
  0.15 µs, with `encoding/binary` from `GetStructArray()`'s bytes, and
  tafla-gen in 0.12 µs, from its view's `Slice()`, which returns the structs
  in place. FlatBuffers writes its vector of structs in 2.0 µs and reads it
  in 0.69 µs. protobuf and vtprotobuf write and read a message for each
  struct, in 2.0 to 5.5 µs.
- tafla, tafla-gen and `encoding/binary` write the fixed-size record in 8.6
  to 8.9 ns, with the same calls, most of it the allocation of its 12 bytes.
  Reading it takes `encoding/binary` and tafla-gen 2.0 ns, and tafla's
  `ByteBuffer` getters 3.2 ns, which return an error for each read.

Bytes allocated per call, and the allocations that ask for them, on average
over the calls where they don't all allocate alike:

| Benchmark                                             | tafla            | tafla-gen     | flatbuffers    | protobuf      | vtprotobuf    | msgpack           | encoding/binary |
| ----------------------------------------------------- | ---------------: | ------------: | -------------: | ------------: | ------------: | ----------------: | --------------: |
| record: serialize                                     |          240 (2) |        64 (1) |        392 (8) |       128 (2) |        48 (1) |           160 (3) |                 |
| record: deserialize                                   |            0 (0) |         0 (0) |         16 (1) |        96 (2) |        16 (1) |           112 (3) |                 |
| document of 100 rows: serialize                       |        1,840 (3) |     1,408 (1) |     7,160 (17) |     1,488 (3) |     1,280 (1) |         4,193 (8) |                 |
| document of 100 rows: deserialize                     |            0 (0) |         0 (0) |      1,296 (5) |     1,472 (6) |     1,392 (5) |        2,889 (18) |                 |
| document of 100 rows: sum a column per element        |            0 (0) |         0 (0) |         32 (1) |     1,472 (6) |     1,392 (5) |        2,889 (18) |                 |
| document of 100,000 rows: serialize                   | 1,204,657 (3.01) | 1,204,224 (1) | 7,149,306 (37) | 1,188,048 (3) | 1,187,840 (1) | 4,194,909 (19.33) |                 |
| document of 100,000 rows: deserialize                 |            0 (0) |         0 (0) |  1,204,272 (5) | 1,204,448 (6) | 1,204,368 (5) |    2,409,548 (19) |                 |
| document of 100,000 rows: sum a column per element    |            0 (0) |         0 (0) |         32 (1) | 1,204,448 (6) | 1,204,368 (5) |    2,409,548 (19) |                 |
| document of 100,000 rows: serialize into reused bytes |                  |         0 (0) |                |               |               |                   |                 |
| vector of 100 nested tables: serialize                |       17,552 (6) |     4,736 (2) |    14,368 (19) |  14,208 (203) |  14,144 (202) |         8,202 (9) |                 |
| vector of 100 nested tables: deserialize              |        1,280 (1) |     1,280 (1) |      1,280 (1) |  14,712 (210) |  14,648 (209) |         2,705 (7) |                 |
| array of 100 structs: serialize                       |        2,640 (3) |     2,560 (2) |     7,040 (16) |   9,152 (103) |   9,088 (102) |         5,385 (9) |                 |
| array of 100 structs: deserialize                     |        1,280 (1) |     1,280 (1) |      1,280 (1) |   9,912 (110) |   9,848 (109) |       6,409 (109) |                 |
| fixed-size record: serialize                          |           16 (1) |        16 (1) |        200 (6) |        80 (2) |        16 (1) |           128 (3) |          16 (1) |
| fixed-size record: deserialize                        |            0 (0) |         0 (0) |          0 (0) |        64 (1) |         0 (0) |            64 (2) |           0 (0) |

The `tafla` codec allocates the builder and the table's bytes, and what the
builder needs to record nested tables in place: 6 allocations for the
vector's 200 tables, where a builder for each took 204. tafla-gen allocates
only the table's bytes, and the slices of generated fields or structs that
its codec collects from the workloads' types, and nothing into reused bytes.
Neither allocates to read the record or a document: strings and columns
share the bytes.

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

| Benchmark                                       | tafla   | tafla-gen        | tafla-gen A/A    |
| ----------------------------------------------- | ------: | ---------------: | ---------------: |
| vector of 100 nested tables: read the last item |  0.0286 |  0.0241 (0.843×) |  0.0242 (0.846×) |
| vector of 100 nested tables: sum every item     |   1.457 |   0.893 (0.613×) |   0.893 (0.613×) |
| array of 100 structs: read the last item        | 0.00967 | 0.00749 (0.774×) | 0.00749 (0.774×) |
| array of 100 structs: sum every item            |   0.115 |  0.0697 (0.604×) |  0.0698 (0.605×) |

None of them allocates.

- tafla-gen's views read the items faster than `ByteBuffer`'s getters by
  hand. `All()` of the vector's view sums every item in 0.89 µs, against
  1.46 µs by hand, and `Get()` reads the last item in 24 ns, against 29 ns.
  Each item's generated `Fields()` reads its fields' offsets with
  `Uint32FieldOffset()`, which inlines, at the slots that the generator
  knows, where the hand-written loop calls `ByteBuffer.FieldOffset()` for
  each.
- The view of the array of structs returns the last struct by value in
  7.5 ns, against 9.7 ns by hand, and its `Slice()` returns every struct in
  place, which it sums in 70 ns, against 115 ns by hand with
  `encoding/binary`.
