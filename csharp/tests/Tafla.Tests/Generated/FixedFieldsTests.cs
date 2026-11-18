using CsCheck;
using Tafla.Tests.Tables;
using static Tafla.Tests.Generated.GeneratedDescriptions;
using static Tafla.Tests.Generated.Values;

namespace Tafla.Tests.Generated;

/// <summary>
/// The code generated from <c>schemas/fixed_fields.tafla</c>, whose types are named here in full, since
/// <c>Range</c> is also <see cref="System.Range"/>: messages whose fields' sizes the schema fixes, in
/// every offset type and header, which <see cref="GeneratedMessageTests"/> writes and reads with the
/// others, and one that's always too long for its offset type.
/// </summary>
public class FixedFieldsTests
{
    private static readonly Layout TinyLayout = new(true, false, OffsetType.UInt8);
    private static readonly Layout TaggedLayout = new(true, true, OffsetType.UInt8);
    private static readonly Layout HeaderLayout = new(false, false, OffsetType.UInt8);
    private static readonly Layout PixelLayout = new(true, true, OffsetType.UInt16);
    private static readonly Layout StripLayout = new(false, false, OffsetType.UInt16);
    private static readonly Layout BoxLayout = new(false, true, OffsetType.UInt32);

    private static readonly Gen<FixedFields.Vec2> Vec2s = Gen.Select(Float32s, Float32s, (x, y) => new FixedFields.Vec2 { X = x, Y = y });

    private static readonly Gen<FixedFields.RangeFields> Ranges = Gen.Select(
        UInt32s,
        UInt32s,
        (start, end) => new FixedFields.RangeFields { Start = start, End = end }
    );

    private static readonly Gen<FixedFields.FrameFields> Frames = Gen.Select(
        Vec2s,
        Ranges,
        UInt16s,
        (origin, range, flags) => new FixedFields.FrameFields { Origin = origin, Range = range, Flags = flags }
    );

