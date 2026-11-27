using System;
using System.Collections.Generic;
using System.Globalization;
using System.Linq;

namespace Tafla.Benchmarks;

/// <summary>A list of strings that the lists suite writes and reads: its name in the report, and its strings.</summary>
internal sealed record ListShape(string Name, string[] Strings);

/// <summary>
/// The lists of strings of the lists suite: 10, 100 and 10,000 distinct strings of 16 or 256
/// characters of ASCII, or of 8 characters of 3 bytes in UTF-8. They're three of the eight kinds of
/// the TypeScript suite's <c>shapes.ts</c>, with its values: each shape's strings come from a
/// <see cref="Xorshift"/> of the seed that TypeScript gives it, in order, a character at a time.
/// </summary>
internal static class ListShapes
{
    public static readonly int[] Counts = [10, 100, 10_000];

    private const string Ascii = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";

    /// <summary>The first of the CJK ideographs, of 3 bytes in UTF-8, and of one UTF-16 code unit</summary>
    private const int ThreeBytes = 0x4e00;

    /// <summary>The kinds of strings of the TypeScript suite, whose number its seeds count</summary>
    private const int TypeScriptKinds = 8;

    /// <summary>Each kind of string, by its index among the TypeScript suite's kinds</summary>
    private static readonly Kind[] Kinds =
    [
        new(1, "16 chars of ASCII", 16, ThreeByteCharacters: false),
        new(2, "256 chars of ASCII", 256, ThreeByteCharacters: false),
        new(6, "8 chars of 3 bytes", 8, ThreeByteCharacters: true),
    ];

    /// <summary>Every shape, by count and then by kind, as the TypeScript suite orders them.</summary>
    public static List<ListShape> All()
    {
        var shapes = new List<ListShape>();
        for (var countIndex = 0; countIndex < Counts.Length; countIndex++)
        {
            foreach (var kind in Kinds)
            {
                shapes.Add(Shape(Counts[countIndex], kind, 1 + kind.Index + TypeScriptKinds * countIndex));
            }
        }

        return shapes;
    }

    /// <summary><paramref name="count"/> distinct strings of a kind, from a generator of <paramref name="seed"/></summary>
    private static ListShape Shape(int count, Kind kind, int seed)
    {
        var random = new Xorshift(seed);
        var characters = new char[kind.Length];
        var strings = new string[count];
        for (var index = 0; index < count; index++)
        {
            for (var at = 0; at < characters.Length; at++)
            {
                characters[at] = kind.ThreeByteCharacters
                    ? (char)(ThreeBytes + (int)Math.Floor(random.Next() * 0x5000))
                    : Ascii[(int)Math.Floor(random.Next() * Ascii.Length)];
            }

            strings[index] = new string(characters);
        }

        var name = $"{count.ToString("N0", CultureInfo.InvariantCulture)} × {kind.Name}";
        if (strings.Distinct().Count() != count)
        {
            throw new InvalidOperationException($"The strings of {name} aren't distinct");
        }

        return new ListShape(name, strings);
    }

    private sealed record Kind(int Index, string Name, int Length, bool ThreeByteCharacters);
}
