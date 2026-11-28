# Comparing TableBuilder's designs

The harness that measures three designs of `TableBuilder` against each
other, kept so that the figures can be reproduced: main's builder, which
records each field and writes them all in `to_bytes()`; #217's rewrite,
which writes each field when it's set and keeps long values as segments;
and the runtime's builder of this branch, which writes each field into one
buffer and hands the buffer out. It's outside every crate: `setup.py` builds
it in a scratch worktree, beside copies of main's builder and #217's, so drop
this directory before merging.

```sh
python3 rust/benchmarks/builder_rewrite/setup.py /tmp/builder-rewrite
cd /tmp/builder-rewrite/rust
RUSTFLAGS="-C llvm-args=-align-all-functions=6" \
    cargo build --release -p tafla-benchmarks --features main-builder \
    --bin builder_comparison --bin builder_allocation
```

`setup.py` takes main's revision and #217's as further arguments, which are
630f0016 (main with #376) and 53c00ede (#217 brought up to date with it) by
default.

- `builder_comparison` times main's builder, #217's, and the runtime's
  three ways: its buffer taken with `into_bytes()`, a copy from
  `to_bytes()`, and a view from `finish()` of the buffer of one builder
  that the loop reuses, with `reset()`, for every table. Main's builder
  and the runtime's with `into_bytes()` run twice, as A/A pairs. Main's
  and #217's tables are taken with `to_bytes()`. Four variants of the
  runtime's builder follow, each with its buffer taken with
  `into_bytes()`: `variants/early.rs`, which pads a table nested in place
  as soon as a field raises its alignment, and the tables in place that
  hold it, moving what's written of them so far, rather than moving the
  whole table once its fields are set; `variants/declared.rs`, whose
  `set_table_with_alignment()` pads the table before its fields are set,
  for the alignment that it's given, and moves it only if its fields need
  more; and two whose buffer starts with room for at least 1 or 4 KiB
  rather than 256 bytes. Its rows are those of `builders::for_each_row()`:
  the comparison's rows that build tables, the documents with their
  columns padded by a name of 17 bytes, the document of 100,000 rows into
  a buffer with room for it, which the runtime's builder takes from
  `with_buffer()`, a vector of 100 nested tables from a builder per table,
  tables of 32 medium values, of 128 and 192 bytes, and of 32 short
  values, of 64 and 96 bytes, the strings comparison's tables of 16
  strings, and the shapes of #217's second review: tables nested in place
  1 to 1,000 deep, each padded for an array of float64 in the innermost,
  tables of more values than #217's builder keeps within itself, of values
  just shorter than its segments, and of 100-byte values, and 100,000
  builders nested by reference or owned. Each builder makes the tafla
  codec's calls in a function of its own, in a loop of its own, after a
  check that it writes main's builder's bytes twice with the same output,
  and the codec's; rounds rotate the builders' order and reverse it every
  other round, after warm-up rounds, and it reports the fastest round and
  the median. Options: `--rounds` (30), `--warm-up` (5) and `--sample-ms`
  (10).
- `builder_allocation` prints the builders' sizes, the bytes and
  allocations of each row, and the most bytes held at once while it's
  written, and while an uneven vector in place is written, whose first item
  is long and the others empty, and a vector in place that makes a table
  of `uint16` offsets too long.
- `differential.rs`, in the runtime's tests, builds every conformance case
  and generated tables, in every offset type, with fills that fail, panic or
  return early, with main's builder, #217's, the runtime's and the two
  variants that pad differently, and compares every setter's result and
  the bytes:
  `cargo test -p tafla --features main-builder --test differential`.

The figures of 28 November ran the `builder_comparison` binary in 16
processes, one after another, under the repository's benchmark lock, built
with `RUSTFLAGS="-C llvm-args=-align-all-functions=6"`, as #371 recommends,
and `builder_allocation` once after them. On #217's branch, this directory
compares main's builder with #217's alone; its earlier commits held the
variants that chose #217's room, growth and segments.
