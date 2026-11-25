using CsCheck;
using Tafla.Tests.Tables;

namespace Tafla.Tests;

/// <summary>
/// Builders reject what they can't write with a <see cref="BuildException"/>, and leave the builder as
/// it was, and throw argument exceptions for mistakes of the code that calls them.
/// </summary>
public class TableBuilderTests
{
    /// <summary>Tables of the offset types whose limits a test reaches: uint32's is 2 GiB</summary>
    private static readonly Gen<Table> ShortTables =
        Strategy.TablesOf(Strategy.LayoutsOf(Gen.OneOf(OffsetType.UInt8, OffsetType.UInt16)));

    /// <summary>
    /// After any fields, an array of bytes as long as fits is written and reads back, and a byte
    /// longer is rejected with the length that it would make the table
    /// </summary>
    [Fact]
    public void TheLongestFieldThatFitsIsWrittenAndAByteMoreIsRejected() =>
        ShortTables.Check(table =>
        {
            var withEmpty = table with { Fields = new Seq<Field>([.. table.Fields, new ArrayField(ScalarType.UInt8, Array.Empty<ulong>())]) };
            if (Fits(withEmpty) is not { } empty)
            {
                // The table is too full for another field's offset and count
                return;
            }

            var room = (int)(table.Layout.OffsetType.MaxValue - empty.Length);

            var builder = withEmpty.Builder();
            foreach (var field in table.Fields)
            {
                Build.Set(builder, field);
            }

            var error = Assert.Throws<BuildException>(() => builder.SetUInt8Array(new byte[room + 1]));
            Assert.Equal(BuildErrorKind.TooLong, error.Kind);
            Assert.Equal((table.Fields.Count, table.Layout.OffsetType.MaxValue + 1L, table.Layout.OffsetType), (error.Index, error.Length, error.OffsetType));

            // The builder is as it was, and takes the field that fits
            var longest = new ArrayField(ScalarType.UInt8, new ulong[room]);
            Build.Set(builder, longest);
            var bytes = builder.ToArray();

            var full = table with { Fields = new Seq<Field>([.. table.Fields, longest]) };
            Assert.Equal(table.Layout.OffsetType.MaxValue, (uint)bytes.Length);
            Assert.Equal(full, Read.Table(bytes, full));
        });

    /// <summary>Returns the bytes of a table, or null if it's too long for its offset type</summary>
    private static byte[]? Fits(Table table)
    {
        try
        {
            return table.Bytes();
        }
        catch (BuildException error) when (error.Kind == BuildErrorKind.TooLong)
        {
            return null;
        }
    }

    /// <summary>
    /// A field too long for the offset type, set after any of the fields, is rejected, and the builder
    /// writes the same table once the other fields are set
    /// </summary>
    [Fact]
    public void AFieldTooLongIsRejectedAndLeavesTheBuilderAsItWas() =>
        ShortTables.SelectMany(table => Gen.Int[0, table.Fields.Count].Select(at => (table, at))).Check(sample =>
        {
            var (table, at) = sample;
            var builder = table.Builder();

            for (var index = 0; index < table.Fields.Count; index++)
            {
                if (index == at)
                {
                    TooLong(builder, table.Layout.OffsetType, index);
                }

                Build.Set(builder, table.Fields[index]);
            }

            if (at == table.Fields.Count)
            {
                Assert.Equal(BuildErrorKind.TooManyFields, Assert.Throws<BuildException>(() => builder.SetString("")).Kind);
            }

            Assert.Equal(table.Bytes(), builder.ToArray());
        });

    /// <summary>Tries fields of every kind that are too long for the offset type, which each throw TooLong</summary>
    private static void TooLong(TableBuilder builder, OffsetType offsetType, int index)
    {
        var length = (int)offsetType.MaxValue + 1;
        var tooLong = new Action[]
        {
            () => builder.SetUInt8Array(new byte[length]),
            () => builder.SetFloat64Array(new double[length / 8]),
            () => builder.SetString(new string('é', length / 2)),
            () => builder.SetStruct(new byte[length]),
            () => builder.SetStructArray(new byte[length], 4, 4),
            () => builder.SetTable(new byte[length]),
            () => builder.SetVector(new byte[length / 2], new byte[length / 2]),
        };

        foreach (var set in tooLong)
        {
            var error = Assert.Throws<BuildException>(set);
            Assert.Equal((BuildErrorKind.TooLong, index, offsetType), (error.Kind, error.Index, error.OffsetType));
        }
    }

