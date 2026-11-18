namespace Tafla.Tests;

/// <summary>The example of README.md, which documents how a format writes and reads a table.</summary>
public class ExampleTests
{
    // A format's header, shared by its writer and its readers
    private static readonly TableLayout Layout = new() { Sized = true, Versioned = true };

    private const uint FormatVersion = 1;

    [Fact]
    public void WritesAndReadsATableAsTheReadmeShows()
    {
        var series = new TableBuilder(2);
        series.SetUInt32Array([0, 86_400]); // copied when it's set
        series.SetFloat32Array([21.75f, 18.5f]);

        var builder = new TableBuilder(3, Layout, FormatVersion);
        builder.SetFloat64(59.3293);
        builder.SetString("Europe/Stockholm");
        builder.SetTable(series); // copied into the table, aligned for its arrays
        byte[] bytes = builder.ToArray();

        var buffer = new ByteBuffer(bytes, Layout);
        if (buffer.GetVersion() != FormatVersion)
        {
            throw DecodeException.UnsupportedVersion(buffer.GetVersion(), FormatVersion);
        }

        double latitude = buffer.GetFloat64(buffer.RootOffset(0));
        string timeZone = buffer.GetString(buffer.RootOffset(1)); // a new string
        int table = buffer.RootOffset(2);
        ReadOnlySpan<float> values = buffer.GetFloat32Array(buffer.FieldOffset(table, 1)); // in place

        Assert.Equal(59.3293, latitude);
        Assert.Equal("Europe/Stockholm", timeZone);
        Assert.Equal([21.75f, 18.5f], values.ToArray());
        Assert.Equal((uint)bytes.Length, buffer.GetSize());
    }
}
