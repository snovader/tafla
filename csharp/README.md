# tafla for C#

`csharp/` is tafla's runtime for .NET, the namespace `Tafla` in `Tafla.dll`,
for .NET 10. It writes and reads binary tables of scalars, strings, arrays,
structs and nested tables in tafla's format, which the TypeScript, Python,
Java, Rust, Go and C++ runtimes write and read too. `TableBuilder` writes a
table, copying arrays in bulk, `TableSizer` and `TableWriter` write one whose
fields are known in advance into bytes of its exact length, and
`TableReader` reads fields back by offset from a span of any memory, with
arrays, structs and vector items as spans of the bytes. Every read
checks that it stays within the bytes it was given. It depends only on
.NET's base class library, and is marked compatible with trimming and
ahead-of-time compilation, whose analyzers its build runs.

```csharp
using Tafla;

// A format's header, shared by its writer and its readers
static readonly TableLayout Layout = new() { Sized = true, Version = FormatVersion };
const uint FormatVersion = 1;

var series = new TableBuilder(2);
series.SetUInt32Array([0, 86_400]); // copied when it's set
series.SetFloat32Array([21.75f, 18.5f]);

var builder = new TableBuilder(3, Layout);
builder.SetFloat64(59.3293);
builder.SetString("Europe/Stockholm");
builder.SetTable(series); // copied into the table, aligned for its arrays
byte[] bytes = builder.ToArray();

var reader = new TableReader(bytes, Layout);
if (reader.GetVersion() != FormatVersion)
{
    throw DecodeException.UnsupportedVersion(reader.GetVersion(), FormatVersion);
}

double latitude = reader.GetFloat64(reader.RootOffset(0));
string timeZone = reader.GetString(reader.RootOffset(1)); // a new string
int table = reader.RootOffset(2);
ReadOnlySpan<float> values = reader.GetFloat32Array(reader.FieldOffset(table, 1)); // in place
```

`tests/Tafla.Tests/ExampleTests.cs` runs the example. The names are the
other runtimes' in C#'s case: table, field, layout (`TableLayout`), offset
type (`OffsetType`) and offset size, vector, builder (`TableBuilder`) and
decode error (`DecodeException`, as Java names it).

The runtime is released to NuGet as the package `Tafla`, whose page shows
`src/Tafla/README.md`, as [RELEASING.md](../RELEASING.md) describes. It
hasn't been released yet. Nor has `Tafla.Gen`, the package of
[tafla-gen](../rust/gen/README.md) for .NET projects, which generates a
project's schemas' code when the project builds, as
[its README](src/Tafla.Gen/README.md) describes.

`benchmarks/` compares tafla, and the code that tafla-gen generates, with
FlatBuffers, protobuf, MessagePack and MemoryPack, on the same workloads as
the other languages' comparisons. Its
projects are their own, so that the runtime depends on none of their
packages, and its [README](benchmarks/README.md) describes the method and
has results.

## Format

All values are little-endian. A table is its header, one offset per field
and then the fields in the order they were set:

- The header holds the table's byte length when the layout is `Sized`, and
  then the layout's `Version` when it has one.
- Header slots, offsets, counts and vector item ends are all of the layout's
  offset type, `OffsetType.UInt8`, `UInt16` or `UInt32`, whose offset size is
  1, 2 or 4 bytes. It limits the table to 255 bytes, 65,535 bytes, or for
  `UInt32`, to as long as a .NET array can be, `Array.MaxLength` bytes, about
  2 GiB rather than the offsets' 4 GiB. The default `TableLayout` has no
  header and `UInt32` offsets, and `OffsetType.UInt32` is 0 so that it does.
- Offsets count from the table's start. A table nested with `SetTable()` has
  no header, and its offsets count from its own start.
- A struct is its scalars, one after another without padding, which
  `SetStruct()` writes from their bytes and `GetStruct()` returns.
- Arrays, strings and arrays of structs start with their count. Arrays are
  padded so that their elements are aligned to their size from the table's
  start, and arrays of structs to the size of their largest scalar.
  `SetTable()` aligns a nested builder to the largest alignment of its arrays
  and nested tables, and nested bytes to `TableBuilder.DefaultTableAlignment`,
  4, unless it's given another.
