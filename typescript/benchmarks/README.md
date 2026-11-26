# Benchmarks

`yarn bench` times `tafla`'s getters and setters of every element type
on their own, `yarn bench:compare` compares the package with other
JavaScript libraries, `yarn bench:strings` compares how they write and read
strings, and `yarn bench:lists` and `yarn bench:list-layouts` how they write
and read lists of strings. Run them from `typescript/`.

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

## Lists of strings

`yarn bench:lists` compares how tafla and the other libraries write and read
lists of 10, 100 and 10,000 distinct strings: 8, 16 and 256 characters of
ASCII, 16 with two of 2 bytes, 16 of ASCII with one character of 2 bytes in
every tenth string, and 8 and 256 of 3 bytes. tafla writes each list in two
ways, each written with a `TableBuilder` by hand and by the code that
tafla-gen generates from `strings/lists/lists.tafla` into
`strings/lists/generated/`:

- **`[string]`**, the proposed vector of strings, whose items are the
  strings' UTF-8. `TableBuilder` and tafla-gen's code write the same bytes,
  which the suite checks. tafla reads every string with
  `getStringVector()`, tafla-gen with its getter's `StringVector` and
  `toArray()`, which calls it, and **tafla [string] each** reads them a
  string at a time with `StringVector.get(i)`, which shows what decoding a
  run of ASCII at once saves.
- **`[Name]`**, how a schema holds a list of strings without one: a vector
  of tables of one string each. tafla records each table in place with
  `setVectorInPlace()` and reads each string with `getString()`, and
  tafla-gen's code creates the generated fields of each table in each call
  and reads each through a reader of its own.

FlatBuffers writes a vector of strings, protobufjs a repeated string, and
msgpackr an array, as in the suite of strings. Each list is serialized,
deserialized into an array of its strings, and, where the library can find
a string without reading the others, read for its last string. Before
timing, every codec must read back every string.

`yarn bench:list-layouts` times the layouts that a list of strings could
have in a table, each written and read by hand in `strings/lists/layouts.ts`
with the same code for the UTF-8, so that their times differ by the layout
alone: the proposed **ends**, a count, the end of each string and the
strings; the ends as a **`Uint32Array`**, aligned and read through a typed
array; a **length before each string**, as a string field holds it; and a
**table of one string each**, today's vector of tables. The ends are read
all at once three ways: a string at a time, by decoding the strings as **one
run** and slicing it when it's ASCII, and by doing so only **if the first
string is ASCII**, as `getStringVector()` does.

```sh
uptime
yarn bench:lists --no-heap --rounds 41
yarn bench:list-layouts --no-heap --rounds 41
```

### Results

One run of each on 2026-11-26, on an Apple M2 Pro with 32 GB in macOS
27.0.1, with Node 26.13.0, of the commands above. Other processes kept the
1-minute load average at 3.5 to 5 on the 10 cores. Each table gives a
group's fastest round when every call took under 1 µs, and its median round
otherwise, as benk reports, with each time's ratio to the first column. The
A/A pairs were within 2% of their variants, but for the 10,000 strings of
256 characters of ASCII, whose rounds fell into two modes.

Encoded size, in bytes: a vector of strings takes 4 bytes for each string
and 8 for the table's offset and the vector's count beside the strings'
UTF-8, where a vector of tables of one string each takes 12 for each
string. 100 strings of 8 characters of ASCII take 1,208 bytes as a vector
of strings, 2,008 as a vector of tables, 2,024 in FlatBuffers, 1,000 in
protobuf and 903 in MessagePack, and 100 of 256 characters of 3 bytes
77,208, 78,008, 78,024, 77,100 and 77,103.

Serialize:

