# Prototype: a builder that writes each field when it's set

`direct_table_builder.hpp` is a prototype, not part of the runtime. It
evaluates replacing `tafla::TableBuilder`'s design, which records each field
and writes them all in `to_bytes()`, with one that writes each field's bytes
into a growing buffer when it's set, as FlatBuffers', protobuf's and
MessagePack's builders do. The draft PR that holds it describes the evidence,
the design and the proposal.

## What it does

- Same setters, bytes, errors and exception safety as `tafla::TableBuilder`,
  including `set_table(fields, fill)` and `set_vector(count, fields, fill)`.
- The buffer holds 128 bytes within the builder and grows on the heap, at
  least twofold and to at least 512 bytes. `to_bytes()` copies it into
  `Bytes` of the table's exact length, writing a sized table's length.
- Each field's offset is written when it's set. A vector writes its count and
  room for its ends, and each item's end once the item is written.
- A table written in place with `set_table(fields, fill)` starts where its
  parent is, unpadded. Once its fields are set, its alignment is known, and
  it's moved forward by its padding with `memmove()`; its offsets count from
  its own start, so none changes.
- Runs of `SegmentBytes` bytes or more, an array's elements, a string's or a
  nested table's bytes, borrowed or owned, are kept as segments rather than
  copied, and `to_bytes()` copies them between the buffer's bytes, so a large
  borrowed column is copied once, into the result. `DirectTableBuilder` keeps
  runs of 256 bytes or more; `CopyingTableBuilder` keeps none, copying every
  value when it's set.
- A setter checks its field before it writes anything. A nested table or
  vector whose `fill` throws is removed by truncating the buffer, the
  segments and the owned values to a checkpoint.
- A nested builder, whatever its layout, is copied into its parent as its
  bytes and segments, with its length written into a sized header.

## Building and running

From `cpp/`:

```sh
cmake -S . -B build -G Ninja -DCMAKE_BUILD_TYPE=Release -DTAFLA_BUILD_BENCHMARKS=ON -DTAFLA_BUILD_PROTOTYPE=ON
cmake --build build --target direct_table_builder_test direct_table_builder_exception_safety_test \
    tafla_builder_comparison tafla_builder_allocation
build/prototype/direct_table_builder_test
build/prototype/direct_table_builder_exception_safety_test
build/prototype/tafla_builder_comparison
build/prototype/tafla_builder_allocation
```

The comparison takes about 15 seconds and the benchmarks' harness's options:
`--rounds`, `--warmup-rounds`, `--sample-ms`, `--quick`, and parts of names.

## Tests

- `direct_table_builder_test` rebuilds every conformance case and the
  comparison's workloads byte for byte, and checks generated tables, built in
  every offset type so that their fields often pass its limits, against
  `tafla::TableBuilder`: the error of each setter, the length and alignment
  after it, and the bytes or error of `to_bytes()`. Builders that keep every
  run, and runs of 8 bytes or more, as segments run the same checks, so that
  segments meet every part of the format.
- `direct_table_builder_exception_safety_test` fails each allocation of each
  setter in turn, as `tests/exception_safety_test.cpp` does, and checks that
  the builder is left as it was.