    /// <summary>
    /// Tables of any description, with every table nested in them that a builder would write in
    /// place recorded in place instead, and every vector of them, hold the bytes of the tables nested
    /// as builders, or fail as they fail: as generated, with every nested table made one that's
    /// recorded in place, and those narrowed to uint8 offsets, which many are too long for.
    /// </summary>
    /// <remarks>
    /// A vector recorded in place checks its count, its items' ends and their offsets before its
    /// items, which builders of them have already set, so for a table too long the field and length
    /// may differ from the builders', and only the error's kind and offset type are compared. The
    /// builder's tests check them where they're the same.
    /// </remarks>
    [Fact]
    public void TablesRecordedInPlaceHoldTheBytesOfTheBuildersThatTheyReplace() =>
        Gen.Select(Strategy.Tables, Gen.Int[0, 2], (table, change) => change switch
            {
                0 => table.NestedInPlace(),
                1 => EveryTableInPlace(table).NestedInPlace(),
                _ => Narrowed(EveryTableInPlace(table)).NestedInPlace(),
            })
            .Check(table =>
                Assert.Equal(Outcome(() => table.Built().ToArray()), Outcome(() => table.BuiltInPlace().ToArray()))
            );

    /// <summary>
    /// Returns the table with every table nested in it padded to its own alignment, and the items of
    /// each vector cut to the fewest fields among them, so that each is recorded in place
    /// </summary>
    private static Table EveryTableInPlace(Table table)
    {
        static Nested Fields(Nested nested, int count) =>
            nested with { Table = EveryTableInPlace(nested.Table with { Fields = new Seq<Field>(nested.Table.Fields.Take(count)) }) };

        Field InPlace(Field field) =>
            field switch
            {
                TableField nested => new TableField(Fields(nested.Nested, nested.Nested.Table.Fields.Count), null),
                VectorField vector => new VectorField(
                    new Seq<Nested>(vector.Items.Select(item => Fields(item, vector.Items.Min(item => item.Table.Fields.Count))))
                ),
                _ => field,
            };

        return table with { Fields = new Seq<Field>(table.Fields.Select(InPlace)) };
    }

    /// <summary>Returns the table with uint8 offsets, and its version cut to fit them</summary>
    private static Table Narrowed(Table table) =>
        table with
        {
            Layout = table.Layout with { OffsetType = OffsetType.UInt8 },
            Version = Math.Min(table.Version, OffsetType.UInt8.MaxValue),
        };

    /// <summary>
    /// Returns the bytes that a build writes, in hexadecimal, or the message of the build error that
    /// it throws, of which only the offset type for a table too long
    /// </summary>
    private static string Outcome(Func<byte[]> build)
    {
        try
        {
            return Convert.ToHexString(build());
        }
        catch (BuildException error) when (error.Kind == BuildErrorKind.TooLong)
        {
            return $"{error.Kind}: {error.OffsetType}";
        }
        catch (BuildException error)
        {
            return $"{error.Kind}: {error.Message}";
        }
    }

    /// <summary>
    /// A table or vector recorded in place that fails, after any of a table's fields, leaves the
    /// builder as it was, however its fill fails: by throwing, or by leaving fields unset or setting
    /// too many, after setting the fields of a table that records its own nested tables in place,
    /// which roll back with it. A table too full for another field throws TooManyFields before its
    /// fill runs, and a field that its offset type can't address throws TooLong.
    /// </summary>
    [Fact]
    public void ATableOrVectorRecordedInPlaceThatFailsLeavesTheBuilderAsItWas() =>
        Strategy.Tables.Select(table => table.NestedInPlace())
            .Where(table => Fits(table) is not null)
            .SelectMany(table =>
                Gen.Select(
                    Gen.Int[0, table.Fields.Count],
                    Strategy.TablesOf(Gen.Const(Layout.Headerless(table.Layout.OffsetType))),
                    Gen.OneOf(Enum.GetValues<InPlaceFailure>()),
                    (at, content, failure) => (table, at, content: content.NestedInPlace(), failure)
                )
            )
            .Check(sample =>
            {
                var (table, at, content, failure) = sample;
                var builder = table.Builder();

                for (var index = 0; index < table.Fields.Count; index++)
                {
                    if (index == at)
                    {
                        FailInPlace(builder, content, failure, full: false);
                    }

                    Build.SetInPlace(builder, table.Fields[index]);
                }

                if (at == table.Fields.Count)
                {
                    FailInPlace(builder, content, failure, full: true);
                }

                Assert.Equal(table.Bytes(), builder.ToArray());
            });