| Strings                                              | tafla-gen [string] |  tafla [string] | tafla-gen [Name] |    tafla [Name] |     flatbuffers |      protobufjs |        msgpackr |
| ---------------------------------------------------- | -----------------: | --------------: | ---------------: | --------------: | --------------: | --------------: | --------------: |
| 10 × 8 chars of ASCII                                |             451 ns |  461 ns (1.02×) |   613 ns (1.36×) |  769 ns (1.71×) | 2.26 µs (5.01×) |  369 ns (0.82×) |  320 ns (0.71×) |
| 10 × 16 chars of ASCII                               |             652 ns |  660 ns (1.01×) |   814 ns (1.25×) |  955 ns (1.46×) | 2.37 µs (3.63×) |  627 ns (0.96×) |  480 ns (0.74×) |
| 10 × 256 chars of ASCII                              |            1.38 µs | 1.31 µs (0.95×) |  1.56 µs (1.13×) | 1.66 µs (1.20×) | 3.74 µs (2.71×) | 1.48 µs (1.07×) |  492 ns (0.36×) |
| 10 × 16 chars, two of 2 bytes                        |             878 ns |  898 ns (1.02×) |  1.05 µs (1.20×) |  1.2 µs (1.37×) |  2.3 µs (2.62×) |  658 ns (0.75×) |  481 ns (0.55×) |
| 10 × 16 chars, every tenth string one of 2 bytes     |             664 ns |  682 ns (1.03×) |   827 ns (1.25×) |  969 ns (1.46×) |  2.3 µs (3.46×) |  647 ns (0.97×) |  468 ns (0.70×) |
| 10 × 8 chars of 3 bytes                              |             665 ns |  682 ns (1.03×) |   838 ns (1.26×) |    1 µs (1.50×) | 2.49 µs (3.74×) |  642 ns (0.97×) |  396 ns (0.60×) |
| 10 × 256 chars of 3 bytes                            |            4.09 µs |    4 µs (0.98×) |   4.2 µs (1.03×) | 4.37 µs (1.07×) | 6.19 µs (1.51×) | 3.59 µs (0.88×) | 1.74 µs (0.43×) |
| 100 × 8 chars of ASCII                               |            3.13 µs | 3.11 µs (0.99×) |   4.6 µs (1.47×) | 5.54 µs (1.77×) |   21 µs (6.71×) | 3.52 µs (1.12×) | 2.54 µs (0.81×) |
| 100 × 16 chars of ASCII                              |            5.01 µs | 5.03 µs (1.00×) |  6.51 µs (1.30×) | 7.41 µs (1.48×) | 20.5 µs (4.09×) | 5.22 µs (1.04×) | 3.98 µs (0.79×) |
| 100 × 256 chars of ASCII                             |            11.5 µs | 10.9 µs (0.95×) |  13.2 µs (1.15×) |   14 µs (1.22×) | 28.3 µs (2.46×) |  9.2 µs (0.80×) | 4.17 µs (0.36×) |
| 100 × 16 chars, two of 2 bytes                       |            7.57 µs | 7.73 µs (1.02×) |  9.08 µs (1.20×) | 10.3 µs (1.36×) |   22 µs (2.91×) | 5.42 µs (0.72×) | 4.22 µs (0.56×) |
| 100 × 16 chars, every tenth string one of 2 bytes    |            5.37 µs |  5.4 µs (1.01×) |  6.92 µs (1.29×) | 7.82 µs (1.46×) | 21.3 µs (3.97×) | 5.31 µs (0.99×) |    4 µs (0.74×) |
| 100 × 8 chars of 3 bytes                             |            5.42 µs | 5.37 µs (0.99×) |  7.05 µs (1.30×) | 8.11 µs (1.50×) | 22.7 µs (4.19×) | 4.61 µs (0.85×) | 3.34 µs (0.62×) |
| 100 × 256 chars of 3 bytes                           |              40 µs | 39.7 µs (0.99×) |  42.9 µs (1.07×) | 43.7 µs (1.09×) | 54.4 µs (1.36×) | 31.3 µs (0.78×) | 16.8 µs (0.42×) |
| 10,000 × 8 chars of ASCII                            |             293 µs |  288 µs (0.98×) |   418 µs (1.43×) |  529 µs (1.81×) | 1.96 ms (6.69×) |  312 µs (1.06×) |  244 µs (0.83×) |
| 10,000 × 16 chars of ASCII                           |             476 µs |  480 µs (1.01×) |   604 µs (1.27×) |  713 µs (1.50×) | 1.98 ms (4.16×) |  483 µs (1.01×) |  384 µs (0.81×) |
| 10,000 × 256 chars of ASCII                          |             1.1 ms | 1.11 ms (1.01×) |  1.29 ms (1.17×) | 1.42 ms (1.29×) | 2.54 ms (2.31×) |  789 µs (0.72×) |  424 µs (0.39×) |
| 10,000 × 16 chars, two of 2 bytes                    |             709 µs |  707 µs (1.00×) |   841 µs (1.19×) |  947 µs (1.34×) | 1.94 ms (2.74×) |  488 µs (0.69×) |  401 µs (0.57×) |
| 10,000 × 16 chars, every tenth string one of 2 bytes |             511 µs |  506 µs (0.99×) |   643 µs (1.26×) |  751 µs (1.47×) | 1.98 ms (3.87×) |  482 µs (0.94×) |  396 µs (0.77×) |
| 10,000 × 8 chars of 3 bytes                          |             505 µs |  507 µs (1.00×) |   644 µs (1.28×) |  759 µs (1.50×) | 2.16 ms (4.28×) |  382 µs (0.76×) |  330 µs (0.65×) |
| 10,000 × 256 chars of 3 bytes                        |            3.81 ms | 3.78 ms (0.99×) |  3.99 ms (1.05×) | 4.38 ms (1.15×) |  4.6 ms (1.21×) |  2.8 ms (0.73×) | 1.72 ms (0.45×) |

