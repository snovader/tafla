using CsCheck;
using Tafla.Tests.Tables;

namespace Tafla.Tests.Generated;

/// <summary>
/// A message of the code that tafla-gen generates from an example schema: how the tests generate its
/// fields and describe them, write them with the generated code into new bytes and into given ones,
/// and read every field of bytes with the generated reader, through its views.
/// </summary>
public abstract class GeneratedMessage
{
    protected GeneratedMessage(string name)
    {
        Name = name;
    }

    /// <summary>The schema's name and the message's, such as <c>shapes.Point</c>.</summary>
    public string Name { get; }

    /// <summary>Fields of the message, each written with the generated code.</summary>
    public abstract Gen<Written> Samples { get; }

    /// <summary>
    /// Reads every field of the bytes with the generated reader, the items of vectors and arrays of
    /// structs through their views' enumerators, into their description.
    /// </summary>
    public abstract Table Read(ReadOnlySpan<byte> bytes);

    public override string ToString() => Name;
}

/// <summary>
/// The description of a message's fields, the bytes that its <c>Serialize()</c> writes, and its
/// <c>SerializeInto()</c> of the fields.
/// </summary>
public sealed record Written(Table Description, byte[] Bytes, Func<byte[], int> SerializeInto)
{
    public override string ToString() => Description.ToString();
}

/// <summary>A message whose fields the generated code writes from a <typeparamref name="TFields"/>.</summary>
public sealed class GeneratedMessage<TFields>(
    string name,
    Gen<TFields> fields,
    Func<TFields, Table> describe,
    Func<TFields, byte[]> serialize,
    Func<TFields, byte[], int> serializeInto,
    Func<ReadOnlySpan<byte>, Table> read
) : GeneratedMessage(name)
{
    public override Gen<Written> Samples =>
        fields.Select(value => new Written(describe(value), serialize(value), bytes => serializeInto(value, bytes)));

    public override Table Read(ReadOnlySpan<byte> bytes) => read(bytes);
}

/// <summary>
/// Properties of every message of the example schemas' generated code: it writes what
/// <see cref="TableBuilder"/> writes of the same fields, reads them back, writes into the caller's bytes
/// as into new ones, and reads corrupted and arbitrary bytes as values or a <see cref="DecodeException"/>.
/// </summary>
public class GeneratedMessageTests
{
    public static readonly GeneratedMessage[] Messages =
    [
        .. ShapesTests.Messages,
        .. AllTypesTests.Messages,
        .. EnumsTests.Messages,
        .. LayoutsTests.Messages,
        .. FixedFieldsTests.Messages,
        .. PresenceTests.Messages,
    ];

    public static TheoryData<string> Names => [.. Messages.Select(message => message.Name)];

    private static GeneratedMessage Named(string name) => Messages.Single(message => message.Name == name);

    [Theory]
    [MemberData(nameof(Names))]
    public void IsWrittenAsTableBuilderWritesIt(string name) =>
        Named(name).Samples.Check(written => Assert.Equal(written.Description.Bytes(), written.Bytes));

    /// <summary>
    /// Floats compare by their bits. The message is read at any byte offset from an aligned address,
    /// so that its arrays are read in place both aligned and unaligned.
    /// </summary>
    [Theory]
    [MemberData(nameof(Names))]
    public void ReadsBackItsFieldsAtAnyByteOffset(string name)
    {
        var message = Named(name);

        Gen.Select(message.Samples, Gen.Int[0, 7]).Check(sample =>
        {
            var (written, byteOffset) = sample;
            Assert.Equal(written.Description, message.Read(Bytes.AtByteOffset(written.Bytes, byteOffset).Span));
        });
    }

    [Theory]
    [MemberData(nameof(Names))]
    public void IsWrittenIntoTheStartOfLongerBytesAsIntoNewBytesLeavingTheRest(string name) =>
        Gen.Select(Named(name).Samples, Gen.Int[0, 16], Gen.Byte).Check(sample =>
        {
            var (written, extra, fill) = sample;
            var bytes = new byte[written.Bytes.Length + extra];
            bytes.AsSpan().Fill(fill);

            var length = written.SerializeInto(bytes);

            Assert.Equal(written.Bytes.Length, length);
            Assert.Equal(written.Bytes, bytes[..length]);
            Assert.All(bytes[length..], value => Assert.Equal(fill, value));
        });

    [Theory]
    [MemberData(nameof(Names))]
    public void BytesShorterThanTheMessageThrowArgumentExceptionBeforeAnyIsWritten(string name) =>
        Named(name).Samples.SelectMany(written => Gen.Int[0, written.Bytes.Length - 1].Select(length => (written, length)))
            .Check(sample =>
            {
                var (written, length) = sample;
                var bytes = new byte[length];
                bytes.AsSpan().Fill(0xa5);

                Assert.Throws<ArgumentException>(() => written.SerializeInto(bytes));
                Assert.All(bytes, value => Assert.Equal(0xa5, value));
            });

    /// <summary>Some of a message's bytes, anywhere in it, header and offsets too, replaced by any others</summary>
    [Theory]
    [MemberData(nameof(Names))]
    public void CorruptedBytesReadAsValuesOrThrowDecodeException(string name)
    {
        var message = Named(name);
        var corruptions = Gen.Select(Gen.Int[0, int.MaxValue], Gen.Byte).Array[1, 4];

        Gen.Select(message.Samples, corruptions).Check(sample =>
        {
            var (written, replacements) = sample;
            var bytes = written.Bytes.ToArray();
            foreach (var (position, value) in replacements)
            {
                bytes[position % bytes.Length] = value;
            }

            ReadsAsValuesOrThrowsDecodeException(() => message.Read(bytes));
        });
    }

    [Theory]
    [MemberData(nameof(Names))]
    public void AnyBytesReadAsValuesOrThrowDecodeException(string name)
    {
        var message = Named(name);
        Gen.Byte.Array[0, 64].Check(bytes => ReadsAsValuesOrThrowsDecodeException(() => message.Read(bytes)));
    }

    /// <summary>Checks that a read returns or throws a <see cref="DecodeException"/>, and nothing else.</summary>
    internal static void ReadsAsValuesOrThrowsDecodeException(Func<object> read)
    {
        try
        {
            _ = read();
        }
        catch (DecodeException)
        {
            // Corrupted bytes may fail to read, with the decode error alone
        }
    }
}
