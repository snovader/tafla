using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Linq;
using System.Threading;

namespace Tafla.Benchmarks;

/// <summary>
/// One way to do a benchmark's work: a loop of its own, which does it <c>n</c> times and stores each
/// result in <see cref="Sink"/>, so that the JIT can't leave the work out, and a check of the result
/// that a call stored, which <see cref="Comparison.Check"/> runs before the variant is timed. Each
/// variant's loop is a method of its own, so that dynamic PGO profiles its calls apart from the
/// others'.
/// </summary>
internal sealed record Variant(string Name, Action<long> Loop, Func<bool> Check);

/// <summary>
/// A variant's fastest and median round, in nanoseconds per call, and the bytes that a call
/// allocates on the managed heap.
/// </summary>
internal sealed record Measurement(string Name, double Fastest, double Median, double Bytes);

/// <summary>How long each variant runs.</summary>
internal sealed record HarnessOptions(int Rounds, int WarmupRounds, TimeSpan SampleTime, int TieringPasses);

/// <summary>
/// Times variants of a benchmark interleaved, as the other languages' comparisons do: every round
/// runs each variant once, in alternating order, and the fastest round counts. Load from other
/// processes then slows every variant alike, and only for some rounds. Each variant's sample, in
/// every round, starts after a full garbage collection, so that one variant's garbage doesn't slow
/// another, and the collections that a variant's allocations cause within its sample count as its
/// own.
/// </summary>
internal static class Harness
{
    /// <summary>The calls of each tiering pass, more than the 30 that promote a method at each tier</summary>
    private const int TieringCalls = 40;

    /// <summary>
    /// The pause after each tiering pass, longer than the 100 ms of quiet that tiered compilation waits
    /// for before it counts calls, and than its background compilation
    /// </summary>
    private static readonly TimeSpan TieringPause = TimeSpan.FromMilliseconds(250);

    public static Measurement[] Measure(IReadOnlyList<Variant> variants, HarnessOptions options)
    {
        Tier(variants, options.TieringPasses);

        var repetitions = variants.Select(variant => Calibrate(variant, options.SampleTime)).ToArray();
        if (options.WarmupRounds > 0)
        {
            var warm = Rounds(variants, repetitions, options.WarmupRounds);
            for (var index = 0; index < variants.Count; index++)
            {
                repetitions[index] = RepetitionsOf(warm[index].Min(), options.SampleTime);
            }
        }

        var timed = Rounds(variants, repetitions, options.Rounds);

        var measurements = new Measurement[variants.Count];
        for (var index = 0; index < variants.Count; index++)
        {
            var samples = timed[index].Order().ToArray();
            var bytes = Allocated(variants[index], repetitions[index]);
            measurements[index] = new Measurement(variants[index].Name, samples[0], samples[samples.Length / 2], bytes);
        }

        return measurements;
    }

    /// <summary>
    /// Calls every variant's loop in passes, with a pause after each, so that tiered compilation
    /// promotes the loops and what they call through tier 0, its instrumented tier and tier 1 before
    /// any are timed, as it promotes an application's hot methods
    /// </summary>
    private static void Tier(IReadOnlyList<Variant> variants, int passes)
    {
        for (var pass = 0; pass < passes; pass++)
        {
            foreach (var variant in variants)
            {
                for (var call = 0; call < TieringCalls; call++)
                {
                    variant.Loop(1);
                }
            }

            Thread.Sleep(TieringPause);
        }
    }

    /// <summary>
    /// Runs rounds of the variants, each once a round, in alternating order, and returns each one's
    /// time per call in every round, in nanoseconds
    /// </summary>
    private static List<double>[] Rounds(IReadOnlyList<Variant> variants, long[] repetitions, int count)
    {
        var samples = variants.Select(_ => new List<double>(count)).ToArray();

        for (var round = 0; round < count; round++)
        {
            for (var position = 0; position < variants.Count; position++)
            {
                var index = round % 2 == 0 ? position : variants.Count - 1 - position;
                samples[index].Add(PerCall(variants[index], repetitions[index]));
            }
        }

        return samples;
    }

    /// <summary>Returns the time per call of <paramref name="n"/> calls, in nanoseconds, after a full collection</summary>
    private static double PerCall(Variant variant, long n)
    {
        Collect();

        var start = Stopwatch.GetTimestamp();
        variant.Loop(n);
        var elapsed = Stopwatch.GetElapsedTime(start);

        return elapsed.TotalNanoseconds / n;
    }

    /// <summary>Counts the repetitions that take about <paramref name="sampleTime"/>, from calls that take at least a tenth of it</summary>
    private static long Calibrate(Variant variant, TimeSpan sampleTime)
    {
        for (long n = 1; ; n *= 2)
        {
            var start = Stopwatch.GetTimestamp();
            variant.Loop(n);
            var elapsed = Stopwatch.GetElapsedTime(start);

            if (elapsed >= sampleTime / 10)
            {
                return RepetitionsOf(elapsed.TotalNanoseconds / n, sampleTime);
            }
        }
    }

    /// <summary>Returns the calls of <paramref name="perCall"/> nanoseconds that take about <paramref name="sampleTime"/></summary>
    private static long RepetitionsOf(double perCall, TimeSpan sampleTime) =>
        Math.Max(1, (long)(sampleTime.TotalNanoseconds / Math.Max(perCall, 1)));

    /// <summary>Returns the bytes that a call allocates, the fewest of three samples of <paramref name="n"/> calls</summary>
    private static double Allocated(Variant variant, long n)
    {
        var fewest = double.MaxValue;
        for (var sample = 0; sample < 3; sample++)
        {
            Collect();

            var before = GC.GetAllocatedBytesForCurrentThread();
            variant.Loop(n);
            var after = GC.GetAllocatedBytesForCurrentThread();

            fewest = Math.Min(fewest, (double)(after - before) / n);
        }

        return fewest;
    }

    private static void Collect()
    {
        GC.Collect();
        GC.WaitForPendingFinalizers();
        GC.Collect();
    }
}