Deserialize:

| Strings                                              | tafla-gen [string] | tafla [string] each | tafla-gen [Name] |    tafla [Name] |      flatbuffers |      protobufjs |        msgpackr |
| ---------------------------------------------------- | -----------------: | ------------------: | ---------------: | --------------: | ---------------: | --------------: | --------------: |
| 10 × 8 chars of ASCII                                |             246 ns |      326 ns (1.33×) |   389 ns (1.58×) |  351 ns (1.43×) |   929 ns (3.78×) |  240 ns (0.98×) |  225 ns (0.91×) |
| 10 × 16 chars of ASCII                               |             231 ns |      489 ns (2.12×) |   544 ns (2.35×) |  512 ns (2.22×) |   933 ns (4.04×) |  374 ns (1.62×) |  657 ns (2.84×) |
| 10 × 256 chars of ASCII                              |             381 ns |      967 ns (2.54×) |  1.07 µs (2.81×) |  992 ns (2.60×) |  1.05 µs (2.76×) | 1.04 µs (2.73×) |  979 ns (2.57×) |
| 10 × 16 chars, two of 2 bytes                        |             734 ns |      764 ns (1.04×) |   805 ns (1.10×) |  793 ns (1.08×) |  1.27 µs (1.73×) | 1.49 µs (2.03×) |  703 ns (0.96×) |
| 10 × 16 chars, every tenth string one of 2 bytes     |             697 ns |      528 ns (0.76×) |   574 ns (0.82×) |  547 ns (0.78×) |   962 ns (1.38×) |  481 ns (0.69×) |  657 ns (0.94×) |
| 10 × 8 chars of 3 bytes                              |             636 ns |      690 ns (1.08×) |   716 ns (1.13×) |  706 ns (1.11×) |  1.82 µs (2.86×) | 1.97 µs (3.10×) |  626 ns (0.98×) |
| 10 × 256 chars of 3 bytes                            |            3.89 µs |     3.97 µs (1.02×) |  4.02 µs (1.03×) | 3.98 µs (1.02×) |  4.51 µs (1.16×) | 4.06 µs (1.04×) |  4.4 µs (1.13×) |
| 100 × 8 chars of ASCII                               |            1.07 µs |     2.89 µs (2.70×) |  3.23 µs (3.02×) | 2.95 µs (2.76×) |  9.09 µs (8.50×) | 2.41 µs (2.25×) | 2.18 µs (2.04×) |
| 100 × 16 chars of ASCII                              |             864 ns |     4.57 µs (5.29×) |  4.93 µs (5.71×) | 4.65 µs (5.38×) | 8.94 µs (10.35×) | 3.77 µs (4.36×) | 6.52 µs (7.55×) |
| 100 × 256 chars of ASCII                             |            1.93 µs |     9.19 µs (4.76×) |  9.55 µs (4.95×) |  9.3 µs (4.82×) |    10 µs (5.18×) | 10.1 µs (5.23×) | 9.63 µs (4.99×) |
| 100 × 16 chars, two of 2 bytes                       |            6.76 µs |     7.18 µs (1.06×) |  7.52 µs (1.11×) | 7.28 µs (1.08×) |  12.2 µs (1.80×) | 14.7 µs (2.17×) | 6.74 µs (1.00×) |
| 100 × 16 chars, every tenth string one of 2 bytes    |            5.58 µs |     4.93 µs (0.88×) |  5.36 µs (0.96×) | 5.01 µs (0.90×) |  9.31 µs (1.67×) | 4.94 µs (0.89×) | 6.59 µs (1.18×) |
| 100 × 8 chars of 3 bytes                             |            5.87 µs |     6.31 µs (1.07×) |  6.71 µs (1.14×) | 6.38 µs (1.09×) |  18.6 µs (3.17×) | 19.7 µs (3.36×) | 6.25 µs (1.06×) |
| 100 × 256 chars of 3 bytes                           |            38.6 µs |       39 µs (1.01×) |  39.5 µs (1.02×) | 39.1 µs (1.01×) |  45.2 µs (1.17×) | 40.7 µs (1.05×) | 44.2 µs (1.15×) |
| 10,000 × 8 chars of ASCII                            |            92.8 µs |      279 µs (3.01×) |   308 µs (3.32×) |  290 µs (3.12×) |   890 µs (9.59×) |  237 µs (2.55×) |  213 µs (2.30×) |
| 10,000 × 16 chars of ASCII                           |            71.4 µs |      447 µs (6.26×) |   478 µs (6.69×) |  460 µs (6.44×) |  897 µs (12.56×) |  372 µs (5.21×) |  648 µs (9.08×) |
| 10,000 × 256 chars of ASCII                          |             158 µs |      940 µs (5.95×) |   952 µs (6.03×) |  939 µs (5.94×) |   990 µs (6.27×) |    1 ms (6.33×) |  965 µs (6.11×) |
| 10,000 × 16 chars, two of 2 bytes                    |             667 µs |      702 µs (1.05×) |   729 µs (1.09×) |  708 µs (1.06×) |  1.22 ms (1.83×) | 1.44 ms (2.16×) |  666 µs (1.00×) |
| 10,000 × 16 chars, every tenth string one of 2 bytes |             541 µs |      487 µs (0.90×) |   530 µs (0.98×) |  499 µs (0.92×) |   938 µs (1.73×) |  501 µs (0.93×) |  678 µs (1.25×) |
| 10,000 × 8 chars of 3 bytes                          |             555 µs |      600 µs (1.08×) |   634 µs (1.14×) |  605 µs (1.09×) |  1.71 ms (3.08×) |  1.9 ms (3.42×) |  590 µs (1.06×) |
| 10,000 × 256 chars of 3 bytes                        |            3.95 ms |     4.04 ms (1.02×) |  4.06 ms (1.03×) | 4.04 ms (1.02×) |  4.56 ms (1.15×) | 4.12 ms (1.04×) | 4.55 ms (1.15×) |

