# Tafla

tafla's runtime for .NET: binary tables of scalars, strings, arrays, structs
and nested tables, in tafla's format, which its runtimes for TypeScript,
Python, Java, Rust, Go, C++ and Swift write and read too. `TableBuilder`
writes a table, copying arrays in bulk, and `TableReader` reads fields back by
offset from a span of any memory, with arrays, structs and vector items as
spans of the bytes. Every read checks that it stays within the bytes it was
given, and fails with a `DecodeException` if it would leave them, so tables
can come from untrusted sources.

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

`TableBuilder.SetTable(fields, fill)` and `SetVector(count, fields, fill)`
record a nested table, or each item of a vector, in place among its
parent's fields, rather than from a builder of its own that's then copied.

`TableSizer` and `TableWriter` write a table whose fields are known in
advance, as the code that tafla-gen generates in the other languages does,
into bytes of its exact length, with nested tables and vector items written
in place among their parent's fields.

The package is for .NET 10. It depends only on .NET's base class library,
and is trimmable and compatible with Native AOT.
[The runtime's README](https://github.com/snovader/tafla/tree/main/csharp#readme)
describes the format, which values are copied and which are shared with the
bytes, the errors, and how values convert to their fields' types,
[SPEC.md](https://github.com/snovader/tafla/blob/main/SPEC.md) specifies the
format, and
[the changelog](https://github.com/snovader/tafla/blob/main/csharp/CHANGELOG.md)
lists each release's changes.
