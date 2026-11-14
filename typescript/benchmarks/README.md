# Benchmarks

`yarn bench` times `tafla`'s getters and setters of every element type
on their own, and `yarn bench:compare` compares the package with other
JavaScript libraries. Run both from `typescript/`.

## Comparison

The comparison times tafla and other JavaScript libraries that encode small
tables and typed numeric columns, on generic table shapes:

- **record**: one table of two float64s, an int64 and a 16-byte string.
- **document of 100 and 100,000 rows**: a sized, versioned table of two
  float64s and a string, with a nested table of an int64 and three columns, a
  Uint32 and two Float32.
- **vector of 100 nested tables**: a table holding a vector of tables, each a
  float32 and a nested table of two float32s.
- **array of 100 structs**: a table holding the same items as an array of
  structs of their three float32s, 12 bytes each, written with `StructWriter`
  and `setStructArray()` and read with `getStructArray()` and `readFloat32()`.
- **fixed-size record**: a uint32 and two float32s at fixed offsets, written
  with `StructWriter`, or into new bytes with `writeUint32()`/`writeFloat32()`
  by tafla-gen's code, and read with `readUint32()`/`readFloat32()`.

Each shape is serialized and deserialized, with columns read back as typed
arrays. Documents are also serialized from `number[]` columns, and a column is
summed through each library's per-element accessor. The suite also prints
encoded sizes and what each library does with a truncated document.

**document of 100,000 rows: serialize into reused bytes** writes the large
document into bytes kept between calls, with the libraries that can:
tafla-gen's `serializeInto()` writes it into the start of bytes made once, as
long as its table, rather than into new bytes, and msgpackr's `pack()` is
timed again beside it, since it reuses its `Packr`'s buffer in every call,
the serialize row's too. The other libraries write each message into a
buffer of its own.

tafla is timed twice, and both write the same bytes:

- **tafla** writes each table with a `TableBuilder` by hand, recording its
  nested tables among the builder's fields with `setTableInPlace()` and
  `setVectorInPlace()`, and reads the tables with `ByteBuffer`'s getters.
- **tafla-gen** is the code that tafla-gen generates from
  `comparison/workloads.tafla`, in `comparison/generated/tafla/`. A message's
  `serialize()` counts the table's length with a `TableSizer`, and then writes
  every field, nested tables included, into bytes of that length with a
  `TableWriter`. Its readers read each field with a getter, and the getters
  of the vector and of the array of structs return views of their items,
  which read each item when `get(i)` asks for it. Where the generated fields
  are the workload's output, it reads them whole instead, without a reader
  for each struct: the record with `fields()`, the array of structs with its
  view's `fields()`, and the fixed-size record with the struct's static
  `read()`. It takes the workloads' inputs, as the other codecs do, so it
  creates the generated fields of the document and of each item of the
  vector in each call, as the protobufjs codec creates its messages, and
  creates their outputs from the generated readers. The workloads' items and
  records have the generated structs' fields already, so they're passed as
  they are.

Every language's suite has a copy of the schema. After changing them,
regenerate the code as `rust/gen/README.md` describes.

Four more groups time only tafla's bytes of the vector of nested tables and
of the array of structs, read in two ways: **read the last item**, which a
reader can find without reading the others, and **sum every item**. Each way
sums the three float32s of the items it reads, rather than creating objects
of its own, and `comparison/itemAccess.ts` holds them:

- **tafla** reads with `ByteBuffer`'s getters by hand: `getVectorOffset()` of
  the last item or `getVectorOffsets()` of every item, and `getStructArray()`
  with `readFloat32()`.
- **tafla-gen** reads with the generated getters, `items()`, which return
  views: the vector's `get(i)` reads one item, and the array of structs'
  getter of each field, such as `value(i)`, reads that field of one struct
  without creating the struct. A loop reads every item.
- **tafla-gen A/A** reads with the getters again, an A/A pair that shows the
  harness's noise.