Read the last string, without reading the others:

| Strings                                              | tafla-gen [string] | tafla-gen [Name] |    tafla [Name] |    flatbuffers |
| ---------------------------------------------------- | -----------------: | ---------------: | --------------: | -------------: |
| 10 × 8 chars of ASCII                                |            80.0 ns |  95.3 ns (1.19×) | 83.6 ns (1.04×) | 148 ns (1.85×) |
| 10 × 16 chars of ASCII                               |            96.2 ns |   113 ns (1.17×) |  102 ns (1.06×) | 146 ns (1.52×) |
| 10 × 256 chars of ASCII                              |             144 ns |   152 ns (1.06×) |  145 ns (1.01×) | 149 ns (1.03×) |
| 10 × 16 chars, two of 2 bytes                        |             121 ns |   129 ns (1.07×) |  128 ns (1.06×) | 174 ns (1.44×) |
| 10 × 16 chars, every tenth string one of 2 bytes     |             123 ns |   129 ns (1.05×) |  129 ns (1.05×) | 165 ns (1.34×) |
| 10 × 8 chars of 3 bytes                              |             111 ns |   119 ns (1.07×) |  116 ns (1.05×) | 223 ns (2.01×) |
| 10 × 256 chars of 3 bytes                            |             447 ns |   443 ns (0.99×) |  439 ns (0.98×) | 479 ns (1.07×) |
| 100 × 8 chars of ASCII                               |            81.6 ns |  87.8 ns (1.08×) | 87.1 ns (1.07×) | 144 ns (1.76×) |
| 100 × 16 chars of ASCII                              |            97.0 ns |   103 ns (1.06×) |  102 ns (1.05×) | 141 ns (1.45×) |
| 100 × 256 chars of ASCII                             |             148 ns |   157 ns (1.06×) |  146 ns (0.99×) | 160 ns (1.08×) |
| 100 × 16 chars, two of 2 bytes                       |             122 ns |   126 ns (1.03×) |  126 ns (1.03×) | 178 ns (1.46×) |
| 100 × 16 chars, every tenth string one of 2 bytes    |             124 ns |   129 ns (1.04×) |  127 ns (1.02×) | 172 ns (1.39×) |
| 100 × 8 chars of 3 bytes                             |             120 ns |   119 ns (0.99×) |  121 ns (1.01×) | 228 ns (1.90×) |
| 100 × 256 chars of 3 bytes                           |             449 ns |   442 ns (0.98×) |  454 ns (1.01×) | 488 ns (1.09×) |
| 10,000 × 8 chars of ASCII                            |            84.4 ns |  86.8 ns (1.03×) | 89.2 ns (1.06×) | 148 ns (1.75×) |
| 10,000 × 16 chars of ASCII                           |             104 ns |   102 ns (0.98×) |  108 ns (1.04×) | 148 ns (1.42×) |
| 10,000 × 256 chars of ASCII                          |             150 ns |   153 ns (1.02×) |  148 ns (0.99×) | 154 ns (1.03×) |
| 10,000 × 16 chars, two of 2 bytes                    |             127 ns |   127 ns (1.00×) |  131 ns (1.03×) | 178 ns (1.40×) |
| 10,000 × 16 chars, every tenth string one of 2 bytes |             130 ns |   130 ns (1.00×) |  132 ns (1.02×) | 172 ns (1.32×) |
| 10,000 × 8 chars of 3 bytes                          |             116 ns |   120 ns (1.03×) |  120 ns (1.03×) | 228 ns (1.97×) |
| 10,000 × 256 chars of 3 bytes                        |             449 ns |   448 ns (1.00×) |  459 ns (1.02×) | 492 ns (1.10×) |