    public static readonly GeneratedMessage[] Messages =
    [
        new GeneratedMessage<FixedFields.TinyFields>(
            "fixed_fields.Tiny",
            Gen.Select(UInt16s, UInt8s, (id, kind) => new FixedFields.TinyFields { Id = id, Kind = kind }),
            fields => Message(TinyLayout, Of(fields.Id), Of(fields.Kind)),
            fields => fields.Serialize(),
            (fields, bytes) => fields.SerializeInto(bytes),
            bytes =>
            {
                var tiny = new FixedFields.Tiny(bytes);
                return Message(TinyLayout, Of(tiny.Id), Of(tiny.Kind));
            }
        ),
        new GeneratedMessage<FixedFields.TaggedFields>(
            "fixed_fields.Tagged",
            Gen.Select(
                UInt32s,
                Vec2s,
                Strings,
                (id, position, label) => new FixedFields.TaggedFields { Id = id, Position = position, Label = label }
            ),
            fields => Message(TaggedLayout, FixedFields.Tagged.FormatVersion, Of(fields.Id), Describe(fields.Position), Of(fields.Label)),
            fields => fields.Serialize(),
            (fields, bytes) => fields.SerializeInto(bytes),
            bytes =>
            {
                var tagged = new FixedFields.Tagged(bytes);
                return Message(TaggedLayout, FixedFields.Tagged.FormatVersion, Of(tagged.Id), Describe(tagged.Position), Of(tagged.Label));
            }
        ),
        new GeneratedMessage<FixedFields.HeaderFields>(
            "fixed_fields.Header",
            Gen.Select(
                UInt8s,
                UInt16s,
                ArraysOf(UInt8s),
                (kind, length, values) => new FixedFields.HeaderFields { Kind = kind, Length = length, Values = values }
            ),
            fields => Message(HeaderLayout, Of(fields.Kind), Of(fields.Length), Of(fields.Values.Span)),
            fields => fields.Serialize(),
            (fields, bytes) => fields.SerializeInto(bytes),
            bytes =>
            {
                var header = new FixedFields.Header(bytes);
                return Message(HeaderLayout, Of(header.Kind), Of(header.Length), Of(header.Values));
            }
        ),
        new GeneratedMessage<FixedFields.PixelFields>(
            "fixed_fields.Pixel",
            Gen.Select(Vec2s, UInt32s, (position, color) => new FixedFields.PixelFields { Position = position, Color = color }),
            fields => Message(PixelLayout, FixedFields.Pixel.FormatVersion, Describe(fields.Position), Of(fields.Color)),
            fields => fields.Serialize(),
            (fields, bytes) => fields.SerializeInto(bytes),
            bytes =>
            {
                var pixel = new FixedFields.Pixel(bytes);
                return Message(PixelLayout, FixedFields.Pixel.FormatVersion, Describe(pixel.Position), Of(pixel.Color));
            }
        ),
        new GeneratedMessage<FixedFields.StripFields>(
            "fixed_fields.Strip",
            Gen.Select(
                UInt32s,
                Vec2s,
                ArraysOf(UInt16s),
                (id, origin, values) => new FixedFields.StripFields { Id = id, Origin = origin, Values = values }
            ),
            fields => Message(StripLayout, Of(fields.Id), Describe(fields.Origin), Of(fields.Values.Span)),
            fields => fields.Serialize(),
            (fields, bytes) => fields.SerializeInto(bytes),
            bytes =>
            {
                var strip = new FixedFields.Strip(bytes);
                return Message(StripLayout, Of(strip.Id), Describe(strip.Origin), Of(strip.Values));
            }
        ),
        new GeneratedMessage<FixedFields.BoxFields>(
            "fixed_fields.Box",
            Gen.Select(Vec2s, Vec2s, Ranges, (min, max, range) => new FixedFields.BoxFields { Min = min, Max = max, Range = range }),
            fields => Message(BoxLayout, FixedFields.Box.FormatVersion, Describe(fields.Min), Describe(fields.Max), Describe(fields.Range)),
            fields => fields.Serialize(),
            (fields, bytes) => fields.SerializeInto(bytes),
            bytes =>
            {
                var box = new FixedFields.Box(bytes);
                return Message(BoxLayout, FixedFields.Box.FormatVersion, Describe(box.Min), Describe(box.Max), Describe(box.Range));
            }
        ),
        new GeneratedMessage<FixedFields.TrackFields>(
            "fixed_fields.Track",
            Gen.Select(
                UInt32s,
                Vec2s,
                Frames,
                ArraysOf(Float32s),
                Strings,
                (id, origin, frame, points, name) => new FixedFields.TrackFields
                {
                    Id = id,
                    Origin = origin,
                    Frame = frame,
                    Points = points,
                    Name = name,
                }
            ),
            fields =>
                Message(
                    DefaultLayout,
                    Of(fields.Id),
                    Describe(fields.Origin),
                    Describe(fields.Frame),
                    Of(fields.Points.Span),
                    Of(fields.Name)
                ),
            fields => fields.Serialize(),
            (fields, bytes) => fields.SerializeInto(bytes),
            bytes =>
            {
                var track = new FixedFields.Track(bytes);
                return Message(
                    DefaultLayout,
                    Of(track.Id),
                    Describe(track.Origin),
                    Describe(track.Frame),
                    Of(track.Points),
                    Of(track.Name)
                );
            }
        ),
        new GeneratedMessage<FixedFields.LabelFields>(
            "fixed_fields.Label",
            Strings.Select(text => new FixedFields.LabelFields { Text = text }),
            fields => Message(DefaultLayout, Of(fields.Text)),
            fields => fields.Serialize(),
            (fields, bytes) => fields.SerializeInto(bytes),
            bytes => Message(DefaultLayout, Of(new FixedFields.Label(bytes).Text))
        ),
        new GeneratedMessage<FixedFields.SpanFields>(
            "fixed_fields.Span",
            Gen.Select(
                Strings,
                Ranges,
                Frames,
                ArraysOf(Ranges, 6),
                (name, range, frame, ranges) => new FixedFields.SpanFields
                {
                    Name = name,
                    Range = range,
                    Frame = frame,
                    Ranges = ranges,
                }
            ),
            fields =>
                Message(
                    DefaultLayout,
                    Of(fields.Name),
                    Describe(fields.Range),
                    Describe(fields.Frame),
                    Vector(fields.Ranges.ToArray().Select(Describe))
                ),
            fields => fields.Serialize(),
            (fields, bytes) => fields.SerializeInto(bytes),
            bytes =>
            {
                var span = new FixedFields.Span(bytes);
                return Message(
                    DefaultLayout,
                    Of(span.Name),
                    Describe(span.Range),
                    Describe(span.Frame),
                    Vector(DescribeRanges(span.Ranges))
                );
            }
        ),
    ];