Each library writes the array of structs in its own fixed-size encoding:
FlatBuffers as a vector of structs, protobuf as a repeated message of the three
floats, which it writes as fixed32s, and msgpackr as an array of each struct's
fields in order, without their keys, in an array for the table, as rmp-serde's
`to_vec()` packs them. protobuf has no structs, and packed repeated floats
would make a column of each field, which the documents time already. msgpackr's
records extension, which `new Packr()` enables, already packs the vector's
items as their keys once and then their values, so its two shapes are about the
same size.

### Libraries

| Library               | Why it's comparable                                                                                               |
| --------------------- | ----------------------------------------------------------------------------------------------------------------- |
| `flatbuffers` 25.9.23 | Zero-copy tables with offsets and vectors, like tafla. flatc's generated code is in `comparison/generated/`.      |
| `protobufjs` 8.8.1    | The most used protobuf runtime, with packed repeated floats. Decoding copies into `number[]`.                     |
| `msgpackr` 2.1.4      | A fast schemaless format that packs typed arrays as byte copies. The suite uses its plain JavaScript entry point. |

### Method

Each group runs its variants interleaved, in alternating order, over 31 rounds
by default, after 10 warm-up rounds, and reports the fastest call. Load from
other processes then slows every variant alike, and only for some rounds. A
round repeats a variant's call for about `--sample-ms`, a number of times
counted from its fastest warm-up round, since the code that V8 optimizes
during the warm-up runs several times as fast as the first calls. Every
variant has its own timer loop, compiled from a unique source, so that
call sites and type feedback aren't shared. Groups with `number[]` inputs run
last, since they change the type feedback of code the typed-array groups
share. Before timing, every codec must read back exactly what it wrote.

Beside each time, the suite prints the bytes that a call allocates on V8's
heap. After a group's timed rounds, each variant's timer loop runs 5 more
samples of its calls, apart from the timings, with a `GCProfiler` from
`node:v8` started around each. A sample's bytes are how much the heap's used
size grew, plus what the garbage collections during it freed, which the
profiler reports, less what a sample of nothing takes. The fewest over the
samples counts, since compiling and deoptimizing allocate too, and it's what
the optimized code allocates, after escape analysis has removed the objects
that don't escape. V8's own count of allocated bytes,
`total_allocated_bytes`, grows in steps of whole allocation buffers, of
100 KiB or more, which is too coarse for a call.

### Running

```sh
uptime
yarn bench:compare --rounds 41
```

`--filter <regex>` runs only the matching groups, and `--sample-ms` sets how
long one round of one variant takes (1 ms by default). Check `uptime` first:
a high load from other processes makes even the minimum unreliable.

### Limitations

- Node has wider Smis than V8 builds with pointer compression, such as
  Cloudflare's workerd, so integer-heavy code can run differently there.
- protobufjs generates its codecs with `new Function`, which Workers don't
  allow. `pbjs` emits the same code as a static module for that.
- msgpackr packs successive messages into one shared buffer and returns views
  of it, so it allocates once for many messages. The other libraries allocate
  a buffer for each message, apart from tafla-gen's `serializeInto()` in the
  row of reused bytes.
- msgpackr packs each of the vector's items as the workload gives it, one
  flat object of its three floats, where tafla, FlatBuffers and protobufjs
  write a pair nested in each item, a table or message of its own. msgpackr
  writes and reads 100 objects, and the others 200 tables or messages.
- The heap bytes leave out what V8 allocates outside its heap: the contents of
  every `ArrayBuffer` of more than 64 bytes, such as the bytes of most
  messages and the buffers that builders grow, and protobufjs's and msgpackr's
  pooled buffers. A typed array of up to 64 bytes counts with its contents,
  and every typed array counts its object, about 200 bytes, so they compare
  objects, arrays and small buffers, not all the memory a call takes.
- tafla-gen's codec creates the generated fields from the workloads' flat
  inputs in each call where their shapes differ, which a caller that holds
  its values in the generated shape wouldn't: an object for the document's
  columns, and an item and a pair for each of the vector's 100 items, which
  took 0.52–0.54 µs and 13.7 KB of the vector's 4.0–4.1 µs when run alone.
  It passes the records and the array of structs as they are. It reads the
  document's and the vector's outputs from the generated readers, objects of
  their own that the builder codec doesn't create.

