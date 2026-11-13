# tafla for Go

`github.com/snovader/tafla/go` is tafla's runtime for Go, the package `tafla`.
It writes and reads binary tables of scalars, strings, arrays, structs and
nested tables in tafla's format, which the TypeScript, Python, Java and Rust
runtimes write and read too. `TableBuilder` writes a table at its exact size,
copying arrays in bulk, and `ByteBuffer` reads fields back by offset, sharing
arrays, strings and structs with the bytes. Every read checks that it stays
within the bytes it was given. It depends only on Go's standard library.

Each release of the module is tagged `go/v<version>`, and
[CHANGELOG.md](CHANGELOG.md) lists the changes that it brings. The Go module
proxy serves it from its tag, so `go get` adds a release by its version, or the
latest release:

```sh
go get github.com/snovader/tafla/go@v0.1.0
go get github.com/snovader/tafla/go@latest
```

```go
import tafla "github.com/snovader/tafla/go"

// A format's header, shared by its writer and its readers
var layout = tafla.TableLayout{}.Sized().Versioned()

const formatVersion = 1

series, err := tafla.NewTableBuilder(2, tafla.TableLayout{})
err = series.SetUint32Array([]uint32{0, 86_400}) // kept until the table is written
err = series.SetFloat32Array([]float32{21.75, 18.5})

builder, err := tafla.NewVersionedTableBuilder(3, layout, formatVersion)
err = builder.SetFloat64(59.3293)
err = builder.SetString("Europe/Stockholm")
err = builder.SetTable(series) // written straight into the table
bytes, err := builder.Bytes()

buffer := tafla.NewByteBuffer(bytes, layout)
version, err := buffer.Version()
if version != formatVersion {
	return tafla.NewUnsupportedVersionError(version, formatVersion)
}
position, err := buffer.RootOffset(1)
timeZone, err := buffer.GetString(position) // "Europe/Stockholm", sharing the bytes
table, err := buffer.RootOffset(2)
position, err = buffer.FieldOffset(table, 1)
values, err := buffer.GetFloat32Array(position) // an ArrayView[float32]
values.Slice()                                  // []float32{21.75, 18.5}, in place
```

Every call above returns an error, which the example leaves unchecked for
brevity; `example_test.go` checks them. Records at fixed offsets without a
header or offsets, such as stream frames, are better written and read with
`encoding/binary`'s `LittleEndian`.

`benchmarks/` compares tafla with FlatBuffers, protobuf, vtprotobuf,
MessagePack and `encoding/binary`, on the same workloads as the other
languages' comparisons. It's a module of its own, so that the runtime's
module requires none of its libraries, and its
[README](benchmarks/README.md) describes the method and has results.

## Format

All values are little-endian. A table is its header, one offset per field
and then the fields in the order they were set:

- The header holds the table's byte length when the layout is `Sized()`,
  and then its version when it's `Versioned()`.
- Header slots, offsets, counts and vector item ends are all of the layout's
  offset type, `tafla.Uint8`, `tafla.Uint16` or `tafla.Uint32`, whose offset
  size is 1, 2 or 4 bytes. It limits the table to 255 bytes, 64 KiB or
  4 GiB. The zero `TableLayout` has no header and `uint32` offsets, the
  default.
- Offsets count from the table's start. A table nested with `SetTable()` has
  no header, and its offsets count from its own start.
- A struct is its scalars, one after another without padding, which
  `SetStruct()` writes from their bytes and `GetStruct()` returns.
- Arrays, strings and arrays of structs start with their count. Arrays are
  padded so that their elements are aligned to their size from the table's
  start, and arrays of structs to the size of their largest scalar.
  `SetTable()` aligns a nested table to the largest alignment of its arrays
  when given its builder, and to 4 bytes by default when given bytes.
- A vector starts with its item count, then the end of each item, counted
  from the vector's start, and then the items, so `Vector.Get()` finds any
  item from two ends without reading the others. An item without a header and
  with its parent's offset type is read in place, from the bytes that
  `Vector.GetInPlace()` returns, which run on to the end of its parent's.

