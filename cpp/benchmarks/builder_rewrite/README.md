# Comparing TableBuilder's designs

The harness that measures three designs of C++'s `TableBuilder` against each
other, kept so that the figures can be reproduced: main's builder, which
records each field and writes them all in `to_bytes()`; #212's rewrite,
which writes each field when it's set and keeps values of 64 bytes or more
as segments, copied by `to_bytes()`; and the runtime's builder of this
branch, which copies each value into one buffer when it's set and hands the
buffer out. It's outside every CMake target, so drop this directory before
merging.

```sh
sh cpp/benchmarks/builder_rewrite/build.sh /tmp/builder-rewrite
/tmp/builder-rewrite/timing > run.tsv
/tmp/builder-rewrite/allocation > allocation.tsv
```

`setup.py`, which `build.sh` runs, copies main's headers (630f0016, with
#376) and #212's (59d1c2d7, #212 merged with main) into namespaces of their
own, `tafla_main` and `tafla_212`, beside this branch's, which the harness
includes as `tafla`. It takes other revisions as further arguments. It also
writes `tafla_raw`, this branch's builder with its buffer allocated by
`std::make_unique_for_overwrite()` rather than held in a `std::vector`, which
doesn't zero the room that it grows into, and handed over as `Bytes`.
`build.sh` compiles with `-O3 -falign-functions=64`, so that a function's
placement doesn't depend on the code before it.

- `timing` times every builder's variant of each row: main's and #212's,
  each table taken with `to_bytes()`, and this branch's, taken with
  `into_buffer()`, with `to_bytes()`, and as a view from `finish()` of one
  builder that the loop keeps and `reset()`s for every table, then with each
  table in place given its alignment, and the raw variant. Main's builder
  and this branch's with `into_buffer()` run twice, as A/A pairs whose code
  is compiled again, and so placed elsewhere. Each variant's row is a
  function of its own, called in a loop of its own, after a check that it
  writes main's builder's bytes twice through the same state. Rounds start
  from the next variant every other round and reverse the order every other
  round, after warm-up rounds. It prints, for each variant, its fastest
  round's and its median round's time per call, in nanoseconds. Options:
  `--rounds` (20), `--warm-up` (5) and `--sample-ms` (5); other arguments
  select rows by a part of their name.
- `allocation` prints the builders' sizes, and the bytes and allocations of
  each variant's call, and the most bytes held at once during it.

The rows are the comparison's rows that build tables; Rust's harness's rows
in #378: the documents with their columns padded by a name of 17 bytes, the
document of 100,000 rows into a buffer with room for it, tables of 32
medium and short values, the strings comparison's tables of 16 strings,
tables nested in place 1 to 1,000 deep, each padded for an array of
float64 in the innermost, tables of arrays and of float64 columns, and
100,000 builders nested by reference or owned; and the shapes of #212's
reviews: many strings and structs about its 64-byte threshold, a vector in
place of strings, builders nested by value and by reference, a chain of
builders, and a vector in place too long for its table.