### Results

One run on 2026-11-14, of commit 0529dbd, on an Apple M2 Pro with 32 GB in
macOS 27.0.1, with Node 27.0.0 (an alpha, with V8 15.6), of
`yarn bench:compare --rounds 61 --sample-ms 5`. It took 37 seconds. The
1-minute load average was 2.1–2.9 on the 10 cores, and other processes used
at most 1.1 cores in samples every 2 seconds. A second run right after it
gave fastest times within 7% of these, apart from tafla's write of the
fixed-size record, 10% slower, and two reads of the last item, 8–9% faster,
and the median difference was 1.0%. A run with Node 26.13.0, with V8 14.6,
differed from this one by 5.3% at the median. It took 1.6 to 2.1 times as
long to read the vector of nested tables and to sum its items, and 23–31%
longer to write the large document, as msgpackr did too.

Fastest call in µs, and its ratio to tafla's:

| Benchmark                                             | tafla |     tafla-gen |     flatbuffers |        protobufjs |       msgpackr |
| ----------------------------------------------------- | ----: | ------------: | --------------: | ----------------: | -------------: |
| record: serialize                                     | 0.173 | 0.122 (0.70×) |   0.499 (2.89×) |     0.209 (1.21×) |  0.348 (2.02×) |
| record: deserialize                                   | 0.159 | 0.152 (0.95×) |   0.204 (1.28×) |     0.264 (1.65×) |  0.293 (1.84×) |
| document of 100 rows: serialize                       | 0.652 | 0.497 (0.76×) |   2.682 (4.11×) |     2.553 (3.92×) |  1.135 (1.74×) |
| document of 100 rows: deserialize                     | 0.266 | 0.263 (0.99×) |   0.325 (1.22×) |     1.954 (7.36×) |  1.410 (5.31×) |
| document of 100 rows: sum a column per element        | 0.160 | 0.178 (1.11×) |   0.660 (4.13×) |     1.149 (7.20×) |  1.490 (9.33×) |
| document of 100,000 rows: serialize                   |  30.7 |  31.0 (1.01×) | 1776.8 (57.84×) |   1991.9 (64.84×) |   21.9 (0.71×) |
| document of 100,000 rows: deserialize                 | 0.265 | 0.260 (0.98×) |   0.319 (1.20×) | 1029.0 (3878.32×) | 34.5 (130.14×) |
| document of 100,000 rows: sum a column per element    |  86.0 |  86.0 (1.00×) |   592.9 (6.90×) |    931.0 (10.83×) |  129.0 (1.50×) |
| document of 100,000 rows: serialize into reused bytes |     — |          19.1 |               — |                 — |           21.7 |
| vector of 100 nested tables: serialize                |  10.2 | 5.793 (0.57×) |    13.6 (1.33×) |     7.263 (0.71×) |   11.2 (1.10×) |
| vector of 100 nested tables: deserialize              | 0.981 | 1.709 (1.74×) |   4.756 (4.85×) |     6.314 (6.44×) |  3.778 (3.85×) |
| array of 100 structs: serialize                       | 0.804 | 0.835 (1.04×) |   1.434 (1.78×) |     5.295 (6.58×) |  5.005 (6.22×) |
| array of 100 structs: deserialize                     | 1.010 | 0.935 (0.93×) |   1.555 (1.54×) |     4.772 (4.72×) |  3.260 (3.23×) |
| fixed-size record: serialize                          | 0.039 | 0.033 (0.84×) |   0.324 (8.20×) |     0.122 (3.09×) |  0.274 (6.94×) |
| fixed-size record: deserialize                        | 0.010 | 0.009 (0.90×) |   0.072 (7.49×) |     0.040 (4.11×) | 0.175 (18.09×) |
| document of 100 rows: serialize number[]              | 0.801 | 0.640 (0.80×) |   2.588 (3.23×) |     2.566 (3.20×) |  1.861 (2.32×) |
| document of 100,000 rows: serialize number[]          | 147.8 | 147.8 (1.00×) | 1655.6 (11.20×) |   1989.3 (13.46×) |  179.0 (1.21×) |

