using CsCheck;
using Tafla.Tests.Tables;
using static Tafla.Tests.Generated.GeneratedDescriptions;
using static Tafla.Tests.Generated.Values;

namespace Tafla.Tests.Generated;

/// <summary>
/// The code generated from <c>schemas/all_types.tafla</c>, whose types are named here in full, since
/// they share names with the tests' model: its messages, which <see cref="GeneratedMessageTests"/>
/// writes and reads with the others, its bools, and the views of its vectors and arrays of structs.
/// </summary>
public class AllTypesTests
{
    /// <summary>How many levels of trees a tree holds below itself at most</summary>
    private const int TreeDepth = 2;

    /// <summary>
    /// The most trees that reading one tree reads, which bounds reading corrupted bytes whose trees
    /// hold the same trees many times over
    /// </summary>
    private const int MostTrees = 1_000;

    private static readonly Gen<AllTypes.ScalarsFields> ScalarsFields = Gen.Select(
        Gen.Select(Int8s, UInt8s, Int16s, UInt16s, Int32s, UInt32s),
        Gen.Select(Int64s, UInt64s, Float32s, Float64s, Gen.Bool),
        (small, large) => new AllTypes.ScalarsFields
        {
            Int8 = small.Item1,
            Uint8 = small.Item2,
            Int16 = small.Item3,
            Uint16 = small.Item4,
            Int32 = small.Item5,
            Uint32 = small.Item6,
            Int64 = large.Item1,
            Uint64 = large.Item2,
            Float32 = large.Item3,
            Float64 = large.Item4,
            Bool = large.Item5,
        }
    );

    private static readonly Gen<AllTypes.ArraysFields> ArraysFields = Gen.Select(
        Gen.Select(ArraysOf(Int8s), ArraysOf(UInt8s), ArraysOf(Int16s), ArraysOf(UInt16s), ArraysOf(Int32s)),
        Gen.Select(ArraysOf(UInt32s), ArraysOf(Int64s), ArraysOf(UInt64s), ArraysOf(Float32s), ArraysOf(Float64s)),
        (small, large) => new AllTypes.ArraysFields
        {
            Int8 = small.Item1,
            Uint8 = small.Item2,
            Int16 = small.Item3,
            Uint16 = small.Item4,
            Int32 = small.Item5,
            Uint32 = large.Item1,
            Int64 = large.Item2,
            Uint64 = large.Item3,
            Float32 = large.Item4,
            Float64 = large.Item5,
        }
    );

    private static readonly Gen<AllTypes.OuterFields> Outers = Gen.Select(
        UInt32s,
        Strings,
        ArraysOf(Float64s),
        (count, name, values) => new AllTypes.OuterFields
        {
            Count = count,
            Inner = new() { Name = name, Values = values },
        }
    );

    public static readonly Gen<AllTypes.NestedFields> NestedFields = Gen.Select(
        Strings,
        Outers,
        ArraysOf(Trees(TreeDepth), 3),
        (label, outer, trees) => new AllTypes.NestedFields { Label = label, Outer = outer, Trees = trees }
    );

    private static readonly Gen<AllTypes.ReservedFields> ReservedFields = Gen.Select(
        Gen.Select(UInt8s, UInt8s, UInt8s, UInt8s, UInt8s),
        Strings,
        (keywords, type) => new AllTypes.ReservedFields
        {
            Keywords = new()
            {
                Class = keywords.Item1,
                Default = keywords.Item2,
                In = keywords.Item3,
                Match = keywords.Item4,
                Self = keywords.Item5,
                Type = type,
            },
        }
    );

    private static readonly Gen<AllTypes.Point> Points = Gen.Select(
        Float32s,
        Float32s,
        (x, y) => new AllTypes.Point { X = x, Y = y }
    );

    private static readonly Gen<AllTypes.Segment> Segments = Gen.Select(
        Points,
        Points,
        (start, end) => new AllTypes.Segment { Start = start, End = end }
    );

    public static readonly Gen<AllTypes.Packed> Packeds = Gen.Select(
        Gen.Select(Int8s, UInt8s, Int16s, UInt16s, Int32s, UInt32s),
        Gen.Select(Int64s, UInt64s, Float32s, Float64s, Gen.Bool),
        (small, large) => new AllTypes.Packed
        {
            Int8 = small.Item1,
            Uint8 = small.Item2,
            Int16 = small.Item3,
            Uint16 = small.Item4,
            Int32 = small.Item5,
            Uint32 = small.Item6,
            Int64 = large.Item1,
            Uint64 = large.Item2,
            Float32 = large.Item3,
            Float64 = large.Item4,
            Bool = large.Item5,
        }
    );