The runtime writes the same bytes as the other runtimes for the same fields,
and reads their tables: `conformance_test.go` reads and rebuilds the
repository's conformance suite, which covers the whole format, and
`tests/generated` writes and reads the tables of `conformance/generated`
with the code that tafla-gen generates.

## Sharing and copying

- **Builders keep their values.** A setter of an array, a string or a
  struct's bytes keeps the slice or string, and `SetTable()` and
  `SetVector()` keep the builder or the `TableBytes`, until `Bytes()` writes
  the table, so their elements must not change in between. A table's length
  is counted when each field is set, so a value that changed length couldn't
  be written, but a slice's length can't change.
- **Nest builders rather than bytes.** `SetTable()` and `SetVector()` take a
  `*TableBuilder` or `TableBytes`, and write a builder of the parent's offset
  type and no header straight into its parent, so that the outermost table
  allocates and writes all the bytes once. A builder of another layout is
  written by a writer of its own into the bytes that it takes in its parent.
- **Nested tables can be recorded in place.** `SetTableFunc()` and
  `SetVectorFunc()` record a nested table, or each item of a vector,
  straight among the builder's fields, through the `Item` that their
  function sets the fields on, rather than from a builder of its own. They
  write the bytes that nested builders write, and leave the builder as it
  was if the function returns an error or leaves fields unset. The
  benchmarks' vector of 100 nested tables took 5.5 µs so, in 6 allocations,
  against 10.3 µs and 204 allocations from a builder per item, and 6.3 µs
  through FlatBuffers, interleaved in one process with the comparison's
  harness on 2026-11-13. A table nested from a builder or bytes,
  with `SetTable()` and `SetVector()`, is still what a table built
  elsewhere, or reused, needs.
- **Arrays are read in place.** `ByteBuffer`'s array getters return an
  `ArrayView[T]`, which reads elements from the bytes at any alignment and in
  any host byte order. `AsSlice()` returns them as a slice of the bytes when
  the host is little-endian and the elements are aligned to their type in
  memory, and `Slice()` falls back to a copy otherwise. `TableBuilder` aligns
  arrays from the table's start, so they're aligned in memory when the table
  is, as the bytes of a slice that Go allocates for a table usually are,
  though Go doesn't promise it. A slice in place shares the bytes, so writing
  to it writes to them. Since whether `Slice()` shares them depends on the
  host and the bytes' address, treat its result as read-only, and take a
  copy to own with `Append(nil)`.
- **Strings share the bytes too.** `GetString()` returns a string of the
  table's bytes rather than a copy, as FlatBuffers' Go runtime does, so the
  bytes must not change while the string is in use, as for `unsafe.String`.
  `strings.Clone` copies one to keep it past them. Byte arrays, structs and
  vector items are slices of the bytes.

## Errors and panics

Every getter returns its value and an `error`, which is a `*DecodeError` when
the bytes can't be read, and `errors.As` finds it:

- Every read checks its range against the bytes the `ByteBuffer` was given,
  and fails with `OutOfRange` if it would leave them, which gives the read's
  position and length and the bytes available. Strings must be valid UTF-8,
  or they fail with `InvalidUTF8`. A vector item that ends before the one
  before it fails with `ItemEndsBeforeStart`. No bytes make a read panic, nor
  allocate more than they hold.
- `ByteBuffer` doesn't compare versions. A reader returns
  `NewUnsupportedVersionError()` when `Version()` isn't one it reads, as the
  example does, so that every decoding failure is a `*DecodeError`.
- On a 32-bit host, an offset or length past the largest `int`, 2 GiB, fails
  with `OutOfRange` before it's read, since no bytes reach it.

The getters return errors, rather than recording the first error in the
buffer and returning zero values until it's checked, as `bufio.Scanner`
does, since each value is then known to be read: an error that's ignored
can't make a later read return a value of the wrong field. Returning an
error costs a register and a branch that the processor predicts, and
`GetFloat64()` and the other getters of fixed size inline into their
callers, which then make no call to read the value.
`getter_errors_bench_test.go` measures it: four `float64` reads took 3.95 ns
with errors, 3.67 ns with a `bool` in their place, and 3.92 ns recording the
first error in the reader, on an Apple M2 Pro.

`TableBuilder` returns a `*BuildError`, whatever the values, when it can't
write a table, and leaves the builder as it was:

