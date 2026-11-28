namespace Tafla.Benchmarks.Tests;

// The generator makes the TypeScript comparison's values, which these were printed from, and which
// Go's workloads test checks too, so that the languages' benchmarks time the same work
public class WorkloadsTests
{
    [Fact]
    public void GeneratesTheTypeScriptDocumentOf100Rows()
    {
        var document = Workloads.NewDocument(100);

        Assert.Equal(14_727_226u, document.Keys[0]);
        Assert.Equal(10_335_322u, document.Keys[99]);
        Assert.Equal(825_833_448ul, Sum(document.Keys));
        Assert.Equal(0xc221_9b29u, BitConverter.SingleToUInt32Bits(document.A[0]));
        Assert.Equal(0xc163_647eu, BitConverter.SingleToUInt32Bits(document.B[99]));
        Assert.Equal(1_493_961_445u, HashColumns(document));
    }

    [Fact]
    public void GeneratesTheTypeScriptDocumentOf100000Rows()
    {
        var document = Workloads.NewDocument(100_000);

        Assert.Equal(13_320_945u, document.Keys[0]);
        Assert.Equal(11_621_954u, document.Keys[99_999]);
        Assert.Equal(839_050_642_924ul, Sum(document.Keys));
        Assert.Equal(0xc1d4_d7f6u, BitConverter.SingleToUInt32Bits(document.A[0]));
        Assert.Equal(0x3e8c_91f5u, BitConverter.SingleToUInt32Bits(document.B[99_999]));
        Assert.Equal(1_138_446_770u, HashColumns(document));
    }

    [Fact]
    public void GeneratesTheTypeScriptVectorOfNestedTables()
    {
        var items = Workloads.NewList(Workloads.ListItems).Items;

        Assert.Equal(100, items.Length);
        Assert.Equal((0x41d2_acb6u, 0xbfce_db15u, 0x41e3_2e23u), Bits(items[0]));
        Assert.Equal((0x41c1_2b3au, 0x3ea9_43c1u, 0x422e_ae51u), Bits(items[1]));
        Assert.Equal((0x4193_d915u, 0x3f97_9e05u, 0x418f_26e1u), Bits(items[99]));
    }

    [Fact]
    public void SameComparesFloatsByTheirBits()
    {
        var record = Workloads.TheRecord;

        Assert.False((record with { X = 0.0 }).Same(record with { X = -0.0 }));
        Assert.True((record with { X = double.NaN }).Same(record with { X = double.NaN }));
    }

    private static ulong Sum(uint[] keys)
    {
        var total = 0ul;
        foreach (var key in keys)
        {
            total += key;
        }

        return total;
    }

    /// <summary>
    /// Hashes the float columns' bits, row by row, as the TypeScript script that printed these did
    /// with 32-bit integers
    /// </summary>
    private static uint HashColumns(Document document)
    {
        var hash = 0u;
        for (var row = 0; row < document.A.Length; row++)
        {
            hash = unchecked(hash * 31 + BitConverter.SingleToUInt32Bits(document.A[row]));
            hash = unchecked(hash * 31 + BitConverter.SingleToUInt32Bits(document.B[row]));
        }

        return hash;
    }

    private static (uint, uint, uint) Bits(Item item) =>
        (
            BitConverter.SingleToUInt32Bits(item.Value),
            BitConverter.SingleToUInt32Bits(item.First),
            BitConverter.SingleToUInt32Bits(item.Second)
        );
}
