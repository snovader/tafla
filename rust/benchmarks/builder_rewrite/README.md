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

- `builder_comparison` times main's builder, the rewrite, each twice as an
  A/A pair, and seven variants of the rewrite (without the room for a
  vector's items, with an owned builder boxed in a `NestedTable`, keeping
  values from 128 or from 64 bytes rather than 256, starting with room for
  64 or 128 bytes a field, and growing fourfold), on the
  comparison's rows that build tables, a vector of 100 nested tables from a
  builder per table, and tables of 32 medium values, of 128 and 192 bytes,
  and of 32 short values, of 64 and 96 bytes. Each builder makes
  the tafla codec's calls in a function of its own, in a loop of its own,
  after a check that it writes the codec's bytes; rounds rotate the
  builders' order and reverse it every other round, after warm-up rounds,
  and it reports the fastest round and the median. Options: `--rounds`
  (30), `--warm-up` (5) and `--sample-ms` (10).
- `builder_allocation` prints the builders' sizes, the bytes and allocations
  of each row, and the most bytes that each holds at once while it writes
  an uneven vector in place, whose first item is long and the others empty,
  and a vector in place that makes a table of `uint16` offsets too long.
- `differential.rs`, in the runtime's tests, builds every conformance case
  and generated tables, in every offset type, with fills that fail, panic or
  return early, with main's builder and the rewrite, and compares every
  setter's result and the bytes:
  `cargo test -p tafla --features main-builder --test differential`.

The figures in #217 ran the frozen `builder_comparison` binary three times,
one process after another, under the repository's benchmark lock, and
`builder_allocation` once after them.
