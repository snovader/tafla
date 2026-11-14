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
of the array of structs, read in two ways: **read the last item**, which a
reader can find without reading the others, and **sum every item**. Each way
sums the three float32s of the items it reads, rather than creating objects
of its own, and `item_access.py` holds them:

- **tafla** reads with `ByteBuffer`'s getters by hand: `get_vector_offset()`
  of the last item or `get_vector_offsets()` of every item, and
  `get_struct_array()` with `Struct("<fff")`.
- **tafla-gen** reads with the generated getters, `items()`, which return
  views that read each item when it's asked for, as a tuple of its values.
  `item_field_values(-1)` reads the last item's alone: the vector's in one
  call, and the array's as `Struct("<fff")` unpacks them by hand.
  `iter_field_values()` iterates over every item's: the vector's through the
  runtime's `iter_vector_fields()`, which reads each item when it reaches
  it, and the array's as `Struct("<fff")` iterates over them.
- **tafla-gen A/A** reads with the getters again, an A/A pair that shows the
  harness's noise.

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
- protobufjs, and msgpackr's plain JavaScript entry point, which the
  TypeScript comparison uses, are JavaScript, so the pure-Python builds of
  protobuf and msgpack are arguably their closer analogues. Their C builds are
  what their wheels import by default, so the suite times both.
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
| `protobuf` 7.36.2 in pure Python           | The same wheels' pure-Python implementation, which `PROTOCOL_BUFFERS_PYTHON_IMPLEMENTATION=python` selects. It writes upb's bytes.         |
| `msgpack` 1.2.4                            | MessagePack's Python implementation, with a C extension: a schemaless format, whose tables are maps with their keys.                       |
| `msgpack` 1.2.4 in pure Python             | The same wheels' pure-Python build, `msgpack.fallback`, which msgpack imports where its C extension isn't built. It packs the same bytes.  |
| `struct` (the standard library's)          | For the fixed-size record only: fields at fixed offsets, packed and unpacked in one call, which tafla's documentation recommends for them. |

Each library is used as its documentation shows, and as the TypeScript
comparison uses its counterpart:

- **tafla** records nested tables among their parent builder's fields with
  `set_table_in_place()` and `set_vector_in_place()`, and reads columns as
  views of the bytes. Arrays of the columns' element types are copied as bytes, and lists
  are converted as `array()` converts them. Summing a column indexes its view,
  as tafla's documentation advises over a getter call per element. The array
  of structs is packed into a `bytearray` and unpacked from
  `get_struct_array()`'s view with a precompiled `Struct("<fff")`, as
  tafla-gen's code does.
- **tafla-gen** is the code that tafla-gen generates from
  `generated/workloads.tafla`, in `generated/tafla/`. A message's
  `serialize()` appends every field, nested tables included, to a
  `TableWriter`'s `bytearray` in one call, and returns a view of it, and
  writes a vector of tables of fixed layout in one call too. A message of
  only fixed fields and a string, as the record is, is written into bytes of
  its own in one call instead, with the runtime's
  `TableWriter.write_fields()`, without a writer. The record and the
  fixed-size `Sample` are written from their values, as positional
  arguments, with `serialize_values()`, and read into tuples of them with
  `read_values()`, without a reader: the record in one call, with
  `ByteBuffer.read_fields()`. The document is written from the generated
  fields' dicts, and read into a tuple of its values with `read_values()`,
  its columns' in a tuple of their own, as views. The
  vector is written from a tuple of each item's values, each pair's in place
  of it, with `serialize_values()`, and read into them with the
  `field_values()` of the view that its getter returns, which reads every
  item's values at once. The array of structs is written from the
  workload's own items, whose attributes `TripleFields`, the struct's
  protocol of fields, types, as an application's objects would be, and read
  with its view's `iter_field_values()`, into a tuple of each struct's
  values. It takes the workloads' dataclasses, as the other codecs do, so it
  creates the document's dicts and the vector's tuples from them in each
  call, as the protobuf codec builds its messages, and reading creates the
  workloads' dataclasses from the generated readers' values. Every suite has
  a copy of the schema; after changing them, regenerate the code as
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
- **protobuf (pure)** is the same codec in protobuf's pure-Python
  implementation. protobuf selects its implementation from
  `PROTOCOL_BUFFERS_PYTHON_IMPLEMENTATION` when a process first imports it, so
  the codec takes its name from the implementation that its process imported;
  see [Method](#method). On free-threaded builds of CPython, for which protobuf
  publishes no upb wheels, uv installs its pure-Python wheel, and the codec
  runs in pure Python without the variable.
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
- **msgpack (pure)** is the same codec with msgpack's pure-Python build, the
  `Packer` and `unpackb()` of `msgpack.fallback`, which msgpack imports where
  its C extension isn't built. They have the C extension's API, and importing
  them from `fallback` puts both builds in one process.
- **struct** packs and unpacks the fixed-size record with one precompiled
  `Struct("<Iff")`.

## Method

pyperf 2.10.0 runs each benchmark in worker processes of its own. The first
calibrates the number of calls that one value times, so that it takes at least
100 ms, and each of the others runs a warm-up and then 3 values. A full run
runs the suite twice, with 5 workers per benchmark, and appends their values
to one file: 10 workers and 30 values per benchmark. The second run reverses
the order of the codecs in each group, so that each benchmark's workers are
spread over the whole run. `tafla_benchmarks.report` then prints each
benchmark's fastest value, and its ratio to tafla's, as the TypeScript
comparison reports the fastest round, since load from other processes only
slows values down.

Until 2026-11-14, a full run ran the suite 4 times, alternating the order,
which took twice as long. On that day, eight runs of the suite in alternating
order were split into two sets of 4 and four pairs. Each pair's fastest values
were 0.1% to 0.2% slower in the median benchmark than those of the set of 4
that it wasn't part of, and within 1.0% for 90% of the benchmarks, where the
two sets of 4 differed by 0.7%.

- A worker runs one benchmark, so CPython's specializing interpreter
  specializes its timing loop for one operation, as the TypeScript harness's
  separate timer loops ensure for V8. CPython doesn't eliminate unused results,
  so the loop drops each result, which frees it before the next call, as a
  caller's would be.
- A worker prepares its benchmark's input when it first times it, after
  checking that the codec reads back exactly what it wrote, with floats
  compared by their bits. `tests/test_round_trips.py` checks every codec on
  every workload, and runs again in a process of its own to check protobuf's
  pure-Python implementation.
- protobuf selects its implementation for the whole process, so each run
  invokes the suite a second time for its pure-Python implementation, which
  times only that codec, after the others. The suite passes
  `PROTOCOL_BUFFERS_PYTHON_IMPLEMENTATION` on to its workers, which pyperf
  otherwise wouldn't, and each worker names the codec after the implementation
  that it imported, so `protobuf (pure)`'s values were timed in pure Python.
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
one codec's workers and not another's, but the 2 runs spread each benchmark
over the whole run, and its fastest value is unaffected unless the load lasts
for all of them. Check `uptime` before a run, and compare the fastest values
with pyperf's means, which the report summarizes.

## Running

```sh
uptime
for order in "" --reverse; do
    uv run --python 3.14+gil python -m tafla_benchmarks --processes 5 --append results.json $order
    PROTOCOL_BUFFERS_PYTHON_IMPLEMENTATION=python uv run --python 3.14+gil python -m tafla_benchmarks --codec protobuf --processes 5 --append results.json $order
done
uv run --python 3.14+gil python -m tafla_benchmarks.report results.json
uv run --python 3.14+gil python -m tafla_benchmarks.allocation
PROTOCOL_BUFFERS_PYTHON_IMPLEMENTATION=python uv run --python 3.14+gil python -m tafla_benchmarks.allocation --codec protobuf
```

The full run takes about 20 minutes, and the allocation report about 10
seconds, which CI runs too. `--append` adds to an existing file, so
remove the last run's `results.json` first. `--filter <regex>` runs only the
groups whose name matches, and `--reverse` runs each group's codecs in reverse
order. pyperf's own options apply too, and its commands read the results:
`python -m pyperf stats results.json` describes each benchmark's values, for
example.

A quick run of one worker per benchmark, which calls each operation once,
checks that every benchmark works in about 14 seconds, most of which its 82
workers spend starting and importing numpy. CI runs it after the round-trip
test, on Python 3.14. Its numbers mean little.

```sh
uv run python -m tafla_benchmarks --debug-single-value --quiet
```

Pure protobuf's invocations set `PROTOCOL_BUFFERS_PYTHON_IMPLEMENTATION=python`,
and `--codec <regex>` runs only the codecs whose name matches, in the suite and
in `tafla_benchmarks.allocation`. Pure protobuf's quick run, which CI runs too:

```sh
PROTOCOL_BUFFERS_PYTHON_IMPLEMENTATION=python uv run python -m tafla_benchmarks --codec protobuf --debug-single-value --quiet
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
- protobuf's pure-Python implementation runs in invocations of the suite of
  its own, so its workers follow every other codec's in each run, rather than
  running among them. The 2 runs still spread them over the whole run.
- msgpack's column extensions are in the host's byte order, as msgpackr's are,
  and are this suite's own, so another MessagePack reader would see them as
  unknown extensions.
- FlatBuffers' builder starts at 1 KiB and doubles by copying. Reusing a
  `Builder` with `Clear()` would make small messages cheaper, but the
  TypeScript comparison doesn't reuse it either.
- tafla-gen's codec creates the generated fields' dicts of the document and
  the vector from the workloads' dataclasses in each call, which a caller
  that holds its values in them wouldn't.
- The peak bytes aren't the bytes that a call allocates in all, which
  tracemalloc doesn't count: CPython frees most temporary objects as soon as
  they're unused, so a call that creates many in turn holds few at once.

## Results

One full run on 2026-11-01, on an Apple M2 Pro with 32 GB in macOS 27.0.1,
with Homebrew's CPython 3.14.10, built with the GIL, PGO, LTO and the
tail-calling interpreter, without the JIT. It took 18 minutes. Other processes
kept the 1-minute load average between 1.1 and 2.9 on the 10 cores, with a
median of 1.4. pyperf's mean was within 5% of the fastest value for 53 of the
58 benchmarks. The others serialize documents: for tafla's, msgpack's and
FlatBuffers' large documents, and msgpack's small one from lists, the medians
were 9% to 14% above the fastest values, as the means were, so most values were
slower than the fastest, rather than a few of them slowed by load. msgpack's
small document had a few slow values, which put its mean 8% above its fastest
value and its median 1%.

Fastest value in µs per call, and its ratio to tafla's:

| Benchmark                                          |  tafla |       flatbuffers |         protobuf |        msgpack |        struct |
| -------------------------------------------------- | -----: | ----------------: | ---------------: | -------------: | ------------: |
| record: serialize                                  |  2.444 |     8.299 (3.40×) |    0.497 (0.20×) |  0.284 (0.12×) |               |
| record: deserialize                                |  1.225 |     4.771 (3.89×) |    0.436 (0.36×) |  0.393 (0.32×) |               |
| document of 100 rows: serialize                    |  7.316 |      20.5 (2.80×) |    8.571 (1.17×) |  1.847 (0.25×) |               |
| document of 100 rows: deserialize                  |  2.646 |      12.7 (4.79×) |    1.721 (0.65×) |  1.175 (0.44×) |               |
| document of 100 rows: sum a column per element     |  3.536 |    142.9 (40.40×) |    4.791 (1.35×) |  3.709 (1.05×) |               |
| document of 100,000 rows: serialize                |   31.7 |     176.9 (5.57×) | 7577.7 (238.75×) |   59.7 (1.88×) |               |
| document of 100,000 rows: deserialize              |  2.660 |      12.7 (4.79×) |  618.3 (232.48×) |   20.3 (7.64×) |               |
| document of 100,000 rows: sum a column per element | 3056.8 | 140796.6 (46.06×) |   5051.1 (1.65×) | 3091.2 (1.01×) |               |
| vector of 100 nested tables: serialize             |  259.7 |     527.9 (2.03×) |     41.3 (0.16×) |   20.8 (0.08×) |               |
| vector of 100 nested tables: deserialize           |   93.3 |     636.5 (6.82×) |     39.3 (0.42×) |   30.2 (0.32×) |               |
| fixed-size record: serialize                       |  0.637 |    6.520 (10.24×) |    0.453 (0.71×) |  0.226 (0.36×) | 0.074 (0.12×) |
| fixed-size record: deserialize                     |  0.636 |     3.515 (5.53×) |    0.385 (0.61×) |  0.318 (0.50×) | 0.142 (0.22×) |
| document of 100 rows: serialize lists              |   11.1 |      25.2 (2.27×) |    5.625 (0.51×) |  4.868 (0.44×) |               |
| document of 100,000 rows: serialize lists          | 2960.5 |    4908.1 (1.66×) |   4309.9 (1.46×) | 2990.8 (1.01×) |               |

- tafla reads both documents in 2.7 µs, since its columns are views, whatever
  their length. FlatBuffers' numpy views take 12.7 µs at both sizes, so its
  pure-Python accessors' calls cost that, not the columns. msgpack and
  protobuf read the small document faster, in C, but copy the large one's
  columns: msgpack in 20 µs, and protobuf, which parses them into its
  containers, in 618 µs.
- tafla writes the large document fastest, in 32 µs, copying its arrays as
  bytes; msgpack copies them twice, into extensions and then into the message.
  protobuf converts every element, which took 7.6 ms from arrays and 4.3 ms
  from lists, whose elements are Python floats already.
- Small tables cost tafla its Python calls, a few per field, as its
  documentation says: protobuf and msgpack, in C, write the record 5 to 9 times
  faster and read it about 3 times faster. Only FlatBuffers, also pure Python,
  is slower.
- The vector of 100 nested tables is where tafla is furthest behind: 260 µs to
  write, against msgpack's 21 µs and protobuf's 41 µs, and 93 µs to read,
  against 30 µs and 39 µs.
- Summing a column through a memoryview takes about 31 ns per element, for
  tafla and msgpack alike, and through protobuf's container 51 ns.
  FlatBuffers' generated accessor takes 1.4 µs per element.
- Converting lists dominates writing the large document from them, which took
  every library 3 to 5 ms.
- `struct` packs the fixed-size record in 74 ns and unpacks it in 142 ns, about
  half of which is creating the result. tafla's codec took 637 ns and 636 ns,
  when it wrote the record with `ByteBuffer`'s setters, which are gone.

Every deserialization includes creating the workload's dataclasses, about
65 ns each, which the codecs share: 6.5 µs of reading the vector of 100 nested
tables, for example.

The array of 100 structs was added after that run, and timed on the same day
with the vector of nested tables, in their groups alone: the full run's four
runs with `--filter 'structs|nested'`. It took 5 minutes, while another session
ran benchmarks, which kept the 1-minute load average between 2.7 and 3.5, with
a median of 3.2. pyperf's mean was within 4% of the fastest value for all 16
benchmarks, and the vector's fastest values were within 6% of the full run's.

| Benchmark                                | tafla |    flatbuffers |     protobuf |       msgpack |
| ---------------------------------------- | ----: | -------------: | -----------: | ------------: |
| vector of 100 nested tables: serialize   | 264.8 |  541.7 (2.05×) | 42.5 (0.16×) |  21.5 (0.08×) |
| vector of 100 nested tables: deserialize |  87.7 |  656.6 (7.49×) | 40.6 (0.46×) |  31.2 (0.36×) |
| array of 100 structs: serialize          | 9.852 | 114.5 (11.63×) | 29.2 (2.97×) | 9.412 (0.96×) |
| array of 100 structs: deserialize        |  14.9 | 247.7 (16.66×) | 28.9 (1.94×) |  17.1 (1.15×) |

- tafla writes the 100 structs in 9.9 µs, against 265 µs for the same items as
  nested tables, packing them with one `Struct` call each, and reads them in
  15 µs, against 88 µs, 6.5 µs of which creates the 100 dataclasses. msgpack,
  in C, takes 9.4 µs and 17 µs, and protobuf 29 µs each way.
- FlatBuffers creates and reads each struct with pure-Python calls, in 115 µs
  and 248 µs.

tafla-gen's codec was added after those runs, and timed on 2026-11-02 in a
full run of every codec, with the same Python, followed by
`tafla_benchmarks.allocation`. It took 26 minutes. Other processes kept the
1-minute load average between 1.2 and 10.9, with a median of 1.8. pyperf's mean was within 5% of the fastest
value for 55 of the 82 benchmarks, and at most 28% above it, for tafla-gen's
write of the large document. The fastest values of tafla and the other
libraries were within 11% of the runs above, with a median difference of
1.6%.

Fastest value in µs per call, and tafla-gen's ratio to tafla's:

| Benchmark                                          |  tafla |      tafla-gen |
| -------------------------------------------------- | -----: | -------------: |
| record: serialize                                  |  2.669 |  1.418 (0.53×) |
| record: deserialize                                |  1.296 |  1.437 (1.11×) |
| document of 100 rows: serialize                    |  7.754 |  3.874 (0.50×) |
| document of 100 rows: deserialize                  |  2.738 |  3.027 (1.11×) |
| document of 100 rows: sum a column per element     |  3.638 |  3.833 (1.05×) |
| document of 100,000 rows: serialize                |   33.2 |   22.9 (0.69×) |
| document of 100,000 rows: deserialize              |  2.789 |  3.067 (1.10×) |
| document of 100,000 rows: sum a column per element | 3092.6 | 3075.5 (0.99×) |
| vector of 100 nested tables: serialize             |  268.8 |  134.6 (0.50×) |
| vector of 100 nested tables: deserialize           |   85.9 |  106.3 (1.24×) |
| array of 100 structs: serialize                    |  9.671 |   33.2 (3.44×) |
| array of 100 structs: deserialize                  |   14.9 |   39.2 (2.64×) |
| fixed-size record: serialize                       |  0.704 |  0.337 (0.48×) |
| fixed-size record: deserialize                     |  0.705 |  0.462 (0.65×) |
| document of 100 rows: serialize lists              |   11.5 |  7.656 (0.66×) |
| document of 100,000 rows: serialize lists          | 3020.1 | 3037.5 (1.01×) |

Peak bytes per call, as `tafla_benchmarks.allocation` traces them:

| Benchmark                                          |     tafla | tafla-gen | flatbuffers |  protobuf |   msgpack | struct |
| -------------------------------------------------- | --------: | --------: | ----------: | --------: | --------: | -----: |
| record: serialize                                  |     1,062 |       537 |       1,490 |       192 |        86 |        |
| record: deserialize                                |       561 |       601 |         400 |       233 |       121 |        |
| document of 100 rows: serialize                    |     2,618 |     1,761 |       3,770 |     1,468 |     1,517 |        |
| document of 100 rows: deserialize                  |     1,113 |     1,201 |       1,905 |       625 |     2,684 |        |
| document of 100 rows: sum a column per element     |       688 |       736 |         400 |       208 |     2,684 |        |
| document of 100,000 rows: serialize                | 1,201,418 | 1,200,561 |   3,297,674 | 3,998,040 | 1,200,323 |        |
| document of 100,000 rows: deserialize              |     1,113 |     1,201 |       1,905 |       625 | 1,201,484 |        |
| document of 100,000 rows: sum a column per element |       688 |       736 |         400 |       208 | 1,201,484 |        |
| vector of 100 nested tables: serialize             |    39,593 |    27,135 |       8,186 |     2,221 |     9,471 |        |
| vector of 100 nested tables: deserialize           |    12,156 |    17,392 |      11,784 |    11,770 |    15,968 |        |
| array of 100 structs: serialize                    |     3,418 |     9,082 |       3,666 |     1,941 |     3,709 |        |
| array of 100 structs: deserialize                  |    11,896 |    17,768 |      11,696 |    11,690 |    15,736 |        |
| fixed-size record: serialize                       |       553 |       101 |       1,455 |       161 |        65 |     45 |
| fixed-size record: deserialize                     |       408 |       144 |         432 |       200 |        88 |     88 |
| document of 100 rows: serialize lists              |     4,058 |     2,241 |       3,802 |     1,468 |     2,957 |        |
| document of 100,000 rows: serialize lists          | 2,401,658 | 1,600,641 |   3,546,058 | 1,187,728 | 2,400,563 |        |

- tafla-gen writes the vector of 100 nested tables in 135 µs, against 269 µs
  with tafla's builders, holding 27 KB at its peak against 40 KB. It writes
  the record and the small document twice as fast, in 1.4 and 3.9 µs against
  2.7 and 7.8 µs, and the large document in 23 µs against 33 µs.
- The array of structs takes 33 µs against 9.7 µs, and reading it 39 µs
  against 15 µs, since the generated structs are frozen dataclasses: the codec
  creates a `Triple` for each item before writing, and the generated reader
  creates one for each struct before the codec creates the workload's items.
  In a separate run of each step alone, creating the 100 `Triple`s took
  24.2 µs and `Triples.serialize()` of them 8.7 µs, and `Triples.items()`
  took 32.4 µs and creating the `ItemInput`s from them 6.3 µs. For the vector,
  creating the items' dicts took 11.7 µs and `ItemList.serialize()` of them
  118.8 µs. Since that run, the generated structs set their fields through
  their slots rather than through the frozen dataclass's own `__init__()`:
  in an A/B run, creating the 100 `Triple`s took 15.6 µs rather than 24.1 µs,
  and the codec wrote the array 25% faster and read it 23% faster.
- The generated readers read the record's and the documents' fields 10% to
  11% slower than `ByteBuffer`'s getters called in place, and sum the column
  within 5% of them. They create an object for each table they read, which
  the vector's peak bytes show, 17 KB against 12 KB, and read it 24% slower,
  in 106 µs against 86 µs.
- The fixed-size record is a frozen `Sample`, whose `serialize()` and `read()`
  pack and unpack it with one `Struct` call each: 0.34 and 0.46 µs against
  the tafla codec's 0.70 µs, through `ByteBuffer`'s setters, which are gone,
  and its getters. Creating the `Sample` took 0.24 µs of the 0.34 µs.

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

Four runs of the four groups that read tafla's items, on 2026-11-02 on the
same machine and Python, alternating their order as above, with
`--filter item`. Other processes kept the 1-minute load average between 1.7
and 3.8, with a median of 2.5. pyperf's mean was within 5% of the fastest value
for 14 of the 16 benchmarks, and at most 11% above it.

Fastest value in µs, and its ratio to `ByteBuffer`'s by hand:

| Benchmark                                       | tafla |     tafla-gen | tafla-gen view | tafla-gen view A/A |
| ----------------------------------------------- | ----: | ------------: | -------------: | -----------------: |
| vector of 100 nested tables: read the last item | 1.629 |  13.5 (8.28×) |  2.006 (1.23×) |      2.008 (1.23×) |
| vector of 100 nested tables: sum every item     |  83.5 | 102.5 (1.23×) |  102.1 (1.22×) |      102.8 (1.23×) |
| array of 100 structs: read the last item        | 0.812 | 33.0 (40.67×) |  1.310 (1.61×) |      1.315 (1.62×) |
| array of 100 structs: sum every item            | 8.583 |  35.9 (4.18×) |   35.6 (4.15×) |       35.7 (4.16×) |

The most bytes that a call held at once, as tracemalloc traces them:

| Benchmark                                       | tafla | tafla-gen | tafla-gen view | tafla-gen view A/A |
| ----------------------------------------------- | ----: | --------: | -------------: | -----------------: |
| vector of 100 nested tables: read the last item |   356 |     6,628 |            424 |                424 |
| vector of 100 nested tables: sum every item     | 1,453 |     6,628 |          1,741 |              1,741 |
| array of 100 structs: read the last item        |   504 |    11,936 |            592 |                592 |
| array of 100 structs: sum every item            |   632 |    11,936 |            696 |                696 |

- The views read the last item 6.7 times faster than the getters for the
  vector, 2.0 µs against 13.5 µs, and 25 times faster for the array, 1.3 µs
  against 33.0 µs: the getters create a reader or a `Triple` for each of the
  100 items first. `ByteBuffer` by hand takes 1.6 µs and 0.8 µs.
- Iterating over every item takes as long as through the getters, 102.1 µs
  against 102.5 µs for the vector and 35.6 µs against 35.9 µs for the array,
  since CPython creates each item's reader or `Triple` either way. The views
  save the list of them, so a call holds 1.7 KB at most against 6.6 KB for
  the vector, and 0.7 KB against 11.9 KB for the array. Reading the floats by
  hand takes 83.5 µs and 8.6 µs.
