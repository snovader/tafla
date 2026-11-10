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

tafla is timed twice, and both write the same bytes:

- **tafla** nests `TableBuilder`s by hand, a builder for each table, and reads
  the tables with `ByteBuffer`'s getters.
- **tafla-gen** is the code that tafla-gen generates from
  `comparison/workloads.tafla`, in `comparison/generated/tafla/`. A message's
  `serialize()` counts the table's length with a `TableSizer`, and then writes
  every field, nested tables included, into bytes of that length with a
  `TableWriter`. Its readers read each field with a getter. It takes the
  workloads' inputs, as the other codecs do, so it creates the generated
  fields of the document and of each item of the vector in each call, as the
  protobufjs codec creates its messages, and creates the workloads' outputs
  from the generated readers. The workloads' items and records have the
  generated structs' fields already, so they're passed as they are.

Every language's suite has a copy of the schema. After changing them,
regenerate the code as `rust/gen/README.md` describes.

Four more groups time only tafla's bytes of the vector of nested tables and
of the array of structs, read in three ways: **read the last item**, which a
reader can find without reading the others, and **sum every item**. Each way
sums the three float32s of the items it reads, rather than creating objects
of its own, and `comparison/itemAccess.ts` holds them:

- **tafla** reads with `ByteBuffer`'s getters by hand: `getVectorOffset()` of
  the last item or `getVectorOffsets()` of every item, and `getStructArray()`
  with `readFloat32()`.
- **tafla-gen** reads with the generated getters, `items()`, which read every
  item into an array first.
- **tafla-gen view** reads with the generated views, `itemsView()`, whose
  `get(i)` reads one item, in a loop of `get(i)` for every item.
- **tafla-gen view A/A** reads with the views again, an A/A pair that shows
  the harness's noise.

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
| `msgpackr` 2.1.2      | A fast schemaless format that packs typed arrays as byte copies. The suite uses its plain JavaScript entry point. |

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
  a buffer for each message. tafla-gen's `serializeInto()`, which writes into
  bytes that the caller reuses, isn't timed here; the core README's
  performance section gives its figures.
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
  took 0.52–0.54 µs and 13.7 KB of the vector's 5.3–5.5 µs when run alone.
  It passes the records and the array of structs as they are. It reads its
  outputs from the generated readers, objects of their own that the builder
  codec doesn't create.

### Results

One run on 2026-11-10, on an Apple M2 Pro with 32 GB in macOS 27.0.1, with
Node 27.0.0 (an alpha, with V8 15.6), of
`yarn bench:compare --rounds 61 --sample-ms 5`. It took 39 seconds. The
1-minute load average was 3.1–3.2 on the 10 cores, and other processes used
at most 3.5 cores in samples every 2 seconds. A second run right after it
gave fastest times within 7% of these, apart from tafla's read of the
fixed-size record, 0.010 µs against 0.009 µs, and the median difference was
1.3%. A run with Node 26.13.0, with V8 14.6, differed from this one by 5.7%
at the median, and tafla wrote the large document 26% slower there, in
39.1 µs.

Fastest call in µs, and its ratio to tafla's:

| Benchmark                                          | tafla |     tafla-gen |     flatbuffers |        protobufjs |       msgpackr |
| -------------------------------------------------- | ----: | ------------: | --------------: | ----------------: | -------------: |
| record: serialize                                  | 0.174 | 0.119 (0.68×) |   0.491 (2.82×) |     0.211 (1.21×) |  0.347 (1.99×) |
| record: deserialize                                | 0.166 | 0.172 (1.04×) |   0.200 (1.20×) |     0.254 (1.53×) |  0.286 (1.72×) |
| document of 100 rows: serialize                    | 0.599 | 0.475 (0.79×) |   2.625 (4.38×) |     2.545 (4.25×) |  1.152 (1.92×) |
| document of 100 rows: deserialize                  | 0.280 | 0.293 (1.05×) |   0.317 (1.13×) |     1.965 (7.02×) |  1.371 (4.90×) |
| document of 100 rows: sum a column per element     | 0.161 | 0.182 (1.13×) |   0.669 (4.16×) |     1.122 (6.98×) |  1.461 (9.09×) |
| document of 100,000 rows: serialize                |  31.1 |  30.7 (0.99×) | 1736.3 (55.92×) |   2002.6 (64.49×) |   21.6 (0.69×) |
| document of 100,000 rows: deserialize              | 0.277 | 0.295 (1.07×) |   0.313 (1.13×) | 1002.8 (3621.94×) | 35.0 (126.31×) |
| document of 100,000 rows: sum a column per element |  86.1 |  85.8 (1.00×) |   591.9 (6.87×) |    932.3 (10.83×) |  128.6 (1.49×) |
| vector of 100 nested tables: serialize             |  13.5 | 5.769 (0.43×) |    13.2 (0.98×) |     7.225 (0.54×) |   11.2 (0.83×) |
| vector of 100 nested tables: deserialize           | 1.349 | 2.611 (1.94×) |   4.653 (3.45×) |     6.231 (4.62×) |  3.772 (2.80×) |
| array of 100 structs: serialize                    | 0.788 | 0.817 (1.04×) |   1.437 (1.83×) |     5.303 (6.73×) |  5.026 (6.38×) |
| array of 100 structs: deserialize                  | 0.977 | 1.845 (1.89×) |   1.516 (1.55×) |     4.705 (4.82×) |  3.266 (3.34×) |
| fixed-size record: serialize                       | 0.039 | 0.033 (0.85×) |   0.333 (8.60×) |     0.119 (3.06×) |  0.270 (6.97×) |
| fixed-size record: deserialize                     | 0.009 | 0.011 (1.19×) |   0.072 (7.64×) |     0.039 (4.12×) | 0.176 (18.57×) |
| document of 100 rows: serialize number[]           | 0.766 | 0.650 (0.85×) |   2.539 (3.31×) |     2.531 (3.30×) |  1.825 (2.38×) |
| document of 100,000 rows: serialize number[]       | 148.9 | 146.5 (0.98×) | 1639.7 (11.01×) |   1986.6 (13.34×) |  180.0 (1.21×) |

Bytes allocated on V8's heap per call, which leave out the contents of
`ArrayBuffer`s of more than 64 bytes, such as most messages' bytes:

| Benchmark                                          |  tafla | tafla-gen | flatbuffers | protobufjs | msgpackr |
| -------------------------------------------------- | -----: | --------: | ----------: | ---------: | -------: |
| record: serialize                                  |    680 |       472 |       1,448 |        577 |      456 |
| record: deserialize                                |    464 |       496 |         568 |        896 |      776 |
| document of 100 rows: serialize                    |  1,216 |       808 |       1,840 |      4,936 |    1,256 |
| document of 100 rows: deserialize                  |    848 |       920 |       1,024 |      5,930 |    2,633 |
| document of 100 rows: sum a column per element     |    320 |       392 |         256 |      4,709 |    2,649 |
| document of 100,000 rows: serialize                |  1,216 |       808 |       5,760 |  3,202,173 |    1,499 |
| document of 100,000 rows: deserialize              |    848 |       920 |       1,024 |  4,371,664 |    2,632 |
| document of 100,000 rows: sum a column per element |    319 |       391 |         256 |  4,374,669 |    2,647 |
| vector of 100 nested tables: serialize             | 47,787 |    17,279 |       2,769 |     15,270 |    5,256 |
| vector of 100 nested tables: deserialize           | 11,522 |    21,999 |      18,704 |     25,823 |   17,033 |
| array of 100 structs: serialize                    |    976 |       776 |       1,432 |      1,585 |   13,029 |
| array of 100 structs: deserialize                  | 10,787 |    21,269 |      14,685 |     22,623 |   23,442 |
| fixed-size record: serialize                       |    264 |       264 |       1,040 |        417 |      504 |
| fixed-size record: deserialize                     |     96 |       192 |         296 |        272 |      680 |
| document of 100 rows: serialize number[]           |  1,216 |       808 |       1,840 |      4,935 |    1,952 |
| document of 100,000 rows: serialize number[]       |  1,208 |       799 |       5,675 |  3,202,088 |    2,186 |

- tafla-gen writes a table 1.3 to 2.3 times as fast as the hand-built
  builders: the record in 0.12 µs against 0.17 µs, the small document in
  0.48 µs against 0.60 µs, and the vector of 100 nested tables in 5.8 µs
  against 13.5 µs, allocating 17 KB on the heap against 48 KB. That includes
  creating the generated fields of each of the 100 items from the
  workload's: in separate runs of each step alone, in 6 processes, in which
  the whole call took 5.3–5.5 µs, creating them took 0.52–0.54 µs and
  13.7 KB, and `ItemList.serialize()` of fields created beforehand
  4.8–4.9 µs and 3.6 KB.
