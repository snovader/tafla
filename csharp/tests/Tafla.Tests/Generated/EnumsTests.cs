using CsCheck;
using Tafla.Tests.Tables;
using static Tafla.Tests.Generated.GeneratedDescriptions;
using static Tafla.Tests.Generated.Values;

namespace Tafla.Tests.Generated;

/// <summary>
/// The enums of <c>schemas/all_types.tafla</c>, C# enums of the integer types that store them, so that a
/// field of one reads any integer of its type, whether the schema names it or not, and the message of
/// them, <c>Enums</c>, which <see cref="GeneratedMessageTests"/> writes and reads with the others, with
/// integers across each type's range, most of which have no name.
/// </summary>
public class EnumsTests
{
    private static readonly Gen<AllTypes.Turn> Turns = Gen.Select(
        Int8s,
        UInt64s,
        (direction, mask) => new AllTypes.Turn { Direction = (AllTypes.Direction)direction, Mask = (AllTypes.Mask)mask }
    );

    private static readonly Gen<AllTypes.Mark> Marks = Gen.Select(
        UInt8s,
        Int16s,
        Float32s,
        Int64s,
        Turns,
        (level, change, weight, extent, turn) => new AllTypes.Mark
        {
            Level = (AllTypes.Level)level,
            Change = (AllTypes.Change)change,
            Weight = weight,
            Extent = (AllTypes.Extent)extent,
            Turn = turn,
        }
    );

    private static readonly Gen<AllTypes.StepFields> Steps = Gen.Select(
        UInt8s,
        Marks,
        Strings,
        (level, mark, name) => new AllTypes.StepFields { Level = (AllTypes.Level)level, Mark = mark, Name = name }
    );

    private static readonly Gen<AllTypes.EnumsFields> EnumsFields = Gen.Select(
        Gen.Select(UInt8s, ArraysOf(UInt8s), Int8s, ArraysOf(Int8s), UInt16s, ArraysOf(UInt16s)),
        Gen.Select(Int16s, ArraysOf(Int16s), UInt32s, ArraysOf(UInt32s), Int32s, ArraysOf(Int32s)),
        Gen.Select(UInt64s, ArraysOf(UInt64s), Int64s, ArraysOf(Int64s), UInt8s),
        Gen.Select(Marks, ArraysOf(Marks, 6), ArraysOf(Steps, 6)),
        (small, medium, large, tables) => new AllTypes.EnumsFields
        {
            Level = (AllTypes.Level)small.Item1,
            Levels = small.Item2,
            Direction = (AllTypes.Direction)small.Item3,
            Directions = small.Item4,
            Unit = (AllTypes.Unit)small.Item5,
            Units = small.Item6,
            Change = (AllTypes.Change)medium.Item1,
            Changes = medium.Item2,
            Status = (AllTypes.Status)medium.Item3,
            Statuses = medium.Item4,
            Sign = (AllTypes.Sign)medium.Item5,
            Signs = medium.Item6,
            Mask = (AllTypes.Mask)large.Item1,
            Masks = large.Item2,
            Extent = (AllTypes.Extent)large.Item3,
            Extents = large.Item4,
            Word = (AllTypes.Word)large.Item5,
            Mark = tables.Item1,
            Marks = tables.Item2,
            Steps = tables.Item3,
        }
    );

    public static readonly GeneratedMessage[] Messages =
    [
        new GeneratedMessage<AllTypes.EnumsFields>(
            "all_types.Enums",
            EnumsFields,
            Describe,
            fields => fields.Serialize(),
            (fields, bytes) => fields.SerializeInto(bytes),
            bytes => Describe(new AllTypes.Enums(bytes))
        ),
    ];

    /// <summary>The integer types that store a mark's enums and its float, in the order of their offsets</summary>
    private static readonly ScalarType[] MarkTypes =
    [
        ScalarType.UInt8,
        ScalarType.Int16,
        ScalarType.Float32,
        ScalarType.Int64,
        ScalarType.Int8,
        ScalarType.UInt64,
    ];

