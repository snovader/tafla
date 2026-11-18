using System.Diagnostics.CodeAnalysis;
using System.Runtime.CompilerServices;

namespace Tafla.Fuzz;

/// <summary>
/// A check of a fuzz target that failed, which, as any exception other than a
/// <see cref="DecodeException"/>, SharpFuzz reports to libFuzzer as a crash
/// </summary>
[SuppressMessage(
    "Design",
    "CA1032:Implement standard exception constructors",
    Justification = "Only Check throws it, with the check that failed"
)]
internal sealed class CheckFailedException(string message) : Exception(message);

internal static class Check
{
    /// <summary>Throws a <see cref="CheckFailedException"/> that names the condition unless it holds</summary>
    public static void That(
        [DoesNotReturnIf(false)] bool condition,
        [CallerArgumentExpression(nameof(condition))] string? expression = null
    )
    {
        if (!condition)
        {
            throw new CheckFailedException($"Check failed: {expression}");
        }
    }

    /// <summary>Throws a <see cref="CheckFailedException"/> that describes what failed</summary>
    [DoesNotReturn]
    public static void Fail(string message) => throw new CheckFailedException(message);
}
