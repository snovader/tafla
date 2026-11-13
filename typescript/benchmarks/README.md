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

- **tafla** writes each table with a `TableBuilder` by hand, recording its
  nested tables among the builder's fields with `setTableInPlace()` and
  `setVectorInPlace()`, and reads the tables with `ByteBuffer`'s getters.
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

One run on 2026-11-02, on an Apple M2 Pro with 32 GB in macOS 27.0.1, with
Node 26.13.0, of `yarn bench:compare --rounds 61 --sample-ms 5`. It took 23
seconds. Other processes kept the 1-minute load average at 3.6 on the 10
cores. A run 12 minutes earlier, with the same timings but another count of
heap bytes, gave fastest times within 6% of these for tafla and tafla-gen, and
within 10% for every codec, with a median difference of 1.6%. tafla and
tafla-gen then wrote the fixed-size record with the same code, an A/A pair:
0.038 and 0.039 µs. tafla-gen's code has since written a struct into new bytes
with the write functions, which took 0.030 µs against 0.040 µs on 2026-11-03,
with the same Node, in a comparison of the generated code before and after.

Fastest call in µs, and its ratio to tafla's:

| Benchmark                                          | tafla |     tafla-gen |     flatbuffers |        protobufjs |       msgpackr |
| -------------------------------------------------- | ----: | ------------: | --------------: | ----------------: | -------------: |
| record: serialize                                  | 0.420 | 0.159 (0.38×) |   0.574 (1.37×) |     0.233 (0.56×) |  0.369 (0.88×) |
| record: deserialize                                | 0.151 | 0.160 (1.06×) |   0.185 (1.22×) |     0.284 (1.88×) |  0.295 (1.95×) |
| document of 100 rows: serialize                    | 0.757 | 0.497 (0.66×) |   2.739 (3.62×) |     2.606 (3.44×) |  1.173 (1.55×) |
| document of 100 rows: deserialize                  | 0.276 | 0.286 (1.03×) |   0.302 (1.09×) |     2.072 (7.50×) |  1.397 (5.06×) |
| document of 100 rows: sum a column per element     | 0.177 | 0.179 (1.01×) |   0.659 (3.72×) |     1.216 (6.87×) |  1.582 (8.93×) |
| document of 100,000 rows: serialize                |  38.1 |  32.2 (0.85×) | 1803.4 (47.39×) |   2028.2 (53.30×) |   21.4 (0.56×) |
| document of 100,000 rows: deserialize              | 0.277 | 0.276 (1.00×) |   0.295 (1.06×) | 1305.2 (4715.94×) | 36.1 (130.36×) |
| document of 100,000 rows: sum a column per element |  86.1 |  85.8 (1.00×) |   577.7 (6.71×) |   1096.9 (12.74×) |  128.2 (1.49×) |
| vector of 100 nested tables: serialize             |  12.9 | 4.516 (0.35×) |    14.8 (1.15×) |     8.075 (0.63×) |   11.8 (0.91×) |
| vector of 100 nested tables: deserialize           | 1.863 | 2.976 (1.60×) |   5.332 (2.86×) |     6.458 (3.47×) |  3.822 (2.05×) |
| array of 100 structs: serialize                    | 0.853 | 0.782 (0.92×) |   2.088 (2.45×) |     5.731 (6.72×) |  5.747 (6.74×) |
| array of 100 structs: deserialize                  | 0.975 | 2.284 (2.34×) |   1.635 (1.68×) |     5.249 (5.38×) |  3.672 (3.77×) |
| fixed-size record: serialize                       | 0.038 | 0.039 (1.03×) |   0.316 (8.29×) |     0.121 (3.17×) |  0.280 (7.33×) |
| fixed-size record: deserialize                     | 0.010 | 0.012 (1.20×) |   0.073 (7.47×) |     0.040 (4.10×) | 0.204 (20.93×) |
| document of 100 rows: serialize number[]           | 1.000 | 0.706 (0.71×) |   2.691 (2.69×) |     2.589 (2.59×) |  1.868 (1.87×) |
| document of 100,000 rows: serialize number[]       | 236.3 | 236.4 (1.00×) |  1662.5 (7.03×) |    1994.4 (8.44×) |  263.3 (1.11×) |

Bytes allocated on V8's heap per call, which leave out the contents of
`ArrayBuffer`s of more than 64 bytes, such as most messages' bytes:

| Benchmark                                          |  tafla | tafla-gen | flatbuffers | protobufjs | msgpackr |
| -------------------------------------------------- | -----: | --------: | ----------: | ---------: | -------: |
| record: serialize                                  |    976 |       440 |       1,336 |        529 |      440 |
| record: deserialize                                |    440 |       472 |         552 |        896 |      776 |
| document of 100 rows: serialize                    |  1,496 |       784 |       1,696 |      4,757 |    1,192 |
| document of 100 rows: deserialize                  |    776 |       848 |         960 |      5,821 |    2,441 |
| document of 100 rows: sum a column per element     |    296 |       368 |         256 |      4,695 |    2,456 |
| document of 100,000 rows: serialize                |  1,502 |       783 |       5,264 |  3,202,180 |    1,485 |
| document of 100,000 rows: deserialize              |    776 |       880 |         960 |  4,348,056 |    2,440 |
| document of 100,000 rows: sum a column per element |    296 |       369 |         272 |  4,358,400 |    2,440 |
| vector of 100 nested tables: serialize             | 51,070 |    14,043 |       7,471 |     15,092 |    5,240 |
| vector of 100 nested tables: deserialize           | 10,858 |    19,748 |      18,697 |     25,823 |   17,033 |
| array of 100 structs: serialize                    |  1,072 |       768 |       6,152 |      1,406 |   13,011 |
| array of 100 structs: deserialize                  | 10,769 |    21,251 |      14,684 |     24,218 |   23,442 |
| fixed-size record: serialize                       |    232 |       232 |       1,024 |        385 |      488 |
| fixed-size record: deserialize                     |     96 |       192 |         296 |        272 |      680 |
| document of 100 rows: serialize number[]           |  1,496 |       784 |       1,696 |      4,757 |    1,792 |
| document of 100,000 rows: serialize number[]       |  1,481 |       751 |       5,192 |  3,201,956 |    2,048 |

- tafla-gen writes a table 1.2 to 2.9 times faster than the hand-built
  builders: the record in 0.16 µs against 0.42 µs, the small document in
  0.50 µs against 0.76 µs, and the vector of 100 nested tables in 4.5 µs
  against 12.9 µs, allocating 14 KB on the heap against 51 KB. That includes
  creating the generated fields of each of the 100 items from the
  workload's: in a separate run of each step alone, in which the whole call
  took 5.4 µs, creating them took 0.57 µs and 13.7 KB, and
  `ItemList.serialize()` of fields created beforehand 4.5 µs and 3.6 KB.
- The large document, which is mostly its columns' 1.2 MB, takes 32 µs
  against 38 µs. From `number[]` columns, which both convert element by
  element, it takes 236 µs either way.
- The array of structs is written in about the same time, 0.78 µs against
  0.85 µs, by the same `StructWriter` sequence, into a table that the
  writer sizes rather than a builder.
- The generated readers read the record and the documents within 6% of the
  hand-written reads, which call the same `ByteBuffer` getters. They're
  slower where they create objects of their own: the vector's reader creates
  an `Item` and a `Pair` for each item, and the array's reader a `Triple` for
  each struct, from which the codec creates the workload's items. Reading the
  vector takes 3.0 µs against 1.9 µs, and the array 2.3 µs against 1.0 µs,
  allocating about twice the heap bytes.

#### Reading items through views

One run of the four groups that read tafla's items, on 2026-11-02 on the same
machine, with Node 27.0.0 (an alpha, with V8 15.6), of
`yarn bench:compare --filter item --rounds 61 --sample-ms 5`. Other processes
kept the 1-minute load average at 1.9. A second run, with V8's young
generation fixed at 64 MiB by `--min-semi-space-size=64
--max-semi-space-size=64`, gave times within 5% of these, but for the views'
sum of the array of structs: 1.20 µs rather than 1.33 µs. V8 resizes its young
generation as a process allocates, which moved that sum between 1.1 and 4.8
µs from one process to another while the views were designed.

Fastest call in µs, and its ratio to `ByteBuffer`'s by hand:

| Benchmark                                       | tafla |      tafla-gen | tafla-gen view | tafla-gen view A/A |
| ----------------------------------------------- | ----: | -------------: | -------------: | -----------------: |
| vector of 100 nested tables: read the last item | 0.091 | 1.147 (12.56×) |  0.092 (1.00×) |      0.092 (1.00×) |
| vector of 100 nested tables: sum every item     | 1.677 |  1.939 (1.16×) |  1.966 (1.17×) |      1.990 (1.19×) |
| array of 100 structs: read the last item        | 0.103 | 1.268 (12.36×) |  0.115 (1.12×) |      0.115 (1.12×) |
| array of 100 structs: sum every item            | 0.782 |  1.611 (2.06×) |  1.334 (1.71×) |      1.314 (1.68×) |

Bytes allocated on V8's heap per call:

| Benchmark                                       | tafla | tafla-gen | tafla-gen view | tafla-gen view A/A |
| ----------------------------------------------- | ----: | --------: | -------------: | -----------------: |
| vector of 100 nested tables: read the last item |   208 |     5,322 |            328 |                328 |
| vector of 100 nested tables: sum every item     |   440 |     5,322 |          4,289 |              4,289 |
| array of 100 structs: read the last item        |   328 |    10,807 |            496 |                496 |
| array of 100 structs: sum every item            |   328 |    10,809 |         10,001 |             10,001 |

- The views read the last item without reading the others, as `ByteBuffer`
  does by hand: 0.092 µs against 0.091 µs for the vector, and 0.115 µs
  against 0.103 µs for the array, 12 times faster than the getters, which
  read all 100 items first.
- Summing every item of the vector through its view takes as long as through
  the getter, 1.97 µs against 1.94 µs, about the A/A pair's spread of 1.2%:
  both create an `Item` and a `Pair` for each item, which V8 doesn't remove,
  and the view saves only the array of them, 1 KB of the 5.3 KB. The reads by
  hand take 1.68 µs.
- Summing the array of structs through its view takes 1.33 µs against
  1.61 µs, 17% less, though it creates a `Triple` for each struct as the
  getter does. `readFloat32()` by hand takes 0.78 µs.
