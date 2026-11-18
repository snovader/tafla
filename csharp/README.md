# tafla for C#

`csharp/` is tafla's runtime for .NET, the namespace `Tafla` in `Tafla.dll`,
for .NET 10. It writes and reads binary tables of scalars, strings, arrays,
structs and nested tables in tafla's format, which the TypeScript, Python,
Java, Rust, Go and C++ runtimes write and read too. `TableBuilder` writes a
table, copying arrays in bulk, and `ByteBuffer` reads fields back by offset,
with arrays, structs and vector items as spans of the bytes. Every read
checks that it stays within the bytes it was given. It depends only on
.NET's base class library, and is marked compatible with trimming and
ahead-of-time compilation, whose analyzers its build runs.

```csharp
using Tafla;

// A format's header, shared by its writer and its readers
static readonly TableLayout Layout = new() { Sized = true, Versioned = true };
const uint FormatVersion = 1;

var series = new TableBuilder(2);
series.SetUInt32Array([0, 86_400]); // copied when it's set
series.SetFloat32Array([21.75f, 18.5f]);

var builder = new TableBuilder(3, Layout, FormatVersion);
builder.SetFloat64(59.3293);
builder.SetString("Europe/Stockholm");
builder.SetTable(series); // copied into the table, aligned for its arrays
byte[] bytes = builder.ToArray();

var buffer = new ByteBuffer(bytes, Layout);
if (buffer.GetVersion() != FormatVersion)
{
    throw DecodeException.UnsupportedVersion(buffer.GetVersion(), FormatVersion);
}

double latitude = buffer.GetFloat64(buffer.RootOffset(0));
string timeZone = buffer.GetString(buffer.RootOffset(1)); // a new string
int table = buffer.RootOffset(2);
ReadOnlySpan<float> values = buffer.GetFloat32Array(buffer.FieldOffset(table, 1)); // in place
```

`tests/Tafla.Tests/ExampleTests.cs` runs the example. The names are the
other runtimes' in C#'s case: table, field, layout (`TableLayout`), offset
type (`OffsetType`) and offset size, vector, builder (`TableBuilder`) and
decode error (`DecodeException`, as Java names it).

The runtime isn't published to NuGet yet, and has no version until it is.

## Format

All values are little-endian. A table is its header, one offset per field
and then the fields in the order they were set:

- The header holds the table's byte length when the layout is `Sized`, and
  then its version when it's `Versioned`.
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
- **Nested tables are copied.** `SetTable()` and `SetVector()` copy the bytes
  of a complete builder or of a table's bytes. `SetVector()` takes
  `NestedTable`s, which convert implicitly from a `TableBuilder`, a `byte[]`
  or a `ReadOnlyMemory<byte>`, so a vector can mix them.
- **Arrays are read in place.** `ByteBuffer` holds a `ReadOnlyMemory<byte>`,
  and its array getters return a `ReadOnlySpan<T>` of the table's bytes, at
  any alignment, where the host reads them so: on a little-endian host other
  than 32-bit Arm, such as x64 and Arm64. A span in place shows writes made
  to the bytes later, so copy it with `ToArray()` to keep the values.
  Elsewhere the getters return a span of a copy, as the section on byte
  orders describes.
- **Structs and vector items share the bytes too.** `GetStruct()` and
  `GetStructArray()` return a `ReadOnlySpan<byte>` of the structs, whose
  scalars `BinaryPrimitives` reads at their offsets, and a `Vector`'s items
  are `ReadOnlyMemory<byte>`, which a `ByteBuffer` of the item's layout reads.
- **Strings are decoded.** `GetString()` checks that the bytes are valid
  UTF-8 and decodes them into a new `string`, in one pass.

## Errors

Every read of a `ByteBuffer` or a `Vector` checks its range against the bytes
that it was given, and throws a `DecodeException` if it would leave them,
whose `Kind` says why and whose properties describe the read:

- `OutOfRange`: a read of `ByteLength` bytes at `Position` that would leave
  the `Available` bytes. An array's count is checked against the bytes
  before anything is allocated for it. An offset past the largest `int`
  leads past any bytes, which a .NET array can't hold, and fails when it's
  read, since a position can't hold it.
