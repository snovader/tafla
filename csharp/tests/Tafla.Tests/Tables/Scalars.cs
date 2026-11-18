using System.Buffers.Binary;

namespace Tafla.Tests.Tables;

/// <summary>The sizes, names and bits of scalar types, and their setters and getters of each runtime type.</summary>
public static class Scalars
{
    /// <summary>Every scalar type.</summary>
    public static readonly ScalarType[] All = Enum.GetValues<ScalarType>();

    /// <summary>Returns the scalar's size in bytes, which is also its alignment in an array.</summary>
    public static int Size(this ScalarType type) =>
        type switch
        {
            ScalarType.Int8 or ScalarType.UInt8 => 1,
            ScalarType.Int16 or ScalarType.UInt16 => 2,
            ScalarType.Int32 or ScalarType.UInt32 or ScalarType.Float32 => 4,
            _ => 8,
        };

    /// <summary>Returns the type's name in descriptions, such as <c>uint16</c>.</summary>
    public static string Name(this ScalarType type) => type.ToString().ToLowerInvariant();

    /// <summary>Returns the scalar type named <paramref name="name"/> in descriptions, if any.</summary>
    public static ScalarType? Named(string name) => All.Select(type => (ScalarType?)type).FirstOrDefault(type => type!.Value.Name() == name);

    /// <summary>Returns the bits of a value of the type, as wide as the type: its low bits.</summary>
    public static ulong Truncate(ScalarType type, ulong bits) =>
        type.Size() == 8 ? bits : bits & ((1UL << (8 * type.Size())) - 1);

    /// <summary>Returns the value of the type that its bits hold, for messages.</summary>
    public static object Value(ScalarType type, ulong bits) =>
        type switch
        {
            ScalarType.Int8 => (sbyte)bits,
            ScalarType.UInt8 => (byte)bits,
            ScalarType.Int16 => (short)bits,
            ScalarType.UInt16 => (ushort)bits,
            ScalarType.Int32 => (int)bits,
            ScalarType.UInt32 => (uint)bits,
            ScalarType.Int64 => (long)bits,
            ScalarType.UInt64 => bits,
            ScalarType.Float32 => BitConverter.UInt32BitsToSingle((uint)bits),
            _ => BitConverter.UInt64BitsToDouble(bits),
        };

    /// <summary>Returns the size of a struct of scalars of types: the sum of theirs.</summary>
    public static int StructSize(IEnumerable<ScalarType> types) => types.Sum(Size);

    /// <summary>Returns the alignment of an array of structs of scalars of types: the largest of their sizes.</summary>
    public static int StructAlignment(IEnumerable<ScalarType> types) => types.Select(Size).DefaultIfEmpty(1).Max();

    /// <summary>Returns the bytes of a struct's scalars: their little-endian bits, one after another.</summary>
    public static byte[] StructBytes(IEnumerable<Scalar> scalars)
    {
        var bytes = new List<byte>();
        Span<byte> value = stackalloc byte[sizeof(ulong)];

        foreach (var scalar in scalars)
        {
            BinaryPrimitives.WriteUInt64LittleEndian(value, scalar.Bits);
            bytes.AddRange(value[..scalar.Type.Size()]);
        }

        return [.. bytes];
    }

    /// <summary>Reads the scalars of a struct of types at their offsets in its bytes.</summary>
    public static Seq<Scalar> ReadStruct(ReadOnlySpan<byte> bytes, IEnumerable<ScalarType> types)
    {
        var scalars = new List<Scalar>();
        Span<byte> value = stackalloc byte[sizeof(ulong)];

        var offset = 0;
        foreach (var type in types)
        {
            value.Clear();
            bytes.Slice(offset, type.Size()).CopyTo(value);
            scalars.Add(new Scalar(type, BinaryPrimitives.ReadUInt64LittleEndian(value)));
            offset += type.Size();
        }

        return scalars;
    }

    /// <summary>Sets a scalar field with the builder's setter of its type.</summary>
    public static void Set(TableBuilder builder, Scalar scalar)
    {
        var bits = scalar.Bits;
        switch (scalar.Type)
        {
            case ScalarType.Int8:
                builder.SetInt8((sbyte)bits);
                break;
            case ScalarType.UInt8:
                builder.SetUInt8((byte)bits);
                break;
            case ScalarType.Int16:
                builder.SetInt16((short)bits);
                break;
            case ScalarType.UInt16:
                builder.SetUInt16((ushort)bits);
                break;
            case ScalarType.Int32:
                builder.SetInt32((int)bits);
                break;
            case ScalarType.UInt32:
                builder.SetUInt32((uint)bits);
                break;
            case ScalarType.Int64:
                builder.SetInt64((long)bits);
                break;
            case ScalarType.UInt64:
                builder.SetUInt64(bits);
                break;
            case ScalarType.Float32:
                builder.SetFloat32(BitConverter.UInt32BitsToSingle((uint)bits));
                break;
            default:
                builder.SetFloat64(BitConverter.UInt64BitsToDouble(bits));
                break;
        }
    }

