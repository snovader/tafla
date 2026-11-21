using CsCheck;

namespace Tafla.Tests.Tables;

/// <summary>
/// Generators of table descriptions in the conformance suite's shape: tables of every layout, with
/// fields of every type holding values across their range, and tables nested in them, from builders
/// and from bytes, to a bounded depth.
/// </summary>
/// <remarks>
/// Tables follow the rules of the format and the builder's documentation: a version fits in its
/// slot, a table nested with SetTable has no header, and a table and the tables in it fit in their
/// offset types.
/// </remarks>
public static class Strategy
{
    /// <summary>How deeply tables nest: the root's fields can hold tables whose fields hold tables</summary>
    private const int Depth = 2;

    /// <summary>Every offset type.</summary>
    public static readonly OffsetType[] OffsetTypes = [OffsetType.UInt8, OffsetType.UInt16, OffsetType.UInt32];

    /// <summary>Tables of any layout.</summary>
    public static Gen<Table> Tables => TablesOf(Layouts);

    /// <summary>Tables of the layouts that <paramref name="layouts"/> generates.</summary>
    public static Gen<Table> TablesOf(Gen<Layout> layouts) => Table(layouts, Depth, small: false);

    /// <summary>Layouts of any header and offset type.</summary>
    public static Gen<Layout> Layouts => LayoutsOf(Gen.OneOf(OffsetTypes));

    /// <summary>Layouts of any header, of the offset types that <paramref name="offsetTypes"/> generates.</summary>
    public static Gen<Layout> LayoutsOf(Gen<OffsetType> offsetTypes) =>
        Gen.Select(Gen.Bool, Gen.Bool, offsetTypes, (sized, versioned, offsetType) => new Layout(sized, versioned, offsetType));

    /// <summary>
    /// The most fields, array elements, characters, structs in an array and vector items that a table
    /// holds. A table of uint8 offsets, or nested in one, holds few enough to fit in its 255 bytes
    /// more often than not, and others hold enough for uint16 lengths and offsets past 255.
    /// </summary>
    private sealed record Limits(int Fields, int Elements, int Characters, int Structs, int Items);

    private static Limits LimitsOf(bool small) => small ? new(5, 6, 6, 2, 2) : new(8, 24, 16, 6, 4);

    /// <summary>Tables of the layouts whose fields hold tables <paramref name="depth"/> deep, and hold few values when small</summary>
    private static Gen<Table> Table(Gen<Layout> layouts, int depth, bool small) =>
        layouts.SelectMany(layout =>
        {
            var isSmall = small || layout.OffsetType == OffsetType.UInt8;
            var maxVersion = layout.OffsetType.MaxValue;
            var versions = layout.Versioned
                ? Gen.Frequency<uint>((1, Gen.Const(0u)), (1, Gen.Const(maxVersion)), (1, Gen.UInt[0, maxVersion]))
                : Gen.Const(0u);
            var fields = Field(layout.OffsetType, depth, isSmall).Array[0, LimitsOf(isSmall).Fields];

            return Gen.Select(versions, fields, (version, values) => Fitted(new Table(layout, version, values)));
        });

    /// <summary>A field of any type, or a table nested <paramref name="depth"/> deep</summary>
    private static Gen<Field> Field(OffsetType offsetType, int depth, bool small) =>
        depth == 0
            ? Value(LimitsOf(small))
            : Gen.Frequency<Field>(
                (8, Value(LimitsOf(small))),
                (1, NestedTable(offsetType, depth - 1, small)),
                (1, Vector(depth - 1, small))
            );

    /// <summary>A scalar, array, string or struct, which hold values rather than tables</summary>
    private static Gen<Field> Value(Limits limits)
    {
        var choices = new List<(int, IGen<Field>)>();
        foreach (var type in Scalars.All)
        {
            choices.Add((1, Scalar(type).Select(scalar => (Field)scalar)));
            choices.Add((1, Bits(type).Array[0, limits.Elements].Select(bits => (Field)new ArrayField(type, bits))));
        }

        // Each as likely as three types of number
        choices.Add((3, String(limits.Characters).Select(value => (Field)new StringField(value))));
        choices.Add((3, StructTypes.SelectMany(ScalarsOf).Select(scalars => (Field)new StructField(scalars))));
        choices.Add((3, StructArray(limits.Structs)));

        return Gen.Frequency<Field>([.. choices]);
    }

