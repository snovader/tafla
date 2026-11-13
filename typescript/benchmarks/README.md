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
  took 0.51–0.52 µs and 13.7 KB of the vector's 4.0 µs when run alone.
  It passes the records and the array of structs as they are. It reads its
  outputs from the generated readers, objects of their own that the builder
  codec doesn't create.

### Results

One run on 2026-11-13, of commit c43286c, on an Apple M2 Pro with 32 GB in
macOS 27.0.1, with Node 27.0.0 (an alpha, with V8 15.6), of
`yarn bench:compare --rounds 61 --sample-ms 5`. It took 38 seconds. The
1-minute load average was 2.8–3.1 on the 10 cores, and other processes used
at most 1 core in samples every 2 seconds. A second run right after it gave
fastest times within 7% of these, apart from four of the five reads of the
fixed-size record, 8–27% slower, and tafla-gen's sum of the array of
structs, 9% slower, and the median difference was 1.0%. A run with
Node 26.13.0, with V8 14.6, differed from this one by 2.9% at the median,
and tafla wrote the large document 15% slower there, in 36.3 µs.

Fastest call in µs, and its ratio to tafla's:

| Benchmark                                          | tafla |     tafla-gen |     flatbuffers |        protobufjs |       msgpackr |
| -------------------------------------------------- | ----: | ------------: | --------------: | ----------------: | -------------: |
| record: serialize                                  | 0.177 | 0.118 (0.66×) |   0.502 (2.83×) |     0.216 (1.22×) |  0.354 (2.00×) |
| record: deserialize                                | 0.178 | 0.186 (1.04×) |   0.214 (1.20×) |     0.264 (1.48×) |  0.293 (1.64×) |
| document of 100 rows: serialize                    | 0.613 | 0.492 (0.80×) |   2.723 (4.44×) |     2.619 (4.27×) |  1.164 (1.90×) |
| document of 100 rows: deserialize                  | 0.295 | 0.298 (1.01×) |   0.327 (1.11×) |     2.015 (6.82×) |  1.446 (4.90×) |
| document of 100 rows: sum a column per element     | 0.163 | 0.186 (1.14×) |   0.675 (4.15×) |     1.144 (7.02×) |  1.516 (9.31×) |
| document of 100,000 rows: serialize                |  31.6 |  31.8 (1.01×) | 1772.2 (56.12×) |   2016.4 (63.85×) |   21.8 (0.69×) |
| document of 100,000 rows: deserialize              | 0.294 | 0.295 (1.00×) |   0.328 (1.12×) | 1029.0 (3497.95×) | 35.4 (120.30×) |
| document of 100,000 rows: sum a column per element |  86.4 |  86.5 (1.00×) |   598.7 (6.93×) |    948.1 (10.97×) |  132.3 (1.53×) |
| vector of 100 nested tables: serialize             |  14.1 | 5.752 (0.41×) |    13.1 (0.93×) |     7.359 (0.52×) |   11.3 (0.80×) |
| vector of 100 nested tables: deserialize           | 1.367 | 2.590 (1.89×) |   4.823 (3.53×) |     6.413 (4.69×) |  3.804 (2.78×) |
| array of 100 structs: serialize                    | 0.820 | 0.834 (1.02×) |   1.450 (1.77×) |     5.385 (6.57×) |  4.768 (5.81×) |
| array of 100 structs: deserialize                  | 1.000 | 1.877 (1.88×) |   1.554 (1.55×) |     4.807 (4.81×) |  3.234 (3.24×) |
| fixed-size record: serialize                       | 0.044 | 0.034 (0.77×) |   0.334 (7.67×) |     0.126 (2.88×) |  0.273 (6.26×) |
| fixed-size record: deserialize                     | 0.010 | 0.011 (1.18×) |   0.072 (7.51×) |     0.040 (4.12×) | 0.177 (18.42×) |
| document of 100 rows: serialize number[]           | 0.786 | 0.664 (0.85×) |   2.606 (3.32×) |     2.550 (3.24×) |  1.913 (2.43×) |
| document of 100,000 rows: serialize number[]       | 148.4 | 147.8 (1.00×) | 1642.6 (11.07×) |   1978.0 (13.33×) |  181.9 (1.23×) |

