using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Globalization;
using System.Linq;
using System.Runtime;
using System.Runtime.InteropServices;
using System.Text.RegularExpressions;
using Tafla.Benchmarks;

// Times tafla's C# runtime and the libraries that it's compared with on every workload, interleaved,
// and prints each benchmark's fastest call, its ratio to tafla's, the median call and what a call
// allocates, or, with encoding, each codec's encoded sizes. With lists, it times the lists suite's
// lists of strings instead. ../README.md describes the method.
//
//   dotnet run -c Release -- [lists] [--run regex] [--rounds n] [--warmup n] [--sample ms] [--tiering n] [--quick] [--tsv path] [--markdown path]
//   dotnet run -c Release -- encoding

var arguments = Arguments.Parse(args);
if (arguments.Encoding)
{
    EncodingReport.Print();
    return 0;
}

#if DEBUG
if (!arguments.Quick)
{
    Console.Error.WriteLine("Build in Release to time the benchmarks: dotnet run -c Release");
    return 1;
}
#endif

var options = arguments.Quick
    ? new HarnessOptions(Rounds: 2, WarmupRounds: 0, SampleTime: TimeSpan.FromMilliseconds(1), TieringPasses: 1)
    : new HarnessOptions(
        arguments.Rounds,
        arguments.Warmup,
        TimeSpan.FromMilliseconds(arguments.SampleMilliseconds),
        arguments.Tiering
    );

var selected = new Regex(arguments.Run, RegexOptions.CultureInvariant);
var suite = arguments.Lists ? StringLists.All() : Comparison.All();
var benchmarks = suite.Where(benchmark => selected.IsMatch(benchmark.Name)).ToList();
foreach (var benchmark in benchmarks)
{
    Comparison.Check(benchmark);
}

Console.WriteLine(
    $"{RuntimeInformation.FrameworkDescription} on {RuntimeInformation.OSDescription} {RuntimeInformation.ProcessArchitecture}, "
        + $"{Environment.ProcessorCount} CPUs, {(GCSettings.IsServerGC ? "server" : "workstation")} GC"
);
Console.WriteLine($"Start: {Machine.Uptime()}");
var started = Stopwatch.GetTimestamp();

var results = new List<Result>();
foreach (var benchmark in benchmarks)
{
    var measurements = Harness.Measure(benchmark.Variants, options);
    results.Add(new Result(benchmark, measurements));
    Console.Error.WriteLine($"{benchmark.Name}: {Report.Summary(measurements)}");
}

Console.WriteLine($"End: {Machine.Uptime()}");
Console.WriteLine(
    $"Took {Stopwatch.GetElapsedTime(started).TotalSeconds:F0} s, {options.Rounds} rounds of {options.SampleTime.TotalMilliseconds} ms "
        + $"after {options.TieringPasses} tiering passes and {options.WarmupRounds} warm-up rounds"
);
Console.WriteLine();
Report.Print(results);

if (arguments.Tsv is { } tsv)
{
    Report.WriteTsv(tsv, results);
}

if (arguments.Markdown is { } markdown)
{
    Report.WriteMarkdown(markdown, results);
}

Sink.KeepAlive();
return 0;

/// <summary>The command's options.</summary>
internal sealed record Arguments(
    bool Encoding,
    bool Lists,
    string Run,
    int Rounds,
    int Warmup,
    int SampleMilliseconds,
    int Tiering,
    bool Quick,
    string? Tsv,
    string? Markdown
)
{
    public static Arguments Parse(string[] args)
    {
        var parsed = new Arguments(
            Encoding: false,
            Lists: false,
            Run: "",
            Rounds: 30,
            Warmup: 3,
            SampleMilliseconds: 20,
            Tiering: 4,
            Quick: false,
            Tsv: null,
            Markdown: null
        );
        for (var index = 0; index < args.Length; index++)
        {
            parsed = args[index] switch
            {
                "encoding" when index == 0 => parsed with { Encoding = true },
                "lists" when index == 0 => parsed with { Lists = true },
                "--run" => parsed with { Run = args[++index] },
                "--rounds" => parsed with { Rounds = int.Parse(args[++index], CultureInfo.InvariantCulture) },
                "--warmup" => parsed with { Warmup = int.Parse(args[++index], CultureInfo.InvariantCulture) },
                "--sample" => parsed with { SampleMilliseconds = int.Parse(args[++index], CultureInfo.InvariantCulture) },
                "--tiering" => parsed with { Tiering = int.Parse(args[++index], CultureInfo.InvariantCulture) },
                "--quick" => parsed with { Quick = true },
                "--tsv" => parsed with { Tsv = args[++index] },
                "--markdown" => parsed with { Markdown = args[++index] },
                var unknown => throw new ArgumentException($"Unknown option {unknown}", nameof(args)),
            };
        }

        return parsed;
    }
}

internal static class Machine
{
    /// <summary>Returns uptime's line, with the load averages, to check the machine's load.</summary>
    public static string Uptime()
    {
        try
        {
            using var process = Process.Start(new ProcessStartInfo("uptime") { RedirectStandardOutput = true });
            var output = process!.StandardOutput.ReadToEnd().Trim();
            process.WaitForExit();

            return output;
        }
        catch (System.ComponentModel.Win32Exception)
        {
            return "(uptime isn't available)";
        }
    }
}