- A vector starts with its item count, then the end of each item, counted
  from the vector's start, and then the items, so `Vector.Get()` finds any
  item from two ends without reading the others. An item without a header and
  with its parent's offset type is read in place, from the bytes that
  `Vector.GetInPlace()` returns, which run on to the end of its parent's.

`ConformanceTests` reads and rebuilds the repository's conformance suite,
which covers the whole format, byte for byte.

## Copying and sharing

- **Builders copy their values.** Each setter writes its field into the
  builder's bytes when it's called, so an array, a struct's bytes or a nested
  builder may change once it's set, and a builder may be nested several
  times. Setters take spans, such as a `ReadOnlySpan<int>` of an array, a
  slice of one or a collection expression. `ToArray()` copies the finished
  table, at its exact length, out of the builder's bytes, which grow as
  fields are set.
- **Nested tables are copied, or recorded in place.** `SetTable()` and
  `SetVector()` copy the bytes of a complete builder or of a table's bytes.
  `SetVector()` takes `NestedTable`s, which convert implicitly from a
  `TableBuilder`, a `byte[]` or a `ReadOnlyMemory<byte>`, so a vector can mix
  them. Their overloads that take a `fill` record a nested table, or each item
  of a vector, in place instead, as the next section describes.
- **A reader is a view of the bytes.** `TableReader` is a `ref struct` over a
  `ReadOnlySpan<byte>`, which can be an array's, which converts to it, a
  slice's, a `ReadOnlyMemory<byte>`'s `Span`, or stack or native memory. It
  lives on the stack, as a span does: it can't be stored in a class, captured
  by a lambda or kept across an `await`. Code that keeps a table keeps its
  bytes, and creates a reader, which is cheap, where it reads them. Holding a
  span saves each read the conversion of a `ReadOnlyMemory<byte>` to one.
- **Arrays are read in place.** The array getters return a `ReadOnlySpan<T>`
  of the table's bytes, at any alignment, where the host reads them so: on a
  little-endian host other than 32-bit Arm, such as x64 and Arm64. A span in
  place shows writes made to the bytes later, so copy it with `ToArray()` to
  keep the values. Elsewhere the getters return a span of a copy, as the
  section on byte orders describes.
- **Bools are copied.** An array of bools is a `uint8` array of 1 and 0,
  which `SetBoolArray()` writes from a span of bools, and `GetBoolArray()`
  reads into a new `bool[]`, true where a byte isn't 0, since a .NET bool of
  a byte other than 1 or 0 misbehaves. `GetUInt8Array()` reads its bytes in
  place.
- **Structs and vector items share the bytes too.** `GetStruct()` and
  `GetStructArray()` return a `ReadOnlySpan<byte>` of the structs, whose
  scalars `BinaryPrimitives` reads at their offsets. A `Vector` and its
  enumerator are `ref struct`s too, and its items are `ReadOnlySpan<byte>`s,
  which a `TableReader` of the item's layout reads. To nest an item in
  another vector, copy it with `ToArray()`, since a `NestedTable` holds
  memory rather than a span.
- **Strings are decoded.** `GetString()` checks that the bytes are valid
  UTF-8 and decodes them into a new `string`, in one pass.

## Recording nested tables in place

`SetTable(fields, fill)` records a nested table straight among its parent's
bytes, rather than from a `TableBuilder` of its own that's then copied, and
`SetVector(count, fields, fill)` records each item of a vector so. `fill`
sets the table's fields, in order, on the `TableBuilder.Item` that it's
given, whose setters are the builder's, and can record the tables nested in
it in place too. The bytes are those that nesting builders of the same fields
writes, so the example's table can be written so:

```csharp
var builder = new TableBuilder(3, Layout);
builder.SetFloat64(59.3293);
builder.SetString("Europe/Stockholm");
builder.SetTable(2, static series =>
{
    series.SetUInt32Array([0, 86_400]);
    series.SetFloat32Array([21.75f, 18.5f]);
});
byte[] bytes = builder.ToArray();
```