    /// <summary>Scalars of a type, across its range.</summary>
    public static Gen<Scalar> Scalar(ScalarType type) => Bits(type).Select(bits => new Scalar(type, bits));

    /// <summary>The bits of values of a type, across its range.</summary>
    public static Gen<ulong> Bits(ScalarType type) =>
        type switch
        {
            ScalarType.Int8 => Integer(Gen.SByte, sbyte.MinValue, sbyte.MaxValue).Select(value => (ulong)(byte)value),
            ScalarType.UInt8 => Integer(Gen.Byte, byte.MinValue, byte.MaxValue).Select(value => (ulong)value),
            ScalarType.Int16 => Integer(Gen.Short, short.MinValue, short.MaxValue).Select(value => (ulong)(ushort)value),
            ScalarType.UInt16 => Integer(Gen.UShort, ushort.MinValue, ushort.MaxValue).Select(value => (ulong)value),
            ScalarType.Int32 => Integer(Gen.Int, int.MinValue, int.MaxValue).Select(value => (ulong)(uint)value),
            ScalarType.UInt32 => Integer(Gen.UInt, uint.MinValue, uint.MaxValue).Select(value => (ulong)value),
            ScalarType.Int64 => Integer(Gen.Long, long.MinValue, long.MaxValue).Select(value => (ulong)value),
            ScalarType.UInt64 => Integer(Gen.ULong, ulong.MinValue, ulong.MaxValue),
            ScalarType.Float32 => Float32Bits,
            _ => Float64Bits,
        };

    /// <summary>Integers across their range, and their limits and 0 more often than chance</summary>
    private static Gen<T> Integer<T>(Gen<T> any, T lowest, T highest)
        where T : struct => Gen.Frequency<T>((1, Gen.OneOf(lowest, default, highest)), (3, any));

    /// <summary>
    /// Floats of every class and sign, NaNs with any payload among them, from any bits, and -0, NaN,
    /// the infinities and the limits more often than chance
    /// </summary>
    private static readonly Gen<ulong> Float32Bits = Gen.Frequency<uint>(
            (1, Gen.OneOf<uint>(
                0x8000_0000, // -0
                0x7fc0_0000, // NaN
                0xffc0_0000, // NaN of the other sign
                0x7f80_0001, // a signalling NaN
                BitConverter.SingleToUInt32Bits(float.PositiveInfinity),
                BitConverter.SingleToUInt32Bits(float.NegativeInfinity),
                BitConverter.SingleToUInt32Bits(float.MinValue),
                BitConverter.SingleToUInt32Bits(float.MaxValue),
                0x0080_0000, // the smallest normal
                1 // the smallest subnormal
            )),
            (3, Gen.UInt)
        )
        .Select(bits => (ulong)bits);

    /// <summary>Floats like <see cref="Float32Bits"/>, as float64s</summary>
    private static readonly Gen<ulong> Float64Bits = Gen.Frequency<ulong>(
        (1, Gen.OneOf<ulong>(
            0x8000_0000_0000_0000,
            0x7ff8_0000_0000_0000,
            0xfff8_0000_0000_0000,
            0x7ff0_0000_0000_0001,
            BitConverter.DoubleToUInt64Bits(double.PositiveInfinity),
            BitConverter.DoubleToUInt64Bits(double.NegativeInfinity),
            BitConverter.DoubleToUInt64Bits(double.MinValue),
            BitConverter.DoubleToUInt64Bits(double.MaxValue),
            0x0010_0000_0000_0000,
            1
        )),
        (3, Gen.ULong)
    );

    /// <summary>
    /// Strings of up to <paramref name="most"/> characters: ASCII, characters of two, three and four
    /// bytes in UTF-8, the last outside the Basic Multilingual Plane, as surrogate pairs, and U+FEFF,
    /// which isn't a byte order mark within a table, leading or not
    /// </summary>
    public static Gen<string> String(int most) =>
        Gen.Frequency<int>(
                (3, Gen.Int['a', 'z']),
                (1, Gen.Const(0xfeff)),
                (1, Gen.Int[0x80, 0x7ff]),
                // Of three bytes, but the surrogates, which aren't characters
                (1, Gen.Int[0x800, 0xffff - 0x800].Select(character => character >= 0xd800 ? character + 0x800 : character)),
                (1, Gen.Int[0x1_0000, 0x10_ffff])
            )
            .Array[0, most]
            .Select(characters => string.Concat(characters.Select(char.ConvertFromUtf32)));

