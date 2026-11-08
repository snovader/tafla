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
other processes then slows every variant alike, and only for some rounds.
Every variant has its own timer loop, compiled from a unique source, so that
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
  a buffer for each message.
- The heap bytes leave out what V8 allocates outside its heap: the contents of
  every `ArrayBuffer` of more than 64 bytes, such as the bytes of most
  messages and the buffers that builders grow, and protobufjs's and msgpackr's
  pooled buffers. A typed array of up to 64 bytes counts with its contents,
  and every typed array counts its object, about 200 bytes, so they compare
  objects, arrays and small buffers, not all the memory a call takes.
- tafla-gen's codec creates the generated fields from the workloads' flat
  inputs in each call, which a caller that holds its values in the generated
  shape wouldn't, and reads its outputs from the generated readers, objects
  of their own that the builder codec doesn't create.

### Results

One run on 2026-11-08, on an Apple M2 Pro with 32 GB in macOS 27.0.1, with
Node 27.0.0 (an alpha, with V8 15.6), of
`yarn bench:compare --rounds 61 --sample-ms 5`. It took 29 seconds. Other
processes kept the 1-minute load average at 2.8–2.9 on the 10 cores. A second
run right after it gave fastest times within 6% of these for tafla, and within
10% for all but 5 of the 80 times in the first table, which were up to 24%
slower, among them tafla-gen's read of the array of structs, 21% slower. The
median difference was 2.1%. A run with Node 26.13.0, with V8 14.6, differed
from this one by 3.3% at the median, and tafla wrote the large document 19%
slower there, in 36.8 µs.

Fastest call in µs, and its ratio to tafla's:

| Benchmark                                          | tafla |     tafla-gen |     flatbuffers |       protobufjs |       msgpackr |
| -------------------------------------------------- | ----: | ------------: | --------------: | ---------------: | -------------: |
| record: serialize                                  | 0.165 | 0.118 (0.72×) |   0.494 (2.99×) |    0.214 (1.29×) |  0.351 (2.12×) |
| record: deserialize                                | 0.162 | 0.172 (1.06×) |   0.206 (1.28×) |    0.273 (1.69×) |  0.283 (1.75×) |
| document of 100 rows: serialize                    | 0.583 | 0.460 (0.79×) |   2.662 (4.57×) |    2.581 (4.43×) |  1.143 (1.96×) |
| document of 100 rows: deserialize                  | 0.278 | 0.290 (1.04×) |   0.322 (1.16×) |    1.976 (7.11×) |  1.429 (5.15×) |
| document of 100 rows: sum a column per element     | 0.162 | 0.177 (1.09×) |   0.662 (4.08×) |    1.168 (7.20×) |  1.473 (9.08×) |
| document of 100,000 rows: serialize                |  30.8 |  30.4 (0.99×) | 1718.6 (55.74×) |  1964.9 (63.73×) |   20.9 (0.68×) |
| document of 100,000 rows: deserialize              | 0.276 | 0.288 (1.04×) |   0.316 (1.14×) | 992.8 (3597.91×) | 32.7 (118.51×) |
| document of 100,000 rows: sum a column per element |  85.8 |  85.8 (1.00×) |   588.2 (6.86×) |   922.9 (10.76×) |  126.3 (1.47×) |
| vector of 100 nested tables: serialize             |  13.1 | 5.657 (0.43×) |    12.8 (0.98×) |    7.156 (0.55×) |   11.0 (0.84×) |
| vector of 100 nested tables: deserialize           | 1.300 | 2.534 (1.95×) |   4.567 (3.51×) |    6.193 (4.76×) |  3.685 (2.83×) |
| array of 100 structs: serialize                    | 0.774 | 0.786 (1.01×) |   1.426 (1.84×) |    5.217 (6.74×) |  4.788 (6.18×) |
| array of 100 structs: deserialize                  | 0.951 | 1.859 (1.95×) |   1.456 (1.53×) |    4.713 (4.96×) |  3.238 (3.40×) |
| fixed-size record: serialize                       | 0.038 | 0.030 (0.79×) |   0.321 (8.34×) |    0.123 (3.18×) |  0.271 (7.05×) |
| fixed-size record: deserialize                     | 0.009 | 0.011 (1.19×) |   0.070 (7.62×) |    0.038 (4.14×) | 0.171 (18.53×) |
| document of 100 rows: serialize number[]           | 0.765 | 0.638 (0.83×) |   2.536 (3.31×) |    2.495 (3.26×) |  1.802 (2.35×) |
| document of 100,000 rows: serialize number[]       | 143.3 | 143.8 (1.00×) | 1597.3 (11.15×) |  1951.7 (13.62×) |  173.1 (1.21×) |

