using System.Collections;
using System.Globalization;

namespace Tafla.Tests.Tables;

// The tests' one model of tables: the conformance suite's descriptions, which Cases parses from
// cases.json and Strategy generates, with the builder and reader that every test builds and reads
// them with. See conformance/README.md at the root of the repository.
//
// Scalars hold their value's bits, as many as the type is wide, so that equal descriptions hold
// floats of the same bits: NaN equals itself, and -0 doesn't equal 0.

/// <summary>The type of a scalar, of which structs and arrays are made.</summary>
public enum ScalarType
{
    Int8,
    UInt8,
    Int16,
    UInt16,
    Int32,
    UInt32,
    Int64,
    UInt64,
    Float32,
    Float64,
}

/// <summary>A table: its layout, its version, present only when it's versioned, and its fields, in the order that they're set.</summary>
public sealed record Table(Layout Layout, uint Version, Seq<Field> Fields)
{
    public Table(Layout layout, params Field[] fields)
        : this(layout, 0, fields) { }
}

/// <summary>A table's layout, as a description gives it.</summary>
public sealed record Layout(bool Sized, bool Versioned, OffsetType OffsetType)
{
    /// <summary>A layout without header slots, which tables nested with SetTable have.</summary>
    public static Layout Headerless(OffsetType offsetType) => new(false, false, offsetType);

    /// <summary>
    /// The runtime's layout, with <paramref name="version"/> when it's versioned. A reader reads
    /// tables of any version, so it needs none.
    /// </summary>
    public TableLayout ToTableLayout(uint version = 0) =>
        new()
        {
            Sized = Sized,
            Version = Versioned ? version : null,
            OffsetType = OffsetType,
        };
}

/// <summary>A field of each type that a table holds, with its value.</summary>
public abstract record Field;

/// <summary>A scalar field, or a struct's scalar, of the bits of its value.</summary>
public sealed record Scalar(ScalarType Type, ulong Bits) : Field
{
    public override string ToString() =>
        $"{Type}({Convert.ToString(Scalars.Value(Type, Bits), CultureInfo.InvariantCulture)})";
}

/// <summary>
/// A bool, which the runtimes have no API for: it's written as the uint8 1 or 0, and read as true
/// when its byte isn't 0. It's a field only, rather than a scalar of structs, and an array of bools
/// is a <see cref="BoolArray"/>.
/// </summary>
public sealed record Bool(bool Value) : Field;

/// <summary>An array of scalars of a type, of their bits.</summary>
public sealed record ArrayField(ScalarType Type, Seq<ulong> Bits) : Field;

/// <summary>
/// An array of bools, which the runtime writes as a uint8 array of 1 and 0 with SetBoolArray, and
/// GetBoolArray reads as true where a byte isn't 0.
/// </summary>
public sealed record BoolArray(Seq<bool> Values) : Field;

public sealed record StringField(string Value) : Field;

/// <summary>A vector of strings, empty too, and of empty strings.</summary>
public sealed record StringVectorField(Seq<string> Values) : Field;

/// <summary>A struct of scalars, one after another.</summary>
public sealed record StructField(Seq<Scalar> Scalars) : Field;

/// <summary>An array of structs of scalars of types, as the scalars of each struct.</summary>
public sealed record StructArray(Seq<ScalarType> Types, Seq<Seq<Scalar>> Items) : Field;

/// <summary>A table nested with SetTable, aligned explicitly when an alignment is given.</summary>
public sealed record TableField(Nested Nested, int? Alignment) : Field;

public sealed record VectorField(Seq<Nested> Items) : Field;

/// <summary>A table nested from its builder or from the bytes that its builder writes.</summary>
public sealed record Nested(Source From, Table Table);

/// <summary>What a nested table is set from.</summary>
public enum Source
{
    Builder,
    Bytes,
}

/// <summary>
/// An immutable list that equals another of equal items, so that records that hold it compare by
/// value, and prints its items.
/// </summary>
public readonly struct Seq<T> : IEquatable<Seq<T>>, IReadOnlyList<T>
{
    private readonly T[]? _items;

    public Seq(IEnumerable<T> items)
    {
        _items = [.. items];
    }

    private T[] Items => _items ?? [];

    public int Count => Items.Length;

    public T this[int index] => Items[index];

    public static implicit operator Seq<T>(T[] items) => new(items);

    public static implicit operator Seq<T>(List<T> items) => new(items);

    public bool Equals(Seq<T> other) => Items.AsSpan().SequenceEqual(other.Items);

    public override bool Equals(object? obj) => obj is Seq<T> other && Equals(other);

    public override int GetHashCode()
    {
        var hash = new HashCode();
        foreach (var item in Items)
        {
            hash.Add(item);
        }

        return hash.ToHashCode();
    }

    public override string ToString() => $"[{string.Join(", ", Items)}]";

    public IEnumerator<T> GetEnumerator() => ((IEnumerable<T>)Items).GetEnumerator();

    IEnumerator IEnumerable.GetEnumerator() => GetEnumerator();

    public static bool operator ==(Seq<T> left, Seq<T> right) => left.Equals(right);

    public static bool operator !=(Seq<T> left, Seq<T> right) => !left.Equals(right);
}