    [Fact]
    public void EachEnumIsOfItsIntegerTypeAndNamesTheIntegersThatTheSchemaNames()
    {
        Assert.Equal(new() { ["Low"] = 0, ["Medium"] = 1, ["High"] = 2 }, Named<AllTypes.Level, byte>());
        Assert.Equal(
            new() { ["Backward"] = -1, ["None"] = 0, ["Forward"] = 1, ["FurthestBack"] = -128 },
            Named<AllTypes.Direction, sbyte>()
        );
        Assert.Equal(new() { ["Last"] = 65_535, ["First"] = 0, ["V2"] = 2 }, Named<AllTypes.Unit, ushort>());
        Assert.Equal(new() { ["Least"] = -32_768, ["Greatest"] = 32_767 }, Named<AllTypes.Change, short>());
        Assert.Equal(
            new() { ["Ok"] = 0, ["NotFound"] = 404, ["Greatest"] = 4_294_967_295 },
            Named<AllTypes.Status, uint>()
        );
        Assert.Equal(
            new() { ["Least"] = -2_147_483_648, ["Greatest"] = 2_147_483_647 },
            Named<AllTypes.Sign, int>()
        );
        Assert.Equal(
            new() { ["None"] = 0, ["One"] = 1, ["BeyondFloat64"] = 9_007_199_254_740_993, ["All"] = 18_446_744_073_709_551_615 },
            Named<AllTypes.Mask, ulong>()
        );
        Assert.Equal(
            new() { ["Least"] = -9_223_372_036_854_775_808, ["Zero"] = 0, ["Greatest"] = 9_223_372_036_854_775_807 },
            Named<AllTypes.Extent, long>()
        );
        Assert.Equal(
            new()
            {
                ["Class"] = 0,
                ["Default"] = 1,
                ["In"] = 2,
                ["Match"] = 3,
                ["Self"] = 4,
                ["Type"] = 5,
                ["True"] = 6,
                ["Null"] = 7,
                ["None"] = 8,
                ["Init"] = 9,
                ["New"] = 10,
                ["Name"] = 11,
                ["Value"] = 12,
            },
            Named<AllTypes.Word, byte>()
        );
    }

    /// <summary>The level is the message's first field, and the turn's direction the mark's 16th byte</summary>
    [Fact]
    public void AnEnumReadsAnyIntegerOfItsTypeAsItsValueNamedOrNot() =>
        Gen.Select(EnumsFields, Gen.Byte).Check(sample =>
        {
            var (fields, value) = sample;

            var message = fields.Serialize();
            message[new TableReader(message).RootOffset(0)] = value;
            Assert.Equal((AllTypes.Level)value, new AllTypes.Enums(message).Level);

            var mark = fields.Mark.Serialize();
            mark[15] = value;
            Assert.Equal((AllTypes.Direction)(sbyte)value, new AllTypes.Mark(mark).Turn.Direction);
        });

    [Fact]
    public void TheValuesThatAnEnumNamesAreWrittenAsTheirIntegers()
    {
        var mark = new AllTypes.Mark
        {
            Level = AllTypes.Level.High,
            Change = AllTypes.Change.Least,
            Weight = 0.5f,
            Extent = AllTypes.Extent.Greatest,
            Turn = new() { Direction = AllTypes.Direction.Backward, Mask = AllTypes.Mask.BeyondFloat64 },
        };
        var scalars = Struct(
            Of((byte)2),
            Of((short)-32_768),
            Of(0.5f),
            Of(long.MaxValue),
            Of((sbyte)-1),
            Of(9_007_199_254_740_993UL)
        );

        Assert.Equal(Scalars.StructBytes(scalars.Scalars), mark.Serialize());
    }