A lambda that captures variables allocates whenever it's created, so the
overloads that take a `state` pass it to a `static` lambda instead, which
allocates nothing. The vector's `fill` is given each item's index:

```csharp
(uint Time, float Value)[] readings = [(0, 21.75f), (86_400, 18.5f)];

var list = new TableBuilder(1);
list.SetVector(readings.Length, 2, readings, static (item, index, readings) =>
{
    item.SetUInt32(readings[index].Time);
    item.SetFloat32(readings[index].Value);
});
```

The item counts the table's own fields and length, as a builder of it would,
so its setters throw as that builder's would. A table recorded in place has
no header and its parent's offset type, and is padded as `SetTable()` pads a
builder, to the largest alignment of its arrays and nested tables. That's
known once its fields are set, so a table that needs padding is moved by those
few bytes once `fill` returns. A vector's items aren't padded, so they're
never moved.

`TableBuilder.Item` is a `ref struct`, valid only while `fill` runs, which
keeps a lambda from capturing it. The builder is used only through it
meanwhile: `ToArray()` and nesting the builder throw
`InvalidOperationException` until `fill` returns. Whatever `fill` throws is
passed on, and the builder is left as it was, as it is when the errors below
are thrown.

## Writing tables of known fields

Code that knows a table's fields before it writes them, such as the code
that tafla-gen generates, counts the table's bytes
with a `TableSizer` and then writes it with the sizer's `TableWriter`, field
by field, into bytes of exactly its length. The writer writes nested tables
and vector items in place among their parent's fields, rather than copying
them, so the table is the one allocation, and with
`TableSizer.CreateWriter(byte[])` it writes into the start of bytes that the
caller reuses, which it leaves as they were after the table. The bytes are
those that a `TableBuilder` of the same fields writes:

```csharp
var sizer = new TableSizer(3, Layout);
sizer.AddFloat64();
sizer.AddString("Europe/Stockholm");
sizer.AddTable(4); // aligned for its arrays of 4-byte elements
var nested = sizer.StartTable(2);
sizer.AddUInt32Array(2);
sizer.AddFloat32Array(2);
sizer.EndTable(nested);

var writer = sizer.CreateWriter();
writer.SetFloat64(0, 59.3293);
writer.SetString(1, "Europe/Stockholm");
writer.SetTable(2, 4);
var series = writer.StartTable(2);
writer.SetUInt32Array(0, [0, 86_400]);
writer.SetFloat32Array(1, [21.75f, 18.5f]);
writer.EndTable(series);
byte[] written = writer.Bytes; // the builder's bytes, of the table's length
```

The sizer adds the fields in order, each with what decides its length, and
the writer takes the same fields in the same order, each with its index in
its table. A nested table's fields follow its parent's `AddTable()` and
`SetTable()`, between `StartTable()` and `EndTable()`, and a vector's items
follow its `AddVector()` and `SetVector()` the same way, each followed by the
writer's `SetItemEnd()`. `ReserveStruct()` and `ReserveStructArray()` return
the bytes of structs for the caller to write in place.

A table whose fields' sizes are fixed, of scalars, structs and such tables,
has a fixed length, and code that lays it out itself, as tafla-gen's does,
writes it whole rather than field by field. `AddTableBytes()` adds tables of
one length in place of their fields, every item of a vector of them at once,
and `ReserveTableBytes()` returns each one's bytes, for the caller to write
its offsets and its fields at the positions that it works out. A message of
such fields, and a string after them, is written whole without a sizer and a
writer, into the bytes that `TableWriter.CreateTableBytes()` returns with
its header, after `TableSizer.Utf8Length()` counts the string. On an Apple M2
Pro with .NET 10, tafla-gen's code wrote a vector of 100 tables of a
`float32` and a nested table of two `float32`s in 0.31 of the time that
writing each field took, and a table of three 8-byte scalars and a 16-byte
string in 0.67 of the time.