- `InvalidUtf8`: a string that isn't valid UTF-8, such as one with an
  overlong encoding or a surrogate.
- `ItemEndsBeforeStart`: a vector item that ends before the item before it.
- `UnsupportedVersion`: `ByteBuffer` doesn't compare versions. A reader
  throws `DecodeException.UnsupportedVersion()` when `GetVersion()` isn't its
  own, as the example does, so that every failure to decode is a
  `DecodeException`.

No bytes make a read throw any other exception: not an
`IndexOutOfRangeException`, an `ArgumentException`, an `OverflowException`
nor an `OutOfMemoryException`.

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

Mistakes of the code that calls the runtime, which no bytes or values cause,
throw the argument exceptions of .NET's own APIs:

| API                                             | Throws                                                                                    |
| ----------------------------------------------- | ----------------------------------------------------------------------------------------- |
| `RootOffset()`, `FieldOffset()`                 | `ArgumentOutOfRangeException` for a negative field index                                  |
| `GetStruct()`, `GetStructArray()`               | `ArgumentOutOfRangeException` for a negative size, or a size of 0 of an array of structs  |
| `GetVersion()`                                  | `InvalidOperationException` for a layout without a version                                |
| `Vector.Get()`, `Vector.GetInPlace()`           | `ArgumentOutOfRangeException` for an index that isn't an item's                           |
| `TableLayout.OffsetType`                        | `ArgumentOutOfRangeException` for a value that isn't an offset type                       |
| `new TableBuilder()`                            | `ArgumentOutOfRangeException` for a negative number of fields, and `ArgumentException` for a versioned layout without a version, or a version without one |
| `SetTable()`, `SetStructArray()`                | `ArgumentOutOfRangeException` for an alignment other than 1, 2, 4 or 8, or a struct size below 1 |
| `SetString()`, `SetTable()`, `NestedTable`      | `ArgumentNullException` for a null string, builder or array                               |

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
path, without the byte swap, which only a big-endian host runs.

32-bit x86 returns floats on the x87 stack, whose loads may quiet a
signalling NaN, so `GetFloat32()` and `GetFloat64()` may return one quieted
there. The bytes themselves, and the spans of arrays, keep every bit.

## Development

The runtime targets .NET 10, the current long-term support release, with
C# 14, and `global.json` asks for its SDK, 10.0.100 or a later feature
band. Run the commands below from this directory:

```sh
dotnet format --verify-no-changes
dotnet build
dotnet test
dotnet test -p:TaflaPortable=true
```

`dotnet format` checks the formatting and code style of `.editorconfig`, and
the build treats every warning as an error, with .NET's recommended
analyzers, the code style and the trimming and AOT analyzers, and requires
documentation of every public API. `dotnet test` runs the tests with
Microsoft.Testing.Platform, which `global.json` selects, as xUnit v3 needs.
The tests' packages, xUnit v3 and CsCheck, are pinned in
`Directory.Packages.props` and locked in `packages.lock.json`, which CI
restores in locked mode. The build writes everything under `artifacts/`.

The tests in `tests/Tafla.Tests` are:

- `ConformanceTests`: the conformance suite in `conformance/`: valid tables
  read back their values and rebuild byte for byte, equivalent tables read
  their valid case's values, and invalid tables fail with a
  `DecodeException` at their field.
- `RoundTripTests`, `FormatTests`, `DecodeExceptionTests` and
  `TableBuilderTests`: properties of tables built from the conformance
  suite's descriptions, which `Tables/` parses, generates with CsCheck, builds
  and reads, as Go's `tests/internal/tables` does, and examples of the errors
  and of every boundary: signed zero, NaN, integer limits, empty arrays, the
  limits of each offset type, strings outside the Basic Multilingual Plane
  and unaligned bytes.
- `ExampleTests`: the example above.

The property tests run 256 cases each, on every core, and print the seed of
a failing case, which `CsCheck_Seed=<seed> dotnet test` runs again.

Not yet part of the runtime: `TableSizer` and `TableWriter`, which the code
that tafla-gen generates writes tables with in the other languages, and
recording nested tables in place, as their `SetTableFunc()` does.
