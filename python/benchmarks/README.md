# Benchmarks

`python -m tafla_benchmarks` compares `tafla` with other Python libraries
that encode small tables and numeric columns, in pyperf benchmarks. It runs the
workloads of the TypeScript comparison in `typescript/benchmarks/`, with the
same values, so that the languages' numbers compare. Run the commands below
from `python/`.

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

Each shape is serialized and deserialized, with columns read back as each
library returns them: tafla's as memoryviews of the bytes, FlatBuffers' as
numpy arrays that view them, protobuf's as the message's repeated fields, and
MessagePack's as memoryviews of the bytes that it unpacks. The documents'
columns are `array`s, and documents are also serialized from lists, as parsed
JSON would have them. Column `a` of each document is summed through each
library's per-element accessor. `workloads.py` ports the TypeScript generator,
so the inputs are the same values, which `tests/test_workloads.py` checks
against values that the TypeScript generator printed.

Four more groups time only tafla's bytes of the vector of nested tables and
of the array of structs, read in three ways: **read the last item**, which a
reader can find without reading the others, and **sum every item**. Each way
sums the three float32s of the items it reads, rather than creating objects
of its own, and `item_access.py` holds them:

- **tafla** reads with `ByteBuffer`'s getters by hand: `get_vector_offset()`
  of the last item or `get_vector_offsets()` of every item, and
  `get_struct_array()` with `Struct("<fff")`.
- **tafla-gen** reads with the generated getters, `items()`, which read every
  item into a list first, a `Triple` for each struct.
- **tafla-gen view** reads with the generated views, `items_view()`: `[-1]`
  reads the last item alone, and `for` iterates over every item, creating its
  reader or `Triple` as it reaches it.
- **tafla-gen view A/A** reads with the views again, an A/A pair that shows
  the harness's noise.

`python -m tafla_benchmarks.encoding` prints each library's encoded sizes and
what it does with a truncated document, as the TypeScript comparison does, and
`python -m tafla_benchmarks.allocation` prints the memory that each
benchmark's call allocates.

### What differs from TypeScript

- Columns are read back as each library returns them, rather than converted to
  one type. Python has no typed array that every library returns, as
  JavaScript's typed arrays are, and converting would time the conversion
  rather than the library.
- Lists stand in for `number[]`: the groups that serialize documents from
  `number[]` serialize them from lists here.
- tafla's Python runtime has no `StructWriter` or `readUint32()`. Its
  documentation recommends the standard library's `struct` for records at
  fixed offsets, so tafla's codec packs the fixed-size record field by field
  with `struct` into a new 12-byte `bytearray`, and reads it with
  `ByteBuffer`'s getters at fixed positions. The `struct` codec packs and
  unpacks the whole record in one call.
- FlatBuffers' Python runtime copies vectors in bulk only from and to numpy
  arrays, so its columns are written and read through numpy. Without numpy, it
  has only a call per element, which is all it has for a vector of structs.
- msgpack's columns are extension types of their arrays' bytes, which this
  suite defines, as msgpackr's typed arrays are msgpackr's own.
- flatbuffers' latest Python release is 25.12.19, where the TypeScript
  comparison uses flatbuffers 25.9.23.
