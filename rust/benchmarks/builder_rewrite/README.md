# Comparing the rewritten TableBuilder with main's

The harness that measured pull request #217's rewrite of `TableBuilder`
against main's builder, kept so that its figures can be reproduced. It's
outside every crate: `setup.py` builds it in a scratch worktree, beside a
copy of main's builder, so drop this directory before merging.

```sh
python3 rust/benchmarks/builder_rewrite/setup.py /tmp/builder-rewrite
cd /tmp/builder-rewrite/rust
cargo build --release -p tafla-benchmarks --features main-builder \
    --bin builder_comparison --bin builder_allocation
```

- `builder_comparison` times main's builder and the rewrite, each twice as
  an A/A pair. Its rows are those of `builders::for_each_row()`: the
  comparison's rows that build tables, a vector of 100 nested tables from
  a builder per table, tables of 32 medium values, of 128 and 192 bytes,
  and of 32 short values, of 64 and 96 bytes, and the shapes of the second
  review: tables nested in place 1 to 1,000 deep, each padded for an array
  of float64 in the innermost, tables of more values than a builder keeps
  within itself, of values just shorter than a segment, and of 100-byte
  values, and 100,000 builders nested by reference or owned. Each builder
  makes the tafla codec's calls in a function of its own, in a loop of its
  own, after a check that it writes main's builder's bytes, and the
  codec's; rounds rotate the builders' order and reverse it every other
  round, after warm-up rounds, and it reports the fastest round and the
  median. Options: `--rounds` (30), `--warm-up` (5) and `--sample-ms`
  (10).
- `builder_allocation` prints the builders' sizes, the bytes and
  allocations of each row, and the most bytes held at once while it's
  written, and while an uneven vector in place is written, whose first item
  is long and the others empty, and a vector in place that makes a table
  of `uint16` offsets too long.
- `differential.rs`, in the runtime's tests, builds every conformance case
  and generated tables, in every offset type, with fills that fail, panic or
  return early, with main's builder and the rewrite, and compares every
  setter's result and the bytes:
  `cargo test -p tafla --features main-builder --test differential`.

The figures in #217 of 15 November ran the frozen `builder_comparison`
binary three times, one process after another, under the repository's
benchmark lock, and `builder_allocation` once after them, against main's
builder at 7238a1b. Those of 28 November, against main's builder at
630f0016, with #376, ran it in 16 processes, built with
`RUSTFLAGS="-C llvm-args=-align-all-functions=6"`, as #371 recommends.
Earlier commits of this directory held the variants that chose the room
and growth of the first review (room for 64 or 128 bytes a field, growing
fourfold, and both), and the room that a vector in place makes for its
items and an owned builder boxed in a `NestedTable`, and of the second
review: without the room that segments make past the first four, growing
twofold, room for 16 or 32 bytes a field, keeping runs of 64 bytes or more
as segments rather than grow long bytes for them, and keeping runs from
128 or 64 bytes rather than 256.