    /// <summary>How a table or vector recorded in place fails, once its fill set the fields of a table</summary>
    public enum InPlaceFailure
    {
        /// <summary>The table's fill throws.</summary>
        Thrown,

        /// <summary>The table holds a field more than its fill sets.</summary>
        FieldLeft,

        /// <summary>The table holds a field fewer than its fill sets, or none.</summary>
        FieldBeyond,

        /// <summary>The second item of a vector throws.</summary>
        ItemThrown,

        /// <summary>The second item of a vector holds a field more than its fill sets.</summary>
        ItemFieldLeft,
    }

    /// <summary>Thrown by a fill, which the builder passes on</summary>
    private sealed class FillException : Exception;

    /// <summary>
    /// Records a table or vector in place that fails as <paramref name="failure"/> says, after
    /// setting <paramref name="content"/>'s fields in it, and checks what it throws: TooManyFields
    /// when the builder is <paramref name="full"/>, the failure, or TooLong for content too long for
    /// the offset type
    /// </summary>
    private static void FailInPlace(TableBuilder builder, Table content, InPlaceFailure failure, bool full)
    {
        var fields = content.Fields;
        var offsetType = builder.Layout.OffsetType;
        var state = (fields, offsetType);

        Action fail = failure switch
        {
            InPlaceFailure.Thrown => () => builder.SetTable(fields.Count, state, static (table, state) =>
            {
                SetFields(table, state);
                throw new FillException();
            }),
            InPlaceFailure.FieldLeft => () => builder.SetTable(fields.Count + 1, state, SetFields),
            InPlaceFailure.FieldBeyond => () => builder.SetTable(Math.Max(fields.Count - 1, 0), state, static (table, state) =>
            {
                SetFields(table, state);
                table.SetUInt8(1);
            }),
            InPlaceFailure.ItemThrown => () => builder.SetVector(2, fields.Count, state, static (item, index, state) =>
            {
                SetFields(item, state);
                if (index == 1)
                {
                    throw new FillException();
                }
            }),
            _ => () => builder.SetVector(2, fields.Count + 1, state, static (item, index, state) =>
            {
                SetFields(item, state);
                if (index == 0)
                {
                    item.SetUInt8(1);
                }
            }),
        };

        var thrown = Record.Exception(fail);
        if (full)
        {
            Assert.Equal(BuildErrorKind.TooManyFields, Assert.IsType<BuildException>(thrown).Kind);
        }
        else if (thrown is BuildException { Kind: var kind })
        {
            var expected = failure switch
            {
                InPlaceFailure.FieldLeft or InPlaceFailure.ItemFieldLeft => BuildErrorKind.Incomplete,
                InPlaceFailure.FieldBeyond => BuildErrorKind.TooManyFields,
                _ => BuildErrorKind.TooLong,
            };

            Assert.Contains(kind, new[] { expected, BuildErrorKind.TooLong });
        }
        else
        {
            Assert.IsType<FillException>(thrown);
            Assert.True(failure is InPlaceFailure.Thrown or InPlaceFailure.ItemThrown);
        }
    }

    /// <summary>Sets a table's fields in a table recorded in place, recording its own nested tables in place</summary>
    private static void SetFields(TableBuilder.Item table, (Seq<Field> Fields, OffsetType OffsetType) state)
    {
        foreach (var field in state.Fields)
        {
            Build.SetInPlace(table, field, state.OffsetType);
        }
    }