The writer's calls follow from a schema, so it doesn't check them again: a
field index outside its table, or an item outside its vector, leaves the
table's bytes undefined, though every write stays within the writer's bytes.
Fields that differ from those that the sizer counted, in their lengths or
number, never write past the table, even in reused bytes longer than it.
Hand-written tables use `TableBuilder`, which checks every field. Both are
mutable structs, so that sizing and writing a table allocates only its
bytes: use them as locals, or pass them by `ref`.

## Errors

Every read of a `TableReader` or a `Vector` checks its range against the bytes
that it was given, and throws a `DecodeException` if it would leave them,
whose `Kind` says why and whose properties describe the read:

- `OutOfRange`: a read of `ByteLength` bytes at `Position` that would leave
  the `Available` bytes. An array's count is checked against the bytes
  before anything is allocated for it. An offset past the largest `int`
  leads past any bytes, since no span is that long, and fails when it's
  read, as a position can't hold it. Code that reads bytes without a
  `TableReader`, as tafla-gen's structs read theirs, throws
  `DecodeException.OutOfRange()`.
- `InvalidUtf8`: a string that isn't valid UTF-8, such as one with an
  overlong encoding or a surrogate.
- `StringTooLong`: a string of `ByteLength` bytes that hold more characters
  than a .NET `string` can, 1,073,741,791. A table of `UInt32` offsets can
  hold a longer string, so the bytes may be valid, but .NET can't decode it.
  Only a string of more bytes than that can fail so, and its characters are
  counted before anything is allocated for it.
- `ItemEndsBeforeStart`: a vector item that ends before the item before it.
- `UnsupportedVersion`: `TableReader` doesn't compare versions. A reader
  throws `DecodeException.UnsupportedVersion()` when `GetVersion()` isn't its
  own, as the example does, so that every failure to decode is a
  `DecodeException`.

No bytes make a read throw any other exception: not an
`IndexOutOfRangeException`, an `ArgumentException`, an `OverflowException`
nor an `OutOfMemoryException`, as the fuzz targets check.

`TableBuilder` throws a `BuildException` when it can't write a table, whatever
the values, and leaves the builder as it was:

- A constructor throws `VersionTooLarge` for a version that doesn't fit in
  the header's slot, and `HeaderTooLong` for a header too long for the offset
  type.
- A setter throws `TooManyFields` for a field beyond the number the table was
  created with, and `TooLong` for one that would make the table too long for
  its offset type, with the length that it would make it, before writing any
  of it. `SetStructArray()` throws `NotWholeStructs` for bytes that aren't a
  whole number of structs.
- `ToArray()`, and nesting a builder, throw `Incomplete` while fields are left
  to set.
- `SetTable(fields, fill)` and `SetVector(count, fields, fill)` throw
  `TooManyFields` as the other setters do, and `HeaderTooLong` when the
  nested table's offsets alone are too long for the offset type, before
  `fill` runs. They throw `Incomplete` when `fill` leaves fields of the
  nested table unset, and `TooLong` when the table, with its padding, makes
  its parent too long. A vector checks its count, its items' ends and the
  offsets that every item holds before calling `fill`, so a vector that they
  alone make too long throws `TooLong` first, with the length that they make.
  The item's setters throw as the builder's do, for the nested table's own
  fields and length, and so do their tables recorded in place.

`TableSizer`'s constructors check the version and the header as
`TableBuilder`'s do, and `GetByteLength()` and `CreateWriter()` throw
`TooLong` for a table too long for its offset type, before anything is
allocated, with the table's length and its last field's index.
`TableWriter.SetStructArray()` throws `NotWholeStructs` as the builder does.
A writer whose fields differ from those that its sizer counted throws
`NotAsSized`, with the bytes that the fields wrote, `Written`, and the
table's `Length`: a field that would end past the table throws it without
writing past it, even in longer bytes, after which the writer mustn't be
used, and `Bytes` and `ByteLength` throw it when the fields didn't fill the
table.

Mistakes of the code that calls the runtime, which no bytes or values cause,
throw the argument exceptions of .NET's own APIs:

| API                                             | Throws                                                                                    |
| ----------------------------------------------- | ----------------------------------------------------------------------------------------- |
| `RootOffset()`, `FieldOffset()`                 | `ArgumentOutOfRangeException` for a negative field index                                  |
| `GetStruct()`, `GetStructArray()`               | `ArgumentOutOfRangeException` for a negative size, or a size of 0 of an array of structs  |
| `GetVersion()`                                  | `InvalidOperationException` for a layout without a version                                |
| `Vector.Get()`, `Vector.GetInPlace()`           | `ArgumentOutOfRangeException` for an index that isn't an item's                           |
| `TableLayout.OffsetType`                        | `ArgumentOutOfRangeException` for a value that isn't an offset type                       |
| `new TableBuilder()`                            | `ArgumentOutOfRangeException` for a negative number of fields |
| `SetTable()`, `SetStructArray()`                | `ArgumentOutOfRangeException` for an alignment other than 1, 2, 4 or 8, or a struct size below 1 |
| `SetString()`, `SetTable()`, `NestedTable`      | `ArgumentNullException` for a null string, builder or array                               |
| `SetTable()` and `SetVector()` with a `fill`    | `ArgumentOutOfRangeException` for a negative number of fields or count, `ArgumentNullException` for a null `fill`, and while `fill` runs, `ToArray()` and nesting the builder throw `InvalidOperationException` |
| `new TableSizer()`                              | As `new TableBuilder()`                                                                   |
| `TableSizer`'s and `TableWriter`'s methods      | `ArgumentOutOfRangeException` for a negative count, number of fields or struct size, a size of 0 of an array of structs, or an alignment other than 1, 2, 4 or 8, and `ArgumentNullException` for a null string |
| `TableWriter`'s field indices, `SetItemEnd()`   | `ArgumentOutOfRangeException` for an offset or item end that lies outside the writer's bytes. Within them, it writes over another slot or field |
| `TableSizer.CreateWriter(byte[])`               | `ArgumentException` for bytes shorter than the table, before any of them is written      |

## Conversions

Each setter takes its field's own C# type, `sbyte`, `byte`, `short`,
`ushort`, `int`, `uint`, `long`, `ulong`, `float` or `double`, so the caller
converts a value to it, as C# converts:

| Value                                  | Written as                                                                                                     |
| -------------------------------------- | -------------------------------------------------------------------------------------------------------------- |
| An integer outside its field's range   | A cast keeps the low bits in an unchecked context, C#'s default: `unchecked((byte)300)` is 44. A checked one throws an `OverflowException`, and a constant out of range doesn't compile |
| A fraction or NaN for an integer field | A cast truncates toward zero, and saturates at the type's limits, with NaN as 0, since .NET 9                  |
| A `double` too large for a `float`     | `(float)` makes it infinity of its sign, and rounds other values to the nearest `float`                        |
| A lone surrogate in a string           | U+FFFD, as `Encoding.UTF8` writes it, which TypeScript and Java write too                                      |
| A bool of a byte other than 1 or 0     | Its byte, which readers read as true. C# makes every bool 1 or 0, which `SetBoolArray()` copies at once; only unsafe code or interop makes another |

Floats keep all their bits, a NaN's payload too, in both directions.

## Byte orders and 32-bit hosts

On a little-endian host, arrays are copied into a table in bulk and read in
place. A big-endian host, which .NET runs on as s390x, copies and swaps the
bytes of each element instead, and 32-bit Arm copies the elements that it
reads, since it faults on some unaligned loads of floats and 64-bit integers,
and a byte array's elements are aligned only to 4 bytes there, and may move
to another multiple of 4 when the garbage collector compacts the heap.

Building with `-p:TaflaPortable=true` copies every array that the runtime
reads on any host, so that `dotnet test -p:TaflaPortable=true` tests that
path, without the byte swap, which only a big-endian host runs. CI runs the
tests on s390x as well, emulated by QEMU, so that the byte swap is tested
too, as the Development section describes.

