using System;
using System.Collections.Generic;
using System.Globalization;
using System.IO;
using System.Linq;
using System.Text;

namespace Tafla.Benchmarks;

/// <summary>A benchmark's measurements, one for each variant, in the benchmark's order.</summary>
internal sealed record Result(Benchmark Benchmark, Measurement[] Measurements);

/// <summary>Prints and writes the measurements.</summary>
internal static class Report
{
    /// <summary>Returns a line of each variant's fastest round, and its ratio to the first's.</summary>
    public static string Summary(Measurement[] measurements)
    {
        var baseline = measurements[0].Fastest;
        return string.Join(
            ", ",
            measurements.Select(measurement =>
                $"{measurement.Name} {Nanoseconds(measurement.Fastest)} ({Ratio(measurement.Fastest / baseline)})"
            )
        );
    }

    /// <summary>Prints every variant's fastest round, its ratio to the first's, its median round and what a call allocates.</summary>
    public static void Print(IReadOnlyList<Result> results)
    {
        foreach (var (benchmark, measurements) in results)
        {
            Console.WriteLine(benchmark.Name);

            var width = Math.Max(24, measurements.Max(measurement => measurement.Name.Length));
            Console.WriteLine($"  {"variant".PadRight(width)}  {"fastest",12}  {"ratio",8}  {"median",12}  {"allocated",12}");

            var baseline = measurements[0].Fastest;
            foreach (var measurement in measurements)
            {
                Console.WriteLine(
                    $"  {measurement.Name.PadRight(width)}  {Nanoseconds(measurement.Fastest),12}  "
                        + $"{Ratio(measurement.Fastest / baseline),8}  "
                        + $"{Nanoseconds(measurement.Median),12}  {Bytes(measurement.Bytes),12}"
                );
            }

            Console.WriteLine();
        }
    }

    /// <summary>Writes every measurement as a line of tab-separated values, to compare runs.</summary>
    public static void WriteTsv(string path, IReadOnlyList<Result> results)
    {
        var text = new StringBuilder("group\tbenchmark\tvariant\tfastest_ns\tmedian_ns\tbytes\n");
        foreach (var (benchmark, measurements) in results)
        {
            foreach (var measurement in measurements)
            {
                text.Append(
                    CultureInfo.InvariantCulture,
                    $"{benchmark.Group}\t{benchmark.Name}\t{measurement.Name}\t{measurement.Fastest:F2}\t{measurement.Median:F2}\t{measurement.Bytes:F1}\n"
                );
            }
        }

        File.WriteAllText(path, text.ToString());
    }

    /// <summary>
    /// Writes the README's tables of each group of benchmarks: the fastest round in µs per call, with
    /// its ratio to the row's first variant's, and the bytes that a call allocates, with a column for
    /// each variant.
    /// </summary>
    public static void WriteMarkdown(string path, IReadOnlyList<Result> results)
    {
        var text = new StringBuilder();
        foreach (var group in results.GroupBy(result => result.Benchmark.Group))
        {
            var rows = group.ToList();
            var variants = rows.SelectMany(result => result.Measurements.Select(Column)).Distinct().ToList();

            text.Append(CultureInfo.InvariantCulture, $"Fastest round in µs per call, and its ratio to {variants[0]}'s ({group.Key}):\n\n");
            AppendTable(text, rows, variants, Time);
            text.Append(CultureInfo.InvariantCulture, $"\nBytes allocated per call ({group.Key}):\n\n");
            AppendTable(text, rows, variants, (measurement, _) => Bytes(measurement.Bytes).Replace(" B", "", StringComparison.Ordinal));
            text.Append('\n');
        }

        File.WriteAllText(path, text.ToString());
    }

    private static void AppendTable(
        StringBuilder text,
        List<Result> rows,
        List<string> variants,
        Func<Measurement, Measurement, string> cell
    )
    {
        List<string[]> table = [["Benchmark", .. variants]];
        foreach (var (benchmark, measurements) in rows)
        {
            var baseline = measurements[0];
            var line = new string[variants.Count + 1];
            line[0] = benchmark.Name;
            for (var column = 0; column < variants.Count; column++)
            {
                var measurement = measurements.FirstOrDefault(measurement => Column(measurement) == variants[column]);
                line[column + 1] = measurement is null ? "" : cell(measurement, baseline);
            }

            table.Add(line);
        }

        var widths = Enumerable.Range(0, variants.Count + 1).Select(column => table.Max(line => line[column].Length)).ToArray();
        AppendLine(text, table[0], widths, index => false);
        text.Append('|');
        for (var column = 0; column < widths.Length; column++)
        {
            text.Append(column == 0 ? $" {new string('-', widths[column])} |" : $" {new string('-', widths[column] - 1)}: |");
        }

        text.Append('\n');
        foreach (var line in table.Skip(1))
        {
            AppendLine(text, line, widths, column => column > 0);
        }
    }

    private static void AppendLine(StringBuilder text, string[] line, int[] widths, Func<int, bool> rightAligned)
    {
        text.Append('|');
        for (var column = 0; column < line.Length; column++)
        {
            var value = rightAligned(column) ? line[column].PadLeft(widths[column]) : line[column].PadRight(widths[column]);
            text.Append(CultureInfo.InvariantCulture, $" {value} |");
        }

        text.Append('\n');
    }

    /// <summary>
    /// A variant's column in the tables: its name, or <c>A/A</c> for an A/A copy, whose column holds
    /// the copy of each row's first variant
    /// </summary>
    private static string Column(Measurement measurement) =>
        measurement.Name.EndsWith(Again.Suffix, StringComparison.Ordinal) ? Again.Suffix.Trim() : measurement.Name;

    /// <summary>The fastest round in µs, and its ratio to the baseline's, but for the baseline itself</summary>
    private static string Time(Measurement measurement, Measurement baseline)
    {
        var microseconds = Significant(measurement.Fastest / 1000);
        return measurement == baseline ? microseconds : $"{microseconds} ({Ratio(measurement.Fastest / baseline.Fastest)})";
    }

    private static string Ratio(double ratio) => Significant(ratio) + "×";

    /// <summary>
    /// Formats a value with at least 3 significant digits, trailing zeros included: 3 below 1, and at
    /// least 4, with at least one decimal, from 1
    /// </summary>
    private static string Significant(double value)
    {
        if (value <= 0)
        {
            return "0";
        }

        var magnitude = (int)Math.Floor(Math.Log10(value));
        var decimals = value < 1 ? 2 - magnitude : Math.Max(1, 3 - magnitude);
        return value.ToString("F" + decimals.ToString(CultureInfo.InvariantCulture), CultureInfo.InvariantCulture);
    }

    private static string Nanoseconds(double value) =>
        value < 100
            ? value.ToString("F1", CultureInfo.InvariantCulture) + " ns"
            : value.ToString("N0", CultureInfo.InvariantCulture) + " ns";

    private static string Bytes(double value) => value.ToString("N0", CultureInfo.InvariantCulture) + " B";
}
