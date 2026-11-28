using Tafla.Tests.Tables;

namespace Tafla.Fuzz;

/// <summary>
/// Reads its input as every valid table of the conformance suite describes it, following their
/// offsets into nested tables, vectors, arrays, strings and structs, as the conformance test reads
/// them.
/// </summary>
/// <remarks>
/// Its seeds are the suite's tables, <c>conformance/valid/*.bin</c>,
/// <c>conformance/equivalent/*.bin</c> and <c>conformance/invalid/*.bin</c>, so that the fuzzer
/// mutates tables that the descriptions read deep into. Reads may fail with a
/// <see cref="DecodeException"/>, or read values or numbers of items other than described, as a
/// <see cref="MismatchException"/> says, but a valid case's own bytes read as it describes them.
/// Any other exception escapes, and is a finding, such as an <see cref="InconsistentReadException"/>,
/// of a vector whose items read in order differ from its count, or from those that finding them by
/// index finds.
/// </remarks>
internal static class ConformanceTarget
{
    /// <summary>The suite's valid cases, which the conformance test parses</summary>
    private static readonly ValidCase[] Cases = [.. Suite.Instance.Valid];

    public static void Run(ReadOnlySpan<byte> input)
    {
        // A copy of exactly the input, so that a read past it throws
        var bytes = input.ToArray();

        foreach (var valid in Cases)
        {
            try
            {
                var read = Read.Table(bytes, valid.Table);
                Check.That(!bytes.AsSpan().SequenceEqual(valid.Bytes) || read == valid.Table);
            }
            catch (DecodeException error)
            {
                Check.That(error.Message.Length > 0);
            }
            catch (MismatchException mismatch)
            {
                Check.That(mismatch.Message.Length > 0);
            }
        }
    }
}