Bytes allocated on V8's heap per call, which leave out the contents of
`ArrayBuffer`s of more than 64 bytes, such as most messages' bytes:

| Benchmark                                             |  tafla | tafla-gen | flatbuffers | protobufjs | msgpackr |
| ----------------------------------------------------- | -----: | --------: | ----------: | ---------: | -------: |
| record: serialize                                     |    704 |       472 |       1,448 |        577 |      456 |
| record: deserialize                                   |    464 |       496 |         568 |        896 |      776 |
| document of 100 rows: serialize                       |  1,368 |       808 |       1,840 |      4,936 |    1,256 |
| document of 100 rows: deserialize                     |    848 |       976 |       1,024 |      5,928 |    2,633 |
| document of 100 rows: sum a column per element        |    320 |       448 |         256 |      4,709 |    2,649 |
| document of 100,000 rows: serialize                   |  1,368 |       808 |       5,760 |  3,202,172 |    1,500 |
| document of 100,000 rows: deserialize                 |    848 |       976 |       1,024 |  4,373,978 |    2,632 |
| document of 100,000 rows: sum a column per element    |    319 |       447 |         256 |  4,374,669 |    2,647 |
| document of 100,000 rows: serialize into reused bytes |      — |       696 |           — |          — |    1,499 |
| vector of 100 nested tables: serialize                | 30,796 |    17,015 |       2,768 |     15,270 |    5,256 |
| vector of 100 nested tables: deserialize              | 11,482 |    14,745 |      18,705 |     25,822 |   17,033 |
| array of 100 structs: serialize                       |  1,000 |       776 |       1,432 |      1,585 |   13,029 |
| array of 100 structs: deserialize                     | 10,787 |    10,859 |      14,685 |     22,619 |   23,442 |
| fixed-size record: serialize                          |    264 |       264 |       1,040 |        417 |      504 |
| fixed-size record: deserialize                        |     96 |        96 |         296 |        272 |      680 |
| document of 100 rows: serialize number[]              |  1,368 |       808 |       1,840 |      4,935 |    1,952 |
| document of 100,000 rows: serialize number[]          |  1,359 |       800 |       5,675 |  3,202,040 |    2,174 |

- tafla-gen writes a table 1.3 to 1.8 times as fast as tafla's
  `TableBuilder`: the record in 0.12 µs against 0.17 µs, the small document
  in 0.50 µs against 0.65 µs, and the vector of 100 nested tables in 5.8 µs
  against 10.2 µs, allocating 17 KB on the heap against 31 KB. That includes
  creating the generated fields of each of the 100 items from the
  workload's: in separate runs of each step alone, in 6 processes, in which
  the whole call took 4.0–4.1 µs rather than the suite's 5.5–5.8 µs,
  creating them took 0.52–0.54 µs and 13.7 KB, and `ItemList.serialize()`
  of fields created beforehand 3.4–3.5 µs and 0.4 KB.
- tafla-gen's vector serialize can settle at one of two speeds in each
  process. In 8 runs of its group alone, it took 5.5–5.7 µs and 17.1–17.2 KB
  in every one, but in 8 runs on 2026-11-13, of c43286c, 3 took 5.9–6.0 µs
  and 14.1–14.3 KB. `ItemList.serialize()` alone took 3.4–3.5 µs in all 10
  of this run's processes, and 5.0 µs and 3.6 KB in some processes on
  2026-11-13. Compare its times between runs by their heap bytes, which tell
  the two apart.
- tafla's codec records the vector's items, and the pair that each nests,
  in place with `setVectorInPlace()` and `setTableInPlace()`. It writes the
  vector in 10.2 µs and 31 KB, against 13.7–14.3 µs and 53 KB from a
  builder per item in separate runs, ahead of flatbuffers' 13.6 µs and
  msgpackr's 11.2 µs, and behind protobufjs's 7.3 µs.