32-bit x86 returns floats on the x87 stack, whose loads quiet a signalling
NaN, so `GetFloat32()` and `GetFloat64()` may return one quieted there, as
may any method that returns a float, such as `BitConverter.UInt32BitsToSingle()`
in unoptimized code. A float passed to a method keeps every bit, so the
setters write every bit of the floats that they're given, and the bytes and
the spans of arrays keep them too: read a signalling NaN's bits with
`GetUInt32()` or `GetUInt64()`, or from an array. The tests make their floats
from bits in memory, and accept a quieted signalling NaN only from the scalar
getters, and only in a 32-bit x86 process.

CI runs the tests in a 32-bit process on both: on x86 with Microsoft's x86
SDK on Windows, and on 32-bit Arm natively, on GitHub's Arm runners, as the
Development section describes. A 32-bit process skips one test, of a string
longer than a .NET string holds, which needs 1 GiB of it.

## Development

The runtime targets .NET 10, the current long-term support release, with
C# 14, and `global.json` asks for its SDK, 10.0.100 or a later feature
band. Run the commands below from this directory:

```sh
dotnet format --verify-no-changes
dotnet format whitespace --folder --include-generated --verify-no-changes --include tests/Tafla.Generated/ tests/names/ benchmarks/Tafla.Benchmarks/Generated/TaflaGen/
dotnet build
dotnet test
dotnet test -p:TaflaPortable=true
```

`dotnet format` checks the formatting and code style of `.editorconfig`, and
the build treats every warning as an error, with .NET's recommended
analyzers, the code style and the trimming and AOT analyzers, and requires
documentation of every public API. `dotnet test` runs the tests with
Microsoft.Testing.Platform, which `global.json` selects, as xUnit v3 needs,
and the benchmarks' check that every codec reads back what it wrote. The
tests' packages, xUnit v3 and CsCheck, the fuzz targets', SharpFuzz, and the
libraries that the benchmarks compare tafla with, are pinned in
`Directory.Packages.props` and locked in each project's `packages.lock.json`,
which CI restores in locked mode. The runtime and the library of generated
code depend on no package, and have no lock file: the SDK adds its trimming
analyzers' package to them, at the SDK's own version, which a lock file would
tie to one SDK. The build writes
everything under `artifacts/`.

```sh
dotnet pack src/Tafla
scripts/test-package.sh
```

`dotnet pack` packs the runtime into `artifacts/package/release/`, with its
symbols in a `.snupkg`, as the release publishes it, and
`scripts/test-package.sh` tests the package as a project that uses it
would: `tests/package` restores it from there and runs, and then runs
compiled ahead of time with Native AOT, whose analysis of the trimmed package
fails on any warning. Native AOT needs a C toolchain, and its packages come
from nuget.org. Homebrew's .NET SDK, which is built from source, links
executables of Native AOT with OpenSSL and Brotli, which
`LIBRARY_PATH="$(brew --prefix openssl@3)/lib:$(brew --prefix brotli)/lib"`
lets the linker find.

```sh
host=$(scripts/build-gen.sh)
dotnet pack src/Tafla
dotnet pack src/Tafla.Gen -p:TaflaGenHosts=$host
scripts/test-gen-package.sh
```

`src/Tafla.Gen` packs tafla-gen for .NET projects: the tafla-gen command of
each host, and the MSBuild targets in `src/Tafla.Gen/build` that generate a
project's schemas' code with the host's. It builds no assembly, so it's
outside `Tafla.slnx`. `scripts/build-gen.sh` builds this host's command from
`rust/gen`, with cargo, into `artifacts/tafla-gen/`, and prints the host's
name, and `-p:TaflaGenHosts` packs that host's command alone, where the
release packs every host's. `scripts/test-gen-package.sh` tests the package
as a project that uses it would: `tests/gen-package` restores it and the
runtime's package, generates the code of an example schema and of a schema of
its own as it builds, and runs it. The script then checks that a build that
changes nothing doesn't generate again, and that a schema's error fails the
build.

The byte swap of a big-endian host runs only there. Microsoft builds no .NET
for s390x, but Red Hat does, so CI runs the tests that `dotnet build` built,
which are IL, in Red Hat's container of the .NET 10 runtime for s390x,
emulated by QEMU. Docker Desktop emulates it already, and Docker on Linux
once `docker run --privileged --rm tonistiigi/binfmt --install s390x` has
registered QEMU:

```sh
docker run --rm --platform linux/s390x -v "$PWD/..:/repo:ro" -w /repo/csharp \
    registry.access.redhat.com/ubi9/dotnet-100-runtime \
    dotnet artifacts/bin/Tafla.Tests/debug/Tafla.Tests.dll
```

CI runs the tests in a 32-bit process too. On Windows, Microsoft's x86 SDK
builds and runs them as x86, as `dotnet test` from its directory does,
`C:\Program Files (x86)\dotnet`. 32-bit Arm runs the same IL in Microsoft's
container of the .NET 10 runtime for it, on a 64-bit Arm host that runs
32-bit Arm natively, as GitHub's Arm runners do. Apple silicon doesn't, and
QEMU's emulation of 32-bit Arm crashes .NET's runtime, in Docker Desktop and
in QEMU 10.0 alike:

```sh
docker run --rm --platform linux/arm/v7 -v "$PWD/..:/repo:ro" -w /repo/csharp \
    mcr.microsoft.com/dotnet/runtime:10.0 \
    dotnet artifacts/bin/Tafla.Tests/debug/Tafla.Tests.dll
```

The tests in `tests/Tafla.Tests` are:

- `ConformanceTests`: the conformance suite in `conformance/`: valid tables
  read back their values and rebuild byte for byte, equivalent tables read
  their valid case's values, and invalid tables fail with a
  `DecodeException` at their field.
- `RoundTripTests`, `FormatTests`, `DecodeExceptionTests`,
  `TableBuilderTests` and `TableWriterTests`: properties of tables built,
  and sized and written, from the conformance
  suite's descriptions, which `Tables/` parses, generates with CsCheck, builds
  and reads, as Go's `tests/internal/tables` does, and examples of the errors
  and of every boundary: signed zero, NaN, integer limits, empty arrays, the
  limits of each offset type, strings outside the Basic Multilingual Plane
  and unaligned bytes.
- `ExampleTests`: the examples above.
- `Generated/`: the code that tafla-gen generates from the example schemas
  at the root of the repository, in `tests/Tafla.Generated`, writes what
  `TableBuilder` writes of the same fields, reads them back, writes into the
  caller's bytes as into new ones, reads corrupted and arbitrary bytes as
  values or a `DecodeException`, and writes and reads the cases of
  `conformance/generated` byte for byte, as every language's does.
  `NamesTests` runs the code of `tests/names.tafla`, of names that would
  clash in C#.

The property tests run 256 cases each, on every core, and print the seed of
a failing case, which `CsCheck_Seed=<seed> dotnet test` runs again.

### Fuzzing