- A vector of strings is written in 0.68–0.97 times the time of tafla-gen's
  vector of tables of one string each, which creates and writes a table for
  each string, and in 0.55–0.93 times the time of `TableBuilder`'s.
  protobufjs writes the lists in 0.69–1.12 times the time of the vector of
  strings, and msgpackr, which reuses its buffer, in 0.36–0.83 times.
- Lists of ASCII are read in 0.15–0.63 times the time of the vector of
  tables, since their strings lie in one run, which is decoded once. Read a
  string at a time, a vector of strings takes 0.84–0.99 times its time,
  without a reader for each table. Lists outside ASCII are read in 0.87–0.98
  times its time.
- The lists of ASCII with a character of 2 bytes in every tenth string are
  read in 1.02–1.21 times the time of the vector of tables, and 1.11–1.32
  times that of reading them a string at a time: their first string is
  ASCII, so their run is decoded, and then read a string at a time.
- A string is found from its ends in either vector, in 80–450 ns, most of
  which is the reader's construction and the string's decoding: the vector
  of tables takes 0.98–1.19 times as long, and FlatBuffers 1.03–2.01 times.

Layouts, read every string, with the ends' time and each layout's ratio to
it:

| Strings                           |    ends | ends, run decoded once | ends, run if first is ASCII | ends as a Uint32Array | length before each string | table of one string each |
| --------------------------------- | ------: | ---------------------: | --------------------------: | --------------------: | ------------------------: | -----------------------: |
| 10 × 8 chars of ASCII             |  285 ns |                  0.65× |                       0.90× |                 1.07× |                     1.01× |                    1.04× |
| 10 × 16 chars of ASCII            |  418 ns |                  0.40× |                       0.59× |                 1.06× |                     1.00× |                    1.04× |
| 10 × 256 chars of ASCII           |  897 ns |                  0.29× |                       0.43× |                 1.03× |                     1.00× |                    1.01× |
| 10 × 16 chars, two of 2 bytes     | 1.42 µs |                  1.28× |                       1.14× |                 1.01× |                     1.02× |                    1.01× |
| 10 × 8 chars of 3 bytes           | 1.87 µs |                  1.14× |                       1.12× |                 1.01× |                     1.01× |                    1.00× |
| 10 × 256 chars of 3 bytes         | 3.94 µs |                  1.74× |                       1.10× |                 1.00× |                     1.00× |                    1.00× |
| 100 × 8 chars of ASCII            | 2.47 µs |                  0.33× |                       0.36× |                 1.01× |                     0.99× |                    1.05× |
| 100 × 16 chars of ASCII           | 3.83 µs |                  0.16× |                       0.18× |                 1.00× |                     0.99× |                    1.03× |
| 100 × 256 chars of ASCII          | 8.83 µs |                  0.19× |                       0.19× |                 1.01× |                     0.99× |                    1.01× |
| 100 × 16 chars, two of 2 bytes    | 13.4 µs |                  1.18× |                       1.01× |                 1.01× |                     1.01× |                    1.01× |
| 100 × 8 chars of 3 bytes          | 17.9 µs |                  1.06× |                       1.02× |                 1.00× |                     1.02× |                    1.01× |
| 100 × 256 chars of 3 bytes        | 37.9 µs |                  1.72× |                       1.01× |                 1.00× |                     1.00× |                    1.00× |
| 10,000 × 8 chars of ASCII         |  238 µs |                  0.29× |                       0.29× |                 1.00× |                     1.01× |                    1.07× |
| 10,000 × 16 chars of ASCII        |  380 µs |                  0.14× |                       0.13× |                 1.01× |                     1.01× |                    1.05× |
| 10,000 × 256 chars of ASCII       |  888 µs |                  0.15× |                       0.15× |                 0.98× |                     0.99× |                    1.01× |
| 10,000 × 16 chars, two of 2 bytes | 1.34 ms |                  1.16× |                       1.01× |                 1.01× |                     1.01× |                    1.01× |
| 10,000 × 8 chars of 3 bytes       | 1.75 ms |                  1.06× |                       1.00× |                 1.01× |                     1.02× |                    1.02× |
| 10,000 × 256 chars of 3 bytes     | 3.89 ms |                  1.68× |                       0.99× |                 1.00× |                     0.99× |                    0.99× |

