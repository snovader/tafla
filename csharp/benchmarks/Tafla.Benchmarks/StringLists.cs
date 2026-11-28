using System.Collections.Generic;
using Tafla.Benchmarks.Codecs;

namespace Tafla.Benchmarks;

/// <summary>
/// The lists suite's benchmarks: for each of the <see cref="ListShapes"/>, writing the list, reading
/// every string and reading the last string, as a vector of strings, by tafla-gen's
/// <see cref="TaflaGenStringList"/>, first and again as an A/A pair, and as a vector of tables of one
/// string each, by <see cref="TaflaGenNameList"/>.
/// </summary>
internal static class StringLists
{
    /// <summary>The report's table of the lists of strings</summary>
    public const string Group = "lists";

    public static List<Benchmark> All()
    {
        var all = new List<Benchmark>();
        foreach (var shape in ListShapes.All())
        {
            all.AddRange(Benchmarks(shape));
        }

        return all;
    }

    private static Benchmark[] Benchmarks(ListShape shape)
    {
        var strings = shape.Strings;
        var stringList = default(TaflaGenStringList).SerializeStrings(strings);
        var nameList = default(TaflaGenNameList).SerializeStrings(strings);
        var last = strings[^1];

        Variant[] serialize =
        [
            .. Loops.WithCopy(new SerializeStrings<TaflaGenStringList>(strings)),
            Loops.Of<SerializeStrings<TaflaGenNameList>, Original>(new(strings)),
        ];

        Variant[] deserialize =
        [
            .. Loops.WithCopy(new DeserializeStrings<TaflaGenStringList>(stringList, strings)),
            Loops.Of<DeserializeStrings<TaflaGenNameList>, Original>(new(nameList, strings)),
        ];

        Variant[] readLast =
        [
            .. Loops.WithCopy(new ReadLastString<TaflaGenStringList>(stringList, last)),
            Loops.Of<ReadLastString<TaflaGenNameList>, Original>(new(nameList, last)),
        ];

        return
        [
            new(Group, $"{shape.Name}: serialize", serialize),
            new(Group, $"{shape.Name}: deserialize", deserialize),
            new(Group, $"{shape.Name}: read the last string", readLast),
        ];
    }
}