- The large document, which is mostly its columns' 1.2 MB, takes 31 µs
  either way. Into reused bytes, tafla-gen's `serializeInto()` writes it in
  19.1 µs, 38% less, since it doesn't allocate and zero new bytes, against
  msgpackr's 21.7 µs, whose `pack()` reuses its buffer in the serialize row
  too. From `number[]` columns, which both convert element by element, it
  takes 148 µs either way.
- The array of structs is written in about the same time, 0.84 µs against
  0.80 µs, by the same `StructWriter` sequence, into a table that the
  writer sizes rather than a builder.
- The generated readers read as fast as the hand-written reads, which call
  the same `ByteBuffer` getters, where they read the fields whole: the record
  with `fields()` in 0.15 µs against 0.16 µs, the array of structs with its
  view's `fields()` in 0.94 µs against 1.01 µs, the fixed-size record with
  `Sample.read()`, and the documents within 2%. Summing the small document's
  column took 0.17–0.18 µs against 0.16 µs. The vector's reader creates an
  `Item` and a `Pair` for each item, from which the codec creates the
  workload's items, and takes 1.7 µs against 1.0 µs, allocating 14.7 KB
  against 11.5 KB.

#### Reading items

Two runs of the four groups that read tafla's items, right after the run
above, with the same Node, of
`yarn bench:compare --filter item --rounds 61 --sample-ms 5`. The 1-minute
load average was 2.2–2.4, and other processes used at most 1.4 cores. The
tables give the second run, and the first's times were within 2.3% of it.
The getter's A/A pair was within 1.1%.

Fastest call in µs, and its ratio to `ByteBuffer`'s by hand:

| Benchmark                                       | tafla |     tafla-gen | tafla-gen A/A |
| ----------------------------------------------- | ----: | ------------: | ------------: |
| vector of 100 nested tables: read the last item | 0.077 | 0.086 (1.12×) | 0.086 (1.12×) |
| vector of 100 nested tables: sum every item     | 0.750 | 0.838 (1.12×) | 0.837 (1.12×) |
| array of 100 structs: read the last item        | 0.100 | 0.104 (1.03×) | 0.104 (1.04×) |
| array of 100 structs: sum every item            | 0.797 | 0.816 (1.02×) | 0.825 (1.04×) |

Bytes allocated on V8's heap per call:

| Benchmark                                       | tafla | tafla-gen | tafla-gen A/A |
| ----------------------------------------------- | ----: | --------: | ------------: |
| vector of 100 nested tables: read the last item |   200 |       280 |           280 |
| vector of 100 nested tables: sum every item     | 1,048 |       280 |           280 |
| array of 100 structs: read the last item        |   320 |       392 |           392 |
| array of 100 structs: sum every item            |   320 |       392 |           392 |

- The views that the getters return read the last item without reading the
  others, as `ByteBuffer` does by hand: 0.086 µs against 0.077 µs for the
  vector, and 0.104 µs against 0.100 µs for the array.
- Summing every item of the vector through `get(i)` takes 0.84 µs, against
  0.75 µs by hand. V8 inlines every read of the loop and removes the `Item`
  and the `Pair` that `get(i)` and `pair()` create, so the sum allocates
  280 bytes in all, where the reads by hand allocate 1 KB for the array of
  offsets that `getVectorOffsets()` returns.
- The array of structs' view has a getter of each field of its structs,
  such as `value(i)`, which reads it without creating the struct. Summing
  every struct with them takes 0.82 µs, against 0.80 µs with `readFloat32()`
  by hand.
- The suite runs the TypeScript source as Node loads it, module by module.
  Two runs of the same groups from an esbuild 0.28 bundle of the suite, made
  with `esbuild benchmarks/comparison/main.ts --bundle --platform=node
  --format=esm --target=es2022 --conditions=tafla-source`, with the same Node
  and options, read the last items as fast and summed every item 7–27%
  faster, with the same heap bytes: the getters summed the vector in
  0.76–0.77 µs and the array of structs in 0.65–0.66 µs, against
  0.70 µs and 0.58–0.59 µs by hand.
