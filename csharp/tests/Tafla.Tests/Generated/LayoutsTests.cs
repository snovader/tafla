using CsCheck;
using Layouts;
using Tafla.Tests.Tables;
using static Tafla.Tests.Generated.GeneratedDescriptions;
using static Tafla.Tests.Generated.Values;

namespace Tafla.Tests.Generated;

/// <summary>
/// The code generated from <c>schemas/layouts.tafla</c>: a message of the same fields in every layout,
/// which <see cref="GeneratedMessageTests"/> writes and reads with the others, at the limits of their
/// offset types and versions.
/// </summary>
public class LayoutsTests
{
    private static readonly Layout Uint8 = new(false, false, OffsetType.UInt8);
    private static readonly Layout Uint8Sized = new(true, false, OffsetType.UInt8);
    private static readonly Layout Uint8Versioned = new(false, true, OffsetType.UInt8);
    private static readonly Layout Uint8SizedVersioned = new(true, true, OffsetType.UInt8);
    private static readonly Layout Uint16 = new(false, false, OffsetType.UInt16);
    private static readonly Layout Uint16Sized = new(true, false, OffsetType.UInt16);
    private static readonly Layout Uint16Versioned = new(false, true, OffsetType.UInt16);
    private static readonly Layout Uint16SizedVersioned = new(true, true, OffsetType.UInt16);
    private static readonly Layout Uint32 = new(false, false, OffsetType.UInt32);
    private static readonly Layout Uint32Sized = new(true, false, OffsetType.UInt32);
    private static readonly Layout Uint32Versioned = new(false, true, OffsetType.UInt32);
    private static readonly Layout Uint32SizedVersioned = new(true, true, OffsetType.UInt32);