    /// <summary>
    /// The struct of 256 bytes takes the message past the 255 bytes that its offsets address, whatever
    /// its values, before any bytes are allocated or written
    /// </summary>
    [Fact]
    public void AMessageLongerThanItsOffsetTypeAddressesThrowsBuildException()
    {
        var quad = new FixedFields.Quad { A = 1, B = 2, C = 3, D = 4, E = 5, F = 6, G = 7, H = 8 };
        var fields = new FixedFields.OverflowFields
        {
            Id = 1,
            Block = new() { First = quad, Second = quad, Third = quad, Fourth = quad },
        };
        var bytes = new byte[1024];

        var exception = Assert.Throws<BuildException>(() => fields.Serialize());
        Assert.Equal(BuildErrorKind.TooLong, exception.Kind);
        Assert.Equal(OffsetType.UInt8, exception.OffsetType);
        Assert.Equal(2 + 1 + 256, exception.Length);
        Assert.Throws<BuildException>(() => fields.SerializeInto(bytes));
        Assert.All(bytes, value => Assert.Equal(0, value));
    }

    /// <summary>
    /// A message of fixed fields and a string after them is written whole, which only its string can
    /// make too long, as it's written field by field: its header, offsets and fixed fields take 18 of
    /// the 255 bytes that uint8 offsets address
    /// </summary>
    [Fact]
    public void AMessageWrittenWholeIsAsLongAsItsOffsetTypeAddressesAndRejectedPastIt()
    {
        var longest = new FixedFields.TaggedFields { Id = 1, Position = new() { X = 1, Y = 2 }, Label = new string('a', 237) };
        Assert.Equal(255, longest.Serialize().Length);
        Assert.Equal(longest.Label, new FixedFields.Tagged(longest.Serialize()).Label);

        var tooLong = longest with { Label = new string('a', 238) };
        var exception = Assert.Throws<BuildException>(() => tooLong.Serialize());
        Assert.Equal((BuildErrorKind.TooLong, 2, 256L), (exception.Kind, exception.Index, exception.Length));
        Assert.Equal(BuildErrorKind.TooLong, Assert.Throws<BuildException>(() => tooLong.SerializeInto(new byte[512])).Kind);
        Assert.Throws<ArgumentNullException>(() => (longest with { Label = null! }).Serialize());
    }

    private static StructField Describe(FixedFields.Vec2 vec2) => Struct(Of(vec2.X), Of(vec2.Y));

    private static TableField Describe(FixedFields.RangeFields range) => Nested(Of(range.Start), Of(range.End));

    private static TableField Describe(FixedFields.Range range) => Nested(Of(range.Start), Of(range.End));

    /// <summary>The ranges of a vector, read through its enumerator, as <c>foreach</c> reads them</summary>
    private static List<TableField> DescribeRanges(FixedFields.RangeVector ranges)
    {
        var described = new List<TableField>();
        foreach (var range in ranges)
        {
            described.Add(Describe(range));
        }

        return described;
    }

    private static TableField Describe(FixedFields.FrameFields frame) =>
        Nested(Describe(frame.Origin), Describe(frame.Range), Of(frame.Flags));

    private static TableField Describe(FixedFields.Frame frame) =>
        Nested(Describe(frame.Origin), Describe(frame.Range), Of(frame.Flags));
}