    /// <summary>
    /// Tables recorded in place five deep, each padded for an array after a uint8, hold the bytes of
    /// builders nested so, and a fill that throws at the deepest leaves the builder as it was
    /// </summary>
    [Fact]
    public void RecordsTablesInPlaceSeveralLevelsDeep()
    {
        var builder = new TableBuilder(2);
        builder.SetUInt8(7);
        Assert.Throws<FillException>(() => builder.SetTable(3, 5, Deep(throwAtDeepest: true)));

        builder.SetTable(3, 5, Deep(throwAtDeepest: false));
        var expected = new TableBuilder(2);
        expected.SetUInt8(7);
        expected.SetTable(DeepBuilders(5));

        Assert.Equal(expected.ToArray(), builder.ToArray());
    }

    /// <summary>A table of a uint8, an array of int16s, int32s or float64s by its depth, and the table nested in it</summary>
    private static Action<TableBuilder.Item, int> Deep(bool throwAtDeepest) =>
        (table, depth) =>
        {
            table.SetUInt8((byte)depth);
            SetDeepArray(table, depth);
            if (depth > 1)
            {
                table.SetTable(3, depth - 1, Deep(throwAtDeepest));
            }
            else if (throwAtDeepest)
            {
                throw new FillException();
            }
            else
            {
                table.SetTable(ReadOnlySpan<byte>.Empty, 1);
            }
        };

    private static TableBuilder DeepBuilders(int depth)
    {
        var table = new TableBuilder(3);
        table.SetUInt8((byte)depth);
        switch (depth % 3)
        {
            case 0:
                table.SetInt16Array([(short)depth, -1]);
                break;
            case 1:
                table.SetInt32Array([depth, -1]);
                break;
            default:
                table.SetFloat64Array([depth, -1]);
                break;
        }

        if (depth > 1)
        {
            table.SetTable(DeepBuilders(depth - 1));
        }
        else
        {
            table.SetTable(ReadOnlySpan<byte>.Empty, 1);
        }

        return table;
    }

    private static void SetDeepArray(TableBuilder.Item table, int depth)
    {
        switch (depth % 3)
        {
            case 0:
                table.SetInt16Array([(short)depth, -1]);
                break;
            case 1:
                table.SetInt32Array([depth, -1]);
                break;
            default:
                table.SetFloat64Array([depth, -1]);
                break;
        }
    }

    /// <summary>
    /// A table recorded in place is padded to its alignment once its fields are set, and that
    /// padding counts against its parent's length, as a nested builder's does: here a uint8 array of
    /// <paramref name="before"/> bytes leaves a table of 61 int32s, 248 bytes aligned to 4, padded by
    /// 1, 0 or 3 bytes, the last of which make its parent of uint8 offsets 256 bytes long
    /// </summary>
    [Theory]
    [InlineData(0, null)]
    [InlineData(1, null)]
    [InlineData(2, 256L)]
    public void APaddedTableRecordedInPlaceCountsItsPaddingAsANestedBuilderDoes(int before, long? tooLong)
    {
        var layout = new TableLayout { OffsetType = OffsetType.UInt8 };
        var elements = new int[61];

        var nested = new TableBuilder(1, layout);
        nested.SetInt32Array(elements);
        var parent = new TableBuilder(2, layout);
        parent.SetUInt8Array(new byte[before]);
        var builder = new TableBuilder(2, layout);
        builder.SetUInt8Array(new byte[before]);

        var expected = Record.Exception(() => parent.SetTable(nested));
        var actual = Record.Exception(() =>
            builder.SetTable(1, elements, static (table, elements) => table.SetInt32Array(elements))
        );

        if (tooLong is { } length)
        {
            var error = Assert.IsType<BuildException>(actual);
            Assert.Equal((BuildErrorKind.TooLong, 1, length), (error.Kind, error.Index, error.Length));
            Assert.Equal(Assert.IsType<BuildException>(expected).Message, error.Message);

            // The builder is as it was, and takes a shorter table
            parent.SetTable(new TableBuilder(0, layout));
            builder.SetTable(0, static _ => { });
        }
        else
        {
            Assert.Null(expected);
            Assert.Null(actual);
        }

        Assert.Equal(parent.ToArray(), builder.ToArray());
    }

