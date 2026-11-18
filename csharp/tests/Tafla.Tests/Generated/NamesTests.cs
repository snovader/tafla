using Names.@event.@global;
using static Tafla.Tests.Generated.GeneratedDescriptions;

namespace Tafla.Tests.Generated;

/// <summary>
/// The code generated from <c>csharp/tests/names.tafla</c>, in a namespace of keywords, whose fields
/// named after the generated members take a <c>_</c> and keep their values, and whose items named after
/// their views' members read through them.
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
}