Bytes allocated on V8's heap per call, which leave out the contents of
`ArrayBuffer`s of more than 64 bytes, such as most messages' bytes:

| Benchmark                                          |  tafla | tafla-gen | flatbuffers | protobufjs | msgpackr |
| -------------------------------------------------- | -----: | --------: | ----------: | ---------: | -------: |
| record: serialize                                  |    680 |       472 |       1,448 |        577 |      456 |
| record: deserialize                                |    464 |       496 |         568 |        896 |      776 |
| document of 100 rows: serialize                    |  1,216 |       808 |       1,840 |      4,935 |    1,256 |
| document of 100 rows: deserialize                  |    848 |       920 |       1,024 |      5,930 |    2,633 |
| document of 100 rows: sum a column per element     |    320 |       392 |         256 |      4,708 |    2,648 |
| document of 100,000 rows: serialize                |  1,215 |       807 |       5,760 |  3,202,172 |    1,492 |
| document of 100,000 rows: deserialize              |    848 |       920 |       1,024 |  4,373,564 |    2,632 |
| document of 100,000 rows: sum a column per element |    319 |       391 |         256 |  4,371,451 |    2,648 |
| vector of 100 nested tables: serialize             | 47,786 |    17,277 |       2,769 |     15,270 |    5,256 |
| vector of 100 nested tables: deserialize           | 11,520 |    21,999 |      18,709 |     25,823 |   17,028 |
| array of 100 structs: serialize                    |    976 |       776 |       1,432 |      1,585 |   13,029 |
| array of 100 structs: deserialize                  | 10,787 |    21,269 |      14,684 |     22,623 |   23,442 |
| fixed-size record: serialize                       |    264 |       264 |       1,040 |        417 |      504 |
| fixed-size record: deserialize                     |     96 |       192 |         296 |        272 |      680 |
| document of 100 rows: serialize number[]           |  1,216 |       808 |       1,840 |      4,935 |    1,952 |
| document of 100,000 rows: serialize number[]       |  1,206 |       798 |       5,675 |  3,202,044 |    2,163 |

- tafla-gen writes a table 1.3 to 2.3 times as fast as the hand-built
  builders: the record in 0.12 µs against 0.17 µs, the small document in
  0.46 µs against 0.58 µs, and the vector of 100 nested tables in 5.7 µs
  against 13.1 µs, allocating 17 KB on the heap against 48 KB. That includes
  creating the generated fields of each of the 100 items from the
  workload's: in a separate run of each step alone, in which the whole call
  took 5.9 µs, creating them took 0.57 µs and 13.7 KB, and
  `ItemList.serialize()` of fields created beforehand 4.8 µs and 3.6 KB.
- tafla-gen's vector serialize settles at one of two speeds in each process:
  in 8 runs of its group alone, it took 5.6–5.7 µs and 17.3 KB in 6, and
  6.1–6.2 µs and 14.1 KB in the other 2, as in the second full run. Compare
  its times between runs by their heap bytes, which tell the two apart.
- The large document, which is mostly its columns' 1.2 MB, takes 30 µs
  either way. From `number[]` columns, which both convert element by
  element, it takes 143 µs either way.