    public static readonly GeneratedMessage[] Messages =
    [
        Message(
            "layouts.Uint8Offsets",
            Uint8,
            0,
            (id, name, values) => new Uint8OffsetsFields { Id = id, Name = name, Values = values },
            fields => (fields.Id, fields.Name, fields.Values),
            fields => fields.Serialize(),
            (fields, bytes) => fields.SerializeInto(bytes),
            bytes =>
            {
                var message = new Uint8Offsets(bytes);
                return Describe(Uint8, 0, message.Id, message.Name, message.Values);
            }
        ),
        Message(
            "layouts.Uint8OffsetsSized",
            Uint8Sized,
            0,
            (id, name, values) => new Uint8OffsetsSizedFields { Id = id, Name = name, Values = values },
            fields => (fields.Id, fields.Name, fields.Values),
            fields => fields.Serialize(),
            (fields, bytes) => fields.SerializeInto(bytes),
            bytes =>
            {
                var message = new Uint8OffsetsSized(bytes);
                return Describe(Uint8Sized, 0, message.Id, message.Name, message.Values);
            }
        ),
        Message(
            "layouts.Uint8OffsetsVersioned",
            Uint8Versioned,
            Uint8OffsetsVersioned.FormatVersion,
            (id, name, values) => new Uint8OffsetsVersionedFields { Id = id, Name = name, Values = values },
            fields => (fields.Id, fields.Name, fields.Values),
            fields => fields.Serialize(),
            (fields, bytes) => fields.SerializeInto(bytes),
            bytes =>
            {
                var message = new Uint8OffsetsVersioned(bytes);
                return Describe(Uint8Versioned, Uint8OffsetsVersioned.FormatVersion, message.Id, message.Name, message.Values);
            }
        ),
        Message(
            "layouts.Uint8OffsetsSizedVersioned",
            Uint8SizedVersioned,
            Uint8OffsetsSizedVersioned.FormatVersion,
            (id, name, values) => new Uint8OffsetsSizedVersionedFields { Id = id, Name = name, Values = values },
            fields => (fields.Id, fields.Name, fields.Values),
            fields => fields.Serialize(),
            (fields, bytes) => fields.SerializeInto(bytes),
            bytes =>
            {
                var message = new Uint8OffsetsSizedVersioned(bytes);
                return Describe(Uint8SizedVersioned, Uint8OffsetsSizedVersioned.FormatVersion, message.Id, message.Name, message.Values);
            }
        ),
        Message(
            "layouts.Uint16Offsets",
            Uint16,
            0,
            (id, name, values) => new Uint16OffsetsFields { Id = id, Name = name, Values = values },
            fields => (fields.Id, fields.Name, fields.Values),
            fields => fields.Serialize(),
            (fields, bytes) => fields.SerializeInto(bytes),
            bytes =>
            {
                var message = new Uint16Offsets(bytes);
                return Describe(Uint16, 0, message.Id, message.Name, message.Values);
            }
        ),
        Message(
            "layouts.Uint16OffsetsSized",
            Uint16Sized,
            0,
            (id, name, values) => new Uint16OffsetsSizedFields { Id = id, Name = name, Values = values },
            fields => (fields.Id, fields.Name, fields.Values),
            fields => fields.Serialize(),
            (fields, bytes) => fields.SerializeInto(bytes),
            bytes =>
            {
                var message = new Uint16OffsetsSized(bytes);
                return Describe(Uint16Sized, 0, message.Id, message.Name, message.Values);
            }
        ),
        Message(
            "layouts.Uint16OffsetsVersioned",
            Uint16Versioned,
            Uint16OffsetsVersioned.FormatVersion,
            (id, name, values) => new Uint16OffsetsVersionedFields { Id = id, Name = name, Values = values },
            fields => (fields.Id, fields.Name, fields.Values),
            fields => fields.Serialize(),
            (fields, bytes) => fields.SerializeInto(bytes),
            bytes =>
            {
                var message = new Uint16OffsetsVersioned(bytes);
                return Describe(Uint16Versioned, Uint16OffsetsVersioned.FormatVersion, message.Id, message.Name, message.Values);
            }
        ),
        Message(
            "layouts.Uint16OffsetsSizedVersioned",
            Uint16SizedVersioned,
            Uint16OffsetsSizedVersioned.FormatVersion,
            (id, name, values) => new Uint16OffsetsSizedVersionedFields { Id = id, Name = name, Values = values },
            fields => (fields.Id, fields.Name, fields.Values),
            fields => fields.Serialize(),
            (fields, bytes) => fields.SerializeInto(bytes),
            bytes =>
            {
                var message = new Uint16OffsetsSizedVersioned(bytes);
                return Describe(Uint16SizedVersioned, Uint16OffsetsSizedVersioned.FormatVersion, message.Id, message.Name, message.Values);
            }
        ),
        Message(
            "layouts.Uint32Offsets",
            Uint32,
            0,
            (id, name, values) => new Uint32OffsetsFields { Id = id, Name = name, Values = values },
            fields => (fields.Id, fields.Name, fields.Values),
            fields => fields.Serialize(),
            (fields, bytes) => fields.SerializeInto(bytes),
            bytes =>
            {
                var message = new Uint32Offsets(bytes);
                return Describe(Uint32, 0, message.Id, message.Name, message.Values);
            }
        ),
        Message(
            "layouts.Uint32OffsetsSized",
            Uint32Sized,
            0,
            (id, name, values) => new Uint32OffsetsSizedFields { Id = id, Name = name, Values = values },
            fields => (fields.Id, fields.Name, fields.Values),
            fields => fields.Serialize(),
            (fields, bytes) => fields.SerializeInto(bytes),
            bytes =>
            {
                var message = new Uint32OffsetsSized(bytes);
                return Describe(Uint32Sized, 0, message.Id, message.Name, message.Values);
            }
        ),
        Message(
            "layouts.Uint32OffsetsVersioned",
            Uint32Versioned,
            Uint32OffsetsVersioned.FormatVersion,
            (id, name, values) => new Uint32OffsetsVersionedFields { Id = id, Name = name, Values = values },
            fields => (fields.Id, fields.Name, fields.Values),
            fields => fields.Serialize(),
            (fields, bytes) => fields.SerializeInto(bytes),
            bytes =>
            {
                var message = new Uint32OffsetsVersioned(bytes);
                return Describe(Uint32Versioned, Uint32OffsetsVersioned.FormatVersion, message.Id, message.Name, message.Values);
            }
        ),
        Message(
            "layouts.Uint32OffsetsSizedVersioned",
            Uint32SizedVersioned,
            Uint32OffsetsSizedVersioned.FormatVersion,
            (id, name, values) => new Uint32OffsetsSizedVersionedFields { Id = id, Name = name, Values = values },
            fields => (fields.Id, fields.Name, fields.Values),
            fields => fields.Serialize(),
            (fields, bytes) => fields.SerializeInto(bytes),
            bytes =>
            {
                var message = new Uint32OffsetsSizedVersioned(bytes);
                return Describe(Uint32SizedVersioned, Uint32OffsetsSizedVersioned.FormatVersion, message.Id, message.Name, message.Values);
            }
        ),
    ];

    [Fact]
    public void VersionsAreTheLimitsOfTheirOffsetTypesAndZero()
    {
        Assert.Equal(255u, Uint8OffsetsVersioned.FormatVersion);
        Assert.Equal(0u, Uint8OffsetsSizedVersioned.FormatVersion);
        Assert.Equal(65_535u, Uint16OffsetsVersioned.FormatVersion);
        Assert.Equal(1u, Uint16OffsetsSizedVersioned.FormatVersion);
        Assert.Equal(uint.MaxValue, Uint32OffsetsVersioned.FormatVersion);
        Assert.Equal(2u, Uint32OffsetsSizedVersioned.FormatVersion);
    }