    [Fact]
    public void AMarkIsWrittenInIts24BytesLittleEndianAtFixedOffsets() =>
        Marks.Check(mark =>
        {
            var bytes = mark.Serialize();

            Assert.Equal(24, AllTypes.Mark.Size);
            Assert.Equal(8, AllTypes.Mark.Alignment);
            Assert.Equal(Scalars.StructBytes(Describe(mark).Scalars), bytes);
            Assert.Equal(Describe(mark), Describe(new AllTypes.Mark(bytes)));
        });

    /// <summary>
    /// An enum's values by their names, as integers of its type, which must be
    /// <typeparamref name="TInteger"/>
    /// </summary>
    private static Dictionary<string, TInteger> Named<TEnum, TInteger>()
        where TEnum : struct, Enum
    {
        Assert.Equal(typeof(TInteger), Enum.GetUnderlyingType(typeof(TEnum)));

        // A boxed enum unboxes as its integer type
        return Enum.GetValues<TEnum>().ToDictionary(value => value.ToString(), value => (TInteger)(object)value);
    }

    // Descriptions of the fields that are written, and of those that the generated code reads, of the
    // integer types that store their enums

    public static Table Describe(AllTypes.EnumsFields fields) =>
        Message(
            DefaultLayout,
            Of((byte)fields.Level),
            Of(fields.Levels.Span),
            Of((sbyte)fields.Direction),
            Of(fields.Directions.Span),
            Of((ushort)fields.Unit),
            Of(fields.Units.Span),
            Of((short)fields.Change),
            Of(fields.Changes.Span),
            Of((uint)fields.Status),
            Of(fields.Statuses.Span),
            Of((int)fields.Sign),
            Of(fields.Signs.Span),
            Of((ulong)fields.Mask),
            Of(fields.Masks.Span),
            Of((long)fields.Extent),
            Of(fields.Extents.Span),
            Of((byte)fields.Word),
            Describe(fields.Mark),
            StructArrayOf(MarkTypes, fields.Marks.ToArray().Select(Describe)),
            Vector(fields.Steps.ToArray().Select(DescribeStep))
        );

    public static Table Describe(AllTypes.Enums enums) =>
        Message(
            DefaultLayout,
            Of((byte)enums.Level),
            Of(enums.Levels),
            Of((sbyte)enums.Direction),
            Of(enums.Directions),
            Of((ushort)enums.Unit),
            Of(enums.Units),
            Of((short)enums.Change),
            Of(enums.Changes),
            Of((uint)enums.Status),
            Of(enums.Statuses),
            Of((int)enums.Sign),
            Of(enums.Signs),
            Of((ulong)enums.Mask),
            Of(enums.Masks),
            Of((long)enums.Extent),
            Of(enums.Extents),
            Of((byte)enums.Word),
            Describe(enums.Mark),
            StructArrayOf(MarkTypes, Items(enums.Marks).Select(Describe)),
            Vector(DescribeSteps(enums.Steps))
        );

    private static StructField Describe(AllTypes.Mark mark) =>
        Struct(
            Of((byte)mark.Level),
            Of((short)mark.Change),
            Of(mark.Weight),
            Of((long)mark.Extent),
            Of((sbyte)mark.Turn.Direction),
            Of((ulong)mark.Turn.Mask)
        );

    private static TableField DescribeStep(AllTypes.StepFields step) =>
        Nested(Of((byte)step.Level), Describe(step.Mark), Of(step.Name));

    private static TableField DescribeStep(AllTypes.Step step) => Nested(Of((byte)step.Level), Describe(step.Mark), Of(step.Name));

    /// <summary>The steps of a vector, read through its enumerator, as <c>foreach</c> reads them</summary>
    private static List<TableField> DescribeSteps(AllTypes.StepVector steps)
    {
        var described = new List<TableField>();
        foreach (var step in steps)
        {
            described.Add(DescribeStep(step));
        }

        return described;
    }

    private static List<AllTypes.Mark> Items(AllTypes.MarkArray marks)
    {
        var items = new List<AllTypes.Mark>();
        foreach (var mark in marks)
        {
            items.Add(mark);
        }

        return items;
    }
}
