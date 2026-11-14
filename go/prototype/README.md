# A Go TableBuilder that writes each field when it's set

This package is a prototype, for an evaluation: it isn't supported, and the
runtime doesn't use it. `prototype.TableBuilder` has the API of
`tafla.TableBuilder`, writes the same bytes and fails with the same errors,
but writes each field's bytes when it's set, rather than recording the field
and writing every field in `Bytes()`. It ports the design of the C++
prototype in `cpp/prototype/` (draft PR #212).

- A table's bytes go into a buffer of 128 bytes within the builder, and then
  into one on the heap, of 512 bytes or more, which doubles as it grows.
  Offsets are written as the fields are set. A sized table's length is
  written by `Bytes()`, into new bytes of the table's exact length.
- Runs of 256 bytes or more, an array's elements, a string, an array of
  structs or a table nested as bytes, are kept by reference as segments,
  which hold the caller's slice, so the garbage collector keeps it, until
  `Bytes()` copies them in between the buffer's bytes. The first 4 segments
  are kept within the builder. On a big-endian host, arrays are copied.
- A vector writes its count and room for the ends of its items, and writes
  each end after its item. After its first item, the buffer makes room for
  the others, each as long as the first.
- A table nested with `SetTableFunc()` is written unpadded, and then moved
  forward by its padding once its alignment is known. Its offsets count from
  its own start, so none changes.
- A nested builder is copied into its parent, as its bytes and its segments.
- Setters check a field before writing it. A table or vector item that fails
  within its fill function is truncated back to a checkpoint: the buffer's
  length, the segments, and the counters.
- The builder never points into itself: until it outgrows its 128 bytes,
  they're addressed from the builder, so that a builder that no setter keeps,
  as `SetTableFunc()` and `SetVectorFunc()` keep theirs, stays on the stack.
  `NewTableBuilder()` is cheap enough to inline, so the builder is allocated
  in its caller's frame.

`variant.go` holds the constants that the evaluation varies: the inline
buffer, the first capacity on the heap, the growth policy, the shortest
segment, and whether `Bytes()` returns the buffer without copying it.

## Checks

`go/tests/prototype_test.go` checks the prototype against
`tafla.TableBuilder`: every conformance case byte for byte, nested from
builders and in place, and generated tables in every offset type, each
setter's error and the final bytes or error, with a long field anywhere, a
field fewer or more than the header holds, and fill functions that fail.
`go/benchmarks/codecs/prototype_test.go` checks the comparison's workloads
byte for byte.

```sh
cd go/tests
go test -race -run Prototype .
# Every run is a segment, and the buffer starts at a byte
go test -race -run Prototype -tags tafla_prototype_stress .
# Arrays converted element by element, as on a big-endian host
go test -race -run Prototype -tags tafla_portable .
go test -run '^$' -fuzz '^FuzzPrototype$' -fuzztime 60s .
```

## Timing

`go/benchmarks/cmd/prototype` times `tafla.TableBuilder` and the variants of
this package on the comparison's builder rows, interleaved, with the
comparison's harness, and an A/A pair of each builder. `variants.sh`
generates the variants as copies of this package with their own constants,
so that the compiler folds each one's choices.

```sh
cd go/benchmarks/cmd/prototype
sh variants.sh
go run -tags prototype_variants .
```
