namespace Tafla.Tests;

/// <summary>The examples of README.md, which document how a format writes and reads a table.</summary>
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

        var reader = new TableReader(bytes, Layout);
        if (reader.GetVersion() != FormatVersion)
        {
            throw DecodeException.UnsupportedVersion(reader.GetVersion(), FormatVersion);
        }

        double latitude = reader.GetFloat64(reader.RootOffset(0));
        string timeZone = reader.GetString(reader.RootOffset(1)); // a new string
        int table = reader.RootOffset(2);
        ReadOnlySpan<float> values = reader.GetFloat32Array(reader.FieldOffset(table, 1)); // in place

        Assert.Equal(59.3293, latitude);
        Assert.Equal("Europe/Stockholm", timeZone);
        Assert.Equal([21.75f, 18.5f], values.ToArray());
        Assert.Equal((uint)bytes.Length, reader.GetSize());
        Assert.Equal(bytes, Written());
    }

    [Fact]
    public void RecordsNestedTablesInPlaceAsTheReadmeShows()
    {
        var builder = new TableBuilder(3, Layout, FormatVersion);
        builder.SetFloat64(59.3293);
        builder.SetString("Europe/Stockholm");
        builder.SetTable(2, static series =>
        {
            series.SetUInt32Array([0, 86_400]);
            series.SetFloat32Array([21.75f, 18.5f]);
        });
        byte[] bytes = builder.ToArray();

        (uint Time, float Value)[] readings = [(0, 21.75f), (86_400, 18.5f)];

        var list = new TableBuilder(1);
        list.SetVector(readings.Length, 2, readings, static (item, index, readings) =>
        {
            item.SetUInt32(readings[index].Time);
            item.SetFloat32(readings[index].Value);
        });

        // The bytes of the first example, whose series is a builder of its own
        Assert.Equal(Written(), bytes);

        var vector = new TableReader(list.ToArray()).GetVector(4);
        var read = new List<(uint, float)>();
        foreach (var item in vector.Items())
        {
            var reader = new TableReader(item);
            read.Add((reader.GetUInt32(reader.RootOffset(0)), reader.GetFloat32(reader.RootOffset(1))));
        }

        Assert.Equal(readings, read);
    }

    /// <summary>Writes the example's table with a sizer and a writer, as the README shows.</summary>
    private static byte[] Written()
    {
        var sizer = new TableSizer(3, Layout, FormatVersion);
        sizer.AddFloat64();
        sizer.AddString("Europe/Stockholm");
        sizer.AddTable(4); // aligned for its arrays of 4-byte elements
        var nested = sizer.StartTable(2);
        sizer.AddUInt32Array(2);
        sizer.AddFloat32Array(2);
        sizer.EndTable(nested);

        var writer = sizer.CreateWriter();
        writer.SetFloat64(0, 59.3293);
        writer.SetString(1, "Europe/Stockholm");
        writer.SetTable(2, 4);
        var series = writer.StartTable(2);
        writer.SetUInt32Array(0, [0, 86_400]);
        writer.SetFloat32Array(1, [21.75f, 18.5f]);
        writer.EndTable(series);
        byte[] written = writer.Bytes; // the builder's bytes, of the table's length

        return written;
    }
}