Layouts, read the last string:

| Strings                           |    ends | ends as a Uint32Array | length before each string | table of one string each |
| --------------------------------- | ------: | --------------------: | ------------------------: | -----------------------: |
| 10 × 8 chars of ASCII             | 66.6 ns |                 1.32× |                     1.13× |                    1.01× |
| 10 × 16 chars of ASCII            | 81.1 ns |                 1.27× |                     1.12× |                    1.00× |
| 10 × 256 chars of ASCII           |  128 ns |                 1.16× |                     1.05× |                    1.01× |
| 10 × 16 chars, two of 2 bytes     |  169 ns |                 1.12× |                     1.05× |                    1.01× |
| 10 × 8 chars of 3 bytes           |  216 ns |                 1.10× |                     1.03× |                    1.00× |
| 10 × 256 chars of 3 bytes         |  413 ns |                 1.05× |                     1.01× |                    1.00× |
| 100 × 8 chars of ASCII            | 66.2 ns |                 1.33× |                     3.64× |                    1.02× |
| 100 × 16 chars of ASCII           | 80.0 ns |                 1.28× |                     3.21× |                    1.01× |
| 100 × 256 chars of ASCII          |  128 ns |                 1.17× |                     2.31× |                    1.01× |
| 100 × 16 chars, two of 2 bytes    |  170 ns |                 1.13× |                     2.01× |                    1.00× |
| 100 × 8 chars of 3 bytes          |  215 ns |                 1.10× |                     1.81× |                    1.02× |
| 100 × 256 chars of 3 bytes        |  413 ns |                 1.05× |                     1.40× |                    1.00× |
| 10,000 × 8 chars of ASCII         | 70.1 ns |                 1.33× |                   156.92× |                    1.02× |
| 10,000 × 16 chars of ASCII        | 82.6 ns |                 1.27× |                   141.65× |                    1.02× |
| 10,000 × 256 chars of ASCII       |  134 ns |                 1.18× |                   294.78× |                    1.01× |
| 10,000 × 16 chars, two of 2 bytes |  178 ns |                 1.11× |                    76.40× |                    0.99× |
| 10,000 × 8 chars of 3 bytes       |  223 ns |                 1.10× |                    82.06× |                    1.00× |
| 10,000 × 256 chars of 3 bytes     |  424 ns |                 1.06× |                   177.59× |                    1.01× |

