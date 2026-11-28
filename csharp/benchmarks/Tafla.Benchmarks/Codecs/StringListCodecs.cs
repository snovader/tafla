using System.Runtime.CompilerServices;
using Gen = global::Lists;

namespace Tafla.Benchmarks.Codecs;

/// <summary>Encodes a list of strings, as the lists suite writes and reads it.</summary>
internal interface IStringListCodec : ICodec
{
    byte[] SerializeStrings(string[] strings);

    /// <summary>Reads every string, in order.</summary>
    string[] DeserializeStrings(byte[] bytes);

    /// <summary>Reads the last string, without reading the others.</summary>
    string ReadLastString(byte[] bytes);
}

/// <summary>
/// A list of strings as a vector of strings, <c>lists.tafla</c>'s <c>StringList</c>, written by the code
/// that tafla-gen generates in <c>Generated/Lists/</c>, which counts it with a <see cref="TableSizer"/>
/// and encodes each string straight into the table with a <see cref="TableWriter"/>, and read through
/// its getter's <see cref="StringVector"/>: every string with <see cref="StringVector.ToArray"/>, and
/// the last from its two ends with the indexer.
/// </summary>
internal readonly struct TaflaGenStringList : IStringListCodec
{
    public string Name => "tafla-gen [string]";

    [MethodImpl(MethodImplOptions.NoInlining)]
    public byte[] SerializeStrings(string[] strings) => new Gen.StringListFields { Items = strings }.Serialize();

    [MethodImpl(MethodImplOptions.NoInlining)]
    public string[] DeserializeStrings(byte[] bytes) => new Gen.StringList(bytes).Items.ToArray();

    [MethodImpl(MethodImplOptions.NoInlining)]
    public string ReadLastString(byte[] bytes)
    {
        var items = new Gen.StringList(bytes).Items;
        return items[items.Count - 1];
    }
}

/// <summary>
/// A list of strings as a vector of tables of one string each, <c>lists.tafla</c>'s <c>NameList</c>,
/// which is how a schema held a list of strings before the vector of strings, written and read by the
/// code that tafla-gen generates. It creates the generated fields of each table in each call, as the
/// comparison's tafla-gen codec creates the vector's items, and reads every string in order through
/// the view's enumerator, which reads each table's end once, and the last through its indexer.
/// </summary>
internal readonly struct TaflaGenNameList : IStringListCodec
{
    public string Name => "tafla-gen [Name]";

    [MethodImpl(MethodImplOptions.NoInlining)]
    public byte[] SerializeStrings(string[] strings)
    {
        var items = new Gen.NameFields[strings.Length];
        for (var index = 0; index < items.Length; index++)
        {
            items[index] = new Gen.NameFields { Value = strings[index] };
        }

        return new Gen.NameListFields { Items = items }.Serialize();
    }

    [MethodImpl(MethodImplOptions.NoInlining)]
    public string[] DeserializeStrings(byte[] bytes)
    {
        var items = new Gen.NameList(bytes).Items;
        var strings = new string[items.Count];

        var index = 0;
        foreach (var item in items)
        {
            strings[index++] = item.Value;
        }

        return strings;
    }

    [MethodImpl(MethodImplOptions.NoInlining)]
    public string ReadLastString(byte[] bytes)
    {
        var items = new Gen.NameList(bytes).Items;
        return items[items.Count - 1].Value;
    }
}