    /// <summary>
    /// A table recorded in place counts its own length as a builder of it would, so its own field
    /// too long for the offset type throws TooLong with that field's index and the table's length,
    /// and a table or vector whose items fit but make the parent too long throws it with the
    /// parent's
    /// </summary>
    [Fact]
    public void ATableRecordedInPlaceIsTooLongWhereABuilderOfItWouldBe()
    {
        // Two offsets, and a uint8 array's count and 1,000 bytes: 1,006 bytes before the table
        var layout = new TableLayout { OffsetType = OffsetType.UInt16 };
        var builder = new TableBuilder(2, layout);
        builder.SetUInt8Array(new byte[1000]);

        // Two offsets, a uint8, and the array's count and bytes
        var own = Assert.Throws<BuildException>(() =>
            builder.SetTable(2, static table =>
            {
                table.SetUInt8(1);
                table.SetUInt8Array(new byte[65_535]);
            })
        );
        Assert.Equal((BuildErrorKind.TooLong, 1, 4 + 1 + 2 + 65_535L), (own.Kind, own.Index, own.Length));

        var parent = Assert.Throws<BuildException>(() =>
            builder.SetTable(1, static table => table.SetUInt8Array(new byte[65_000]))
        );
        Assert.Equal((BuildErrorKind.TooLong, 1, 1006 + 2 + 2 + 65_000L), (parent.Kind, parent.Index, parent.Length));

        // The vector's count and two ends, and in each item an offset, a count and the bytes
        var item = Assert.Throws<BuildException>(() =>
            builder.SetVector(2, 1, static (item, index) => item.SetUInt8Array(new byte[65_534]))
        );
        Assert.Equal((BuildErrorKind.TooLong, 0, 2 + 2 + 65_534L), (item.Kind, item.Index, item.Length));

        var vector = Assert.Throws<BuildException>(() =>
            builder.SetVector(2, 1, static (item, index) => item.SetUInt8Array(new byte[40_000]))
        );
        Assert.Equal((BuildErrorKind.TooLong, 1, 1006 + 6 + 2 * 40_004L), (vector.Kind, vector.Index, vector.Length));

        builder.SetVector(2, 1, static (item, index) => item.SetUInt8Array(new byte[index]));
        var built = new TableBuilder(2, layout);
        built.SetUInt8Array(new byte[1000]);
        built.SetVector(Items(layout, [], [0]));
        Assert.Equal(built.ToArray(), builder.ToArray());
    }

    /// <summary>Returns builders of tables of one uint8 array each, of the values given</summary>
    private static NestedTable[] Items(TableLayout layout, params byte[][] arrays) =>
        [
            .. arrays.Select(array =>
            {
                var item = new TableBuilder(1, layout);
                item.SetUInt8Array(array);
                return (NestedTable)item;
            }),
        ];

    /// <summary>
    /// A vector recorded in place checks its count, its items' ends and the offsets that every item
    /// holds before calling its fill, and throws TooLong with the length that they make, even past
    /// the longest array for uint32 offsets, without allocating it
    /// </summary>
    [Theory]
    [InlineData(OffsetType.UInt8, 254, 0, 256L)]
    [InlineData(OffsetType.UInt8, 100, 2, 1 + 101 + 200L)]
    [InlineData(OffsetType.UInt16, 32_767, 0, 2 + 32_768 * 2L)]
    [InlineData(OffsetType.UInt32, 1 << 29, 0, 4 + ((1L << 29) + 1) * 4)]
    [InlineData(OffsetType.UInt32, 1 << 28, 1, 4 + ((1L << 28) + 1) * 4 + (1L << 28) * 4)]
    public void AVectorRecordedInPlaceTooLongForItsItemsOffsetsThrowsBeforeItsFill(OffsetType offsetType, int count, int fields, long length)
    {
        var builder = new TableBuilder(1, new TableLayout { OffsetType = offsetType });
        var calls = 0;

        var error = Assert.Throws<BuildException>(() => builder.SetVector(count, fields, (_, _) => calls++));
        Assert.Equal((BuildErrorKind.TooLong, 0, length, offsetType), (error.Kind, error.Index, error.Length, error.OffsetType));
        Assert.Equal(0, calls);

        builder.SetVector(1, fields, (item, _) =>
        {
            calls++;
            for (var field = 0; field < fields; field++)
            {
                item.SetUInt8((byte)field);
            }
        });
        Assert.Equal(1, calls);
    }