- Every layout is written within 3% of the ends' time, but for the ends as
  a `Uint32Array`, at 0.91–1.00 times, which writes them through the typed
  array.
- A length before each string reads every string as fast as the ends, but
  finds the last only by reading the lengths before it: 1.03–1.13 times as
  long for 10 strings, and 76–295 times for 10,000.
- The ends as a `Uint32Array` take 1.05–1.33 times as long to find a string,
  which is the typed array's construction, and pad the ends to 4 bytes.
- Decoding the strings as one run reads lists of ASCII in 0.14–0.65 times
  the time, and in 0.13–0.90 times after reading the first string on its
  own, and only the ends lay the strings out as one run. Decoding a
  run outside ASCII takes 1.06–1.74 times as long, since it's decoded again
  a string at a time, which decoding only after a first string of ASCII
  avoids: 1.00–1.02 times from 100 strings, and 1.10–1.14 for 10.

### Limitations

- The layouts are written by hand, rather than with the runtime, so that
  they share their code for the UTF-8: their times compare the layouts, not
  the runtime's code. Their table of one string each reads its strings
  without a reader of each table, which tafla-gen's code creates.
- A string of more than 12 characters sliced from a decoded run keeps the
  run alive while it's held, as V8 slices strings, so a caller that keeps a
  few strings of a large list keeps all of its characters. The heap bytes
  aren't counted here.
- msgpackr packs into a buffer that its `Packr` reuses, as in the suite of
  strings.