Bytes allocated on V8's heap per call, which leave out the contents of
`ArrayBuffer`s of more than 64 bytes, such as most messages' bytes:

| Benchmark                                          |  tafla | tafla-gen | flatbuffers | protobufjs | msgpackr |
| -------------------------------------------------- | -----: | --------: | ----------: | ---------: | -------: |
| record: serialize                                  |    704 |       472 |       1,448 |        577 |      456 |
| record: deserialize                                |    464 |       496 |         568 |        896 |      776 |
| document of 100 rows: serialize                    |  1,264 |       808 |       1,840 |      4,936 |    1,256 |
| document of 100 rows: deserialize                  |    848 |       920 |       1,024 |      5,930 |    2,633 |
| document of 100 rows: sum a column per element     |    320 |       392 |         255 |      4,709 |    2,649 |
| document of 100,000 rows: serialize                |  1,264 |       808 |       5,760 |  3,202,172 |    1,499 |
| document of 100,000 rows: deserialize              |    848 |       920 |       1,024 |  4,371,933 |    2,632 |
| document of 100,000 rows: sum a column per element |    319 |       391 |         256 |  4,372,742 |    2,647 |
| vector of 100 nested tables: serialize             | 52,605 |    17,281 |       2,769 |     15,270 |    5,256 |
| vector of 100 nested tables: deserialize           | 11,522 |    22,000 |      18,712 |     25,824 |   17,033 |
| array of 100 structs: serialize                    |  1,000 |       776 |       1,432 |      1,585 |   13,029 |
| array of 100 structs: deserialize                  | 10,787 |    21,269 |      14,685 |     22,624 |   23,442 |
| fixed-size record: serialize                       |    264 |       264 |       1,040 |        417 |      504 |
| fixed-size record: deserialize                     |     96 |       192 |         296 |        272 |      680 |
| document of 100 rows: serialize number[]           |  1,264 |       808 |       1,840 |      4,935 |    1,952 |
| document of 100,000 rows: serialize number[]       |  1,255 |       799 |       5,675 |  3,202,044 |    2,186 |

- tafla-gen writes a table 1.2 to 2.5 times as fast as the hand-built
  builders: the record in 0.12 µs against 0.18 µs, the small document in
  0.49 µs against 0.61 µs, and the vector of 100 nested tables in 5.8 µs
  against 14.1 µs, allocating 17 KB on the heap against 53 KB. That includes
  creating the generated fields of each of the 100 items from the
  workload's: in separate runs of each step alone, in 6 processes, in which
  the whole call took 4.0 µs rather than the suite's 5.5–6.0 µs, creating
  them took 0.51–0.52 µs and 13.7 KB, and `ItemList.serialize()` of fields
  created beforehand 3.3–3.4 µs and 0.3–0.8 KB. `ItemList.serialize()`
  alone settles at one of two speeds in each process too: in 4 more
  processes, it took 3.5 µs and 0.4–0.8 KB in 3, and 5.0 µs and 3.6 KB in
  the other.
- tafla-gen's vector serialize settles at one of two speeds in each process:
  in 8 runs of its group alone, it took 5.5–5.6 µs and 17.3 KB in 5, and
  5.9–6.0 µs and 14.1–14.3 KB in the other 3. Compare its times between runs
  by their heap bytes, which tell the two apart.
- The large document, which is mostly its columns' 1.2 MB, takes 32 µs
  either way. From `number[]` columns, which both convert element by
  element, it takes 146–148 µs either way.
- The array of structs is written in about the same time, 0.83 µs against
  0.82 µs, by the same `StructWriter` sequence, into a table that the
  writer sizes rather than a builder.