    /// <summary>Reads a scalar of a type at a position with the buffer's getter of the type.</summary>
    public static Scalar Read(ByteBuffer buffer, ScalarType type, int position)
    {
        ulong bits = type switch
        {
            ScalarType.Int8 => (byte)buffer.GetInt8(position),
            ScalarType.UInt8 => buffer.GetUInt8(position),
            ScalarType.Int16 => (ushort)buffer.GetInt16(position),
            ScalarType.UInt16 => buffer.GetUInt16(position),
            ScalarType.Int32 => (uint)buffer.GetInt32(position),
            ScalarType.UInt32 => buffer.GetUInt32(position),
            ScalarType.Int64 => (ulong)buffer.GetInt64(position),
            ScalarType.UInt64 => buffer.GetUInt64(position),
            ScalarType.Float32 => BitConverter.SingleToUInt32Bits(buffer.GetFloat32(position)),
            _ => BitConverter.DoubleToUInt64Bits(buffer.GetFloat64(position)),
        };

        return new Scalar(type, bits);
    }

    /// <summary>Sets an array field with the builder's setter of its type.</summary>
    public static void SetArray(TableBuilder builder, ArrayField array)
    {
        var bits = array.Bits;
        switch (array.Type)
        {
            case ScalarType.Int8:
                builder.SetInt8Array(Values(bits, value => (sbyte)value));
                break;
            case ScalarType.UInt8:
                builder.SetUInt8Array(Values(bits, value => (byte)value));
                break;
            case ScalarType.Int16:
                builder.SetInt16Array(Values(bits, value => (short)value));
                break;
            case ScalarType.UInt16:
                builder.SetUInt16Array(Values(bits, value => (ushort)value));
                break;
            case ScalarType.Int32:
                builder.SetInt32Array(Values(bits, value => (int)value));
                break;
            case ScalarType.UInt32:
                builder.SetUInt32Array(Values(bits, value => (uint)value));
                break;
            case ScalarType.Int64:
                builder.SetInt64Array(Values(bits, value => (long)value));
                break;
            case ScalarType.UInt64:
                builder.SetUInt64Array(Values(bits, value => value));
                break;
            case ScalarType.Float32:
                builder.SetFloat32Array(Values(bits, value => BitConverter.UInt32BitsToSingle((uint)value)));
                break;
            default:
                builder.SetFloat64Array(Values(bits, BitConverter.UInt64BitsToDouble));
                break;
        }
    }

    /// <summary>Reads an array of a type at a position with the buffer's getter of the type.</summary>
    public static ArrayField ReadArray(ByteBuffer buffer, ScalarType type, int position)
    {
        var bits = type switch
        {
            ScalarType.Int8 => Bits(buffer.GetInt8Array(position), value => (byte)value),
            ScalarType.UInt8 => Bits(buffer.GetUInt8Array(position), value => value),
            ScalarType.Int16 => Bits(buffer.GetInt16Array(position), value => (ushort)value),
            ScalarType.UInt16 => Bits(buffer.GetUInt16Array(position), value => value),
            ScalarType.Int32 => Bits(buffer.GetInt32Array(position), value => (uint)value),
            ScalarType.UInt32 => Bits(buffer.GetUInt32Array(position), value => value),
            ScalarType.Int64 => Bits(buffer.GetInt64Array(position), value => (ulong)value),
            ScalarType.UInt64 => Bits(buffer.GetUInt64Array(position), value => value),
            ScalarType.Float32 => Bits(buffer.GetFloat32Array(position), value => BitConverter.SingleToUInt32Bits(value)),
            _ => Bits(buffer.GetFloat64Array(position), BitConverter.DoubleToUInt64Bits),
        };

        return new ArrayField(type, bits);
    }

    private static T[] Values<T>(Seq<ulong> bits, Func<ulong, T> value) => [.. bits.Select(value)];

    private static Seq<ulong> Bits<T>(ReadOnlySpan<T> values, Func<T, ulong> bits)
    {
        var all = new ulong[values.Length];
        for (var index = 0; index < values.Length; index++)
        {
            all[index] = bits(values[index]);
        }

        return all;
    }
}