- tafla-gen's vector serialize settles at one of two speeds in each process:
  in 8 runs of its group alone, it took 5.5–5.6 µs and 17.3 KB in 6, and
  6.1 µs and 14.1 KB in the other 2. Compare its times between runs by their
  heap bytes, which tell the two apart.
- The large document, which is mostly its columns' 1.2 MB, takes 31 µs
  either way. From `number[]` columns, which both convert element by
  element, it takes 147–149 µs either way.
- The array of structs is written in about the same time, 0.82 µs against
  0.79 µs, by the same `StructWriter` sequence, into a table that the
  writer sizes rather than a builder.
- The generated readers read the record and the documents within 13% of the
  hand-written reads, which call the same `ByteBuffer` getters. They're
  slower where they create objects of their own: the vector's reader creates
  an `Item` and a `Pair` for each item, and the array's reader a `Triple` for
  each struct, from which the codec creates the workload's items. Reading the
  vector takes 2.6 µs against 1.3 µs, and the array 1.8 µs against 0.98 µs,
  allocating about twice the heap bytes.

#### Reading items through views

Two runs of the four groups that read tafla's items, right after the run
above, with the same Node, of
`yarn bench:compare --filter item --rounds 61 --sample-ms 5`. The 1-minute
load average was 3.3–4.4, and other processes used at most 1.9 cores. The
tables give the second run, and the first's times were within 2.4% of it,
apart from the views' A/A sum of the array of structs, 7.3% slower in the
first. V8 resizes its young generation as a process allocates, which moved
the views' sum of the array of structs between 1.1 and 4.8 µs from one
process to another while the views were designed.

Fastest call in µs, and its ratio to `ByteBuffer`'s by hand:

| Benchmark                                       | tafla |      tafla-gen | tafla-gen view | tafla-gen view A/A |
| ----------------------------------------------- | ----: | -------------: | -------------: | -----------------: |
| vector of 100 nested tables: read the last item | 0.076 |  0.638 (8.40×) |  0.091 (1.20×) |      0.091 (1.20×) |
| vector of 100 nested tables: sum every item     | 1.194 |  1.517 (1.27×) |  1.896 (1.59×) |      1.858 (1.56×) |
| array of 100 structs: read the last item        | 0.100 | 1.414 (14.13×) |  0.119 (1.19×) |      0.119 (1.19×) |
| array of 100 structs: sum every item            | 0.809 |  1.723 (2.13×) |  1.416 (1.75×) |      1.460 (1.81×) |

Bytes allocated on V8's heap per call:

| Benchmark                                       | tafla | tafla-gen | tafla-gen view | tafla-gen view A/A |
| ----------------------------------------------- | ----: | --------: | -------------: | -----------------: |
| vector of 100 nested tables: read the last item |   200 |     5,900 |            320 |                320 |
| vector of 100 nested tables: sum every item     | 1,048 |     5,900 |          4,281 |              4,281 |
| array of 100 structs: read the last item        |   320 |    10,802 |            488 |                488 |
| array of 100 structs: sum every item            |   320 |    10,802 |          9,993 |              9,993 |

- The views read the last item without reading the others, as `ByteBuffer`
  does by hand: 0.091 µs against 0.076 µs for the vector, and 0.119 µs
  against 0.100 µs for the array, 7 and 12 times as fast as the getters,
  which read all 100 items first.
- Summing every item of the vector through its view takes 1.86–1.90 µs,
  against 1.52 µs through the getter. The getter finds every item with one
  call of `getVectorOffsets()`, and the view's `get(i)` finds each item with
  `getVectorItemOffset()`, which reads and checks the item's two ends from
  the count that the view read once. Both create an `Item` and a `Pair` for
  each item, and the view saves only the array of them, 1.6 KB of the
  5.9 KB. The reads by hand take 1.19 µs.
- Summing the array of structs through its view takes 1.42–1.46 µs against
  1.72 µs, 15–18% less, though it creates a `Triple` for each struct as the
  getter does. `readFloat32()` by hand takes 0.81 µs.
- V8 removes the views' objects for each item only in some builds of the
  code. The suite runs the TypeScript source as Node loads it, module by
  module, which keeps them. Two runs of the same groups from an esbuild 0.28
  bundle of the suite, made with `esbuild benchmarks/comparison/main.ts
  --bundle --platform=node --format=esm --target=es2022
  --conditions=tafla-source`, with the same Node and options, removed them:
  the views summed the vector in 1.43–1.46 µs and 281 bytes, and the array of
  structs in 0.69–0.71 µs and 393 bytes, against 1.02–1.04 µs and
  0.58–0.59 µs by hand.
