using Tafla.Benchmarks.Codecs;

namespace Tafla.Benchmarks.Tests;

// The lists suite's strings are the TypeScript suite's, which these were printed from, and every
// codec reads back each list exactly, as the suite checks before it times a codec. RoundTripTests
// runs the benchmarks' checks, which share the static Sink.
public class ListsTests
{
    /// <summary>
    /// Each shape's name, count, the first 16 characters of its first and last strings, and a 32-bit
    /// hash of every string's UTF-16 code units, as the TypeScript suite's <c>shapes.ts</c> made them
    /// </summary>
    public static TheoryData<string, int, string, string, uint> TypeScriptShapes =>
        new()
        {
            { "10 × 16 chars of ASCII", 10, "2FsyeXxp1ewxJijA", "CcfRsv23GwWuZIqm", 772_871_957 },
            { "10 × 256 chars of ASCII", 10, "2GYBM2rvPSVojjhN", "3VpjDEeuixVZfJJy", 112_022_847 },
            { "10 × 8 chars of 3 bytes", 10, "鑡絞眙蜩榩眳矝愞", "琑翘趠谎衬耂竎刓", 699_265_638 },
            { "100 × 16 chars of ASCII", 100, "2N45gJLJhyrBp817", "GuM8hMtpdCgpm337", 4_043_576_274 },
            { "100 × 256 chars of ASCII", 100, "2OC9KSUHzW0k790o", "e1d0Fdjo5UszbCUg", 1_275_637_513 },
            { "100 × 8 chars of 3 bytes", 100, "鑗蝝姘徃栲鈞甕顽", "鱵髩痧剻熉五憓荜", 249_289_010 },
            { "10,000 × 16 chars of ASCII", 10_000, "2VGBSOLG6JrrCjwD", "FzBrmc8Y0jcGbWah", 3_888_143_744 },
            { "10,000 × 256 chars of ASCII", 10_000, "2WgxnHUMGp0eUivR", "VU0lrgd6V0hWxlMq", 1_626_208_024 },
            { "10,000 × 8 chars of 3 bytes", 10_000, "鑶鄷攴臚鷑靆畖鱯", "淟坓惗聳侞忝鞂姩", 2_214_151_391 },
        };

    [Fact]
    public void HasTheTypeScriptShapesInTheirOrder() =>
        Assert.Equal(TypeScriptShapes.Select(row => (string)row.Data.Item1), ListShapes.All().Select(shape => shape.Name));

    [Theory]
    [MemberData(nameof(TypeScriptShapes))]
    public void GeneratesTheTypeScriptStrings(string name, int count, string first, string last, uint hash)
    {
        var strings = ListShapes.All().Single(shape => shape.Name == name).Strings;

        Assert.Equal(count, strings.Length);
        Assert.Equal(first, Start(strings[0]));
        Assert.Equal(last, Start(strings[^1]));
        Assert.Equal(hash, Hash(strings));
    }

    [Fact]
    public void EveryCodecReadsBackEveryList()
    {
        IStringListCodec[] codecs = [default(TaflaGenStringList), default(TaflaGenNameList)];
        foreach (var shape in ListShapes.All())
        {
            foreach (var codec in codecs)
            {
                var bytes = codec.SerializeStrings(shape.Strings);

                Assert.Equal(shape.Strings, codec.DeserializeStrings(bytes));
                Assert.Equal(shape.Strings[^1], codec.ReadLastString(bytes));
            }
        }
    }

    /// <summary>tafla-gen writes the tables that TableBuilder writes of the same lists, the vector of tables recorded in place</summary>
    [Fact]
    public void TaflaGenWritesTheBuildersBytes()
    {
        foreach (var shape in ListShapes.All())
        {
            var strings = new TableBuilder(1);
            strings.SetStringVector(shape.Strings);

            var names = new TableBuilder(1);
            names.SetVector(shape.Strings.Length, 1, shape.Strings, static (item, index, strings) => item.SetString(strings[index]));

            Assert.Equal(strings.ToArray(), default(TaflaGenStringList).SerializeStrings(shape.Strings));
            Assert.Equal(names.ToArray(), default(TaflaGenNameList).SerializeStrings(shape.Strings));
        }
    }

    /// <summary>The first 16 characters of a string, or all of a shorter one</summary>
    private static string Start(string value) => value.Length > 16 ? value[..16] : value;

    /// <summary>Hashes every string's UTF-16 code units, as the TypeScript script that printed the hashes did with 32-bit integers</summary>
    private static uint Hash(string[] strings)
    {
        var hash = 0u;
        foreach (var value in strings)
        {
            foreach (var character in value)
            {
                hash = unchecked(hash * 31 + character);
            }
        }

        return hash;
    }
}
