using Tafla.Tests.Tables;

namespace Tafla.Tests;

/// <summary>
/// Reads and rebuilds the tables of tafla's conformance suite, which the TypeScript runtime writes,
/// and its cases of presence, in conformance/presence/: tables with absent fields, and tables that a
/// reader of a later revision of their format reads, finding the fields that it knows after theirs
/// missing. See conformance/README.md at the root of the repository for the format of their
/// descriptions in cases.json.
/// </summary>
public class ConformanceTests
{
    private static Suite Suite => Suite.Instance;

    private static Suite Presence => Suite.Presence;

    public static TheoryData<string> ValidCases => [.. Suite.Valid.Select(valid => valid.Name)];

    public static TheoryData<string> EquivalentCases => [.. Suite.Equivalent.Select(equivalent => equivalent.Name)];

    public static TheoryData<string> InvalidCases => [.. Suite.Invalid.Select(invalid => invalid.Name)];

    public static TheoryData<string> ValidCasesOfPresence => [.. Presence.Valid.Select(valid => valid.Name)];

    public static TheoryData<string> InvalidCasesOfPresence => [.. Presence.Invalid.Select(invalid => invalid.Name)];

    [Theory]
    [MemberData(nameof(ValidCases))]
    public void ReadsEveryValidTable(string name) => ReadsValidTable(Suite, name);

    [Theory]
    [MemberData(nameof(ValidCases))]
    public void RebuildsEveryValidTableByteForByte(string name) => RebuildsValidTable(Suite, name);

    /// <summary>
    /// Rebuilds every valid table with the tables nested in it that a builder writes in place,
    /// without a header and with their parent's offset type, recorded in place, byte for byte
    /// </summary>
    [Theory]
    [MemberData(nameof(ValidCases))]
    public void RebuildsEveryValidTableRecordedInPlaceByteForByte(string name) => RebuildsValidTableInPlace(Suite, name);

    /// <summary>
    /// Writes every valid table whose nested tables a writer writes in place, without a header and
    /// with their parent's offset type, with a sizer and a writer, byte for byte
    /// </summary>
    [Theory]
    [MemberData(nameof(ValidCases))]
    public void RewritesEveryValidTableNestedInPlaceByteForByte(string name) => RewritesValidTable(Suite, name);

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
    public void ReadsInvalidTablesUpToTheFieldThatFails(string name) => ReadsInvalidTable(Suite, name);

    [Fact]
    public void LoadsEveryCase()
    {
        Assert.NotEmpty(Suite.Valid);
        Assert.NotEmpty(Suite.Equivalent);
        Assert.NotEmpty(Suite.Invalid);
    }

    // The cases of presence. Their absent fields must have the offset 0, and a table's count of
    // missing fields must be what its description gives.

    [Theory]
    [MemberData(nameof(ValidCasesOfPresence))]
    public void ReadsEveryValidTableOfPresence(string name) => ReadsValidTable(Presence, name);

    [Theory]
    [MemberData(nameof(ValidCasesOfPresence))]
    public void RebuildsEveryValidTableOfPresenceByteForByte(string name) => RebuildsValidTable(Presence, name);

    [Theory]
    [MemberData(nameof(ValidCasesOfPresence))]
    public void RebuildsEveryValidTableOfPresenceRecordedInPlaceByteForByte(string name) =>
        RebuildsValidTableInPlace(Presence, name);

    [Theory]
    [MemberData(nameof(ValidCasesOfPresence))]
    public void RewritesEveryValidTableOfPresenceNestedInPlaceByteForByte(string name) => RewritesValidTable(Presence, name);

    [Theory]
    [MemberData(nameof(InvalidCasesOfPresence))]
    public void ReadsInvalidTablesOfPresenceUpToTheFieldThatFails(string name) => ReadsInvalidTable(Presence, name);

    [Fact]
    public void LoadsEveryCaseOfPresence()
    {
        Assert.NotEmpty(Presence.Valid);
        Assert.Empty(Presence.Equivalent);
        Assert.NotEmpty(Presence.Invalid);
    }

    private static void ReadsValidTable(Suite suite, string name)
    {
        var valid = suite.Valid.Single(valid => valid.Name == name);

        Assert.Equal(valid.Table, Read.Table(valid.Bytes, valid.Table));
    }

    private static void RebuildsValidTable(Suite suite, string name)
    {
        var valid = suite.Valid.Single(valid => valid.Name == name);

        Assert.Equal(valid.Bytes, valid.Table.Bytes());
    }

    private static void RebuildsValidTableInPlace(Suite suite, string name)
    {
        var valid = suite.Valid.Single(valid => valid.Name == name);

        Assert.Equal(valid.Bytes, valid.Table.BuiltInPlace().ToArray());
    }

    private static void RewritesValidTable(Suite suite, string name)
    {
        var valid = suite.Valid.Single(valid => valid.Name == name);
        if (valid.Table.NestedInPlace() != valid.Table)
        {
            return;
        }

        Assert.Equal(valid.Bytes, valid.Table.Written());
    }

    private static void ReadsInvalidTable(Suite suite, string name)
    {
        var invalid = suite.Invalid.Single(invalid => invalid.Name == name);
        var table = suite.TableOf(invalid.Case);

        for (var index = 0; index < invalid.Field; index++)
        {
            var expected = table.Fields[index];
            Assert.Equal(expected, Read.RootField(invalid.Bytes, table.Layout, index, expected));
        }

        var field = table.Fields[invalid.Field];
        Assert.Throws<DecodeException>(() => Read.RootField(invalid.Bytes, table.Layout, invalid.Field, field));
    }
}