    private static readonly Gen<AllTypes.Documented> Documenteds = Gen.Select(
        UInt8s,
        UInt16s,
        UInt32s,
        (one, two, undocumented) => new AllTypes.Documented { One = one, Two = two, Undocumented = undocumented }
    );

    public static readonly Gen<AllTypes.StructsFields> StructsFields = Gen.Select(
        Packeds,
        ArraysOf(Documenteds, 6),
        Segments,
        ArraysOf(Segments, 6),
        Gen.Select(Points, ArraysOf(Points, 6)),
        (packed, documented, segment, segments, path) => new AllTypes.StructsFields
        {
            Packed = packed,
            Documented = documented,
            Segment = segment,
            Segments = segments,
            Path = new() { Start = path.Item1, Points = path.Item2 },
        }
    );

    public static readonly GeneratedMessage[] Messages =
    [
        new GeneratedMessage<AllTypes.ScalarsFields>(
            "all_types.Scalars",
            ScalarsFields,
            Describe,
            fields => fields.Serialize(),
            (fields, bytes) => fields.SerializeInto(bytes),
            bytes => Describe(new AllTypes.Scalars(bytes))
        ),
        new GeneratedMessage<AllTypes.ArraysFields>(
            "all_types.Arrays",
            ArraysFields,
            Describe,
            fields => fields.Serialize(),
            (fields, bytes) => fields.SerializeInto(bytes),
            bytes => Describe(new AllTypes.Arrays(bytes))
        ),
        new GeneratedMessage<AllTypes.NestedFields>(
            "all_types.Nested",
            NestedFields,
            Describe,
            fields => fields.Serialize(),
            (fields, bytes) => fields.SerializeInto(bytes),
            bytes => Describe(new AllTypes.Nested(bytes))
        ),
        new GeneratedMessage<AllTypes.ReservedFields>(
            "all_types.Reserved",
            ReservedFields,
            Describe,
            fields => fields.Serialize(),
            (fields, bytes) => fields.SerializeInto(bytes),
            bytes => Describe(new AllTypes.Reserved(bytes))
        ),
        new GeneratedMessage<AllTypes.StructsFields>(
            "all_types.Structs",
            StructsFields,
            Describe,
            fields => fields.Serialize(),
            (fields, bytes) => fields.SerializeInto(bytes),
            bytes => Describe(new AllTypes.Structs(bytes))
        ),
    ];

    // Bools

    /// <summary>The bool is the last of the message's fields, and of the struct's 43 bytes</summary>
    [Fact]
    public void ABoolReadsAnyByteBut0AsTrue() =>
        Gen.Select(ScalarsFields, Packeds, Gen.Byte).Check(sample =>
        {
            var (scalars, packed, value) = sample;

            var message = scalars.Serialize();
            message[^1] = value;
            Assert.Equal(value != 0, new AllTypes.Scalars(message).Bool);

            var bytes = packed.Serialize();
            bytes[^1] = value;
            Assert.Equal(value != 0, new AllTypes.Packed(bytes).Bool);
        });

    [Fact]
    public void ABoolIsWrittenAs1Or0() =>
        Gen.Select(ScalarsFields, Packeds).Check(sample =>
        {
            var (scalars, packed) = sample;

            Assert.Equal(scalars.Bool ? 1 : 0, scalars.Serialize()[^1]);
            Assert.Equal(packed.Bool ? 1 : 0, packed.Serialize()[^1]);
        });

    // Structs

    [Fact]
    public void APackedIsWrittenInIts43BytesLittleEndianAtFixedOffsets() =>
        Packeds.Check(packed =>
        {
            var bytes = packed.Serialize();

            Assert.Equal(43, AllTypes.Packed.Size);
            Assert.Equal(8, AllTypes.Packed.Alignment);
            Assert.Equal(Tafla.Tests.Tables.Scalars.StructBytes(DescribeScalars(packed).Scalars), bytes);
            Assert.Equal(DescribeScalars(packed), DescribeScalars(new AllTypes.Packed(bytes)));
        });

    [Fact]
    public void ASegmentHoldsItsPointsBytesInItsPlace() =>
        Segments.Check(segment =>
            Assert.Equal([.. segment.Start.Serialize(), .. segment.End.Serialize()], segment.Serialize())
        );

    // Views

