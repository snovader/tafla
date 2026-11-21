using System.Text.Json;
using Shapes;
using Tafla.Tests.Tables;

namespace Tafla.Tests.Generated;

/// <summary>
/// The code generated from <c>schemas/shapes.tafla</c> writes the tables and the struct of the cases in
/// <c>conformance/generated</c>, which the generated TypeScript wrote, byte for byte, and reads their
/// values back. See <c>conformance/generated/README.md</c> at the root of the repository for the format
/// of <c>shapes.json</c>, whose values <see cref="Suite.ParseScalar"/> reads as it reads those of
/// <c>cases.json</c>.
/// </summary>
public class ShapesCasesTests
{
    private static readonly Lazy<Dictionary<string, ShapesCase>> Cases = new(Load);

    public static TheoryData<string> Names => [.. Cases.Value.Keys];

    [Theory]
    [MemberData(nameof(Names))]
    public void WritesTheCaseByteForByte(string name)
    {
        var shapesCase = Cases.Value[name];
        var fields = shapesCase.Fields;

        var bytes = shapesCase.Type switch
        {
            "Point" => PointOf(fields).Serialize(),
            "Document" => DocumentOf(fields).Serialize(),
            "ItemList" => ItemListOf(fields).Serialize(),
            "Sample" => SampleOf(fields).Serialize(),
            var other => throw UnknownType(other),
        };

        Assert.Equal(shapesCase.Bytes, bytes);
    }

    /// <summary>Floats compare by their bits, as the descriptions hold them.</summary>
    [Theory]
    [MemberData(nameof(Names))]
    public void ReadsTheValuesOfTheCase(string name)
    {
        var shapesCase = Cases.Value[name];
        var (fields, bytes) = (shapesCase.Fields, shapesCase.Bytes);

        switch (shapesCase.Type)
        {
            case "Point":
                Assert.Equal(ShapesTests.Describe(PointOf(fields)), ShapesTests.Describe(new Point(bytes)));
                break;

            case "Document":
                Assert.Equal(ShapesTests.Describe(DocumentOf(fields)), ShapesTests.Describe(new Document(bytes)));
                break;

            case "ItemList":
                Assert.Equal(ShapesTests.Describe(ItemListOf(fields)), ShapesTests.Describe(new ItemList(bytes)));
                break;

            case "Sample":
                var expected = SampleOf(fields);
                var read = new Sample(bytes);
                Assert.Equal(expected.Key, read.Key);
                Assert.Equal(BitConverter.SingleToUInt32Bits(expected.A), BitConverter.SingleToUInt32Bits(read.A));
                Assert.Equal(BitConverter.SingleToUInt32Bits(expected.B), BitConverter.SingleToUInt32Bits(read.B));
                break;

            default:
                throw UnknownType(shapesCase.Type);
        }
    }

    /// <summary>A case of <c>shapes.json</c>: the message or struct of the schema, its fields' values, and its bytes.</summary>
    private sealed record ShapesCase(string Type, JsonElement Fields, byte[] Bytes);

    private static Dictionary<string, ShapesCase> Load()
    {
        var directory = Path.Combine(Suite.Directory, "generated");
        using var json = JsonDocument.Parse(File.ReadAllBytes(Path.Combine(directory, "shapes.json")));

        var cases = new Dictionary<string, ShapesCase>();
        foreach (var description in json.RootElement.GetProperty("cases").EnumerateArray())
        {
            cases.Add(
                description.GetProperty("name").GetString()!,
                new ShapesCase(
                    description.GetProperty("type").GetString()!,
                    description.GetProperty("fields").Clone(),
                    File.ReadAllBytes(Path.Combine(directory, description.GetProperty("file").GetString()!))
                )
            );
        }

        return cases;
    }

    private static InvalidDataException UnknownType(string type) => new($"{type} isn't a message or struct of shapes.tafla");

    // The fields of each type, from the JSON of a case

    private static PointFields PointOf(JsonElement json) =>
        new()
        {
            X = Float64(json, "x"),
            Y = Float64(json, "y"),
            Id = (long)Bits(ScalarType.Int64, json.GetProperty("id")),
            Name = json.GetProperty("name").GetString()!,
        };

    private static DocumentFields DocumentOf(JsonElement json) =>
        new()
        {
            X = Float64(json, "x"),
            Y = Float64(json, "y"),
            Name = json.GetProperty("name").GetString()!,
            Columns = ColumnsOf(json.GetProperty("columns")),
        };

    private static ColumnsFields ColumnsOf(JsonElement json) =>
        new()
        {
            Base = (long)Bits(ScalarType.Int64, json.GetProperty("base")),
            Keys = json.GetProperty("keys").EnumerateArray().Select(key => (uint)Bits(ScalarType.UInt32, key)).ToArray(),
            A = Float32s(json, "a"),
            B = Float32s(json, "b"),
        };

    private static ItemListFields ItemListOf(JsonElement json) =>
        new() { Items = json.GetProperty("items").EnumerateArray().Select(ItemOf).ToArray() };

    private static ItemFields ItemOf(JsonElement json)
    {
        var pair = json.GetProperty("pair");
        return new()
        {
            Value = Float32(json, "value"),
            Pair = new() { First = Float32(pair, "first"), Second = Float32(pair, "second") },
        };
    }

    private static Sample SampleOf(JsonElement json) =>
        new()
        {
            Key = (uint)Bits(ScalarType.UInt32, json.GetProperty("key")),
            A = Float32(json, "a"),
            B = Float32(json, "b"),
        };

    // Values, which JSON holds as cases.json does

    private static ulong Bits(ScalarType type, JsonElement value) => Suite.ParseScalar(type, value).Bits;

    private static float Float32(JsonElement json, string name) =>
        BitConverter.UInt32BitsToSingle((uint)Bits(ScalarType.Float32, json.GetProperty(name)));

    private static double Float64(JsonElement json, string name) =>
        BitConverter.UInt64BitsToDouble(Bits(ScalarType.Float64, json.GetProperty(name)));

    private static float[] Float32s(JsonElement json, string name) =>
        [.. json.GetProperty(name).EnumerateArray().Select(value => BitConverter.UInt32BitsToSingle((uint)Bits(ScalarType.Float32, value)))];
}
