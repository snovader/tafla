using System;
using System.Buffers.Binary;
using System.Runtime.CompilerServices;

namespace Tafla.Benchmarks.Codecs;

/// <summary>
/// The fixed-size record alone, its fields at fixed offsets, written into 12 new bytes and read with
/// <see cref="BinaryPrimitives"/>, as the runtime's README reads a struct's scalars.
/// </summary>
internal readonly struct BinaryPrimitivesCodec : ISampleCodec
{
    public string Name => "BinaryPrimitives";

    [MethodImpl(MethodImplOptions.NoInlining)]
    public byte[] SerializeSample(Sample sample) => Write(sample);

    [MethodImpl(MethodImplOptions.NoInlining)]
    public Sample DeserializeSample(byte[] bytes) =>
        new(
            BinaryPrimitives.ReadUInt32LittleEndian(bytes),
            BinaryPrimitives.ReadSingleLittleEndian(bytes.AsSpan(4)),
            BinaryPrimitives.ReadSingleLittleEndian(bytes.AsSpan(8))
        );

    /// <summary>Writes the fixed-size record into new bytes, as tafla's codec writes it too.</summary>
    public static byte[] Write(Sample sample)
    {
        var bytes = new byte[TaflaCodec.SampleSize];
        BinaryPrimitives.WriteUInt32LittleEndian(bytes, sample.Key);
        BinaryPrimitives.WriteSingleLittleEndian(bytes.AsSpan(4), sample.A);
        BinaryPrimitives.WriteSingleLittleEndian(bytes.AsSpan(8), sample.B);

        return bytes;
    }

}