`fuzz/Tafla.Fuzz` holds the fuzz targets, which mirror `rust/fuzz`'s, and
which libFuzzer runs through [SharpFuzz](https://github.com/Metalnem/sharpfuzz):
SharpFuzz instruments the IL of `Tafla.dll` to record the branches that its
reads take, and [libfuzzer-dotnet](https://github.com/Metalnem/libfuzzer-dotnet)
runs the targets' process for libFuzzer and passes it each input. An exception
that escapes a target is a finding, as are a hang and a run out of memory:

- `table_reader` reads the rest of its input with every getter of
  `TableReader` at a position, in an offset type and header, which the
  input's first four bytes choose, and reads every array, struct array and
  vector there. Each read must return the value or bytes that lie where the
  format puts them, exactly when they lie within the input, and otherwise
  throw a `DecodeException` that describes the very read that leaves it.
  Arrays must be spans of the input where the runtime reads them in place,
  and copies outside it where it doesn't. Any other exception, such as an
  `ArgumentOutOfRangeException` or an `IndexOutOfRangeException`, is a
  finding, since the target makes none of the mistakes of a caller that
  those are for. So is a read that allocates more than the string or copy
  that it returns, or, when it throws, more than 4 KiB for its exception,
  where the most measured was 1,672 bytes. Rust's target reads at every
  position of every input instead, which C++'s doesn't either: a .NET
  exception takes about 3 µs to throw and catch, and most reads of arbitrary
  bytes throw.
- `conformance` reads its input as every valid case of the conformance suite
  describes it, with the conformance test's descriptions and reader in
  `tests/Tafla.Tests/Tables/`. A case's own bytes must read as it describes
  them, and a vector's items read in order must agree with its count and
  with those found by index, or the reader throws an
  `InconsistentReadException`, which escapes.

The targets' process runs out of memory at 2 GiB of heap, as libFuzzer's
default `-rss_limit_mb` bounds the other languages' targets, since libFuzzer
can't bound the memory of the process that libfuzzer-dotnet runs.

libfuzzer-dotnet runs on Linux. `fuzz/fuzz.sh` builds it from its source at a
pinned commit with Clang's libFuzzer, builds the targets, instruments the
runtime, and runs a target, with libFuzzer's options, from the corpus that
its earlier runs grew in `artifacts/fuzz/corpus/` and the conformance
suite's tables. It needs the .NET 10 SDK, Clang and Clang's runtime of
libFuzzer, `libclang-rt-<version>-dev` on Debian and Ubuntu, curl and
`sha256sum`, and the network, the first time, for libfuzzer-dotnet's source
and the targets' packages. `TAFLA_PORTABLE=true` builds the runtime to copy
every array that it reads, as `-p:TaflaPortable=true` does, so that the
copies are fuzzed too:

```sh
fuzz/fuzz.sh table_reader -max_total_time=300 -timeout=10
fuzz/fuzz.sh conformance -max_total_time=300 -timeout=10
TAFLA_PORTABLE=true fuzz/fuzz.sh table_reader -max_total_time=300 -timeout=10
```

Elsewhere, Microsoft's container of the .NET 10 SDK, which is Ubuntu, runs
it:

```sh
docker run --rm -v "$PWD/..:/repo" -w /repo/csharp mcr.microsoft.com/dotnet/sdk:10.0 sh -c '
    apt-get update && apt-get install --yes --no-install-recommends clang &&
    apt-get install --yes --no-install-recommends "libclang-rt-$(clang++ -dumpversion | cut -d. -f1)-dev" &&
    fuzz/fuzz.sh table_reader -max_total_time=300 -timeout=10'
```

libFuzzer writes an input that fails to `artifacts/fuzz/<target>/`. The
targets read files too, on any host, without libFuzzer, as
`dotnet artifacts/bin/Tafla.Fuzz/debug/Tafla.Fuzz.dll table_reader <file>`
does after `dotnet build`, and a finding becomes a regression test in
`tests/`. The C# workflow runs each target for a minute, and `table_reader`
again with arrays copied, and the fuzz workflow runs each target with arrays
in place and copied for five minutes every week, from the corpus that its
earlier runs grew, uploading what fails.

## Generated code

tafla-gen's `csharp` target generates a file of C# from a schema, whose
readers read its messages and tables through a `TableReader`, and whose
fields write them with a `TableSizer` and a `TableWriter`, as
[its README](../rust/gen/README.md) describes. The code that it generates
from the example schemas at the root of the repository is its golden output,
which `rust/gen/tests/golden.rs` keeps as it generates it: regenerate it with
`TAFLA_GEN_UPDATE=1 cargo test -p tafla-gen` from `rust/`, rather than edit
it. It lies in `tests/Tafla.Generated`, a library of its own, which compiles
it as a user's project would, without implicit usings, and with the
analyzers of trimming and ahead-of-time compilation. Its
`<auto-generated>` header keeps a project's analyzers and `dotnet format`
off it; here `.editorconfig` turns the build's analyzers back on for it, and
`dotnet format whitespace --folder --include-generated` checks its
formatting, which plain `dotnet format` skips. `tests/names` holds the code
of `tests/names.tafla`, which `rust/gen/tests/csharp_schemas.rs` keeps.

Not yet part of the runtime: the writer's reservation of a table's leading
fields of fixed sizes among its others, as Go's `ReserveFields()` does, which
wrote the record above, with a sizer and a writer, in 0.89 of the time of
each field's call in a prototype, where writing it whole took 0.56 there.
