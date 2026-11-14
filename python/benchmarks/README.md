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
| `tafla` (`core/`)                          | The reference that the others are compared with: its native classes, and again its pure-Python ones.                                       |
| tafla-gen's code                           | tafla's tables, written and read by the code that tafla-gen generates, with either.                                                        |
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

pyperf 2.11.0 runs each benchmark in worker processes of its own. The first
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
- `tafla` uses its native classes where they're installed, as they are in
  the development environment, so the tafla and tafla-gen codecs time them.
  Each run invokes the suite a third time with `TAFLA_PUREPYTHON=1`, for
  those two codecs alone, which `--inherit-environ TAFLA_PUREPYTHON` passes
  on to the workers. Their names don't say which classes they used, so those
  values go to a file of their own, `results-pure.json`, with a report of
  its own.
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
    TAFLA_PUREPYTHON=1 uv run --python 3.14+gil python -m tafla_benchmarks --codec '^tafla' --inherit-environ TAFLA_PUREPYTHON --processes 5 --append results-pure.json $order
done
uv run --python 3.14+gil python -m tafla_benchmarks.report results.json
uv run --python 3.14+gil python -m tafla_benchmarks.report results-pure.json
uv run --python 3.14+gil python -m tafla_benchmarks.allocation
PROTOCOL_BUFFERS_PYTHON_IMPLEMENTATION=python uv run --python 3.14+gil python -m tafla_benchmarks.allocation --codec protobuf
TAFLA_PUREPYTHON=1 uv run --python 3.14+gil python -m tafla_benchmarks.allocation --codec '^tafla'
```

The full run takes about 28 minutes, and the allocation reports about half a
minute, of which CI runs the first two. `--append` adds to an existing file,
so remove the last run's `results.json` and `results-pure.json` first.
`--filter <regex>` runs only the groups whose name matches, and `--reverse`
runs each group's codecs in reverse order. pyperf's own options apply too, and
its commands read the results: `python -m pyperf stats results.json`
describes each benchmark's values, for example.

A quick run of one worker per benchmark, which calls each operation once,
checks that every benchmark works in about 17 seconds, most of which its 114
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
- protobuf's pure-Python implementation, and tafla's pure-Python classes, run
  in invocations of the suite of their own, so their workers follow every
  other codec's in each run, rather than running among them. The 2 runs still
  spread them over the whole run.
- `uv run` builds the native module with the tests' hooks, which the wheels
  leave out. The only one that the timed code reaches checks the host's byte
  order with an atomic load when a native reader or writer copies an array.
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
- tracemalloc doesn't see objects that CPython reuses from its freelists,
  such as dicts, floats and tuples, nor memory that a C or Rust library
  allocates itself, such as upb's arena and the native classes' own working
  memory, though they write into a `bytearray` that it sees. So protobuf's
  and msgpack's small peaks are lower bounds: protobuf's read of the large
  document peaks at 625 bytes, though it copies 1.2 MB of columns into its
  containers.

## Results

One full run on 2026-11-13, of `main` at c43286c, on an Apple M2 Pro with
32 GB in macOS 27.0.1, with Homebrew's CPython 3.14.10, built with the GIL,
PGO, LTO and the tail-calling interpreter, without the JIT. `uv run` built the
native module with maturin 1.16.0 and Rust 1.100.0. Its four runs each ran
one group at a time, with `--filter`, once other processes used under 3
cores, as a sampler recorded every 2 seconds. Four bursts from other
processes, of 5.4 to 7.4 cores, each lasted under 4 seconds, so no group ran
again. The runs took 55 minutes, and the allocation reports followed them.
Other processes kept the 1-minute load average between 1.2 and 3.2 on the 10
cores, with a median of 1.7. pyperf's mean was within 5% of the fastest value
for 120 of the 130 benchmarks in `results.json`, and 44 of the 48 in
`results-pure.json`. The others' means were 5% to 22% above their fastest
values, and their medians at most 13%: most for writing the large document,
by tafla-gen, 22% and 12%, and FlatBuffers, 12% and 13%.

Fastest value in µs per call, and its ratio to tafla's, with tafla's native
classes:

| Benchmark                                          |  tafla |       flatbuffers |         protobuf |       protobuf (pure) |        msgpack | msgpack (pure) |        struct |
| -------------------------------------------------- | -----: | ----------------: | ---------------: | --------------------: | -------------: | -------------: | ------------: |
| record: serialize                                  |  1.333 |     7.809 (5.86×) |    0.479 (0.36×) |         5.861 (4.40×) |  0.268 (0.20×) |  3.279 (2.46×) |               |
| record: deserialize                                |  0.363 |    4.315 (11.89×) |    0.434 (1.19×) |        5.170 (14.24×) |  0.374 (1.03×) | 5.135 (14.15×) |               |
| document of 100 rows: serialize                    |  3.761 |      19.1 (5.09×) |    8.232 (2.19×) |        142.4 (37.86×) |  1.649 (0.44×) |  8.150 (2.17×) |               |
| document of 100 rows: deserialize                  |  1.030 |     11.6 (11.31×) |    1.686 (1.64×) |       191.6 (186.08×) |  1.062 (1.03×) |  9.753 (9.47×) |               |
| document of 100 rows: sum a column per element     |  2.902 |    130.1 (44.81×) |    4.684 (1.61×) |        194.2 (66.90×) |  3.443 (1.19×) |   12.1 (4.19×) |               |
| document of 100,000 rows: serialize                |   23.1 |     173.5 (7.52×) | 7408.7 (321.30×) |   122661.3 (5319.66×) |   60.4 (2.62×) |   43.9 (1.91×) |               |
| document of 100,000 rows: deserialize              |  1.029 |     11.6 (11.30×) |  632.2 (614.13×) | 176469.0 (171437.12×) |  19.8 (19.28×) |  68.5 (66.50×) |               |
| document of 100,000 rows: sum a column per element | 2819.3 | 127895.5 (45.36×) |   4858.6 (1.72×) |     180683.5 (64.09×) | 2849.2 (1.01×) | 2907.5 (1.03×) |               |
| vector of 100 nested tables: serialize             |  179.8 |     492.7 (2.74×) |     40.4 (0.22×) |         762.8 (4.24×) |   19.2 (0.11×) |  246.0 (1.37×) |               |
| vector of 100 nested tables: deserialize           |   27.6 |    571.7 (20.71×) |     39.4 (1.43×) |        548.6 (19.88×) |   28.7 (1.04×) | 356.6 (12.92×) |               |
| array of 100 structs: serialize                    |  7.686 |    105.5 (13.73×) |     26.9 (3.50×) |        536.9 (69.85×) |  8.092 (1.05×) | 130.4 (16.97×) |               |
| array of 100 structs: deserialize                  |   12.7 |    218.4 (17.25×) |     28.1 (2.22×) |        385.7 (30.47×) |   15.6 (1.23×) | 186.0 (14.70×) |               |
| fixed-size record: serialize                       |  0.168 |    6.026 (35.87×) |    0.426 (2.54×) |        4.756 (28.31×) |  0.212 (1.26×) | 2.627 (15.64×) | 0.062 (0.37×) |
| fixed-size record: deserialize                     |  0.207 |    3.180 (15.37×) |    0.373 (1.80×) |        3.733 (18.04×) |  0.297 (1.43×) | 4.143 (20.02×) | 0.124 (0.60×) |
| document of 100 rows: serialize lists              |  6.503 |      23.8 (3.66×) |    5.516 (0.85×) |        140.8 (21.65×) |  4.151 (0.64×) |   10.7 (1.64×) |               |
| document of 100,000 rows: serialize lists          | 2353.5 |    4910.7 (2.09×) |   4245.7 (1.80×) |     120468.7 (51.19×) | 2397.3 (1.02×) | 2377.8 (1.01×) |               |

- tafla reads both documents in 1.0 µs with its native classes, and 2.3 µs
  with its pure-Python ones, since its columns are views, whatever their
  length. FlatBuffers' numpy views take 11.6 µs at both sizes, so its
  pure-Python accessors' calls cost that, not the columns. msgpack and
  protobuf read the small document in 1.1 µs and 1.7 µs, in C, but copy the
  large one's columns: msgpack in 20 µs, and protobuf, which parses them into
  its containers, in 632 µs. Their pure-Python builds read the small document
  in 9.8 µs and 192 µs, and the large one in 69 µs and 176 ms.
- tafla writes the large document fastest, in 23 µs, copying its arrays as
  bytes. msgpack copies them twice, into extensions and then into the
  message, in 60 µs, and its pure-Python build takes 44 µs. protobuf converts
  every element, which took 7.4 ms from arrays and 4.2 ms from lists, whose
  elements are Python floats already, and its pure-Python implementation
  123 ms and 120 ms.
- Small tables cost tafla's builder its Python calls, a few per field, as its
  documentation says, even with the native classes: it writes the record in
  1.33 µs, against 0.48 µs for protobuf and 0.27 µs for msgpack, in C. It
  reads it in 0.36 µs, as fast as msgpack, in 0.37 µs, and faster than
  protobuf, in 0.43 µs. FlatBuffers, in pure Python, takes 7.8 µs and 4.3 µs,
  and the pure-Python builds of msgpack and protobuf 3.3 µs to 5.9 µs.
- The vector of 100 nested tables is where tafla is furthest behind: its
  codec builds a `TableBuilder` for each of the 200 tables, and writes them in
  180 µs, against msgpack's 19 µs and protobuf's 40 µs. Recording them in
  place, with `set_vector_in_place()` and `set_table_in_place()`, took 0.19 ms
  against 0.25 ms with the pure-Python classes, as the package documentation
  says. tafla reads the vector in 28 µs, as fast as msgpack, in 29 µs, and
  faster than protobuf, in 39 µs. The pure-Python builds of msgpack and
  protobuf write it in 246 µs and 763 µs and read it in 357 µs and 549 µs,
  slower than tafla's pure-Python classes, in 243 µs and 67 µs.
- tafla writes the 100 structs in 7.7 µs, against 180 µs for the same items
  as nested tables, packing them with one `Struct` call each, and reads them
  in 12.7 µs, against 28 µs, 7.6 µs of which creates the 100 dataclasses.
  `struct` does that work with either implementation, so the pure-Python
  classes take about as long, 8.3 µs and 12.9 µs. msgpack, in C, takes
  8.1 µs and 15.6 µs, and protobuf 27 µs and 28 µs. FlatBuffers creates and
  reads each struct with pure-Python calls, in 106 µs and 218 µs, and the
  pure-Python builds of msgpack and protobuf take 130 µs to 537 µs.
- Summing a column through a memoryview takes about 28 ns per element, for
  tafla and both builds of msgpack alike, and through protobuf's container
  49 ns. FlatBuffers' generated accessor takes 1.3 µs per element, and pure
  protobuf's container 1.8 µs.
- Converting lists dominates writing the large document from them, which took
  every library 2.3 ms to 4.9 ms, except pure protobuf, which took 120 ms.
- `struct` packs the fixed-size record in 62 ns and unpacks it in 124 ns, of
  which creating the result takes about 90 ns. tafla's codec packs it field
  by field with `struct` in 168 ns, faster than protobuf and msgpack, and
  reads it with `ByteBuffer`'s getters in 207 ns with the native classes and
  496 ns with the pure-Python ones. Creating a pure-Python `ByteBuffer` takes
  0.24 µs of that, about as long as the three getters, and a native one
  0.08 µs. tafla's documentation recommends `struct` alone for records at
  fixed offsets, which the `struct` row times.

Every deserialization includes creating the workload's dataclasses, which the
codecs share: creating the 100 items from tuples of their values took 7.6 µs,
and from tafla-gen's dicts 8.6 µs.

tafla's and tafla-gen's codecs with each implementation of tafla's classes, in
the same run: fastest value in µs per call. tafla-gen's ratio is to tafla's
with the same classes, and the pure-Python tafla's to the native tafla's:

| Benchmark                                          |  tafla |      tafla-gen |   tafla (pure) | tafla-gen (pure) |
| -------------------------------------------------- | -----: | -------------: | -------------: | ---------------: |
| record: serialize                                  |  1.333 |  0.323 (0.24×) |  2.004 (1.50×) |    0.793 (0.40×) |
| record: deserialize                                |  0.363 |  0.394 (1.09×) |  0.968 (2.67×) |    0.790 (0.82×) |
| document of 100 rows: serialize                    |  3.761 |  1.031 (0.27×) |  6.121 (1.63×) |    3.299 (0.54×) |
| document of 100 rows: deserialize                  |  1.030 |  1.312 (1.27×) |  2.312 (2.25×) |    2.595 (1.12×) |
| document of 100 rows: sum a column per element     |  2.902 |  3.071 (1.06×) |  3.254 (1.12×) |    3.412 (1.05×) |
| document of 100,000 rows: serialize                |   23.1 |   18.9 (0.82×) |   25.7 (1.12×) |     22.1 (0.86×) |
| document of 100,000 rows: deserialize              |  1.029 |  1.307 (1.27×) |  2.326 (2.26×) |    2.611 (1.12×) |
| document of 100,000 rows: sum a column per element | 2819.3 | 2814.1 (1.00×) | 2821.0 (1.00×) |   2823.3 (1.00×) |
| vector of 100 nested tables: serialize             |  179.8 |   22.2 (0.12×) |  242.6 (1.35×) |     51.4 (0.21×) |
| vector of 100 nested tables: deserialize           |   27.6 |   25.2 (0.91×) |   67.3 (2.44×) |     45.4 (0.67×) |
| array of 100 structs: serialize                    |  7.686 |  6.661 (0.87×) |  8.273 (1.08×) |    7.273 (0.88×) |
| array of 100 structs: deserialize                  |   12.7 |   12.5 (0.99×) |   12.9 (1.02×) |     12.7 (0.98×) |
| fixed-size record: serialize                       |  0.168 |  0.093 (0.55×) |  0.169 (1.01×) |    0.094 (0.55×) |
| fixed-size record: deserialize                     |  0.207 |  0.178 (0.86×) |  0.496 (2.39×) |    0.179 (0.36×) |
| document of 100 rows: serialize lists              |  6.503 |  3.984 (0.61×) |  8.987 (1.38×) |    6.061 (0.67×) |
| document of 100,000 rows: serialize lists          | 2353.5 | 2346.7 (1.00×) | 2359.6 (1.00×) |   2355.1 (1.00×) |

- The native classes take tafla's reads to 0.38 to 0.45 times as long as the
  pure-Python ones, and its writes to 0.61 to 0.74 times, since
  `TableBuilder` is Python with either, and only writes through the native
  `TableWriter`. Where `struct`, views or converting lists do the work, as for
  the arrays of structs, the sums and the large documents, they take about as
  long.
- tafla-gen's code calls the native classes for more of its work, so it takes
  0.31 to 0.56 times as long with them as with the pure-Python ones to write
  and read the record, the small document and the vector. It
  writes the record in 0.32 µs, between msgpack's 0.27 µs and protobuf's
  0.48 µs, and the small document in 1.0 µs, against msgpack's 1.6 µs. It
  reads them in 0.39 µs and 1.3 µs, against msgpack's 0.37 µs and 1.1 µs.
- It writes the vector of 100 nested tables with `ItemList.serialize()`, which
  writes a vector of tables of fixed layout in one call: 22 µs against
  tafla's 180 µs, and msgpack's 19 µs, holding 29 KB at its peak against
  40 KB. In a separate run of each step alone, creating the items' dicts took
  11.2 µs and `serialize()` of them 11.1 µs, or 40.6 µs with the pure-Python
  classes.
- It reads the vector with its view's `fields()`, which reads every item's
  dict at once, in 25 µs against tafla's 28 µs: `fields()` took 16.7 µs, and
  creating the workload's items from the dicts 8.6 µs. Holding every item's
  dicts at once takes 34 KB at its peak, against 12.5 KB for tafla's getters.
- It writes the record from its values with `serialize_values()`, 4.1 times as
  fast as tafla's builder, and reads it into a tuple with `field_values()`,
  which took 0.29 µs of its 0.39 µs, against 0.36 µs for `ByteBuffer`'s
  getters called in place.
- It writes the array of structs from the workload's own items in 6.7 µs,
  against 7.7 µs, and reads it with the view's `field_values()`, into a tuple
  of each struct's values, in 12.5 µs, against 12.7 µs: `field_values()` took
  5.1 µs, and creating the items 7.6 µs.
- The generated getters read the documents' fields 27% slower than
  `ByteBuffer`'s getters called in place with the native classes, and 12%
  slower with the pure-Python ones, and sum the column within 6% of them.
- `Sample.serialize_values()` and `read_values()` write and read the
  fixed-size record with one `Struct` call each, in 93 ns and 178 ns with
  either implementation, faster than tafla's codec, msgpack and protobuf.
  `struct` alone takes 62 ns and 124 ns.

Peak bytes per call, as `tafla_benchmarks.allocation` traces them:

| Benchmark                                          |     tafla | tafla-gen | tafla (pure) | tafla-gen (pure) | flatbuffers |  protobuf | protobuf (pure) |   msgpack | msgpack (pure) | struct |
| -------------------------------------------------- | --------: | --------: | -----------: | ---------------: | ----------: | --------: | --------------: | --------: | -------------: | -----: |
| record: serialize                                  |       672 |       536 |          673 |              525 |       1,490 |       192 |             857 |        86 |            248 |        |
| record: deserialize                                |       441 |       417 |          505 |              545 |         400 |       233 |           1,192 |       121 |            686 |        |
| document of 100 rows: serialize                    |     2,391 |     2,119 |        2,033 |            1,761 |       3,770 |     1,468 |           8,619 |     1,517 |          2,151 |        |
| document of 100 rows: deserialize                  |     1,241 |     1,329 |        1,057 |            1,145 |       1,905 |       625 |           7,182 |     2,684 |          4,738 |        |
| document of 100 rows: sum a column per element     |       688 |       736 |          632 |              680 |         400 |       208 |           7,182 |     2,684 |          4,738 |        |
| document of 100,000 rows: serialize                | 1,200,840 | 1,200,568 |    1,200,833 |        1,200,561 |   3,297,674 | 3,998,040 |      11,654,776 | 1,200,323 |      1,600,557 |        |
| document of 100,000 rows: deserialize              |     1,241 |     1,329 |        1,057 |            1,145 |       1,905 |       625 |      10,399,374 | 1,201,484 |      2,801,944 |        |
| document of 100,000 rows: sum a column per element |       688 |       736 |          632 |              680 |         400 |       208 |      10,399,374 | 1,201,484 |      2,801,944 |        |
| vector of 100 nested tables: serialize             |    40,133 |    29,109 |       39,023 |           27,787 |       8,186 |     2,221 |         119,850 |     9,471 |         10,102 |        |
| vector of 100 nested tables: deserialize           |    12,520 |    34,208 |       12,464 |           34,208 |      11,784 |    11,770 |         123,504 |    15,968 |         16,216 |        |
| array of 100 structs: serialize                    |     3,073 |     2,625 |        3,066 |            2,618 |       3,666 |     1,941 |          62,073 |     3,709 |          3,866 |        |
| array of 100 structs: deserialize                  |    11,896 |    12,160 |       11,840 |           12,160 |      11,696 |    11,690 |          66,032 |    15,736 |         16,600 |        |
| fixed-size record: serialize                       |        69 |        45 |           69 |               45 |       1,455 |       161 |             848 |        65 |            229 |     45 |
| fixed-size record: deserialize                     |       224 |        88 |          480 |               88 |         432 |       200 |           1,127 |        88 |            665 |     88 |
| document of 100 rows: serialize lists              |     3,831 |     2,287 |        3,473 |            2,241 |       3,802 |     1,468 |           8,467 |     2,957 |          3,591 |        |
| document of 100,000 rows: serialize lists          | 2,401,080 | 1,600,336 |    2,401,073 |        1,600,641 |   3,546,058 | 1,187,728 |       8,457,856 | 2,400,563 |      2,800,797 |        |

The native classes' peaks are within 1.4 KB of the pure-Python ones'.

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
within 5% of the fastest value for all 16 benchmarks with the native classes,
and 15 of the 16 with the pure-Python ones.

Fastest value in µs, and its ratio to `ByteBuffer`'s by hand, with the native
classes:

| Benchmark                                       | tafla |      tafla-gen | tafla-gen view | tafla-gen view A/A |
| ----------------------------------------------- | ----: | -------------: | -------------: | -----------------: |
| vector of 100 nested tables: read the last item | 0.443 | 6.681 (15.06×) |  0.780 (1.76×) |      0.782 (1.76×) |
| vector of 100 nested tables: sum every item     |  24.3 |   41.6 (1.71×) |   41.6 (1.71×) |       41.1 (1.69×) |
| array of 100 structs: read the last item        | 0.433 |  18.1 (41.91×) |  0.800 (1.85×) |      0.801 (1.85×) |
| array of 100 structs: sum every item            | 7.033 |   21.0 (2.98×) |   21.1 (3.01×) |       21.2 (3.01×) |

With the pure-Python classes:

| Benchmark                                       | tafla |     tafla-gen | tafla-gen view | tafla-gen view A/A |
| ----------------------------------------------- | ----: | ------------: | -------------: | -----------------: |
| vector of 100 nested tables: read the last item | 1.385 |  12.0 (8.70×) |  1.618 (1.17×) |      1.619 (1.17×) |
| vector of 100 nested tables: sum every item     |  64.2 |  81.1 (1.26×) |   81.4 (1.27×) |       81.0 (1.26×) |
| array of 100 structs: read the last item        | 0.624 | 18.3 (29.38×) |  0.998 (1.60×) |      1.002 (1.61×) |
| array of 100 structs: sum every item            | 7.252 |  21.2 (2.93×) |   21.4 (2.95×) |       21.3 (2.94×) |

The most bytes that a call held at once, as tracemalloc traces them, with the
native classes and with the pure-Python ones:

| Benchmark                                       |         tafla |       tafla-gen | tafla-gen view | tafla-gen view A/A |
| ----------------------------------------------- | ------------: | --------------: | -------------: | -----------------: |
| vector of 100 nested tables: read the last item |     320 / 264 |   6,960 / 6,936 |      424 / 368 |          424 / 368 |
| vector of 100 nested tables: sum every item     | 1,256 / 1,464 |   6,960 / 6,936 |  1,688 / 1,752 |      1,688 / 1,752 |
| array of 100 structs: read the last item        |     504 / 448 | 11,936 / 11,880 |      592 / 536 |          592 / 536 |
| array of 100 structs: sum every item            |     632 / 576 | 11,936 / 11,880 |      712 / 712 |          712 / 712 |

- The views read the last item 8.6 times faster than the getters for the
  vector, 0.78 µs against 6.7 µs, and 23 times faster for the array, 0.80 µs
  against 18.1 µs, with the native classes: the getters create a reader or a
  `Triple` for each of the 100 items first. With the pure-Python classes, they
  take 1.6 µs against 12.0 µs, and 1.0 µs against 18.3 µs. `ByteBuffer` by
  hand takes 0.44 µs and 0.43 µs, or 1.4 µs and 0.62 µs.
- Iterating over every item takes as long as through the getters, 41.6 µs for
  the vector either way and 21.1 µs against 21.0 µs for the array, since
  CPython creates each item's reader or `Triple` either way. The views save
  the list of them, so a call holds 1.7 KB at most against 7.0 KB for the
  vector, and 0.7 KB against 11.9 KB for the array. Reading the floats by hand
  takes 24.3 µs and 7.0 µs.
- With the pure-Python classes, iterating takes 81 µs for the vector, and the
  same 21 µs for the array, whose time goes to creating each `Triple`, a
  frozen dataclass, with either implementation.
- The A/A pair is within 1.2% of the views in every group.
