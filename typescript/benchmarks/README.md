# Benchmarks

`yarn bench` times `tafla`'s getters and setters of every element type
on their own, `yarn bench:compare` compares the package with other
JavaScript libraries, `yarn bench:strings` compares how they write and
read strings, and `yarn bench:presence` times optional fields and fields
that later versions append. Run them from `typescript/`.

## Getters and setters

`yarn bench` times `TableReader`'s getters and `TableBuilder`'s setters of
each element type, from int8 to float64, in five groups:

- **primitive getters** read every field of a table of 1000 fields of one
  type.
- **typed array getters** read every array of a table of 100 arrays of 1000
  elements, as views of its bytes.
- **primitive setters** write a table of 1000 fields of one type.
- **typed array setters** write a table of 100 arrays of 1000 elements from
  typed arrays of that type.
- **array setters** write the same table from `number[]`s or `bigint[]`s,
  which the builder converts element by element. They run last, since they
  change the type feedback of the code that the typed array setters share.

benk times them, as it times the comparison, which [Method](#method)
describes, and adds an A/A copy of each group's first variant, such as
getInt8 A/A, whose ratio to the first shows the run's noise.

Each variant's call is compiled from a source of its own, which calls its
getter or setter by name, as a caller of that one method would. V8 shares
type feedback between the closures of one function literal, and between
identical sources through its compilation cache, so a loop that every type
shares calls their getters from one megamorphic call site. The Vitest
benchmark that `fields/` replaced had such a loop, and discarded the values:
getInt8 took 2.7 µs per 1000 reads, getUint8 to getUint16 about 7 µs, and the
other getters 14 to 19 µs, where each now takes 3.2 to 3.5 µs and keeps them.

The getters write each value into a typed array of their type, and each
array into an array, which keeps every result without allocating, so that V8
can't discard the reads: getInt64's values stay in a `BigInt64Array`, rather
than as BigInts on the heap. Before timing, each variant's first call must
read or write the values it should, compared with `Object.is`.

```sh
uptime
yarn bench
```

It takes the comparison's options, which [Running](#running) lists.

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
  `setVectorInPlace()`, and reads the tables with `TableReader`'s getters.
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
of its own, and `comparison/item-access.ts` holds them:

- **tafla** reads with `TableReader`'s getters by hand: `getVectorOffset()` of
  the last item or `getVectorOffsets()` of every item, and `getStructArray()`
  with `readFloat32()`.
- **tafla-gen** reads with the generated getters, `items()`, which return
  views: the vector's `get(i)` reads one item, and the array of structs'
  getter of each field, such as `value(i)`, reads that field of one struct
  without creating the struct. A loop reads every item.

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

[benk](https://github.com/snovader/benk) times the variants. It isn't on npm
yet, so `package.json` installs it from GitHub at a fixed commit, which Yarn
builds as it installs it, and `.yarnrc.yml` approves its repository.

Each group runs its variants interleaved over at least 31 rounds of 1 ms each,
after 10 warm-up rounds, in an order that rotates, so that each variant takes
each position, and follows each other variant, equally often. Load from other
processes then slows every variant alike. benk adds an A/A copy of each
group's first variant, whose ratio to the first shows the run's noise. Every
variant has its own timer loop, compiled from a unique source, which also
counts the calls of a round, so that call sites and type feedback aren't
shared. Groups with `number[]` inputs run last, since they change the type
feedback of code the typed-array groups share. Before timing, every codec
must read back exactly what it wrote.

The scripts run the suite's TypeScript through `@snovader/node-hooks`, as
[the package's README](../README.md) describes, rather than through Node's
own type stripping. Node's stripping invalidates V8's `ArrayBufferDetaching`
protector, in Node 26.13.0 and 27.0.0-alpha alike
([nodejs/node#67632](https://github.com/nodejs/node/issues/67632)). V8's
optimized code then checks typed-array accesses for detached buffers for the
rest of the process, which took tafla's primitive getters about 1.6 times as
long. Run a `.ts` file of the suite by hand with the scripts' flags, and
`node --trace-protector-invalidation` prints nothing.

Each row gives a variant's fastest and median round, per call; their spread,
the interquartile range as a share of the median; the ratio to the group's
first variant; a 95% interval of that ratio from the rounds; and the bytes
that a call allocates on V8's heap. A group compares its variants' fastest
rounds when every median call takes under 1 µs and allocates under 1 KiB, and
their median rounds otherwise, since the fastest round of a longer or
allocating call leaves out garbage collection that its calls cause. A
variant whose rounds fall into two clusters gets a note below the table.
benk's README describes each column.

A call's heap bytes are how much the heap's used size grew over a sample of
calls, plus what the garbage collections during it freed, which a
`GCProfiler` from `node:v8` reports, less what a sample of nothing takes: the
median of 5 samples after the timed rounds. V8's own count of allocated
bytes, `total_allocated_bytes`, grows in steps of whole allocation buffers,
of 100 KiB or more, which is too coarse for a call.

Until 2026-11-19, the suite timed its variants with a harness of its own,
which ran them in alternating order, and counted the calls of a round from a
loop that every variant shared. With it, some short calls took longer in
every process: on 2026-11-19, with Node 27.0.0-alpha, tafla's read of the
last struct took 94 to 97 ns against 84 to 86 ns with benk, and tafla-gen's
record deserialize and protobufjs's fixed-size record serialize took 8 to 10%
longer. When the harness counted the calls with each variant's own timer
loop, as benk does, they took as long as with benk. The two agreed on the
other rows: benk's fastest round was within 2% of the harness's on most of
them, and the median ratio was 0.999.

### Running

```sh
uptime
yarn bench:compare --rounds 41
```

`--rounds` sets the least rounds (31 by default), `--warmup-rounds` the
warm-up rounds (10), `--sample-ms` how long one round of one variant takes
(1 ms), and `--filter <regex>` runs only the matching groups. `--json <path>`
writes every round's time and position as JSON, and `--json -` writes it to
stdout, with the tables, the encoded sizes included, on stderr. `--no-aa`
leaves out the A/A copies, and `--no-heap` the heap bytes. An unknown
option, or an option without a value, stops the run with an error. Check
`uptime` first: a high load from other processes makes even the fastest
round unreliable.

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
  shape wouldn't, and reads the document's and the vector's outputs from the
  generated readers, objects of their own that the builder codec doesn't
  create.

### Results

These results came from the suite's earlier harness, which
[Method](#method) compares with benk, in processes whose type stripping
had invalidated the protector.

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
  hand-written reads, which call the same `TableReader` getters. They're
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

Fastest call in µs, and its ratio to `TableReader`'s by hand:

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

- The views read the last item without reading the others, as `TableReader`
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

## Strings

`yarn bench:strings` compares how tafla and the comparison's libraries write
and read strings, which the comparison's workloads hold one of, of 16 bytes
of ASCII. Each table holds 16 distinct strings of one shape and nothing
else, so that a call spends most of its time on their UTF-8:

- 8 characters of ASCII, 8 with one of 2 bytes, and 8 of 3 bytes
- 16 characters with two of 2 bytes, and 15 with one of 4 bytes, a
  surrogate pair
- 16 and 32 characters of ASCII, 31 with the last of 2 bytes, and 32 of 3
  bytes
- 33 characters of ASCII, and 33 with the last of 2 bytes, either side of the
  longest strings that tafla reads without a `TextDecoder`
- 64, 256 and 1,024 characters of ASCII, and 256 of 3 bytes
- 16 empty strings, which time what a call takes without any: the table, its
  offsets and lengths, and the library's own work

The characters of 2 bytes are Latin-1 letters, which V8 stores in a byte, the
characters of 3 bytes CJK ideographs, and those of 4 bytes emoji. Each string
is flat, as a decoder returns it, rather than a concatenation.

tafla has no vector of strings, so tafla writes a table of 16 string fields
with a `TableBuilder` and reads it with `getString()`, and tafla-gen's code,
generated from `strings/strings.tafla` into `strings/generated/tafla/`,
counts it with a `TableSizer`, writes it with a `TableWriter`, and reads it
with its getters. Both write the same bytes, which the suite checks. The
other libraries write what they'd write for an array of strings: FlatBuffers
a vector of strings, with code written by hand in the shape that flatc
generates, protobufjs a repeated string, and msgpackr an array. Before
timing, every codec must read back every string.

benk times the groups as it times the comparison. In Node 27, the 16 empty
strings took half to two thirds of the time of tafla's tables of 8
characters of ASCII, and a twentieth to a fifth of the time of those of 64
characters and more.

```sh
uptime
yarn bench:strings --rounds 41
```

It takes the comparison's options.

### Comparing two builds

To compare two builds of the package, such as a branch and main, run each
in processes of its own, alternating, and compare their rows. The other
libraries' rows, whose code doesn't change, show the processes' noise.
Leave out the heap bytes with `--no-heap`: in Node 24, 26 and 27, benk's
samples of a group's heap bytes left V8's young generation smaller in some
processes, depending on the build, and the next group's allocating
variants, the other libraries' too, took up to 1.6 times as long. With the young generation's
size fixed by `--min-semi-space-size=16 --max-semi-space-size=16`, or
without the samples, they took as long as in the other processes.

### Limitations

- msgpackr packs into a buffer that its `Packr` reuses, and returns a view of
  it, which a caller that keeps the bytes copies. The other libraries
  allocate a buffer for each table. A copy took 25 ns for the 16 empty
  strings and 560 ns for 256 characters of 3 bytes, in Node 27.
- The suite uses msgpackr's plain JavaScript entry point, as the comparison
  does, without its native string extractor.
- tafla-gen creates its generated fields from the input array in each call,
  as the protobufjs codec creates its message.

## Optional fields and versions

`yarn bench:presence` times what optional fields and fields that a later
version appends cost the code that tafla-gen generates, from
`presence/presence.tafla` into `presence/generated/tafla/`. Each call reads
or writes 64 tables of its own values:

- **required fields**: `x`, `id` and `y` of tables that have only them and
  a string (`Plain`), that also have two optional fields, present or absent
  (`Optional`), and of messages that read one version (`Exact`) or two
  (`Ranged`), from tables of either version.
- **z of root tables**: a required `z`, an optional one, present and
  absent, and one that version 2 appended, read from tables of either
  version. Two more variants read the optional `z` as a getter that returns
  a default for an absent field would, once V8 inlines it: `NaN`, and `0`.
- **z of nested tables**: a required `z` of a nested table, and one that its
  second revision appended, read from tables of either revision.
- **construct**: the constructor of a message without a version, of one
  that checks its own version, and of one that checks a range of versions.
- **write**: the messages above, and `Full`, which has `Optional`'s fields,
  all required.

```sh
uptime
yarn bench:presence --rounds 41 --no-heap
```

On an Apple M2 Pro in Node 26.13.0, in two processes alike, at a load of
1.4:

- Reads of required fields took as long in every kind of table, 0.99 to
  1.02 times `Plain`'s, with an A/A of 1.00: their getters are the same
  code.
- A present optional `z`, and a `z` that version 2 appended, took 1.47 to
  1.49 times a required `z`, and allocated a heap number of 12 bytes each:
  the getter returns `number | undefined`, and V8 boxes a double that
  merges with another type, as it does with the `NaN` default, which
  JavaScript loads as a global (1.39 times). A default of `0` took 1.00 to
  1.01 times, without allocating, so the comparison with 0 is free. An
  absent `z` took 0.91 to 0.93 times, and one missing from version 1, found
  from the version alone, 0.58 times.
- A `z` that the nested table's second revision appended took 1.48 times,
  with the box and a read of the first field's offset, and one missing from
  the first revision 0.98 to 0.99 times.
- Constructing a message that checks a range of versions took 1.01 times
  the time of one that checks its own, which took 1.02 to 1.03 times the
  time of one without a version.
- Writing `Optional` with both optional fields present took as long as
  writing `Full`, whose fields are required, 2.52 to 2.60 times against 2.47
  to 2.58 times `Plain`, within each other's noise in two processes at a
  load of 2.75, and with both absent 1.09 times `Plain`, for their offsets
  and checks. `Ranged` wrote as fast as `Exact`.

