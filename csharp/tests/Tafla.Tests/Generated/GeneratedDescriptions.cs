using CsCheck;
using Tafla.Tests.Tables;

namespace Tafla.Tests.Generated;

/// <summary>
/// The fields of the messages, tables and structs that tafla-gen generates, in the conformance suite's
/// description model, so that <see cref="Build"/> writes the same tables by hand with
/// <see cref="TableBuilder"/>, for tests to compare with the bytes that the generated code writes, and
/// so that the values that the generated code reads compare with those written by their bits.
/// </summary>
/// <remarks>
/// <see cref="Of(sbyte)"/> and its overloads describe a value by its C# type, which is that of one
/// scalar type, so that a description reads as the fields that it describes.
/// </remarks>
public static class GeneratedDescriptions
{
    /// <summary>The layout of nested tables and of messages without attributes.</summary>
    public static readonly Layout DefaultLayout = Layout.Headerless(OffsetType.UInt32);

    public static Scalar Of(sbyte value) => new(ScalarType.Int8, (byte)value);

    public static Scalar Of(byte value) => new(ScalarType.UInt8, value);

    public static Scalar Of(short value) => new(ScalarType.Int16, (ushort)value);

    public static Scalar Of(ushort value) => new(ScalarType.UInt16, value);

    public static Scalar Of(int value) => new(ScalarType.Int32, (uint)value);

    public static Scalar Of(uint value) => new(ScalarType.UInt32, value);

    public static Scalar Of(long value) => new(ScalarType.Int64, (ulong)value);

    public static Scalar Of(ulong value) => new(ScalarType.UInt64, value);

    public static Scalar Of(float value) => new(ScalarType.Float32, BitConverter.SingleToUInt32Bits(value));

    public static Scalar Of(double value) => new(ScalarType.Float64, BitConverter.DoubleToUInt64Bits(value));

    public static Bool Of(bool value) => new(value);

    public static StringField Of(string value) => new(value);

    public static ArrayField Of(ReadOnlySpan<sbyte> values) => ArrayOf(ScalarType.Int8, values, Of);

    public static ArrayField Of(ReadOnlySpan<byte> values) => ArrayOf(ScalarType.UInt8, values, Of);

    public static ArrayField Of(ReadOnlySpan<short> values) => ArrayOf(ScalarType.Int16, values, Of);

    public static ArrayField Of(ReadOnlySpan<ushort> values) => ArrayOf(ScalarType.UInt16, values, Of);

    public static ArrayField Of(ReadOnlySpan<int> values) => ArrayOf(ScalarType.Int32, values, Of);

    public static ArrayField Of(ReadOnlySpan<uint> values) => ArrayOf(ScalarType.UInt32, values, Of);

    public static ArrayField Of(ReadOnlySpan<long> values) => ArrayOf(ScalarType.Int64, values, Of);

    public static ArrayField Of(ReadOnlySpan<ulong> values) => ArrayOf(ScalarType.UInt64, values, Of);

    public static ArrayField Of(ReadOnlySpan<float> values) => ArrayOf(ScalarType.Float32, values, Of);

    public static ArrayField Of(ReadOnlySpan<double> values) => ArrayOf(ScalarType.Float64, values, Of);

    /// <summary>A struct of its scalars, in the order of their offsets, as generated code writes its bytes.</summary>
    public static StructField Struct(params Scalar[] scalars) => new(scalars);

    /// <summary>An array of structs of scalars of the types, each of the scalars of its struct.</summary>
    public static StructArray StructArrayOf(ScalarType[] types, IEnumerable<StructField> structs)
    {
        var items = structs.Select(item => item.Scalars).ToArray();
        Assert.All(items, item => Assert.Equal(types, item.Select(scalar => scalar.Type)));

        return new StructArray(types, items);
    }

    /// <summary>A table nested from its builder at its default alignment, as generated code nests it.</summary>
    public static TableField Nested(params Field[] fields) => new(new Nested(Source.Builder, new Table(DefaultLayout, fields)), null);

    /// <summary>A vector of tables, each from its builder, as generated code writes it.</summary>
    public static VectorField Vector(IEnumerable<TableField> items) => new(new Seq<Nested>(items.Select(item => item.Nested)));

    /// <summary>A message's table, in a layout without a version.</summary>
    public static Table Message(Layout layout, params Field[] fields) => new(layout, fields);

    /// <summary>A message's table, in a versioned layout.</summary>
    public static Table Message(Layout layout, uint version, params Field[] fields) => new(layout, version, fields);

    private static ArrayField ArrayOf<T>(ScalarType type, ReadOnlySpan<T> values, Func<T, Scalar> scalar)
    {
        var bits = new ulong[values.Length];
        for (var index = 0; index < values.Length; index++)
        {
            bits[index] = scalar(values[index]).Bits;
        }

        return new ArrayField(type, bits);
    }
}

/// <summary>
/// Generators of the values of every scalar type, across their range, from the bits that
/// <see cref="Strategy"/> generates, and of strings and arrays.
/// </summary>
public static class Values
{
    public static Gen<sbyte> Int8s => Strategy.Bits(ScalarType.Int8).Select(bits => (sbyte)bits);

    public static Gen<byte> UInt8s => Strategy.Bits(ScalarType.UInt8).Select(bits => (byte)bits);

    public static Gen<short> Int16s => Strategy.Bits(ScalarType.Int16).Select(bits => (short)bits);

    public static Gen<ushort> UInt16s => Strategy.Bits(ScalarType.UInt16).Select(bits => (ushort)bits);

    public static Gen<int> Int32s => Strategy.Bits(ScalarType.Int32).Select(bits => (int)bits);

    public static Gen<uint> UInt32s => Strategy.Bits(ScalarType.UInt32).Select(bits => (uint)bits);

    public static Gen<long> Int64s => Strategy.Bits(ScalarType.Int64).Select(bits => (long)bits);

    public static Gen<ulong> UInt64s => Strategy.Bits(ScalarType.UInt64);

    public static Gen<float> Float32s =>
        Strategy.Bits(ScalarType.Float32).Select(bits => BitConverter.UInt32BitsToSingle((uint)bits));

    public static Gen<double> Float64s => Strategy.Bits(ScalarType.Float64).Select(BitConverter.UInt64BitsToDouble);

    public static Gen<string> Strings => Strategy.String(16);

    /// <summary>Arrays of up to 24 values, empty too.</summary>
    public static Gen<ReadOnlyMemory<T>> ArraysOf<T>(Gen<T> values) => ArraysOf(values, 24);

    /// <summary>Arrays of up to <paramref name="most"/> items, empty too.</summary>
    public static Gen<ReadOnlyMemory<T>> ArraysOf<T>(Gen<T> items, int most) =>
        items.Array[0, most].Select(array => (ReadOnlyMemory<T>)array);
}