    [Fact]
    public void AViewsIndexerReadsTheItemsThatItsEnumeratorReads() =>
        Gen.Select(NestedFields, StructsFields).Check(sample =>
        {
            var (nestedFields, structsFields) = sample;
            var trees = new AllTypes.Nested(nestedFields.Serialize()).Trees;
            var structs = new AllTypes.Structs(structsFields.Serialize());

            var indexed = new List<TableField>();
            for (var index = 0; index < trees.Count; index++)
            {
                indexed.Add(DescribeTree(trees[index]));
            }

            var count = 0;
            Assert.Equal(DescribeTrees(trees, ref count), indexed);
            Assert.Equal(Items(structs.Documented), Indexed(structs.Documented));
            Assert.Equal(Items(structs.Segments), Indexed(structs.Segments));
            Assert.Equal(Items(structs.Path.Points), Indexed(structs.Path.Points));
            Assert.Equal(Items(structs.Documented), structs.Documented.ToArray());
        });

    [Fact]
    public void AViewThrowsArgumentOutOfRangeExceptionForAnItemOutsideIt() =>
        Gen.Select(NestedFields, StructsFields, Gen.Int).Check(sample =>
        {
            var (nestedFields, structsFields, index) = sample;
            var nestedBytes = nestedFields.Serialize();
            var structsBytes = structsFields.Serialize();

            // The views are ref structs, which a lambda can't capture, so each reads them anew
            if (index < 0 || index >= new AllTypes.Nested(nestedBytes).Trees.Count)
            {
                Assert.Throws<ArgumentOutOfRangeException>(() => _ = new AllTypes.Nested(nestedBytes).Trees[index]);
            }

            if (index < 0 || index >= new AllTypes.Structs(structsBytes).Segments.Count)
            {
                Assert.Throws<ArgumentOutOfRangeException>(() => _ = new AllTypes.Structs(structsBytes).Segments[index]);
            }
        });

    [Fact]
    public void ViewsOfEmptyVectorsAndArraysOfStructsHoldNoItems()
    {
        var nested = new AllTypes.NestedFields
        {
            Label = "",
            Outer = new() { Count = 0, Inner = new() { Name = "", Values = default } },
            Trees = default,
        };
        var structs = new AllTypes.StructsFields
        {
            Packed = default,
            Documented = default,
            Segment = default,
            Segments = default,
            Path = new() { Start = default, Points = default },
        };

        var trees = new AllTypes.Nested(nested.Serialize()).Trees;
        var read = new AllTypes.Structs(structs.Serialize());

        Assert.Equal(0, trees.Count);
        var count = 0;
        Assert.Empty(DescribeTrees(trees, ref count));
        Assert.Equal(0, read.Documented.Count);
        Assert.Empty(Items(read.Documented));
        Assert.Empty(read.Segments.ToArray());
        Assert.Equal(0, read.Path.Points.Count);
    }

    /// <summary>
    /// A view reads an item without the items before it, from ends that corrupted bytes can lead
    /// anywhere, so the trees are read item by item through their indexers, to a bounded depth, as a
    /// caller that walks them recursively does
    /// </summary>
    [Fact]
    public void CorruptedBytesReadThroughTheViewsIndexersAsValuesOrThrowDecodeException()
    {
        var corruptions = Gen.Select(Gen.Int[0, int.MaxValue], Gen.Byte).Array[1, 4];

        Gen.Select(NestedFields, StructsFields, corruptions).Check(sample =>
        {
            var (nested, structs, replacements) = sample;
            var nestedBytes = Corrupted(nested.Serialize(), replacements);
            var structsBytes = Corrupted(structs.Serialize(), replacements);

            GeneratedMessageTests.ReadsAsValuesOrThrowsDecodeException(() =>
            {
                return DescribeTreesIndexed(new AllTypes.Nested(nestedBytes).Trees, 8);
            });
            GeneratedMessageTests.ReadsAsValuesOrThrowsDecodeException(() =>
            {
                var read = new AllTypes.Structs(structsBytes);
                return (Indexed(read.Documented), Indexed(read.Segments), Indexed(read.Path.Points));
            });
        });
    }

    // Descriptions of the fields that are written, and of those that the generated code reads

    public static Table Describe(AllTypes.ScalarsFields fields) =>
        Message(
            DefaultLayout,
            Of(fields.Int8),
            Of(fields.Uint8),
            Of(fields.Int16),
            Of(fields.Uint16),
            Of(fields.Int32),
            Of(fields.Uint32),
            Of(fields.Int64),
            Of(fields.Uint64),
            Of(fields.Float32),
            Of(fields.Float64),
            Of(fields.Bool)
        );

