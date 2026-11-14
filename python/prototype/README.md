# Prototype: Python builders that write each field when it's set

These are prototypes, not part of `tafla`. They evaluate replacing
`TableBuilder`'s design, which records each field and writes them all
through a `TableWriter` in `bytes()`, with one that writes each field's
bytes into a growing buffer when it's set, as C++'s prototype in
`cpp/prototype/` does. The draft PR that holds them describes the evidence
and the proposal.

## What's here

- `tafla_prototype/direct_table_builder.py`: pure-Python builders, which
  work with either implementation of `tafla`'s classes:
  - `DirectTableBuilder` appends to a `bytearray` with `+=`, which grows it
    in place with CPython's over-allocation.
  - `PreallocatedTableBuilder` writes with `Struct.pack_into()` and slice
    assignment into room that it allocates ahead, 128 bytes and then at
    least twice as many, copied into a new `bytearray` each time.
  - `CopyingTableBuilder` and `CopyingPreallocatedTableBuilder` copy every
    run when it's set, rather than keeping runs of 256 bytes or more as
    segments.
- `core/native/src/direct_builder.rs`: a native class,
  `tafla._native.DirectTableBuilder`, which owns a `Vec<u8>`, so that each
  setter is one call into native code. Only a build with the native crate's
  feature `prototype` has it, as the development environment's has; no
  wheel does. `_set_prototype_defaults()` sets the shortest segment and
  whether the bytes grow by `realloc()` or by allocating new room and
  copying, for the benchmarks' variants.
- `core/native/src/count_allocations.rs`: a counting allocator for the
  module's Rust heap, which `tracemalloc` doesn't trace, behind the feature
  `count-allocations`, for the allocation report only.
- `tafla_prototype/builders.py`: the builders by name, for the tests and the
  benchmarks.
- `prototype_tests/`: the tests, which build tables with the core tests'
  helpers.
- `tafla_prototype/bench.py`, `report.py`, `allocation.py` and `styles.py`:
  the measurements.

Every builder has `TableBuilder`'s API, bytes and errors, and leaves itself
as it was when a setter raises, with these differences:

- A scalar that its type can't hold raises `ValueError` from its setter,
  rather than from `bytes()`.
- A buffer shorter than a segment is copied when it's set, so a later change
  to it isn't written. A longer one is written as it is when the table is,
  and a change of its length raises `ValueError` from `bytes()`, as
  `TableBuilder` raises for any buffer.
- `bytes()` or `finalize()` within a `fill` raises the `ValueError` that the
  in-place setter would raise once `fill` returned.
- The native class's `bytes()` returns a read-only view of a `_Table` that
  holds the bytes, rather than a writable view.

## Tests

From `python/`:

```sh
uv run python -m unittest discover -s prototype/prototype_tests -t prototype
TAFLA_PUREPYTHON=1 uv run python -m unittest discover -s prototype/prototype_tests -t prototype
```

They check every builder, and builders that keep every run, or runs of 8
bytes or more, as segments, so that segments meet every part of the format,
against `TableBuilder`, with whichever implementation `tafla` uses:

- every conformance case byte for byte, nested from builders and in place,
  and on a big-endian host, as the tests' hooks treat it;
- the comparison's workloads, as the codec writes them;
- generated tables, call by call, each call's result or error compared: in
  every offset type, the same tables in uint8 and uint16 offsets, tables
  whose nested tables are all written in place, long vectors and tables
  written in place, and tables near uint8's limit; with a field more or
  fewer than the header holds, and with a fill that sets half its fields and
  raises;
- each kind of field after an array that starts it at every offset from 200
  to 255 in a table of uint8 offsets;
- `TableBuilder`'s own tests, which pass but for the behaviours above.

Deliberate bugs in the pure and native builders fail them: skipping the
padding move, the segments' move, the truncation of a failed table, the
length checks of scalars, strings, arrays, nested tables and vectors, the
check of an incomplete nested table, a sized nested builder's length, and
segments of the wrong length.

## Measurements

From `python/`, with the prototype on the path:

```sh
export PYTHONPATH=prototype
uptime
for order in "" --reverse; do
    uv run python -m tafla_prototype.bench --processes 5 --append native.json $order
    TAFLA_PUREPYTHON=1 uv run python -m tafla_prototype.bench --processes 5 --append pure.json $order
done
uv run python -m tafla_prototype.report native.json pure.json
uv run python -m tafla_prototype.styles
uv run python -m tafla_prototype.allocation
TAFLA_PUREPYTHON=1 uv run python -m tafla_prototype.allocation
```

The development environment's native module has the tests' hooks, which
cost a little on each array. The PR's figures come from a release wheel
built with only the feature `prototype`, and its allocations from one built
with `count-allocations`, from `core/`:

```sh
uvx --from maturin==1.16.0 maturin build --release --features prototype -i ../.venv/bin/python -o <directory>
```