- The generated readers read the record and the documents within 15% of the
  hand-written reads, which call the same `ByteBuffer` getters. They're
  slower where they create objects of their own: the vector's reader creates
  an `Item` and a `Pair` for each item, and the array's reader a `Triple` for
  each struct, from which the codec creates the workload's items. Reading the
  vector takes 2.6 µs against 1.4 µs, and the array 1.9 µs against 1.0 µs,
  allocating about twice the heap bytes.

#### Reading items through views

Two runs of the four groups that read tafla's items, right after the run
above, with the same Node, of
`yarn bench:compare --filter item --rounds 61 --sample-ms 5`. The 1-minute
load average was 2.9–3.1, and other processes used at most 1.3 cores. The
tables give the second run, and the first's times were within 3.3% of it,
apart from the views' sums of the array of structs, where the view and its
A/A copy were 13% apart in the second run and 8% in the first, in the other
order. V8 resizes its young generation as a process allocates, which moved
the views' sum of the array of structs between 1.1 and 4.8 µs from one
process to another while the views were designed.

Fastest call in µs, and its ratio to `ByteBuffer`'s by hand:

| Benchmark                                       | tafla |      tafla-gen | tafla-gen view | tafla-gen view A/A |
| ----------------------------------------------- | ----: | -------------: | -------------: | -----------------: |
| vector of 100 nested tables: read the last item | 0.076 |  0.638 (8.44×) |  0.093 (1.23×) |      0.094 (1.24×) |
| vector of 100 nested tables: sum every item     | 1.191 |  1.503 (1.26×) |  1.867 (1.57×) |      1.872 (1.57×) |
| array of 100 structs: read the last item        | 0.101 | 1.442 (14.22×) |  0.120 (1.18×) |      0.120 (1.18×) |
| array of 100 structs: sum every item            | 0.805 |  1.746 (2.17×) |  1.423 (1.77×) |      1.606 (1.99×) |

Bytes allocated on V8's heap per call:

| Benchmark                                       | tafla | tafla-gen | tafla-gen view | tafla-gen view A/A |
| ----------------------------------------------- | ----: | --------: | -------------: | -----------------: |
| vector of 100 nested tables: read the last item |   200 |     5,899 |            320 |                320 |
| vector of 100 nested tables: sum every item     | 1,048 |     5,900 |          4,281 |              4,281 |
| array of 100 structs: read the last item        |   320 |    10,802 |            488 |                488 |
| array of 100 structs: sum every item            |   320 |    10,802 |          9,993 |              9,993 |

- The views read the last item without reading the others, as `ByteBuffer`
  does by hand: 0.093 µs against 0.076 µs for the vector, and 0.120 µs
  against 0.101 µs for the array, 7 and 12 times as fast as the getters,
  which read all 100 items first.
- Summing every item of the vector through its view takes 1.87–1.89 µs,
  against 1.50 µs through the getter. The getter finds every item with one
  call of `getVectorOffsets()`, and the view's `get(i)` finds each item with
  `getVectorItemOffset()`, which reads and checks the item's two ends from
  the count that the view read once. Both create an `Item` and a `Pair` for
  each item, and the view saves only the array of them, 1.6 KB of the
  5.9 KB. The reads by hand take 1.19 µs.
- Summing the array of structs through its view takes 1.42–1.61 µs against
  1.75 µs, 8–19% less, though it creates a `Triple` for each struct as the
  getter does. `readFloat32()` by hand takes 0.81 µs.
- V8 removes the views' objects for each item only in some builds of the
  code. The suite runs the TypeScript source as Node loads it, module by
  module, which keeps them. Two runs of the same groups from an esbuild 0.28
  bundle of the suite, made with `esbuild benchmarks/comparison/main.ts
  --bundle --platform=node --format=esm --target=es2022
  --conditions=tafla-source`, with the same Node and options, removed them:
  the views summed the vector in 1.41–1.43 µs and 416–419 bytes, and the
  array of structs in 0.69–0.70 µs and 458–460 bytes, against 1.04 µs and
  0.58 µs by hand.
