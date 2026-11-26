using System.Globalization;
using System.Text.Json;

namespace Tafla.Tests.Tables;

/// <summary>A valid table of the conformance suite, with its description.</summary>
public sealed record ValidCase(string Name, byte[] Bytes, Table Table)
{
    public override string ToString() => Name;
}

/// <summary>
/// A table whose bytes differ from those of a valid case, <see cref="Case"/>, which describes them.
/// <see cref="Field"/> is the root field that fails to read, for an invalid case.
/// </summary>
public sealed record ChangedCase(string Name, byte[] Bytes, string Case, int Field)
{
    public override string ToString() => Name;
}

/// <summary>
/// The conformance suite's cases.json, or the cases of presence in presence/cases.json, with the bytes of
/// each case.
/// </summary>
public sealed class Suite
{
    private static readonly Lazy<Suite> Loaded = new(() => Load(Directory));

    private static readonly Lazy<Suite> LoadedPresence = new(() => Load(Path.Combine(Directory, "presence")));

    private Suite(List<ValidCase> valid, List<ChangedCase> equivalent, List<ChangedCase> invalid)
    {
        Valid = valid;
        Equivalent = equivalent;
        Invalid = invalid;
    }

    /// <summary>The suite, which is loaded once.</summary>
    public static Suite Instance => Loaded.Value;

    /// <summary>
    /// The cases of presence, of absent fields and of tables that a later revision of their format
    /// reads, which lie apart from the suite's other cases, and are loaded once.
    /// </summary>
    public static Suite Presence => LoadedPresence.Value;

    public IReadOnlyList<ValidCase> Valid { get; }

    public IReadOnlyList<ChangedCase> Equivalent { get; }

    public IReadOnlyList<ChangedCase> Invalid { get; }

    /// <summary>Returns the description of the valid case that an equivalent or invalid case names.</summary>
    public Table TableOf(string name) => Valid.Single(valid => valid.Name == name).Table;

    /// <summary>
    /// The conformance suite's directory, at the root of the repository, which the tests find from
    /// the directory that they run in, within it
    /// </summary>
    public static string Directory
    {
        get
        {
            for (var directory = new DirectoryInfo(AppContext.BaseDirectory); directory is not null; directory = directory.Parent)
            {
                var conformance = Path.Combine(directory.FullName, "conformance");
                if (File.Exists(Path.Combine(conformance, "cases.json")))
                {
                    return conformance;
                }
            }

            throw new DirectoryNotFoundException($"No conformance/cases.json above {AppContext.BaseDirectory}");
        }
    }

    /// <summary>Loads the cases.json in <paramref name="directory"/>, whose cases' files lie under it</summary>
    private static Suite Load(string directory)
    {
        using var json = JsonDocument.Parse(File.ReadAllBytes(Path.Combine(directory, "cases.json")));
        var root = json.RootElement;

        var valid = new List<ValidCase>();
        foreach (var description in root.GetProperty("valid").EnumerateArray())
        {
            var name = description.GetProperty("name").GetString()!;
            valid.Add(new ValidCase(name, ReadFile(directory, description), ParseTable(description.GetProperty("table"))));
        }

        return new Suite(valid, ChangedCases(directory, root.GetProperty("equivalent")), ChangedCases(directory, root.GetProperty("invalid")));
    }

    private static List<ChangedCase> ChangedCases(string directory, JsonElement cases)
    {
        var loaded = new List<ChangedCase>();
        foreach (var description in cases.EnumerateArray())
        {
            var field = description.TryGetProperty("field", out var index) ? index.GetInt32() : 0;
            loaded.Add(
                new ChangedCase(
                    description.GetProperty("name").GetString()!,
                    ReadFile(directory, description),
                    description.GetProperty("case").GetString()!,
                    field
                )
            );
        }

        return loaded;
    }

    private static byte[] ReadFile(string directory, JsonElement description) =>
        File.ReadAllBytes(Path.Combine(directory, description.GetProperty("file").GetString()!));

    private static Table ParseTable(JsonElement table)
    {
        var layout = table.GetProperty("layout");
        var offsetType = layout.GetProperty("offsetType").GetString() switch
        {
            "uint8" => OffsetType.UInt8,
            "uint16" => OffsetType.UInt16,
            "uint32" => OffsetType.UInt32,
            var other => throw new InvalidDataException($"{other} isn't an offset type"),
        };

        var version = table.TryGetProperty("version", out var value) ? value.GetUInt32() : 0;
        var fields = table.GetProperty("fields").EnumerateArray().Select(ParseField);
        int? missing = table.TryGetProperty("missing", out var count) ? count.GetInt32() : null;

        return new Table(
            new Layout(layout.GetProperty("sized").GetBoolean(), layout.GetProperty("versioned").GetBoolean(), offsetType),
            version,
            new Seq<Field>(fields)
        )
        {
            Missing = missing,
        };
    }

