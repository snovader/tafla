using System.Buffers.Binary;
using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;

namespace Tafla.Tests.Tables;

/// <summary>The sizes, names and bits of scalar types, and their setters and getters of each runtime type.</summary>
/// <remarks>
/// 32-bit x86 returns floats on the x87 stack, whose loads quiet a signalling NaN, so the tests never
/// pass a float through a method's return there, as <see cref="BitConverter.UInt32BitsToSingle"/> is
/// in their unoptimized build. They make floats from their bits in memory, which passing a float to
/// a method keeps, and read arrays as bits. Only the getters of scalar floats return them, which
/// <see cref="ReadsAs"/> allows for.
/// </remarks>
public static class Scalars
{
    /// <summary>
    /// Whether a float that a method returns may be quieted: in a 32-bit x86 process, which returns
    /// floats on the x87 stack.
    /// </summary>
    public static readonly bool QuietsReturnedFloats = RuntimeInformation.ProcessArchitecture == Architecture.X86;

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

    /// <summary>
    /// Returns whether a getter that returned the bits <paramref name="read"/> read the scalar of the
    /// type of bits <paramref name="described"/>: the same bits, or a signalling NaN quieted where a
    /// returned float may be, as csharp/README.md says <c>GetFloat32()</c> and <c>GetFloat64()</c> return
    /// one on 32-bit x86.
    /// </summary>
    public static bool ReadsAs(ScalarType type, ulong described, ulong read) =>
        read == described || (QuietsReturnedFloats && IsSignallingNaN(type, described) && read == (described | QuietBit(type)));

    /// <summary>The bit of a float type's fraction that is set in a quiet NaN and clear in a signalling one: its top bit</summary>
    private static ulong QuietBit(ScalarType type) => type == ScalarType.Float32 ? 1UL << 22 : 1UL << 51;

    /// <summary>Whether the bits are a float type's signalling NaN: an exponent of all ones, and a fraction that isn't 0, with its quiet bit clear</summary>
    private static bool IsSignallingNaN(ScalarType type, ulong bits) =>
        type switch
        {
            ScalarType.Float32 => (bits & 0x7f80_0000) == 0x7f80_0000 && (bits & 0x007f_ffff) != 0 && (bits & QuietBit(type)) == 0,
            ScalarType.Float64 => (bits & 0x7ff0_0000_0000_0000) == 0x7ff0_0000_0000_0000
                && (bits & 0x000f_ffff_ffff_ffff) != 0
                && (bits & QuietBit(type)) == 0,
            _ => false,
        };

    /// <summary>Returns the float32s of the low bits of each value, made in memory rather than returned.</summary>
    public static ReadOnlySpan<float> Float32s(IEnumerable<ulong> bits) => MemoryMarshal.Cast<uint, float>(bits.Select(value => (uint)value).ToArray());

    /// <summary>Returns the float64s of the bits, made in memory rather than returned.</summary>
    public static ReadOnlySpan<double> Float64s(IEnumerable<ulong> bits) => MemoryMarshal.Cast<ulong, double>(bits.ToArray());

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

    /// <summary>Sets a scalar field with the setter of its type.</summary>
    public static void Set<T>(T target, Scalar scalar)
        where T : ITarget, allows ref struct
    {
        var bits = scalar.Bits;
        switch (scalar.Type)
        {
            case ScalarType.Int8:
                target.SetInt8((sbyte)bits);
                break;
            case ScalarType.UInt8:
                target.SetUInt8((byte)bits);
                break;
            case ScalarType.Int16:
                target.SetInt16((short)bits);
                break;
            case ScalarType.UInt16:
                target.SetUInt16((ushort)bits);
                break;
            case ScalarType.Int32:
                target.SetInt32((int)bits);
                break;
            case ScalarType.UInt32:
                target.SetUInt32((uint)bits);
                break;
            case ScalarType.Int64:
                target.SetInt64((long)bits);
                break;
            case ScalarType.UInt64:
                target.SetUInt64(bits);
                break;
            case ScalarType.Float32:
                // From its bits in memory, as all floats are made
                var single = (uint)bits;
                target.SetFloat32(Unsafe.As<uint, float>(ref single));
                break;
            default:
                target.SetFloat64(Unsafe.As<ulong, double>(ref bits));
                break;
        }
    }

