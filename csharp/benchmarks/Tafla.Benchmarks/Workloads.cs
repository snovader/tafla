using System;
using MemoryPack;
using MessagePack;

namespace Tafla.Benchmarks;

// The comparison suites' workloads, with the values of the TypeScript comparison's seeded generator,
// as go/benchmarks/workloads ports it, so that the languages' numbers compare. The types carry the
// attributes that MessagePack and MemoryPack generate their code from, as Go's carry msgpack's tags.
// Same() compares floats by their bits, so that a codec reads back exactly what it wrote.

/// <summary>A small table: two float64s, an int64 and a 16-byte string.</summary>
[MessagePackObject(AllowPrivate = true)]
[MemoryPackable]
internal sealed partial record Record(
    [property: Key("x")] double X,
    [property: Key("y")] double Y,
    [property: Key("id")] long Id,
    [property: Key("name")] string Name
)
{
    public bool Same(Record other) =>
        Bits.Same(X, other.X) && Bits.Same(Y, other.Y) && Id == other.Id && Name == other.Name;
}

/// <summary>
/// A sized, versioned table of two float64s and a string, with a nested table of an int64 and three
/// columns of the same length. The columns are arrays, which every codec's reader fills from the
/// bytes, since a span of them can't be kept in a record.
/// </summary>
[MessagePackObject(AllowPrivate = true)]
[MemoryPackable]
internal sealed partial record Document(
    [property: Key("x")] double X,
    [property: Key("y")] double Y,
    [property: Key("name")] string Name,
    [property: Key("base")] long Base,
    [property: Key("keys")] uint[] Keys,
    [property: Key("a")] float[] A,
    [property: Key("b")] float[] B
)
{
    public bool Same(Document other) =>
        Bits.Same(X, other.X)
        && Bits.Same(Y, other.Y)
        && Name == other.Name
        && Base == other.Base
        && Keys.AsSpan().SequenceEqual(other.Keys)
        && Bits.Same(A, other.A)
        && Bits.Same(B, other.B);
}

/// <summary>A table of a float32 and a nested table of two float32s, or a struct of the three.</summary>
[MessagePackObject(AllowPrivate = true)]
internal readonly partial record struct Item(
    [property: Key("value")] float Value,
    [property: Key("first")] float First,
    [property: Key("second")] float Second
)
{
    public bool Same(Item other) =>
        Bits.Same(Value, other.Value) && Bits.Same(First, other.First) && Bits.Same(Second, other.Second);
}

/// <summary>A table holding a vector of item tables, or an array of item structs.</summary>
[MessagePackObject(AllowPrivate = true)]
[MemoryPackable]
internal sealed partial record ItemList([property: Key("items")] Item[] Items)
{
    public bool Same(ItemList other)
    {
        if (Items.Length != other.Items.Length)
        {
            return false;
        }

        for (var index = 0; index < Items.Length; index++)
        {
            if (!Items[index].Same(other.Items[index]))
            {
                return false;
            }
        }

        return true;
    }
}

/// <summary>A fixed-size record of a uint32 and two float32s.</summary>
[MessagePackObject(AllowPrivate = true)]
internal readonly partial record struct Sample(
    [property: Key("key")] uint Key,
    [property: Key("a")] float A,
    [property: Key("b")] float B
)
{
    public bool Same(Sample other) => Key == other.Key && Bits.Same(A, other.A) && Bits.Same(B, other.B);
}

internal static class Workloads
{
    /// <summary>The rows of the documents: a small one and a large one.</summary>
    public static readonly int[] DocumentRows = [100, 100_000];

    /// <summary>The items of the vector of nested tables and of the array of structs.</summary>
    public const int ListItems = 100;

    public static readonly Record TheRecord = new(59.3293, 18.0686, -631_152_000, "tafla-benchmarks");

    public static readonly Sample TheSample = new(2_743_826_400, 21.75f, 3.25f);

    public static Document NewDocument(int rows)
    {
        var random = new Xorshift(rows);

        // Keys below 2^24 take 4 bytes in protobuf's varints, like the floats
        var keys = new uint[rows];
        var a = new float[rows];
        var b = new float[rows];
        for (var row = 0; row < rows; row++)
        {
            keys[row] = (uint)Math.Floor(random.Next() * (1 << 24));
            a[row] = (float)((random.Next() - 0.5) * 100);
            b[row] = (float)((random.Next() - 0.5) * 100);
        }

        return new Document(TheRecord.X, TheRecord.Y, TheRecord.Name, TheRecord.Id, keys, a, b);
    }

    public static ItemList NewList(int count)
    {
        var random = new Xorshift(count);

        // Float32 values, so that every format reads them back exactly, evaluated in the order
        // they're written, as in JavaScript
        var items = new Item[count];
        for (var index = 0; index < count; index++)
        {
            var value = (float)(random.Next() * 30);
            var first = (float)(random.Next() * 4 - 2);
            var second = (float)(random.Next() * 50);
            items[index] = new Item(value, first, second);
        }

        return new ItemList(items);
    }

    /// <summary>
    /// Returns the sum of the values in their order, in a double, as the TypeScript comparison adds
    /// them to a number. Every codec sums its column with it, so that their sums have the same bits.
    /// </summary>
    public static double SumColumn(ReadOnlySpan<float> values)
    {
        var sum = 0.0;
        foreach (var value in values)
        {
            sum += value;
        }

        return sum;
    }

    /// <summary>Returns the sum of an item's three float32s, as doubles, as Go's item reads add them.</summary>
    public static double SumItem(float value, float first, float second) => (double)value + first + second;
}

/// <summary>
/// The TypeScript generator's xorshift, in [0, 1), whose 32-bit state JavaScript's shifts keep, which
/// makes the workloads' values and the lists suite's strings.
/// </summary>
internal sealed class Xorshift(int seed)
{
    private uint _state = 0x2545_f491 ^ (uint)seed;

    public double Next()
    {
        _state ^= _state << 13;
        _state ^= _state >> 17;
        _state ^= _state << 5;

        return _state / 4_294_967_296.0;
    }
}

/// <summary>Compares floats by their bits, so that -0 differs from 0 and every NaN keeps its payload.</summary>
internal static class Bits
{
    public static bool Same(double a, double b) =>
        BitConverter.DoubleToUInt64Bits(a) == BitConverter.DoubleToUInt64Bits(b);

    public static bool Same(float a, float b) =>
        BitConverter.SingleToUInt32Bits(a) == BitConverter.SingleToUInt32Bits(b);

    public static bool Same(ReadOnlySpan<float> a, ReadOnlySpan<float> b)
    {
        if (a.Length != b.Length)
        {
            return false;
        }

        for (var index = 0; index < a.Length; index++)
        {
            if (!Same(a[index], b[index]))
            {
                return false;
            }
        }

        return true;
    }
}
