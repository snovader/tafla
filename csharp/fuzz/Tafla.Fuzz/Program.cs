using System.Reflection.Metadata;
using System.Reflection.PortableExecutable;
using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;
using SharpFuzz;

namespace Tafla.Fuzz;

/// <summary>
/// Runs a fuzz target, <c>table_reader</c> or <c>conformance</c>: under libFuzzer, which
/// libfuzzer-dotnet runs this program for and passes the target's name to, or on the files named
/// after it, to run its findings again. <c>instrument</c> instruments the runtime for libFuzzer.
/// </summary>
/// <remarks>
/// SharpFuzz rewrites the IL of <c>Tafla.dll</c> in this program's directory, so that each branch
/// that it takes records itself in the memory that libfuzzer-dotnet shares with the program. It
/// must run before the runtime is loaded, so <see cref="Main"/> and <see cref="Instrument"/> use no
/// type of it, and the build copies the runtime there anew, uninstrumented.
/// </remarks>
internal static class Program
{
    private const string Usage = """
        Usage: Tafla.Fuzz instrument
               Tafla.Fuzz <target> [file ...]

        Targets: table_reader, conformance. Under libfuzzer-dotnet, a target reads the inputs that
        libFuzzer gives it, once Tafla.dll is instrumented. Otherwise it reads each file.
        """;

    /// <summary>The variable that libfuzzer-dotnet sets for the process that it runs</summary>
    private const string SharedMemoryVariable = "__LIBFUZZER_SHM_ID";

    /// <summary>The bytes of the map of branches that SharpFuzz's instrumentation records in</summary>
    private const int BranchMapSize = 1 << 16;

    /// <summary>The assembly that SharpFuzz makes an instrumented assembly reference</summary>
    private const string TraceAssembly = "SharpFuzz.Common";

    public static int Main(string[] args) => args is ["instrument"] ? Instrument() : Run(args);

    /// <summary>Instruments the runtime next to the program, unless it is already</summary>
    [MethodImpl(MethodImplOptions.NoInlining)]
    private static int Instrument()
    {
        var path = Path.Combine(AppContext.BaseDirectory, "Tafla.dll");
        if (IsInstrumented(path))
        {
            Console.WriteLine($"{path} is instrumented already");
            return 0;
        }

        var types = Fuzzer.Instrument(path, _ => true, Options.Value);
        Console.WriteLine($"Instrumented {types.Count()} types of {path}");
        return 0;
    }

    private static bool IsInstrumented(string path)
    {
        using var stream = File.OpenRead(path);
        using var image = new PEReader(stream);
        var metadata = image.GetMetadataReader();

        return metadata.AssemblyReferences.Any(handle =>
            metadata.StringComparer.Equals(metadata.GetAssemblyReference(handle).Name, TraceAssembly)
        );
    }

    [MethodImpl(MethodImplOptions.NoInlining)]
    private static int Run(string[] args)
    {
        if (args is not [var name, .. var files] || Target(name) is not { } target)
        {
            Console.Error.WriteLine(Usage);
            return 2;
        }

        if (files.Length > 0)
        {
            RecordBranchesLocally();
            foreach (var file in files)
            {
                target(File.ReadAllBytes(file));
                Console.WriteLine($"{file}: read as {name} reads it");
            }

            return 0;
        }

        if (Environment.GetEnvironmentVariable(SharedMemoryVariable) is null)
        {
            Console.Error.WriteLine(Usage);
            return 2;
        }

        // Without the instrumentation, libFuzzer would see no coverage, and fuzz blindly
        if (!typeof(TableReader).Assembly.GetReferencedAssemblies().Any(reference => reference.Name == TraceAssembly))
        {
            Console.Error.WriteLine("Tafla.dll isn't instrumented: run `Tafla.Fuzz instrument` first");
            return 1;
        }

        // Reports any exception that escapes the target as a crash
        Fuzzer.LibFuzzer.Run(target);
        return 0;
    }

    /// <summary>
    /// Gives an instrumented runtime memory of its own to record its branches in, which
    /// libfuzzer-dotnet shares with it otherwise, so that it reads files outside libFuzzer too
    /// </summary>
    private static unsafe void RecordBranchesLocally() =>
        SharpFuzz.Common.Trace.SharedMem = (byte*)NativeMemory.AllocZeroed(BranchMapSize);

    private static ReadOnlySpanAction? Target(string name) =>
        name switch
        {
            "table_reader" => TableReaderTarget.Run,
            "conformance" => ConformanceTarget.Run,
            _ => null,
        };
}