    /// <summary>The types of the scalars of a struct, of one to four</summary>
    private static readonly Gen<ScalarType[]> StructTypes = Gen.OneOf(Scalars.All).Array[1, 4];

    /// <summary>The scalars of a struct of types</summary>
    private static Gen<Seq<Scalar>> ScalarsOf(ScalarType[] types) =>
        types.Aggregate(
            Gen.Const(ImmutableScalars.Empty),
            (scalars, type) => scalars.SelectMany(before => Scalar(type).Select(before.Add))
        )
        .Select(scalars => new Seq<Scalar>(scalars));

    /// <summary>An array of up to <paramref name="structs"/> structs of the same scalars, empty too</summary>
    private static Gen<Field> StructArray(int structs) =>
        StructTypes.SelectMany(types =>
            ScalarsOf(types).Array[0, structs].Select(items => (Field)new StructArray(types, items))
        );

    /// <summary>
    /// A table nested with SetTable, which has no header, from its builder or bytes, aligned by
    /// default or explicitly. It's read in place when its offsets are as wide as its parent's, as they
    /// are more often than not.
    /// </summary>
    private static Gen<Field> NestedTable(OffsetType parent, int depth, bool small)
    {
        var offsetTypes = Gen.Frequency<OffsetType>((2, Gen.Const(parent)), (1, Gen.OneOf(OffsetTypes)));
        var alignments = Gen.OneOf<int?>(null, 1, 2, 4, 8);

        return Gen.Select(
            Nested(offsetTypes.Select(Layout.Headerless), depth, small),
            alignments,
            (nested, alignment) => (Field)new TableField(nested, alignment)
        );
    }

    /// <summary>A vector of tables of any layout, from their builders or bytes</summary>
    private static Gen<Field> Vector(int depth, bool small) =>
        Nested(Layouts, depth, small).Array[0, LimitsOf(small).Items].Select(items => (Field)new VectorField(items));

    private static Gen<Nested> Nested(Gen<Layout> layouts, int depth, bool small) =>
        Gen.Select(Gen.OneOf(Source.Builder, Source.Bytes), Table(layouts, depth, small), (from, table) => new Nested(from, table));

    /// <summary>
    /// Keeps the table's first fields that surely fit in its offset type, as many as its header and
    /// their longest lengths leave room for. The tables nested in it were fitted to theirs.
    /// </summary>
    private static Table Fitted(Table table)
    {
        long width = table.Layout.OffsetType.Size;
        long length = table.Layout.ToTableLayout().HeaderSize;

        var fields = new List<Field>();
        foreach (var field in table.Fields)
        {
            length += width + Longest(field, width);
            if (length > table.Layout.OffsetType.MaxValue)
            {
                break;
            }

            fields.Add(field);
        }

        return table with { Fields = fields };
    }

    private static long LongestTable(Table table)
    {
        long width = table.Layout.OffsetType.Size;
        return table.Layout.ToTableLayout().HeaderSize + table.Fields.Sum(field => width + Longest(field, width));
    }

    /// <summary>The longest that a field can be in a table of offsets of <paramref name="width"/> bytes, with the most padding that it can need</summary>
    private static long Longest(Field field, long width)
    {
        long Array(long count, long size) => size - 1 + width + count * size;

        return field switch
        {
            Scalar scalar => scalar.Type.Size(),
            Bool => 1,
            ArrayField array => Array(array.Bits.Count, array.Type.Size()),
            StringField value => width + System.Text.Encoding.UTF8.GetByteCount(value.Value),
            StructField value => Scalars.StructSize(value.Scalars.Select(scalar => scalar.Type)),
            StructArray array => Scalars.StructAlignment(array.Types) - 1 + width + array.Items.Count * (long)Scalars.StructSize(array.Types),
            TableField table => 7 + LongestTable(table.Nested.Table),
            VectorField vector => width + vector.Items.Sum(item => width + LongestTable(item.Table)),
            _ => throw new ArgumentException($"Unknown field {field}", nameof(field)),
        };
    }

    /// <summary>An immutable list of scalars, which a generator appends to without sharing them between samples</summary>
    private static class ImmutableScalars
    {
        public static readonly System.Collections.Immutable.ImmutableList<Scalar> Empty = [];
    }
}