    public static Table Describe(AllTypes.Scalars scalars) =>
        Message(
            DefaultLayout,
            Of(scalars.Int8),
            Of(scalars.Uint8),
            Of(scalars.Int16),
            Of(scalars.Uint16),
            Of(scalars.Int32),
            Of(scalars.Uint32),
            Of(scalars.Int64),
            Of(scalars.Uint64),
            Of(scalars.Float32),
            Of(scalars.Float64),
            Of(scalars.Bool)
        );

    public static Table Describe(AllTypes.ArraysFields fields) =>
        Message(
            DefaultLayout,
            Of(fields.Int8.Span),
            Of(fields.Uint8.Span),
            Of(fields.Int16.Span),
            Of(fields.Uint16.Span),
            Of(fields.Int32.Span),
            Of(fields.Uint32.Span),
            Of(fields.Int64.Span),
            Of(fields.Uint64.Span),
            Of(fields.Float32.Span),
            Of(fields.Float64.Span)
        );

    public static Table Describe(AllTypes.Arrays arrays) =>
        Message(
            DefaultLayout,
            Of(arrays.Int8),
            Of(arrays.Uint8),
            Of(arrays.Int16),
            Of(arrays.Uint16),
            Of(arrays.Int32),
            Of(arrays.Uint32),
            Of(arrays.Int64),
            Of(arrays.Uint64),
            Of(arrays.Float32),
            Of(arrays.Float64)
        );

    public static Table Describe(AllTypes.NestedFields fields) =>
        Message(
            DefaultLayout,
            Of(fields.Label),
            Nested(Of(fields.Outer.Count), Nested(Of(fields.Outer.Inner.Name), Of(fields.Outer.Inner.Values.Span))),
            Vector(fields.Trees.ToArray().Select(DescribeTree))
        );

    public static Table Describe(AllTypes.Nested nested)
    {
        var outer = nested.Outer;
        var trees = 0;
        return Message(
            DefaultLayout,
            Of(nested.Label),
            Nested(Of(outer.Count), Nested(Of(outer.Inner.Name), Of(outer.Inner.Values))),
            Vector(DescribeTrees(nested.Trees, ref trees))
        );
    }

    public static Table Describe(AllTypes.ReservedFields fields)
    {
        var keywords = fields.Keywords;
        return Message(
            DefaultLayout,
            Nested(Of(keywords.Class), Of(keywords.Default), Of(keywords.In), Of(keywords.Match), Of(keywords.Self), Of(keywords.Type))
        );
    }

    public static Table Describe(AllTypes.Reserved reserved)
    {
        var keywords = reserved.Keywords;
        return Message(
            DefaultLayout,
            Nested(Of(keywords.Class), Of(keywords.Default), Of(keywords.In), Of(keywords.Match), Of(keywords.Self), Of(keywords.Type))
        );
    }

    public static Table Describe(AllTypes.StructsFields fields) =>
        Message(
            DefaultLayout,
            DescribeScalars(fields.Packed),
            StructArrayOf(DocumentedTypes, fields.Documented.ToArray().Select(Describe)),
            Describe(fields.Segment),
            StructArrayOf(SegmentTypes, fields.Segments.ToArray().Select(Describe)),
            Nested(Describe(fields.Path.Start), StructArrayOf(PointTypes, fields.Path.Points.ToArray().Select(Describe)))
        );

    public static Table Describe(AllTypes.Structs structs)
    {
        var path = structs.Path;
        return Message(
            DefaultLayout,
            DescribeScalars(structs.Packed),
            StructArrayOf(DocumentedTypes, Items(structs.Documented).Select(Describe)),
            Describe(structs.Segment),
            StructArrayOf(SegmentTypes, Items(structs.Segments).Select(Describe)),
            Nested(Describe(path.Start), StructArrayOf(PointTypes, Items(path.Points).Select(Describe)))
        );
    }

    /// <summary>The types of the scalars of each struct, in the order of their offsets</summary>
    private static readonly ScalarType[] DocumentedTypes = [ScalarType.UInt8, ScalarType.UInt16, ScalarType.UInt32];

    private static readonly ScalarType[] PointTypes = [ScalarType.Float32, ScalarType.Float32];

    private static readonly ScalarType[] SegmentTypes = [.. PointTypes, .. PointTypes];

    /// <summary>A packed's scalars, and its bool after them, which a struct of the tests' model has no type of</summary>
    private static StructField DescribeScalars(AllTypes.Packed packed) =>
        Struct(
            Of(packed.Int8),
            Of(packed.Uint8),
            Of(packed.Int16),
            Of(packed.Uint16),
            Of(packed.Int32),
            Of(packed.Uint32),
            Of(packed.Int64),
            Of(packed.Uint64),
            Of(packed.Float32),
            Of(packed.Float64),
            Of(packed.Bool ? (byte)1 : (byte)0)
        );