    /// <summary>Parses a field from JSON that holds 64-bit integers as strings and the floats that JSON has no number for by name.</summary>
    private static Field ParseField(JsonElement field)
    {
        var type = field.GetProperty("type").GetString()!;
        if (Scalars.Named(type) is { } scalarType)
        {
            return ParseScalar(scalarType, field.GetProperty("value"));
        }

        if (type.EndsWith("[]", StringComparison.Ordinal) && Scalars.Named(type[..^2]) is { } elementType)
        {
            var elements = field.GetProperty("value").EnumerateArray();
            return new ArrayField(elementType, new Seq<ulong>(elements.Select(element => ParseScalar(elementType, element).Bits)));
        }

        switch (type)
        {
            case "bool":
                return new Bool(field.GetProperty("value").GetBoolean());

            case "string":
                return new StringField(field.GetProperty("value").GetString()!);

            case "struct":
                return new StructField(new Seq<Scalar>(field.GetProperty("fields").EnumerateArray().Select(scalar => (Scalar)ParseField(scalar))));

            case "struct[]":
                var types = field.GetProperty("fields").EnumerateArray().Select(name => Scalars.Named(name.GetString()!)!.Value).ToArray();
                var items = field.GetProperty("items").EnumerateArray().Select(item => new Seq<Scalar>(item.EnumerateArray().Zip(types, (value, scalar) => ParseScalar(scalar, value))));
                return new StructArray(types, new Seq<Seq<Scalar>>(items));

            case "table":
                int? alignment = field.TryGetProperty("alignment", out var given) ? given.GetInt32() : null;
                return new TableField(ParseNested(field), alignment);

            case "vector":
                return new VectorField(new Seq<Nested>(field.GetProperty("items").EnumerateArray().Select(ParseNested)));

            case "absent":
                return new Absent();

            default:
                throw new InvalidDataException($"{type} isn't a field's type");
        }
    }

    private static Nested ParseNested(JsonElement nested)
    {
        var from = nested.GetProperty("from").GetString() switch
        {
            "builder" => Source.Builder,
            "bytes" => Source.Bytes,
            var other => throw new InvalidDataException($"{other} isn't a source"),
        };

        return new Nested(from, ParseTable(nested.GetProperty("table")));
    }

    /// <summary>
    /// Parses a scalar of a type, as cases.json and conformance/generated/shapes.json write them: 64-bit
    /// integers as strings, and the floats that JSON has no number for by name.
    /// </summary>
    public static Scalar ParseScalar(ScalarType type, JsonElement value)
    {
        ulong bits = type switch
        {
            ScalarType.Int8 or ScalarType.Int16 or ScalarType.Int32 => (ulong)value.GetInt64(),
            ScalarType.UInt8 or ScalarType.UInt16 or ScalarType.UInt32 => value.GetUInt64(),
            ScalarType.Int64 => (ulong)long.Parse(value.GetString()!, CultureInfo.InvariantCulture),
            ScalarType.UInt64 => ulong.Parse(value.GetString()!, CultureInfo.InvariantCulture),
            ScalarType.Float32 => BitConverter.SingleToUInt32Bits(ParseFloat32(value)),
            _ => BitConverter.DoubleToUInt64Bits(ParseFloat64(value)),
        };

        return new Scalar(type, Scalars.Truncate(type, bits));
    }

    /// <summary>
    /// Parses a float32, which is written as the float64 that equals it, so that it narrows exactly,
    /// and NaN as the quiet NaN that JavaScript writes, 0x7fc00000. .NET's own float.NaN has the sign
    /// bit set.
    /// </summary>
    private static float ParseFloat32(JsonElement value) =>
        value.ValueKind == JsonValueKind.String
            ? value.GetString() switch
            {
                "NaN" => BitConverter.UInt32BitsToSingle(0x7fc0_0000),
                "Infinity" => float.PositiveInfinity,
                "-Infinity" => float.NegativeInfinity,
                "-0" => -0.0f,
                var other => throw new InvalidDataException($"{other} isn't a float"),
            }
            : (float)value.GetDouble();

    /// <summary>Parses a float64, with NaN as the quiet NaN that JavaScript writes, 0x7ff8000000000000.</summary>
    private static double ParseFloat64(JsonElement value) =>
        value.ValueKind == JsonValueKind.String
            ? value.GetString() switch
            {
                "NaN" => BitConverter.UInt64BitsToDouble(0x7ff8_0000_0000_0000),
                "Infinity" => double.PositiveInfinity,
                "-Infinity" => double.NegativeInfinity,
                "-0" => -0.0,
                var other => throw new InvalidDataException($"{other} isn't a float"),
            }
            : value.GetDouble();
}