- `yarn bench`'s benchmarks of each getter and setter aren't ported.
- The variants run in worker processes of their own rather than interleaved
  in one process; see [Method](#method).

## Libraries

| Library                                    | Why it's comparable                                                                                                                        |
| ------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------ |
| `tafla` (`core/`)                          | The reference that the others are compared with.                                                                                           |
| tafla-gen's code                           | tafla's tables, written and read by the code that tafla-gen generates.                                                                     |
| `flatbuffers` 25.12.19, with `numpy` 2.5.4 | Zero-copy tables with offsets and vectors, like tafla.                                                                                     |
| `protobuf` 7.36.2                          | The most used schema-based codec, with packed repeated floats. Its wheels parse and serialize in C, with upb.                              |
| `msgpack` 1.2.4                            | MessagePack's Python implementation, with a C extension: a schemaless format, whose tables are maps with their keys.                       |
| `struct` (the standard library's)          | For the fixed-size record only: fields at fixed offsets, packed and unpacked in one call, which tafla's documentation recommends for them. |

Each library is used as its documentation shows, and as the TypeScript
comparison uses its counterpart:

- **tafla** nests tables from their builders and reads columns as views of the
  bytes. Arrays of the columns' element types are copied as bytes, and lists
  are converted as `array()` converts them. Summing a column indexes its view,
  as tafla's documentation advises over a getter call per element. The array
  of structs is packed into a `bytearray` and unpacked from
  `get_struct_array()`'s view with a precompiled `Struct("<fff")`, as
  tafla-gen's code does.
- **tafla-gen** is the code that tafla-gen generates from
  `generated/workloads.tafla`, in `generated/tafla/`. A message's
  `serialize()` appends every field, nested tables included, to a
  `TableWriter`'s `bytearray` in one call, and returns a view of it. It
  reads the record and the vector's items with `fields()`, which reads a
  table of fixed layout at once into the `TypedDict` that it's written from,
  and the document with the getters, columns as views. It takes the
  workloads' dataclasses, as the other codecs do, so it creates the
  generated fields' dicts, and the frozen `Sample` dataclass that the
  generated struct is, from them in each call, as the protobuf codec builds
  its messages. The array of structs is written from the workload's own
  items, whose attributes `TripleFields`, the struct's protocol of fields,
  types, as an application's objects would be. Reading creates the
  workloads' dataclasses from what the generated code reads, after the
  generated reader of the array of structs creates a `Triple` for each. Every
  suite has a copy of the schema; after changing them, regenerate the code as
  `rust/gen/README.md` describes.
- **FlatBuffers** builds each message with a new `Builder` of the default
  size, adding fields one by one with the generated functions, and reads them
  with the generated accessors. Columns are written with
  `CreateNumpyVector()`, from numpy arrays that view the `array`s or convert
  the lists, and read with the generated `AsNumpy()` accessors, which view the
  bytes. A vector of structs, which has no numpy accessors, is started, filled
  with `CreateTriple()` from its last item to its first, and ended, and its
  structs are read one at a time. The modules in `generated/flatbuffers/` are
  written by hand in the shape that `flatc --python` generates from
  `generated/workloads.fbs`, with only what the benchmarks use, since the
  FlatBuffers project doesn't publish flatc on PyPI, where uv installs tools
  from. As in flatc's, an accessor of a nested table or of a struct in a vector
  imports its class on every call.
- **protobuf** builds messages from keyword arguments and parses them with
  `FromString()`. Nested messages are built in place, as protobuf's tutorial
  builds them: passing a nested message to its parent's constructor copies it,
  which made writing the vector of nested tables 2.6 times slower and the
  large document 13% slower. Repeated fields are extended from the columns,
  which converts them element by element. protobuf has no structs, so the
  array of them is a repeated message of the three floats, which protobuf
  writes as fixed32s; packed repeated floats would make a column of each field,
  which the documents time already.
- **MessagePack** packs a table as a map with its keys, and a document as one
  map without a nested one for its columns, as msgpackr packs them. Floats pack
  as float64s, as msgpack packs Python's floats and msgpackr packs numbers. One
  `Packer` serves every message, as msgpackr reuses its packer, and
  `unpackb()` reads each. Columns pack as extensions of their arrays' bytes,
  as msgpack's documentation packs an `array`, and lists are converted to
  arrays first. The extension hook returns a memoryview of the bytes that the
  unpacker copied out, which took half the time of copying them again into an
  `array` for 100,000 rows. The array of structs packs each struct, and the
  table that holds them, as an array of its fields in order, without their
  keys, as rmp-serde's `to_vec()` packs them.
- **struct** packs and unpacks the fixed-size record with one precompiled
  `Struct("<Iff")`.

## Method

pyperf 2.10.0 runs each benchmark in worker processes of its own. The first
calibrates the number of calls that one value times, so that it takes at least
100 ms, and each of the others runs a warm-up and then 3 values. A full run
runs the suite 4 times, with 5 workers per benchmark, and appends their values
to one file: 20 workers and 60 values per benchmark. The runs alternate the
order of the codecs in each group, so that each benchmark's workers are spread
over the whole run. `tafla_benchmarks.report` then prints each benchmark's
fastest value, and its ratio to tafla's, as the TypeScript comparison reports
the fastest round, since load from other processes only slows values down.

- A worker runs one benchmark, so CPython's specializing interpreter
  specializes its timing loop for one operation, as the TypeScript harness's
  separate timer loops ensure for V8. CPython doesn't eliminate unused results,
  so the loop drops each result, which frees it before the next call, as a
  caller's would be.
- A worker prepares its benchmark's input when it first times it, after
  checking that the codec reads back exactly what it wrote, with floats
  compared by their bits. `tests/test_round_trips.py` checks every codec on
  every workload.
- Every deserialization creates the same dataclasses from the values it read,
  from positional arguments, which took 65 ns for the fixed-size record,
  against 190 ns from keywords.
- The garbage collector stays enabled, as it is for callers.
- `tafla_benchmarks.allocation` runs each benchmark's call with tracemalloc
  tracing, apart from the timings, since tracing slows every allocation. It
  prints the most bytes that a call held at once above what was allocated
  before it, its result included, less what reading the traced memory takes:
  the fewest over 5 calls, after 20 that let CPython specialize the code.
  tracemalloc traces every block that Python's allocators hand out, objects and
  buffers alike, but what is held at once, not every allocation: a call that
  frees a buffer before allocating the next counts only the larger.

pyperf doesn't interleave the codecs as the TypeScript harness does: it runs
one benchmark's workers after another's. Within a run, a burst of load can slow
one codec's workers and not another's, but the 4 runs spread each benchmark
over the whole run, and its fastest value is unaffected unless the load lasts
for all of them. Check `uptime` before a run, and compare the fastest values
with pyperf's means, which the report summarizes.

## Running

```sh
uptime
for order in "" --reverse "" --reverse; do
    uv run --python 3.14+gil python -m tafla_benchmarks --processes 5 --append results.json $order
done
uv run --python 3.14+gil python -m tafla_benchmarks.report results.json
uv run --python 3.14+gil python -m tafla_benchmarks.allocation
```

The full run takes about 30 minutes, and the allocation report about 10
seconds, which CI runs too. `--append` adds to an existing file, so
remove the last run's `results.json` first. `--filter <regex>` runs only the
groups whose name matches, and `--reverse` runs each group's codecs in reverse
order. pyperf's own options apply too, and its commands read the results:
`python -m pyperf stats results.json` describes each benchmark's values, for
example.

A quick run of one worker per benchmark, which calls each operation once,
checks that every benchmark works in about 16 seconds, most of which its 98
workers spend starting and importing numpy. CI runs it after the round-trip
test, on Python 3.14. Its numbers mean little.

```sh
uv run python -m tafla_benchmarks --debug-single-value --quiet
```

## Generated code

`generated/messages_pb2.py` and its stub `messages_pb2.pyi` are generated from
`generated/messages.proto` by the protoc of grpcio-tools 1.85.1, protoc 35.1,
which the `protoc` dependency group installs:

```sh
uv run --package tafla-benchmarks --group protoc python -m grpc_tools.protoc -I benchmarks/src --python_out=benchmarks/src --pyi_out=benchmarks/src benchmarks/src/tafla_benchmarks/generated/messages.proto
```

They're checked in, so that the tests and checks run without protoc, although
protoc marks its output as not meant to be: protobuf's runtime accepts
generated code of its own major version that isn't newer than it, so they need
regenerating when protobuf's major version changes.

## Code checks

Ruff and ty check the benchmarks with the workspace's settings, except for
generated code:

- Ruff skips protoc's `messages_pb2.py` and `messages_pb2.pyi`, which are its
  verbatim output, and ty skips the stub, which leaves `Mapping`'s type
  arguments out.
- The FlatBuffers modules keep the shape of flatc's code, which Ruff formats:
  modules named after their tables, classes that derive from `object` and no
  annotations, which Ruff's rules for them ignore. ty ignores their use of the
  `flatbuffers` package's submodules, which the package imports itself.

## Limitations

- pyperf doesn't interleave the codecs, as above.
- The Apple M2 Pro has 6 performance and 4 efficiency cores, and macOS can move
  a worker to an efficiency core when others are busy.
- Only CPython 3.14's build with the GIL was timed. CI runs the tests on 3.14
  and 3.15, and uv picks free-threaded builds on some machines, which these
  numbers don't describe.
- protobuf's wheels use upb. Its pure-Python implementation, which
  `PROTOCOL_BUFFERS_PYTHON_IMPLEMENTATION=python` selects, would be much slower,
  and isn't timed.
- msgpack's column extensions are in the host's byte order, as msgpackr's are,
  and are this suite's own, so another MessagePack reader would see them as
  unknown extensions.
- FlatBuffers' builder starts at 1 KiB and doubles by copying. Reusing a
  `Builder` with `Clear()` would make small messages cheaper, but the
  TypeScript comparison doesn't reuse it either.
- tafla-gen's codec converts the workloads' values in each call, which a
  caller that holds them in the generated types wouldn't. It writes the
  record, the documents and the array of structs from them directly, but
  creates a dict for each of the vector's items and its pair, and a frozen
  `Sample` for the fixed-size record. Reading, it creates the workloads'
  dataclasses from `fields()`' dicts for the record and the vector's items,
  and from the `Triple` that the generated reader creates for each struct.
  Creating a generated struct takes about twice as long as creating the
  workloads' own dataclasses, which aren't frozen.
- The peak bytes aren't the bytes that a call allocates in all, which
  tracemalloc doesn't count: CPython frees most temporary objects as soon as
  they're unused, so a call that creates many in turn holds few at once.
- tracemalloc doesn't see objects that CPython reuses from its freelists,
  such as dicts, floats and tuples, nor memory that a C library allocates
  itself, such as upb's arena. So protobuf's and msgpack's small peaks are
  lower bounds: protobuf's read of the large document peaks at 625 bytes,
  though it copies 1.2 MB of columns into its containers.

## Results

One full run on 2026-11-10, on an Apple M2 Pro with 32 GB in macOS 27.0.1,
with Homebrew's CPython 3.14.10, built with the GIL, PGO, LTO and the
tail-calling interpreter, without the JIT, followed by
`tafla_benchmarks.allocation`. Its four runs each ran one group at a time,
with `--filter`, so that a group that a burst of load from other processes
reached could run again, as three did. It took 30 minutes. Other
processes kept the 1-minute load average between 1.4 and 4.4 on the 10 cores,
with a median of 1.9. pyperf's mean was within 5% of the fastest value for 93
of the 98 benchmarks. The other five serialize the large document: for
tafla's, tafla-gen's, FlatBuffers' and msgpack's, the medians were 10% to 15%
above the fastest values, and the means 11% to 17%, so most values were slower
than the fastest, rather than a few of them slowed by load, and for msgpack's
from lists, the mean was 5% above and the median 6%.

Fastest value in µs per call, and its ratio to tafla's:

| Benchmark                                          |  tafla |       flatbuffers |         protobuf |        msgpack |        struct |
| -------------------------------------------------- | -----: | ----------------: | ---------------: | -------------: | ------------: |
| record: serialize                                  |  2.139 |     8.410 (3.93×) |    0.509 (0.24×) |  0.287 (0.13×) |               |
| record: deserialize                                |  1.070 |     4.755 (4.44×) |    0.445 (0.42×) |  0.393 (0.37×) |               |
| document of 100 rows: serialize                    |  6.561 |      20.5 (3.12×) |    8.600 (1.31×) |  1.844 (0.28×) |               |
| document of 100 rows: deserialize                  |  2.525 |      12.6 (5.01×) |    1.710 (0.68×) |  1.170 (0.46×) |               |
| document of 100 rows: sum a column per element     |  3.567 |    141.9 (39.78×) |    4.796 (1.34×) |  3.742 (1.05×) |               |
| document of 100,000 rows: serialize                |   26.2 |     176.8 (6.74×) | 7545.8 (287.65×) |   55.8 (2.13×) |               |
| document of 100,000 rows: deserialize              |  2.528 |      12.7 (5.01×) |  619.3 (244.99×) |   20.2 (8.00×) |               |
| document of 100,000 rows: sum a column per element | 3061.4 | 140505.3 (45.90×) |   5043.0 (1.65×) | 3077.6 (1.01×) |               |
| vector of 100 nested tables: serialize             |  259.4 |     527.1 (2.03×) |     41.4 (0.16×) |   20.7 (0.08×) |               |
| vector of 100 nested tables: deserialize           |   74.1 |     638.2 (8.61×) |     39.4 (0.53×) |   30.3 (0.41×) |               |
| array of 100 structs: serialize                    |  9.354 |    111.3 (11.90×) |     28.4 (3.03×) |  9.079 (0.97×) |               |
| array of 100 structs: deserialize                  |   14.5 |    238.9 (16.42×) |     28.0 (1.93×) |   16.5 (1.14×) |               |
| fixed-size record: serialize                       |  0.208 |    6.490 (31.27×) |    0.452 (2.18×) |  0.225 (1.09×) | 0.073 (0.35×) |
| fixed-size record: deserialize                     |  0.559 |     3.494 (6.25×) |    0.388 (0.69×) |  0.318 (0.57×) | 0.142 (0.25×) |
| document of 100 rows: serialize lists              |   10.5 |      25.3 (2.42×) |    5.625 (0.54×) |  5.181 (0.50×) |               |
| document of 100,000 rows: serialize lists          | 2940.2 |    4904.6 (1.67×) |   4301.0 (1.46×) | 2983.1 (1.01×) |               |

- tafla reads both documents in 2.5 µs, since its columns are views, whatever
  their length. FlatBuffers' numpy views take 12.7 µs at both sizes, so its
  pure-Python accessors' calls cost that, not the columns. msgpack and
  protobuf read the small document faster, in C, but copy the large one's
  columns: msgpack in 20 µs, and protobuf, which parses them into its
  containers, in 619 µs.
- tafla writes the large document fastest, in 26 µs, copying its arrays as
  bytes; msgpack copies them twice, into extensions and then into the message.
  protobuf converts every element, which took 7.5 ms from arrays and 4.3 ms
  from lists, whose elements are Python floats already.
- Small tables cost tafla its Python calls, a few per field, as its
  documentation says: protobuf and msgpack, in C, write the record 4 to 7 times
  faster and read it 2.4 to 2.7 times faster. Only FlatBuffers, also pure
  Python, is slower.
- The vector of 100 nested tables is where tafla is furthest behind: 260 µs to
  write, against msgpack's 21 µs and protobuf's 41 µs, and 74 µs to read,
  against 30 µs and 39 µs.
- tafla writes the 100 structs in 9.4 µs, against 260 µs for the same items as
  nested tables, packing them with one `Struct` call each, and reads them in
  14.5 µs, against 74 µs, 6.4 µs of which creates the 100 dataclasses.
  msgpack, in C, takes 9.1 µs and 16.5 µs, and protobuf 28 µs each way.
  FlatBuffers creates and reads each struct with pure-Python calls, in 111 µs
  and 239 µs.
- Summing a column through a memoryview takes about 31 ns per element, for
  tafla and msgpack alike, and through protobuf's container 50 ns.
  FlatBuffers' generated accessor takes 1.4 µs per element.
- Converting lists dominates writing the large document from them, which took
  every library 3 to 5 ms.
- `struct` packs the fixed-size record in 73 ns and unpacks it in 142 ns, about
  half of which is creating the result. tafla's codec packs it field by field
  with `struct` in 208 ns, faster than protobuf and msgpack, and reads it with
  `ByteBuffer`'s getters in 559 ns. Creating the `ByteBuffer` takes 0.25 µs of
  that, about as long as the three getters. tafla's documentation recommends
  `struct` alone for records at fixed offsets, which the `struct` row times.

Every deserialization includes creating the workload's dataclasses, about
65 ns each, which the codecs share: 6.4 µs of reading the vector of 100 nested
tables, for example.

tafla-gen's codec in the same run: fastest value in µs per call, and its ratio
to tafla's:

| Benchmark                                          |  tafla |      tafla-gen |
| -------------------------------------------------- | -----: | -------------: |
| record: serialize                                  |  2.139 |  1.015 (0.47×) |
| record: deserialize                                |  1.070 |  0.969 (0.91×) |
| document of 100 rows: serialize                    |  6.561 |  3.586 (0.55×) |
| document of 100 rows: deserialize                  |  2.525 |  2.769 (1.10×) |
| document of 100 rows: sum a column per element     |  3.567 |  3.748 (1.05×) |
| document of 100,000 rows: serialize                |   26.2 |   23.1 (0.88×) |
| document of 100,000 rows: deserialize              |  2.528 |  2.792 (1.10×) |
| document of 100,000 rows: sum a column per element | 3061.4 | 3061.8 (1.00×) |
| vector of 100 nested tables: serialize             |  259.4 |   64.4 (0.25×) |
| vector of 100 nested tables: deserialize           |   74.1 |   60.8 (0.82×) |
| array of 100 structs: serialize                    |  9.354 |  8.384 (0.90×) |
| array of 100 structs: deserialize                  |   14.5 |   28.8 (1.98×) |
| fixed-size record: serialize                       |  0.208 |  0.248 (1.20×) |
| fixed-size record: deserialize                     |  0.559 |  0.356 (0.64×) |
| document of 100 rows: serialize lists              |   10.5 |  7.339 (0.70×) |
| document of 100,000 rows: serialize lists          | 2940.2 | 2966.3 (1.01×) |

Peak bytes per call, as `tafla_benchmarks.allocation` traces them:

| Benchmark                                          |     tafla | tafla-gen | flatbuffers |  protobuf |   msgpack | struct |
| -------------------------------------------------- | --------: | --------: | ----------: | --------: | --------: | -----: |
| record: serialize                                  |       673 |       525 |       1,490 |       192 |        86 |        |
| record: deserialize                                |       505 |       545 |         400 |       233 |       121 |        |
| document of 100 rows: serialize                    |     2,033 |     1,761 |       3,770 |     1,468 |     1,517 |        |
| document of 100 rows: deserialize                  |     1,057 |     1,145 |       1,905 |       625 |     2,684 |        |
| document of 100 rows: sum a column per element     |       632 |       680 |         400 |       208 |     2,684 |        |
| document of 100,000 rows: serialize                | 1,200,833 | 1,200,561 |   3,297,674 | 3,998,040 | 1,200,323 |        |
| document of 100,000 rows: deserialize              |     1,057 |     1,145 |       1,905 |       625 | 1,201,484 |        |
| document of 100,000 rows: sum a column per element |       632 |       680 |         400 |       208 | 1,201,484 |        |
| vector of 100 nested tables: serialize             |    39,023 |    26,923 |       8,186 |     2,221 |     9,471 |        |
| vector of 100 nested tables: deserialize           |    12,464 |    17,256 |      11,784 |    11,770 |    15,968 |        |
| array of 100 structs: serialize                    |     3,066 |     2,618 |       3,666 |     1,941 |     3,709 |        |
| array of 100 structs: deserialize                  |    11,840 |    17,768 |      11,696 |    11,690 |    15,736 |        |
| fixed-size record: serialize                       |        69 |       101 |       1,455 |       161 |        65 |     45 |
| fixed-size record: deserialize                     |       480 |       144 |         432 |       200 |        88 |     88 |
| document of 100 rows: serialize lists              |     3,473 |     2,241 |       3,802 |     1,468 |     2,957 |        |
| document of 100,000 rows: serialize lists          | 2,401,073 | 1,600,641 |   3,546,058 | 1,187,728 | 2,400,563 |        |

- tafla-gen writes the vector of 100 nested tables in 64 µs, against 260 µs
  with tafla's builders, holding 27 KB at its peak against 39 KB, since
  `serialize()` packs a table's offsets and fixed-size fields with one
  `Struct` call, which writes each item whole. In a separate run of each step
  alone, creating the items' dicts took 11.8 µs and `ItemList.serialize()` of
  them 52.0 µs. It writes the record and the small document 2.1 and 1.8 times
  as fast, in 1.0 and 3.6 µs against 2.1 and 6.6 µs, and the large document
  in 23 µs against 26 µs.
- It writes the array of structs from the workload's own items, in 8.4 µs
  against 9.4 µs. Reading it takes 28.8 µs against 14.5 µs, since the
  generated reader creates a `Triple`, a frozen dataclass, for each struct
  before the codec creates the workload's items: `Triples.items()` took
  22.7 µs and creating the `ItemInput`s from them 6.4 µs. A generated struct
  sets its fields through its slots' descriptors, but creating one in a
  function call still took 0.17 µs, against 0.08 µs for the workloads' own
  dataclasses, which aren't frozen.
- It reads the record and the vector's items with `fields()`, which unpacks a
  table of fixed layout in one call, faster than `ByteBuffer`'s getters called
  in place: the record in 0.97 µs against 1.07 µs, and the vector in 61 µs
  against 74 µs. `Record.fields()` took 0.86 µs of the record's read, and
  `items()` with each item's `fields()` 53 µs of the vector's, and creating
  the workloads' dataclasses from their dicts the rest. `items()` creates a
  reader for each item, which the vector's peak bytes show, 17 KB against
  12 KB.
- The generated getters read the documents' fields 10% slower than
  `ByteBuffer`'s getters called in place, and sum the column within 5% of
  them.
- The fixed-size record is a frozen `Sample`, whose `serialize()` and `read()`
  pack and unpack it with one `Struct` call each: 0.25 and 0.36 µs. Creating
  the `Sample` took 0.17 µs of the 0.25 µs, so the tafla codec, which packs
  the fields into a `bytearray` without creating an object, writes the record
  faster, in 0.21 µs, and reads it slower through `ByteBuffer`'s getters, in
  0.56 µs.

`python -m tafla_benchmarks.encoding` printed:

```
Encoded size (bytes)
                        record     doc 100  doc 100000      vector     structs       fixed
  tafla                     60        1296     1200096        3208        1208          12
  tafla-gen                 60        1296     1200096        3208        1208          12
  flatbuffers               72        1328     1200128        2832        1224          32
  protobuf                  47        1243     1187503        1900        1700          16
  msgpack                   53        1276     1200082        4710        2804          32
  struct                                                                                12

Truncated document of 100000 rows: deserialize()
  tafla             600048 B  raised tafla.errors.DecodeError: Read exceeds the supplied buffer: 400000 bytes at
  tafla                 24 B  raised tafla.errors.DecodeError: Read exceeds the supplied buffer: 8 bytes at 24 of
  tafla-gen         600048 B  raised tafla.errors.DecodeError: Read exceeds the supplied buffer: 400000 bytes at
  tafla-gen             24 B  raised tafla.errors.DecodeError: Read exceeds the supplied buffer: 8 bytes at 24 of
  flatbuffers       600064 B  raised struct.error: unpack_from requires a buffer of at least 800128 b
  flatbuffers           24 B  raised struct.error: unpack_from requires a buffer of at least 28 bytes
  protobuf          593751 B  raised google.protobuf.message.DecodeError: Error parsing message with type 'tafla.benchmarks.
  protobuf              24 B  raised google.protobuf.message.DecodeError: Error parsing message with type 'tafla.benchmarks.
  msgpack           600041 B  raised ValueError: Unpack failed: incomplete input
  msgpack               24 B  raised ValueError: Unpack failed: incomplete input
```

Every library rejects the truncated document. FlatBuffers' Python runtime reads
through `struct`, whose range checks raise `struct.error`. msgpack packs the
floats of the vector, the array of structs and the fixed-size record as
float64s, so its vector is the largest, and its array of structs, at 2,804
bytes, more than twice the size of tafla's 1,208.

### Reading items through views

The four groups that read tafla's items, in the same run. pyperf's mean was
within 5% of the fastest value for all 16 benchmarks.

Fastest value in µs, and its ratio to `ByteBuffer`'s by hand:

| Benchmark                                       | tafla |     tafla-gen | tafla-gen view | tafla-gen view A/A |
| ----------------------------------------------- | ----: | ------------: | -------------: | -----------------: |
| vector of 100 nested tables: read the last item | 1.534 |  13.0 (8.45×) |  1.751 (1.14×) |      1.747 (1.14×) |
| vector of 100 nested tables: sum every item     |  70.9 |  89.2 (1.26×) |   89.5 (1.26×) |       89.6 (1.26×) |
| array of 100 structs: read the last item        | 0.705 | 22.1 (31.35×) |  1.094 (1.55×) |      1.093 (1.55×) |
| array of 100 structs: sum every item            | 8.337 |  25.2 (3.02×) |   24.9 (2.99×) |       25.2 (3.02×) |

The most bytes that a call held at once, as tracemalloc traces them:

| Benchmark                                       | tafla | tafla-gen | tafla-gen view | tafla-gen view A/A |
| ----------------------------------------------- | ----: | --------: | -------------: | -----------------: |
| vector of 100 nested tables: read the last item |   264 |     6,936 |            368 |                368 |
| vector of 100 nested tables: sum every item     | 1,464 |     6,936 |          1,752 |              1,752 |
| array of 100 structs: read the last item        |   448 |    11,880 |            536 |                536 |
| array of 100 structs: sum every item            |   576 |    11,880 |            712 |                712 |

- The views read the last item 7.4 times faster than the getters for the
  vector, 1.8 µs against 13.0 µs, and 20 times faster for the array, 1.1 µs
  against 22.1 µs: the getters create a reader or a `Triple` for each of the
  100 items first. `ByteBuffer` by hand takes 1.5 µs and 0.7 µs.
- Iterating over every item takes as long as through the getters, 89.5 µs
  against 89.2 µs for the vector and 24.9 µs against 25.2 µs for the array,
  since CPython creates each item's reader or `Triple` either way. The views
  save the list of them, so a call holds 1.8 KB at most against 6.9 KB for
  the vector, and 0.7 KB against 11.9 KB for the array. Reading the floats by
  hand takes 70.9 µs and 8.3 µs.
