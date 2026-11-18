# @tafla/native

An experiment: reads of many values of a tafla table in one call, through
the Rust runtime, as a Node-API addon that [napi-rs](https://napi.rs) binds.
It isn't published, it has no tests of its own yet, and CI doesn't build it.

A call from JavaScript into an addon costs more than `TableReader` takes to
read a field, so the addon has no getter of one value: a function that adds
two numbers took 20 ns per call, against 3.9 ns for `getFloat64()`. Each
function reads into typed arrays that the caller passes:

- `get<Type>Fields(bytes, out)` reads the first `out.length` fields of the
  root table, which are all of `out`'s element type.
- `get<Type>Columns(bytes, vectorPosition, path, out, ...)` reads a field of
  every item of a vector of tables into each `out`, and returns the vector's
  length. A `path` holds the field's index in the item, or the indexes of the
  nested tables that lead to it and then its own.

The addon also has two functions of the benchmarks' schema,
`readItemListItems()` and `writeItemListItems()`, in `src/workloads.rs` and
`@tafla/native/workloads`. They're written by hand, as tafla-gen might
generate them for an addon, over the code that it generates for Rust, to time
such code before a generator writes it. They read and write the items of an
`ItemList` as a column of each of an item's three float32s.

The functions of every schema read tables of the default layout, check every read against the bytes,
as the Rust runtime's `TableReader` does, and throw an `Error` with the code
`TAFLA_DECODE` for bytes that they can't read.

## Building

`yarn workspace @tafla/native build` builds the addon with Cargo, for the
platform that it runs on, into `tafla-native.node`, which `source/index.ts`
loads. In `native/`, `cargo clippy --all-targets -- -D warnings` and
`cargo fmt --check` check the crate.

The functions are Node-API callbacks written by hand, in `src/call.rs`,
rather than napi-rs's `#[napi]` functions. The code that napi-derive 3.7.1
generates for a function with arguments took 144 ns per call for the one that
adds two numbers: it creates a scope of borrows in each call, with a mutex
and an allocation.

## Benchmarks

`yarn bench:native`, from `typescript/`, builds the addon and compares it with
the TypeScript runtime, with the harness and the options of
[the comparison](../benchmarks/README.md#method):

- **1000 fields** of int32, int64 and float64 are read into a typed array,
  with `TableReader`'s getter in a loop and with `get<Type>Fields()`.
- **vector of 100 and of 10,000 nested tables: sum every item** sums the three
  float32s of each item, as the comparison's group does: with `TableReader`'s
  getters by hand, with tafla-gen's views, and from three columns that the
  addon reads in one call, into arrays created in each call or kept between
  calls. **native columns** reads them with `getFloat32Columns()`, by their
  paths, and **native generated** with `readItemListItems()`.
- **deserialize** reads the items as the workload's objects, and
  **serialize** writes the table from them, as the comparison's codecs do.
  **native generated** creates the objects from columns that
  `readItemListItems()` reads, and fills columns from the objects for
  `writeItemListItems()`, which writes into bytes kept between calls, of
  which it returns a copy. **from columns** writes columns that the caller
  holds already.

Each group's first variant runs again as its last, an A/A pair.

### Results

Two runs on 2026-11-19, on an Apple M2 Pro with 32 GB in macOS 27.0, with
Node 26.13.0, of `yarn bench:native --rounds 61 --sample-ms 5`: the fields
at a 1-minute load average of 2.3 to 2.5, and the vectors, with
`--filter vector`, at 3.0. The A/A pairs were within 3%. A run of the fields
before the addon's loops were specialized, at a load average of 6, read the
1000 int32s in 0.88 µs.

Fastest call in µs, and its ratio to `TableReader`'s getter:

| Benchmark           | tafla | native        |
| ------------------- | ----: | ------------: |
| 1000 Int32 fields   | 3.428 | 1.199 (0.35×) |
| 1000 Int64 fields   | 3.417 | 0.870 (0.25×) |
| 1000 Float64 fields | 3.401 | 0.868 (0.26×) |

Fastest call in µs for the vector of nested tables, and its ratio to the
hand-written tafla codec's. The arrays are created in each call, or reused:

| Benchmark                     | tafla | tafla-gen | native columns | reused        | native generated | reused or from columns |
| ----------------------------- | ----: | --------: | -------------: | ------------: | ---------------: | ---------------------: |
| 100 items: sum every item     | 1.288 |     1.695 |  2.093 (1.63×) | 1.704 (1.32×) |    1.101 (0.85×) |          0.646 (0.50×) |
| 100 items: deserialize        | 1.821 |     2.663 |                |               |    1.212 (0.67×) |                        |
| 100 items: serialize          | 9.755 |     1.974 |                |               |    0.953 (0.10×) |          0.662 (0.07×) |
| 10,000 items: sum every item  | 121.5 |     161.2 |  116.2 (0.96×) | 112.3 (0.92×) |     59.2 (0.49×) |           55.5 (0.46×) |
| 10,000 items: deserialize     | 168.6 |     257.8 |                |               |     97.2 (0.58×) |                        |
| 10,000 items: serialize       | 914.2 |     178.5 |                |               |    123.7 (0.14×) |           98.2 (0.11×) |

- The fields of one table are read 3 to 4 times faster in one call. A table
  of 1000 fields of one type is a shape of the benchmarks, though, rather
  than of a schema.
- Read by their paths, the columns of a vector's items aren't read faster
  than `TableReader` reads them: 32% to 63% slower for 100 items, and 4% to
  8% faster for 10,000.
- The function of the schema reads them about twice as fast into arrays that
  are kept, 0.65 µs against 1.29 µs and 55.5 µs against 121.5 µs, and 15%
  faster for 100 items into new arrays, whose creation takes 0.45 µs of the
  call. Creating the workload's objects from the columns takes 1.21 µs against
  the hand-written reader's 1.82 µs and tafla-gen's 2.66 µs.
- It writes the items about twice as fast as tafla-gen's TypeScript from the
  workload's objects for 100 items, 0.95 µs against 1.97 µs, and 3 times as
  fast from columns. For 10,000 items it takes 124 µs and 98 µs against
  179 µs.
- A loop written in Rust for the item's three fields took 23 µs for 10,000
  items in a Rust binary, where the function takes about 47 µs of its 55.5 µs
  in the addon. The difference wasn't investigated.

### Limitations

- One machine and one run. Deno and Bun weren't measured.
- The functions of the schema cover one shape, a vector of tables of
  float32s. No strings, BigInts or objects are created by the addon, which
  took longer than in TypeScript when a prototype did: reading a record of four fields into an object took 492 ns, against
  119 ns for `TableReader`.
- `bytes` must not be written to from another thread during a call, through
  a `SharedArrayBuffer`.