- A setter fails with `TooManyFields` for a field beyond the number the
  table was created with, and with `TooLong` for one that would make the
  table too long for its offset type, and `SetStructArray()` with
  `NotWholeStructs` for bytes that aren't a whole number of structs.
  `Bytes()`, and nesting a builder, fail with `Incomplete` while fields are
  left to set, as `SetTableFunc()` and `SetVectorFunc()` do for a table that
  their function leaves incomplete. A version that doesn't fit, or a header
  too long for its offset type, fails the constructor.

Functions panic only for mistakes of the code that calls them, which no bytes
can cause, as Go's standard library panics for an index out of range:

- an index past an `ArrayView`'s or a `Vector`'s length, or negative,
- a negative field index of `RootOffset()` or `FieldOffset()`, or a negative
  struct size of `GetStruct()` or `GetStructArray()`,
- an offset type or an alignment that isn't one of the constants,
- a negative number of fields or of a vector's items, `NewTableBuilder()`
  of a versioned layout or `NewVersionedTableBuilder()` of one without a
  version slot, and `Version()` of a layout without one,
- a nil nested table, and
- a `TableWriter` given other fields than its sizer counted, for a write past
  the table's bytes, rather than writing past them.

## Writing without a builder

`TableSizer` and `TableWriter` write a table whose fields are known before
it's written, as the code that tafla-gen generates writes its tables. The
sizer counts the table's length from its fields, and its writer writes the
same fields, in the same order, into bytes of that length, without recording
them first as `TableBuilder` does. `TableSizer.AppendWriter()` writes the table
after the bytes of a slice, into its capacity when it has room, as `append`
does, so that a caller can reuse bytes.

Fields whose sizes the schema fixes, scalars, structs and nested tables of
only such fields, the generated code lays out itself, at positions that the
generator works out, since Go's compiler keeps the sizer's and the writer's
fields in memory at every call. `TableSizer.AddFields()` adds a table's
first such fields together, and `TableWriter.ReserveFields()` returns their
bytes; `AddTableBytes()` and `ReserveTableBytes()` take a nested table of
only such fields whole. `AppendTable()` appends the bytes of a table that its
caller lays out entirely, such as a message of only such fields and a string
after them, and writes its header, without a sizer and a writer. These six
functions are for the generated code: they're exported because it lies in
its user's package, but they're outside the compatibility promise of the
rest of the API, and may change in any release, with the generator.

`TableWriter` is the target of the generated code, whose calls follow from
the schema, so it doesn't check them again. Its contract, which the other
runtimes share:

- Fields are set in index order, each once, as the sizer added them, and each
  vector item is followed by its `SetItemEnd()`. Fields that the sizer added
  together are reserved together: a table's first fields with
  `ReserveFields()`, before any setter, and a nested table with
  `ReserveTableBytes()`, and the caller writes every byte of them.
- A field index outside its table, or an item outside its vector, writes an
  offset or item end over another slot or field, so the table's bytes are
  undefined. Every write still stays within the table's bytes: one past them
  panics, as indexing a slice does, as Java's writer throws.
- Fields that differ from those that the sizer counted, in their lengths or
  number, never write past the table either, and `Bytes()` fails with
  `NotAsSized` for fields that didn't fill it.

## Conversions

Each setter takes its field's own Go type, such as `uint8` for a `uint8` and
`float32` for a `float32`, so the caller converts a value to it, as Go
converts:

| Value                                  | Written as                                                                                                  |
| -------------------------------------- | ----------------------------------------------------------------------------------------------------------- |
| An integer outside its field's range   | A conversion keeps the low bits: `uint8(300)` of a variable is 44, and a constant out of range won't compile |
| A fraction or NaN for an integer field | A conversion truncates toward zero, and is implementation-defined for NaN and values out of range            |
| A `float64` too large for a `float32`  | `float32()` makes it infinity of its sign, and rounds other values to the nearest `float32`                  |
| Invalid UTF-8 in a string              | Written as its bytes, which readers reject: Go's strings can hold any bytes                                  |

## Development