    /// <summary>
    /// A name of 150 bytes puts the values after it past 127 in uint8 offsets, and one of 33,000 bytes
    /// past 32,767 in uint16 offsets, so that offsets with the top bit set read as unsigned
    /// </summary>
    [Fact]
    public void Uint8AndUint16OffsetsReadBackFieldsAtOffsetsWithTheTopBitSet()
    {
        ushort[] values = [1, 2];

        var uint8 = new Uint8OffsetsFields { Id = 7, Name = new string('a', 150), Values = values }.Serialize();
        Assert.True(new TableReader(uint8, Uint8.ToTableLayout()).RootOffset(2) > sbyte.MaxValue);
        Assert.Equal(values, new Uint8Offsets(uint8).Values.ToArray());

        var uint16 = new Uint16OffsetsFields { Id = 7, Name = new string('b', 33_000), Values = values }.Serialize();
        Assert.True(new TableReader(uint16, Uint16.ToTableLayout()).RootOffset(2) > short.MaxValue);
        Assert.Equal(values, new Uint16Offsets(uint16).Values.ToArray());
    }

    /// <summary>
    /// Three offsets, the id, the name's count and the values' count take 9 bytes in uint8 offsets,
    /// and the values start at an even offset, so a name of 245 ASCII bytes ends the table at 254, the
    /// longest that it can be, and one of 246 at 256
    /// </summary>
    [Fact]
    public void Uint8OffsetsWriteTablesAsLongAsTheyAddressAndThrowBuildExceptionPastThem()
    {
        var longest = new Uint8OffsetsFields { Id = 1, Name = new string('a', 245), Values = default };
        Assert.Equal(254, longest.Serialize().Length);
        Assert.Equal(longest.Name, new Uint8Offsets(longest.Serialize()).Name);

        var tooLong = longest with { Name = new string('a', 246) };
        var exception = Assert.Throws<BuildException>(() => tooLong.Serialize());
        Assert.Equal(BuildErrorKind.TooLong, exception.Kind);
        Assert.Equal(OffsetType.UInt8, exception.OffsetType);
        Assert.Throws<BuildException>(() => tooLong.SerializeInto(new byte[1024]));
    }

    [Fact]
    public void Uint16OffsetsThrowBuildExceptionPastTheirLongestTable()
    {
        var fields = new Uint16OffsetsFields { Id = 1, Name = new string('a', 65_535), Values = default };

        var exception = Assert.Throws<BuildException>(() => fields.Serialize());
        Assert.Equal(BuildErrorKind.TooLong, exception.Kind);
        Assert.Equal(OffsetType.UInt16, exception.OffsetType);
    }

    [Fact]
    public void AVersionedLayoutThrowsDecodeExceptionReadingATableOfAnotherVersion() =>
        Gen.Select(UInt32s, Strings, ArraysOf(UInt16s), Gen.UInt[0, 65_535]).Check(sample =>
        {
            var (id, name, values, version) = sample;
            var fields = new Uint16OffsetsSizedVersionedFields { Id = id, Name = name, Values = values };
            var bytes = (Describe(Uint16SizedVersioned, version, id, name, values.Span)).Bytes();

            Assert.Equal(version, Uint16OffsetsSizedVersioned.VersionOf(bytes));
            if (version == Uint16OffsetsSizedVersioned.FormatVersion)
            {
                Assert.Equal(fields.Serialize(), bytes);
                Assert.Equal(id, new Uint16OffsetsSizedVersioned(bytes).Id);
            }
            else
            {
                var exception = Assert.Throws<DecodeException>(() => new Uint16OffsetsSizedVersioned(bytes));
                Assert.Equal(DecodeErrorKind.UnsupportedVersion, exception.Kind);
            }
        });

    /// <summary>A message of the layout, from its fields' id, name and values, written and read as the generated code does</summary>
    private static GeneratedMessage<TFields> Message<TFields>(
        string name,
        Layout layout,
        uint version,
        Func<uint, string, ReadOnlyMemory<ushort>, TFields> create,
        Func<TFields, (uint Id, string Name, ReadOnlyMemory<ushort> Values)> values,
        Func<TFields, byte[]> serialize,
        Func<TFields, byte[], int> serializeInto,
        Func<ReadOnlySpan<byte>, Table> read
    )
    {
        // Few enough values and characters that a table of uint8 offsets holds them
        var fields = Gen.Select(UInt32s, Strings, ArraysOf(UInt16s, 16), create);

        Table Describe(TFields value)
        {
            var (id, text, elements) = values(value);
            return LayoutsTests.Describe(layout, version, id, text, elements.Span);
        }

        return new GeneratedMessage<TFields>(name, fields, Describe, serialize, serializeInto, read);
    }

    private static Table Describe(Layout layout, uint version, uint id, string name, ReadOnlySpan<ushort> values) =>
        layout.Versioned
            ? GeneratedDescriptions.Message(layout, version, Of(id), Of(name), Of(values))
            : GeneratedDescriptions.Message(layout, Of(id), Of(name), Of(values));
}