    /// <summary>
    /// A table recorded in place has its parent's offset type and no header, so offsets that alone
    /// are too long for the offset type throw HeaderTooLong, as a builder of them does
    /// </summary>
    [Fact]
    public void RejectsATableRecordedInPlaceWhoseOffsetsAreTooLong()
    {
        var layout = new TableLayout { OffsetType = OffsetType.UInt8 };
        var builder = new TableBuilder(1, layout);

        var table = Assert.Throws<BuildException>(() => builder.SetTable(256, static _ => { }));
        Assert.Equal((BuildErrorKind.HeaderTooLong, 256, OffsetType.UInt8), (table.Kind, table.Fields, table.OffsetType));
        var item = Assert.Throws<BuildException>(() => builder.SetVector(1, 256, static (_, _) => { }));
        Assert.Equal((BuildErrorKind.HeaderTooLong, 256), (item.Kind, item.Fields));

        builder.SetTable(254, static table =>
        {
            for (var field = 0; field < 254; field++)
            {
                table.SetTable(ReadOnlySpan<byte>.Empty, 1);
            }
        });
        Assert.Equal(255, builder.ToArray().Length);
    }

    /// <summary>
    /// A fill's state can be a span, such as the values of a vector's items, and an item a table
    /// recorded in place in turn, which writes the bytes of builders of them
    /// </summary>
    [Fact]
    public void PassesASpanToAFillAsItsState()
    {
        ReadOnlySpan<int> values = [1, -2, int.MaxValue];
        var builder = new TableBuilder(1);
        builder.SetVector(values.Length, 2, values, static (item, index, values) =>
        {
            item.SetInt32(values[index]);
            item.SetTable(1, values[..index], static (table, before) => table.SetInt32Array(before));
        });

        var items = new NestedTable[values.Length];
        for (var index = 0; index < values.Length; index++)
        {
            var table = new TableBuilder(1);
            table.SetInt32Array(values[..index]);
            var item = new TableBuilder(2);
            item.SetInt32(values[index]);
            item.SetTable(table);
            items[index] = item;
        }

        var expected = new TableBuilder(1);
        expected.SetVector(items);
        Assert.Equal(expected.ToArray(), builder.ToArray());
    }

    /// <summary>
    /// While a table is recorded in place, the builder's bytes hold it unfinished, so copying them,
    /// by ToArray() or by nesting the builder, throws InvalidOperationException, which the fill
    /// passes on, leaving the builder as it was
    /// </summary>
    [Fact]
    public void RejectsCopyingABuilderWhileATableIsRecordedInPlaceInIt()
    {
        var builder = new TableBuilder(1);
        var other = new TableBuilder(1);

        Assert.Throws<InvalidOperationException>(() => builder.SetTable(0, _ => builder.ToArray()));
        Assert.Throws<InvalidOperationException>(() => builder.SetTable(1, table => table.SetTable(builder)));
        Assert.Throws<InvalidOperationException>(() => builder.SetVector(1, 0, (_, _) => other.SetTable(builder)));
        Assert.Throws<InvalidOperationException>(() => builder.SetVector(1, 1, (item, _) => item.SetVector(builder)));

        builder.SetTable(0, static _ => { });
        Assert.Equal(new byte[] { 4, 0, 0, 0 }, builder.ToArray());
        other.SetTable(builder);
    }

    [Fact]
    public void RejectsAFieldBeyondTheCount()
    {
        var builder = new TableBuilder(1);
        builder.SetUInt8(1);
        var expected = builder.ToArray();

        var error = Assert.Throws<BuildException>(() => builder.SetUInt8(2));
        Assert.Equal((BuildErrorKind.TooManyFields, 1), (error.Kind, error.Fields));
        Assert.Equal("Can't add field 2 to a table of 1", error.Message);
        Assert.Equal(expected, builder.ToArray());
    }

