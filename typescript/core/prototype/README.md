# Prototype: a builder that writes each field when it's set

`directTableBuilder.ts` is a prototype, not part of the package: nothing
exports it, and the package's build leaves it out. It evaluates, for
TypeScript, the design that the C++ prototype in draft PR #212 proposes
for every runtime: replacing `TableBuilder`'s records, which `bytes()` replays through
a `TableWriter`, with a builder that writes each field into a growing buffer
when it's set. The draft PR that holds it describes the evidence and the
recommendation.

## What it does

- `DirectTableBuilder` has `TableBuilder`'s setters, `bytes()` and
  `finalize()`, and writes the same bytes. Its setters throw the same errors
  in the same order, and leave the builder as it was.
- The buffer starts as the one that the last builder to write its table
  left, of up to 64 KiB, or else as a `Uint8Array` of 64 bytes, which V8
  keeps on its heap. It grows to at least 512 bytes, at least doubling.
  `bytes()` copies it into bytes of the table's exact length, writes a sized
  table's length into them, and leaves the buffer for the next builder. No
  table holds it, and builders write every byte of their tables, padding
  included, so what it held before isn't read. This stands in for the 128
  bytes that C++'s builder holds within itself: a JavaScript object can't
  hold bytes, and V8 allocates a `Uint8Array` of more than 64 bytes outside
  its heap, in about 150 ns rather than 25.
- Each field's offset is written when the field is set. A vector writes its
  count and room for its item ends, and each item's end once the item is
  written; after the first item, it makes room for the others, each as long
  as the first.
- A table written in place with `setTableInPlace()` starts at its parent's
  position, unpadded. Once its fields are set, its alignment is known, and
  it's moved forward by its padding with `copyWithin()`. Its offsets count
  from its own start, so none changes.
- Scalars, offsets and lengths are written through a `DataView` of the
  buffer once it's outside V8's heap: once it grew, or reused with its view.
  A new buffer of 64 bytes is written as `TableWriter` writes it, through a
  scratch `DataView` and byte copies, since a view of it would make V8 give
  it an `ArrayBuffer`.
- Runs of 1 KiB or more are kept as segments: an array's elements, an array
  of structs, a struct, or a table given as bytes. `bytes()` writes them
  between the buffer's bytes, with the functions that `TableWriter` writes
  arrays with, so a large typed array is copied once, into the result, and a
  `number[]` converted once. Shorter runs are copied into the buffer when
  they're set.
- Strings are encoded into the buffer when they're set, with
  `encodeInto()`. A string of up to 1,024 UTF-16 code units gets room for its
  longest UTF-8, 3 bytes for each code unit; a longer one gets it when the
  buffer has room for it, and is otherwise counted first, so that the buffer
  grows only by what the string needs.
- A nested builder, whatever its layout, is copied in as its bytes and
  segments, with its length written into a sized header.
- A `fill` that throws, or a table or vector written in place that makes its
  parent too long, is undone by truncating the buffer and the segments to
  where they were, and restoring the counters.

## Variants

`configure()` sets the variant, for the tests; the benchmark copies the
module with other initial values instead, one copy for each variant, so that
V8 keeps their type feedback apart.

| Option         | Values                                                                                                                                                                                                                                        |
| -------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `growth`       | `'copy'`: a new `Uint8Array` and `set()`. `'transfer'`: `ArrayBuffer.prototype.transfer(newLength)`. `'resizable'`: `resize()` of a resizable `ArrayBuffer` that reserves 64 KiB, under length-tracking `Uint8Array` and `DataView` views.    |
| `result`       | `'slice'`: `slice()`. `'transfer'`: `transfer(length)`, or `transferToFixedLength(length)` of a resizable buffer.                                                                                                                             |
| `strings`      | `'short'`: the longest UTF-8 for a string of up to 1,024 code units, and otherwise as `'room'`. `'room'`: the longest UTF-8 when the buffer has room for it, or else counted. `'worst'`: always the longest. `'count'`: always counted first. |
| `scalars`      | `'view'`: a `DataView` of the buffer once it's outside V8's heap. `'bytes'`: always a scratch `DataView` and byte copies.                                                                                                                     |
| `segmentBytes` | The shortest run kept as a segment: 1,024, or another length, or `Infinity` to copy every run into the buffer.                                                                                                                                |
| `reuse`        | `true`: a builder starts from the buffer that the last one left. `false`: from a new one.                                                                                                                                                     |

The proposed design is the first value of each, with runs of 1 KiB or more
as segments.

## Running

From `typescript/`:

```sh
yarn vitest run core/test/prototype
node --conditions=tafla-source benchmarks/prototype/builders.ts
node benchmarks/prototype/buffers.ts
```

`builders.ts` times `TableBuilder` and every variant on the comparison's
builder rows, with the comparison's harness, after checking that each writes
the bytes of the comparison's codec. It takes `--rounds`, `--sample-ms`,
`--filter`, `--reverse`, which reverses the order of the variants, `--out`,
the directory for their copies, and `--json`, a file for the results.
`buffers.ts` times the buffer's steps on their own: making it, growing it
and making the exact bytes.

## Tests

`core/test/prototype/directTableBuilder.test.ts` runs, for every variant and
for builders that keep every run of 1, 8 or 256 bytes or more as a segment:

- every conformance case, byte for byte, from builders and written in place;
- a property that builds tables of every offset type and layout, from the
  conformance suite's generated fields, long arrays and strings with lone
  surrogates, nested from builders or in place, with a field more or fewer
  than the header holds, and with a setter that throws at any call within a
  `fill`, and compares each setter's error and the bytes or error of
  `bytes()` with `TableBuilder`'s;
- examples of what differs from `TableBuilder`: values read when they're
  set, and errors that move from `bytes()` to the setter.

The benchmark checks the comparison's workloads against its codec's bytes.