    /// <summary>
    /// Reads the scalar that <paramref name="described"/> describes at a position, as <see cref="Read"/>
    /// does, and returns it as described when it <see cref="ReadsAs"/> it.
    /// </summary>
    public static Scalar ReadAs(TableReader reader, Scalar described, int position)
    {
        var read = Read(reader, described.Type, position);
        return ReadsAs(described.Type, described.Bits, read.Bits) ? described : read;
    }

    /// <summary>Reads a scalar of a type at a position with the reader's getter of the type.</summary>
    public static Scalar Read(TableReader reader, ScalarType type, int position)
    {
        ulong bits = type switch
        {
            ScalarType.Int8 => (byte)reader.GetInt8(position),
            ScalarType.UInt8 => reader.GetUInt8(position),
            ScalarType.Int16 => (ushort)reader.GetInt16(position),
            ScalarType.UInt16 => reader.GetUInt16(position),
            ScalarType.Int32 => (uint)reader.GetInt32(position),
            ScalarType.UInt32 => reader.GetUInt32(position),
            ScalarType.Int64 => (ulong)reader.GetInt64(position),
            ScalarType.UInt64 => reader.GetUInt64(position),
            ScalarType.Float32 => BitConverter.SingleToUInt32Bits(reader.GetFloat32(position)),
            _ => BitConverter.DoubleToUInt64Bits(reader.GetFloat64(position)),
        };

        return new Scalar(type, bits);
    }

    /// <summary>Sets an array field with the setter of its type.</summary>
    public static void SetArray<T>(T target, ArrayField array)
        where T : ITarget, allows ref struct
    {
        var bits = array.Bits;
        switch (array.Type)
        {
            case ScalarType.Int8:
                target.SetInt8Array(Values(bits, value => (sbyte)value));
                break;
            case ScalarType.UInt8:
                target.SetUInt8Array(Values(bits, value => (byte)value));
                break;
            case ScalarType.Int16:
                target.SetInt16Array(Values(bits, value => (short)value));
                break;
            case ScalarType.UInt16:
                target.SetUInt16Array(Values(bits, value => (ushort)value));
                break;
            case ScalarType.Int32:
                target.SetInt32Array(Values(bits, value => (int)value));
                break;
            case ScalarType.UInt32:
                target.SetUInt32Array(Values(bits, value => (uint)value));
                break;
            case ScalarType.Int64:
                target.SetInt64Array(Values(bits, value => (long)value));
                break;
            case ScalarType.UInt64:
                target.SetUInt64Array(Values(bits, value => value));
                break;
            case ScalarType.Float32:
                target.SetFloat32Array(Float32s(bits));
                break;
            default:
                target.SetFloat64Array(Float64s(bits));
                break;
        }
    }

    /// <summary>Reads an array of a type at a position with the reader's getter of the type.</summary>
    public static ArrayField ReadArray(TableReader reader, ScalarType type, int position)
    {
        var bits = type switch
        {
            ScalarType.Int8 => Bits(reader.GetInt8Array(position), value => (byte)value),
            ScalarType.UInt8 => Bits(reader.GetUInt8Array(position), value => value),
            ScalarType.Int16 => Bits(reader.GetInt16Array(position), value => (ushort)value),
            ScalarType.UInt16 => Bits(reader.GetUInt16Array(position), value => value),
            ScalarType.Int32 => Bits(reader.GetInt32Array(position), value => (uint)value),
            ScalarType.UInt32 => Bits(reader.GetUInt32Array(position), value => value),
            ScalarType.Int64 => Bits(reader.GetInt64Array(position), value => (ulong)value),
            ScalarType.UInt64 => Bits(reader.GetUInt64Array(position), value => value),
            // As their bits in memory, since a float that a method returns may be quieted
            ScalarType.Float32 => Bits(MemoryMarshal.Cast<float, uint>(reader.GetFloat32Array(position)), value => value),
            _ => Bits(MemoryMarshal.Cast<double, ulong>(reader.GetFloat64Array(position)), value => value),
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
