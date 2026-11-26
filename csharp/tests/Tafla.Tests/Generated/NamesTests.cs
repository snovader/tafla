using Names.@event.@global;
using Tafla.Tests.Tables;
using static Tafla.Tests.Generated.GeneratedDescriptions;

namespace Tafla.Tests.Generated;

/// <summary>
/// The code generated from <c>csharp/tests/names.tafla</c>, in a namespace of keywords, whose fields
/// named after the generated members take a <c>_</c> and keep their values, whose items named after
/// their views' members read through them, and whose enums' values named after their enum and its
/// methods are values, in fields named after each other's enums.
/// </summary>
public class NamesTests
{
    [Fact]
    public void FieldsNamedAfterTheGeneratedMembersReadBackTheirValues()
    {
        var frame = new Frame { Size_ = 1, Alignment_ = 2, Write_ = 3, SerializeArray_ = 4, Frame_ = 5 };
        var fields = new MembersFields
        {
            FormatVersion_ = 1,
            VersionOf_ = 2,
            TableLayout_ = 3,
            Serialize_ = 4,
            SerializeInto_ = 5,
            Equals_ = 6,
            GetHashCode_ = 7,
            ToString_ = 8,
            GetType_ = 9,
            PrintMembers_ = 10,
            Members_ = 11,
            MembersFields_ = 12,
            Nested = new() { AddTo_ = 13, WriteTo_ = 14, Nested_ = 15 },
            Frame = frame,
        };

        var bytes = fields.Serialize();
        var members = new Members(bytes);

        Assert.Equal(Members.FormatVersion, Members.VersionOf(bytes));
        Assert.Equal(
            [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15],
            new[]
            {
                members.FormatVersion_,
                members.VersionOf_,
                members.TableLayout_,
                members.Serialize_,
                members.SerializeInto_,
                members.Equals_,
                members.GetHashCode_,
                members.ToString_,
                members.GetType_,
                members.PrintMembers_,
                members.Members_,
                members.MembersFields_,
                members.Nested.AddTo_,
                members.Nested.WriteTo_,
                members.Nested.Nested_,
            }
        );
        Assert.Equal(frame, members.Frame);
        Assert.Equal(5, Frame.Size);
    }

    [Fact]
    public void ItemsNamedAfterTheirViewsMembersReadThroughThem()
    {
        var fields = new ViewsFields
        {
            Count = new Count[] { new() { Value = 1 }, new() { Value = 2 } },
            Current = new Current[] { new() { Value = 3 } },
            MoveNext = new MoveNextFields[] { new() { Value = 4 } },
            Item = new ItemFields[] { new() { Value = 5 } },
            ToArray = new ToArray[] { new() { Value = 6 } },
            GetEnumerator = new GetEnumeratorFields[] { new() { Value = 7 } },
            Point = 8,
            Origin = new() { X = 9.5f },
            Range = new RangeFields[] { new() { Start = 10 } },
            First = new() { Start = 11 },
        };

        var views = new Views(fields.Serialize());

        Assert.Equal(2, views.Count.Count);
        Assert.Equal([1, 2], views.Count.ToArray().Select(count => count.Value));
        foreach (var current in views.Current)
        {
            Assert.Equal(3, current.Value);
        }

        Assert.Equal((1, 4), (views.MoveNext.Count, views.MoveNext[0].Value));
        Assert.Equal(5, views.Item[0].Value);
        Assert.Equal(6, views.ToArray[0].Value);
        Assert.Equal((1, 7), (views.GetEnumerator.Count, views.GetEnumerator[0].Value));
        Assert.Equal(8, views.Point);
        Assert.Equal(Of(9.5f), Of(views.Origin.X));
        Assert.Equal((1, 10), (views.Range.Count, views.Range[0].Start));
        Assert.Equal(11, views.First.Start);
    }

    /// <summary>A value hides the method that it's named after only where the method isn't called</summary>
    [Fact]
    public void ValuesNamedAfterTheirEnumAndItsMethodsAreValuesBesideTheMethods()
    {
        Assert.Equal(
            [0, 1, 2, 3, 4, 5, 6, 7, 8, 9],
            new[]
            {
                (byte)Member.Member,
                (byte)Member.Equals,
                (byte)Member.GetHashCode,
                (byte)Member.ToString,
                (byte)Member.GetType,
                (byte)Member.HasFlag,
                (byte)Member.CompareTo,
                (byte)Member.GetTypeCode,
                (byte)Member.MemberwiseClone,
                (byte)Member.Finalize,
            }
        );
        Assert.Equal("ToString", Member.ToString.ToString());
        Assert.Equal(typeof(Member), Member.GetType.GetType());
        Assert.Equal(TypeCode.Byte, Member.GetTypeCode.GetTypeCode());
    }

    /// <summary>
    /// The message is written whole, and its table nests whole, as <see cref="TableBuilder"/> writes them
    /// field by field
    /// </summary>
    [Fact]
    public void EnumsInFieldsNamedAfterEachOthersEnumsAreWrittenAsTheirIntegersAndReadBack()
    {
        var fields = new EnumsFields
        {
            Shade = Color.Green,
            Color = Shade.Dark,
            Lean = Lean.Left,
            Member = Member.GetHashCode,
            Hue = new() { Shade = (Color)200, Color = (Shade)1234, Lean = (Lean)(-128) },
            Tint = new() { Shade = Color.Red, Color = (Shade)7, Lean = Lean.Right },
            Name = "enums",
        };
        var table = Message(
            DefaultLayout,
            Of((byte)1),
            Of((ushort)65_535),
            Of((sbyte)-1),
            Of((byte)2),
            Nested(Of((byte)200), Of((ushort)1234), Of((sbyte)-128)),
            Struct(Of((byte)0), Of((ushort)7), Of((sbyte)1)),
            Of("enums")
        );

        var bytes = fields.Serialize();
        var into = new byte[bytes.Length];
        var length = fields.SerializeInto(into);
        var enums = new Enums(bytes);
        var hue = enums.Hue;

        Assert.Equal(table.Bytes(), bytes);
        Assert.Equal(bytes, into[..length]);
        Assert.Equal(
            (Color.Green, Shade.Dark, Lean.Left, Member.GetHashCode),
            (enums.Shade, enums.Color, enums.Lean, enums.Member)
        );
        Assert.Equal(fields.Hue, new HueFields { Shade = hue.Shade, Color = hue.Color, Lean = hue.Lean });
        Assert.Equal(fields.Tint, enums.Tint);
    }
}
