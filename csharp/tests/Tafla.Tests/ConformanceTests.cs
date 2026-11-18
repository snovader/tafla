using Tafla.Tests.Tables;

namespace Tafla.Tests;

/// <summary>
/// Reads and rebuilds the tables of tafla's conformance suite, which the TypeScript runtime writes.
/// See conformance/README.md at the root of the repository for the format of their descriptions in
/// cases.json.
/// </summary>
public class ConformanceTests
{
    private static Suite Suite => Suite.Instance;

    public static TheoryData<string> ValidCases => [.. Suite.Valid.Select(valid => valid.Name)];

    public static TheoryData<string> EquivalentCases => [.. Suite.Equivalent.Select(equivalent => equivalent.Name)];

    public static TheoryData<string> InvalidCases => [.. Suite.Invalid.Select(invalid => invalid.Name)];

    [Theory]
    [MemberData(nameof(ValidCases))]
    public void ReadsEveryValidTable(string name)
    {
        var valid = Suite.Valid.Single(valid => valid.Name == name);

        Assert.Equal(valid.Table, Read.Table(valid.Bytes, valid.Table));
    }

    [Theory]
    [MemberData(nameof(ValidCases))]
    public void RebuildsEveryValidTableByteForByte(string name)
    {
        var valid = Suite.Valid.Single(valid => valid.Name == name);

        Assert.Equal(valid.Bytes, valid.Table.Bytes());
    }

    /// <summary>
    /// Writes every valid table whose nested tables a writer writes in place, without a header and
    /// with their parent's offset type, with a sizer and a writer, byte for byte
    /// </summary>
    [Theory]
    [MemberData(nameof(ValidCases))]
    public void RewritesEveryValidTableNestedInPlaceByteForByte(string name)
    {
        var valid = Suite.Valid.Single(valid => valid.Name == name);
        if (valid.Table.NestedInPlace() != valid.Table)
        {
            return;
        }

        Assert.Equal(valid.Bytes, valid.Table.Written());
    }

    /// <summary>
    /// Reads every equivalent table, whose bytes differ from its valid case's, as the valid case's
    /// description, header included
    /// </summary>
    [Theory]
    [MemberData(nameof(EquivalentCases))]
    public void ReadsEquivalentTablesAsTheirValidCase(string name)
    {
        var equivalent = Suite.Equivalent.Single(equivalent => equivalent.Name == name);
        var table = Suite.TableOf(equivalent.Case);

        Assert.Equal(table, Read.Table(equivalent.Bytes, table));
    }

    /// <summary>
    /// Reads the root fields before the invalid case's field, which must hold their described values,
    /// and then the field, which must fail with a decode error
    /// </summary>
    [Theory]
    [MemberData(nameof(InvalidCases))]
    public void ReadsInvalidTablesUpToTheFieldThatFails(string name)
    {
        var invalid = Suite.Invalid.Single(invalid => invalid.Name == name);
        var table = Suite.TableOf(invalid.Case);

        for (var index = 0; index < invalid.Field; index++)
        {
            var expected = table.Fields[index];
            Assert.Equal(expected, Read.RootField(invalid.Bytes, table.Layout, index, expected));
        }

        var field = table.Fields[invalid.Field];
        Assert.Throws<DecodeException>(() => Read.RootField(invalid.Bytes, table.Layout, invalid.Field, field));
    }

    [Fact]
    public void LoadsEveryCase()
    {
        Assert.NotEmpty(Suite.Valid);
        Assert.NotEmpty(Suite.Equivalent);
        Assert.NotEmpty(Suite.Invalid);
    }
}
