// Writes and reads a table with the package, compiled just in time by dotnet run and ahead of time
// by dotnet publish, as scripts/test-package.sh runs it

using System.Runtime;
using Tafla;

var layout = new TableLayout { Sized = true };

var series = new TableBuilder(2);
series.SetString("installed");
series.SetFloat32Array([21.75f, 18.5f]);

var builder = new TableBuilder(1, layout);
builder.SetTable(series);
byte[] bytes = builder.ToArray();

var reader = new TableReader(bytes, layout);
int table = reader.RootOffset(0);
string name = reader.GetString(reader.FieldOffset(table, 0));
float[] values = reader.GetFloat32Array(reader.FieldOffset(table, 1)).ToArray();

if (name != "installed" || values is not [21.75f, 18.5f])
{
    Console.Error.WriteLine($"Read back \"{name}\" and [{string.Join(", ", values)}]");
    return 1;
}

// Bytes cut short fail with the package's decode error
try
{
    var truncated = new TableReader(bytes.AsMemory(0, bytes.Length - 1), layout);
    truncated.GetFloat32Array(truncated.FieldOffset(truncated.RootOffset(0), 1));
    Console.Error.WriteLine("Read an array past the end of the bytes");
    return 1;
}
catch (DecodeException exception) when (exception.Kind == DecodeErrorKind.OutOfRange)
{
}

// Native AOT compiles no method at run time
string compiled = JitInfo.GetCompiledMethodCount() > 0 ? "just in time" : "ahead of time";
Console.WriteLine($"Tafla wrote and read a table, compiled {compiled}");
return 0;