    [Fact]
    public void RejectsWritingBeforeEveryFieldIsSet()
    {
        for (var set = 0; set < 3; set++)
        {
            var builder = new TableBuilder(set + 1);
            for (var field = 0; field < set; field++)
            {
                builder.SetInt32(field);
            }

            var error = Assert.Throws<BuildException>(builder.ToArray);
            Assert.Equal((BuildErrorKind.Incomplete, set, set + 1), (error.Kind, error.FieldsSet, error.Fields));
            Assert.Equal($"Only {set} of the table's {set + 1} fields were set", error.Message);
        }
    }

    /// <summary>A nested builder must have every field set, and the parent is left as it was otherwise</summary>
    [Fact]
    public void RejectsAnIncompleteNestedBuilder()
    {
        var incomplete = new TableBuilder(2);
        incomplete.SetUInt8(1);

        var complete = new TableBuilder(0);
        var builder = new TableBuilder(1);

        Assert.Equal(BuildErrorKind.Incomplete, Assert.Throws<BuildException>(() => builder.SetTable(incomplete)).Kind);
        Assert.Equal(BuildErrorKind.Incomplete, Assert.Throws<BuildException>(() => builder.SetTable(incomplete, 8)).Kind);
        Assert.Equal(BuildErrorKind.Incomplete, Assert.Throws<BuildException>(() => builder.SetVector(complete, incomplete)).Kind);
        Assert.Equal(BuildErrorKind.Incomplete, Assert.Throws<BuildException>(() => builder.SetTable(builder)).Kind);

        builder.SetVector(complete, default(NestedTable));
        Assert.Equal(new byte[] { 4, 0, 0, 0, 2, 0, 0, 0, 12, 0, 0, 0, 12, 0, 0, 0 }, builder.ToArray());
    }

    [Fact]
    public void RejectsBytesThatArentWholeStructs()
    {
        var builder = new TableBuilder(1);

        var error = Assert.Throws<BuildException>(() => builder.SetStructArray(new byte[7], 2, 2));
        Assert.Equal((BuildErrorKind.NotWholeStructs, 7L, 2), (error.Kind, error.Length, error.StructSize));
        Assert.Equal("7 bytes aren't a whole number of 2-byte structs", error.Message);

        builder.SetStructArray(new byte[6], 3, 1);
        Assert.Equal(new byte[] { 4, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0 }, builder.ToArray());
    }

    [Theory]
    [InlineData(OffsetType.UInt8, false, false, 255)]
    [InlineData(OffsetType.UInt8, true, true, 253)]
    [InlineData(OffsetType.UInt16, false, false, 32767)]
    [InlineData(OffsetType.UInt16, true, false, 32766)]
    public void RejectsAHeaderTooLong(OffsetType offsetType, bool sized, bool versioned, int most)
    {
        var layout = new TableLayout { Sized = sized, Version = versioned ? 1 : null, OffsetType = offsetType };
        TableBuilder Start(int fields) => new(fields, layout);

        var builder = Start(most);
        for (var field = 0; field < most; field++)
        {
            builder.SetTable(ReadOnlySpan<byte>.Empty, 1);
        }

        Assert.Equal(layout.HeaderSize + most * layout.OffsetSize, builder.ToArray().Length);

        var error = Assert.Throws<BuildException>(() => Start(most + 1));
        Assert.Equal((BuildErrorKind.HeaderTooLong, most + 1, offsetType), (error.Kind, error.Fields, error.OffsetType));
        Assert.Throws<BuildException>(() => Start(int.MaxValue));
    }

    /// <summary>
    /// A table can't be longer than a .NET array, which a test can't allocate, but a vector that nests
    /// bytes of 64 MiB 32 times is that long without allocating more
    /// </summary>
    [Fact]
    public void RejectsATableOfUInt32OffsetsLongerThanAnArrayBeforeWritingIt()
    {
        var item = new byte[64 << 20];
        var builder = new TableBuilder(1);

        var error = Assert.Throws<BuildException>(() => builder.SetVector([.. Enumerable.Repeat<NestedTable>(item, 32)]));
        Assert.Equal((BuildErrorKind.TooLong, 4 + 33 * 4 + (32L << 26), OffsetType.UInt32), (error.Kind, error.Length, error.OffsetType));
    }

