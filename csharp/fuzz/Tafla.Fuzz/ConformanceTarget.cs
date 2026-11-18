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
/// mutates tables that the descriptions read deep into. Reads may fail, but only with a
/// <see cref="DecodeException"/>, and finding a vector's item by index must find what reading the
/// items in order finds. Any other exception escapes, and is a finding.
/// </remarks>
internal static class ConformanceTarget
{
    /// <summary>The descriptions of the suite's valid tables, which the conformance test parses</summary>
    private static readonly Table[] Tables = [.. Suite.Instance.Valid.Select(valid => valid.Table)];

    public static void Run(ReadOnlySpan<byte> input)
    {
        // A copy of exactly the input, so that a read past it throws
        var bytes = input.ToArray();

        foreach (var table in Tables)
        {
            try
            {
                // The bytes hold values of the described types where it reads them, which may differ
                // from the description's
                var read = Read.Table(bytes, table);
                Check.That(read.Fields.Count == table.Fields.Count);
            }
            catch (DecodeException error)
            {
                Check.That(error.Message.Length > 0);
            }
            // The bytes read, but with a size or a number of items other than described, and never
            // an item found otherwise by index than in order, which escapes
            catch (MismatchException mismatch) when (!mismatch.Message.StartsWith("Get finds", StringComparison.Ordinal)) { }
        }
    }
}