The runtime's module is this directory, the tests' module is `tests/` and
the benchmarks' module is `benchmarks/`, so that the runtime's module
requires nothing, and its tests, the examples, run from the module cache,
without the repository's files. The `go` directive is 1.23, the first release
with iterators, which `ArrayView` and `Vector` return, and which
`pgregory.net/rapid`, the property tests' library, requires. Run the commands
below from this directory.

```sh
gofmt -l .
go vet ./...
staticcheck ./...
go test -race ./...
cd tests
go vet ./...
staticcheck ./...
go test ./...
go test -race ./...
go test -tags tafla_portable ./...
```

`staticcheck` installs with `go install honnef.co/go/tools/cmd/staticcheck@latest`.
The tests in `tests/` are:

- `conformance_test.go`: reads and rebuilds the conformance suite.
- `round_trip_test.go`, `format_test.go`, `decode_error_test.go`,
  `table_builder_test.go` and `table_writer_test.go`: properties of tables
  built from the conformance suite's descriptions, which
  `tests/internal/tables` parses, generates with rapid, builds and reads, as Rust's `tests/common`
  does, and examples of the errors and of every boundary: signed zero, NaN,
  integer limits, empty arrays, the limits of each offset type, strings
  outside the Basic Multilingual Plane and unaligned bytes.
- `fuzz_test.go`: native fuzz targets, which mirror `rust/fuzz`, and whose
  seeds, the conformance suite's tables, run as tests.
- `generated`: the Go that tafla-gen generates from the example
  schemas in `schemas/`, in a package of each schema, and the tests beside
  them, which read back what it writes, write what `TableBuilder` writes,
  reject other versions and read corrupted bytes as values or decode errors,
  through every getter and view, and the cases of `conformance/generated`.
  They're the generator's golden files, which `rust/gen/README.md` describes:
  rewrite them with the generator rather than by hand.

The property tests run 256 cases each, as proptest does in Rust's, which
`-rapid.checks` changes, and save a failing case's input under the
package's `testdata/rapid`, which `.gitignore` leaves out.

### Byte orders and word sizes

On a little-endian host, arrays are read in place and copied in bulk. The
build tag `tafla_portable` turns that off, so that every array is read and
written element by element, through the path that a big-endian host takes,
and `go test -tags tafla_portable ./...` tests that path on any host. CI also
runs the tests on a big-endian host, `s390x` under qemu, and on a 32-bit one,
`386`, where an `int` doesn't hold every `uint32` offset. With Docker, the
same runs from `tests/`, and from `tests/generated`, with the repository
mounted where the tests find the conformance suite:

```sh
root="$(git rev-parse --show-toplevel)"
GOOS=linux GOARCH=s390x go test -c -o tafla-s390x.test .
docker run --rm --platform linux/s390x -v "$root:$root" -w "$PWD" alpine ./tafla-s390x.test
GOOS=linux GOARCH=386 go test -c -o tafla-386.test .
docker run --rm --platform linux/386 -v "$root:$root" -w "$PWD" alpine ./tafla-386.test
```

### Fuzzing

`tests/fuzz_test.go` holds the fuzz targets. A panic, a hang, a run out of memory
or a read that disagrees with `SPEC.md` is a finding:

- `FuzzByteBuffer` reads arbitrary bytes with every getter of `ByteBuffer`,
  at every position and past the end, in every offset type and header, and
  reads every array, struct array and vector there. Each read must return
  the bytes that lie where the format puts them, exactly when they lie
  within the input, and the decode error otherwise.
- `FuzzConformance` mutates the conformance suite's tables and reads them as
  every valid case's description, as the conformance test reads them.

Run each from `tests/` with `-fuzz`, and stop it with Ctrl-C or `-fuzztime`. Go writes an
input that fails to `testdata/fuzz/<target>/`, where `go test` runs it again
as a regression test, so a finding is checked in there once it's fixed. CI
runs each target for a minute on every change, and the weekly Fuzz workflow
for five minutes, from the corpus that its earlier runs grew.

```sh
go test -run '^$' -fuzz FuzzByteBuffer -fuzztime 5m
go test -run '^$' -fuzz FuzzConformance -fuzztime 5m
```

The generator's fuzz target, `gen` in `rust/fuzz`, generates Go too.