    [Theory]
    [InlineData(OffsetType.UInt8, 255u)]
    [InlineData(OffsetType.UInt16, 65535u)]
    public void RejectsAVersionTooLargeForItsSlot(OffsetType offsetType, uint largest)
    {
        var layout = new TableLayout { Version = 0, OffsetType = offsetType };

        // A reader reads a table of any version, whatever its layout's
        var reader = new TableReader(new TableBuilder(0, layout with { Version = largest }).ToArray(), layout);
        Assert.Equal(largest, reader.GetVersion());

        var error = Assert.Throws<BuildException>(() => new TableBuilder(0, layout with { Version = largest + 1 }));
        Assert.Equal((BuildErrorKind.VersionTooLarge, largest + 1, offsetType), (error.Kind, error.Version, error.OffsetType));
        Assert.Equal($"Version {largest + 1} doesn't fit in a {offsetType.Name} slot", error.Message);
    }

    /// <summary>Mistakes of the code that calls the builder, which no values cause, throw argument exceptions</summary>
    [Fact]
    public void ThrowsArgumentExceptionsForMistakesOfTheCaller()
    {
        var builder = new TableBuilder(4);

        Assert.Throws<ArgumentOutOfRangeException>(() => new TableBuilder(-1));
        Assert.Throws<ArgumentOutOfRangeException>(() => builder.SetTable(new byte[4], 3));
        Assert.Throws<ArgumentOutOfRangeException>(() => builder.SetTable(new TableBuilder(0), 16));
        Assert.Throws<ArgumentOutOfRangeException>(() => builder.SetStructArray(new byte[4], 0, 1));
        Assert.Throws<ArgumentOutOfRangeException>(() => builder.SetStructArray(new byte[4], 4, 0));
        Assert.Throws<ArgumentNullException>(() => builder.SetString(null!));
        Assert.Throws<ArgumentNullException>(() => builder.SetTable((TableBuilder)null!));
        Assert.Throws<ArgumentNullException>(() => builder.SetVector((TableBuilder)null!));
        Assert.Throws<ArgumentNullException>(() => builder.SetVector((byte[])null!));
        Assert.Throws<ArgumentNullException>(() => builder.SetTable(1, (Action<TableBuilder.Item>)null!));
        Assert.Throws<ArgumentNullException>(() => builder.SetTable(1, 0, (Action<TableBuilder.Item, int>)null!));
        Assert.Throws<ArgumentNullException>(() => builder.SetVector(1, 1, (Action<TableBuilder.Item, int>)null!));
        Assert.Throws<ArgumentNullException>(() => builder.SetVector(1, 1, 0, (Action<TableBuilder.Item, int, int>)null!));
        Assert.Throws<ArgumentOutOfRangeException>(() => builder.SetTable(-1, static _ => { }));
        Assert.Throws<ArgumentOutOfRangeException>(() => builder.SetVector(-1, 1, static (_, _) => { }));
        Assert.Throws<ArgumentOutOfRangeException>(() => builder.SetVector(1, -1, static (_, _) => { }));

        // None of them set a field
        builder.SetUInt8(1);
        builder.SetUInt8(2);
        builder.SetUInt8(3);
        builder.SetUInt8(4);
        Assert.Equal(new byte[] { 16, 0, 0, 0, 17, 0, 0, 0, 18, 0, 0, 0, 19, 0, 0, 0, 1, 2, 3, 4 }, builder.ToArray());
    }

    /// <summary>
    /// A setter takes its field's own C# type, so a value of another converts as C# converts it: an
    /// integer out of range keeps its low bits in an unchecked cast, and a double narrows to the
    /// nearest float, or to infinity of its sign
    /// </summary>
    [Fact]
    public void ConvertsValuesAsCSharpConvertsThem()
    {
        var wide = 300;
        var large = 1e300;
        var builder = new TableBuilder(3);
        builder.SetUInt8(unchecked((byte)wide));
        builder.SetFloat32((float)large);
        builder.SetFloat32((float)0.1);

        var reader = new TableReader(builder.ToArray());
        Assert.Equal(44, reader.GetUInt8(reader.RootOffset(0)));
        Assert.Equal(float.PositiveInfinity, reader.GetFloat32(reader.RootOffset(1)));
        Assert.Equal(0x3dcc_cccdu, BitConverter.SingleToUInt32Bits(reader.GetFloat32(reader.RootOffset(2))));
    }
}