    private static StructField Describe(AllTypes.Documented documented) =>
        Struct(Of(documented.One), Of(documented.Two), Of(documented.Undocumented));

    private static StructField Describe(AllTypes.Point point) => Struct(Of(point.X), Of(point.Y));

    private static StructField Describe(AllTypes.Segment segment) =>
        Struct([.. Describe(segment.Start).Scalars, .. Describe(segment.End).Scalars]);

    private static TableField DescribeTree(AllTypes.TreeFields tree) =>
        Nested(Of(tree.Value), Vector(tree.Children.ToArray().Select(DescribeTree)));

    private static TableField DescribeTree(AllTypes.Tree tree)
    {
        var trees = 0;
        return DescribeTree(tree, ref trees);
    }

    /// <summary>A tree read through its children's enumerator, up to <see cref="MostTrees"/> trees in all</summary>
    private static TableField DescribeTree(AllTypes.Tree tree, ref int trees)
    {
        if (++trees > MostTrees)
        {
            return Nested(Of(tree.Value));
        }

        return Nested(Of(tree.Value), Vector(DescribeTrees(tree.Children, ref trees)));
    }

    /// <summary>Trees read through their vector's enumerator, up to <see cref="MostTrees"/> trees in all</summary>
    private static List<TableField> DescribeTrees(AllTypes.TreeVector trees, ref int count)
    {
        var described = new List<TableField>();
        foreach (var tree in trees)
        {
            described.Add(DescribeTree(tree, ref count));
        }

        return described;
    }

    /// <summary>A tree read through its children's indexer, <paramref name="depth"/> levels deep at most</summary>
    private static TableField DescribeTreeIndexed(AllTypes.Tree tree, int depth)
    {
        var children = tree.Children;
        if (depth == 0)
        {
            return Nested(Of(tree.Value), Of(children.Count));
        }

        return Nested(Of(tree.Value), Vector(DescribeTreesIndexed(children, depth - 1)));
    }

    /// <summary>Trees read through their vector's indexer, <paramref name="depth"/> levels deep at most</summary>
    private static List<TableField> DescribeTreesIndexed(AllTypes.TreeVector trees, int depth)
    {
        var described = new List<TableField>();
        for (var index = 0; index < trees.Count; index++)
        {
            described.Add(DescribeTreeIndexed(trees[index], depth));
        }

        return described;
    }

    /// <summary>Trees whose children hold children <paramref name="depth"/> levels deep at most, and none below</summary>
    private static Gen<AllTypes.TreeFields> Trees(int depth) =>
        depth == 0
            ? Int32s.Select(value => new AllTypes.TreeFields { Value = value, Children = default })
            : Gen.Select(Int32s, ArraysOf(Trees(depth - 1), 3), (value, children) => new AllTypes.TreeFields { Value = value, Children = children });

    private static byte[] Corrupted(byte[] bytes, (int Position, byte Value)[] replacements)
    {
        foreach (var (position, value) in replacements)
        {
            bytes[position % bytes.Length] = value;
        }

        return bytes;
    }

    private static List<AllTypes.Documented> Items(AllTypes.DocumentedArray structs)
    {
        var items = new List<AllTypes.Documented>();
        foreach (var item in structs)
        {
            items.Add(item);
        }

        return items;
    }

    private static List<AllTypes.Segment> Items(AllTypes.SegmentArray structs)
    {
        var items = new List<AllTypes.Segment>();
        foreach (var item in structs)
        {
            items.Add(item);
        }

        return items;
    }

    private static List<AllTypes.Point> Items(AllTypes.PointArray structs)
    {
        var items = new List<AllTypes.Point>();
        foreach (var item in structs)
        {
            items.Add(item);
        }

        return items;
    }

    private static List<AllTypes.Documented> Indexed(AllTypes.DocumentedArray structs)
    {
        var items = new List<AllTypes.Documented>();
        for (var index = 0; index < structs.Count; index++)
        {
            items.Add(structs[index]);
        }

        return items;
    }

    private static List<AllTypes.Segment> Indexed(AllTypes.SegmentArray structs)
    {
        var items = new List<AllTypes.Segment>();
        for (var index = 0; index < structs.Count; index++)
        {
            items.Add(structs[index]);
        }

        return items;
    }

    private static List<AllTypes.Point> Indexed(AllTypes.PointArray structs)
    {
        var items = new List<AllTypes.Point>();
        for (var index = 0; index < structs.Count; index++)
        {
            items.Add(structs[index]);
        }

        return items;
    }
}
