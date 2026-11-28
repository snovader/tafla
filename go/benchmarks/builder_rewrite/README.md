# The builder rewrite's harness

Times main's `TableBuilder`, #216's and this branch's in the same processes,
interleaved with the comparison's harness, on the comparison's rows and on
shapes that it doesn't time: those of #216's reviews, and those of Rust's
#378. It's for the stacked PR's description, and is to be dropped before
merging.

`setup.sh` copies each runtime into a package of its own under
`builders/`, in a copy of `go/` in the output directory, and generates the
rows of `templates/` into it, so that every builder compiles alike:

- **main** and **main A/A**: main's runtime, at 630f0016, in two packages.
- **#216**: #216's, at 89e75d4f, which later commits changed only in
  comments.
- **buffer** and **buffer A/A**: this branch's, in two packages. The A/A
  twins are copies of their own, so that their difference shows code
  placement as well as noise.
- **buffer, bytes.Clone**: this branch's, copying each table out of the
  buffer, as Rust's `to_bytes()` does.
- **buffer, reused**: this branch's, with one builder that writes every root
  table into the bytes of the one before it, through `Reset()`.
- **co-allocated**: this branch's, with `NewTableBuilder()` allocating the
  builder with its first 256 bytes, in `templates/coalloc_api.go.tmpl`.
- **declared alignment**: the rows that nest tables in place, each through
  `SetTableFuncAligned()` with its fields' alignment.
- **begin/end**: the rows that nest tables in place, through the begin and
  end calls of `templates/beginend_api.go.tmpl` rather than functions, which
  keep the builder on its caller's stack.

Before timing, each builder's call must write main's bytes twice in a row.
Each builder's row is a loop of its own; every round runs each builder once,
in alternating order, after a garbage collection.

```sh
sh go/benchmarks/builder_rewrite/setup.sh <output directory>
<output directory>/builder-timing -check
<output directory>/builder-timing -rotate 3 > run-3.txt
```

Run it under the benchmark lock, in several processes, each with `-rotate`
set to another number, which rotates the builders' order, and compare the
medians over the processes of each builder's fastest round. Each row prints
`ROW`, its name, the builder, the fastest and median round's nanoseconds per
call, and the bytes and allocations per call, separated by tabs.