- The array of structs is written in about the same time, 0.79 µs against
  0.77 µs, by the same `StructWriter` sequence, into a table that the
  writer sizes rather than a builder.
- The generated readers read the record and the documents within 9% of the
  hand-written reads, which call the same `ByteBuffer` getters. They're
  slower where they create objects of their own: the vector's reader creates
  an `Item` and a `Pair` for each item, and the array's reader a `Triple` for
  each struct, from which the codec creates the workload's items. Reading the
  vector takes 2.5 µs against 1.3 µs, and the array 1.9 µs against 0.95 µs,
  allocating about twice the heap bytes.

#### Reading items through views

Two runs of the four groups that read tafla's items, right after the run
above, with the same Node, of
`yarn bench:compare --filter item --rounds 61 --sample-ms 5`. Other processes
kept the 1-minute load average at 2.6–3.0. The tables give the second run, and
the first's times were within 3.4% of it. V8 resizes its young generation as a
process allocates, which moved the views' sum of the array of structs between
1.1 and 4.8 µs from one process to another while the views were designed.

Fastest call in µs, and its ratio to `ByteBuffer`'s by hand:

| Benchmark                                       | tafla |      tafla-gen | tafla-gen view | tafla-gen view A/A |
| ----------------------------------------------- | ----: | -------------: | -------------: | -----------------: |
| vector of 100 nested tables: read the last item | 0.075 |  0.623 (8.26×) |  0.103 (1.36×) |      0.103 (1.37×) |
| vector of 100 nested tables: sum every item     | 1.164 |  1.414 (1.21×) |  1.988 (1.71×) |      1.983 (1.70×) |
| array of 100 structs: read the last item        | 0.099 | 1.244 (12.57×) |  0.111 (1.13×) |      0.112 (1.13×) |
| array of 100 structs: sum every item            | 0.808 |  1.592 (1.97×) |  1.298 (1.61×) |      1.297 (1.61×) |

Bytes allocated on V8's heap per call:

| Benchmark                                       | tafla | tafla-gen | tafla-gen view | tafla-gen view A/A |
| ----------------------------------------------- | ----: | --------: | -------------: | -----------------: |
| vector of 100 nested tables: read the last item |   200 |     5,900 |            360 |                360 |
| vector of 100 nested tables: sum every item     | 1,048 |     5,900 |          4,281 |              4,281 |
| array of 100 structs: read the last item        |   320 |    10,802 |            488 |                488 |
| array of 100 structs: sum every item            |   320 |    10,802 |          9,993 |              9,993 |

- The views read the last item without reading the others, as `ByteBuffer`
  does by hand: 0.103 µs against 0.075 µs for the vector, and 0.111 µs
  against 0.099 µs for the array, 6 and 11 times as fast as the getters,
  which read all 100 items first.
- Summing every item of the vector through its view takes 1.99 µs, against
  1.41 µs through the getter. The getter finds every item with one call of
  `getVectorOffsets()`, and the view's `get(i)` finds each item with
  `getVectorOffset()`, which checks the vector's count and ends again. Both
  create an `Item` and a `Pair` for each item, and the view saves only the
  array of them, 1.6 KB of the 5.9 KB. The reads by hand take 1.16 µs.
- Summing the array of structs through its view takes 1.30 µs against
  1.59 µs, 18% less, though it creates a `Triple` for each struct as the
  getter does. `readFloat32()` by hand takes 0.81 µs.
- V8 removes the views' objects for each item only in some builds of the
  code. The suite runs the TypeScript source as Node loads it, module by
  module, which keeps them. Two runs of the same groups from an esbuild 0.28
  bundle of the suite, made with `esbuild benchmarks/comparison/main.ts
  --bundle --platform=node --format=esm --target=es2022
  --conditions=tafla-source`, with the same Node and options, removed them:
  the views summed the vector in 1.60–1.63 µs and 280 bytes, and the array of
  structs in 0.68–0.70 µs and 392 bytes, against 1.01–1.04 µs and
  0.57–0.58 µs by hand.
