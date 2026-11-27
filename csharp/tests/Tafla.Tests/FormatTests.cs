using System.Text;
using Tafla.Tests.Tables;

namespace Tafla.Tests;

/// <summary>
/// Tables that <see cref="TableBuilder"/> writes follow the format's rules, as SPEC.md at the root of
/// the repository states them: the header holds the length and version, offsets lie within the table,
/// values follow each other in field order with only the padding that aligns arrays and nested
/// tables, multi-byte values are little-endian, and nested tables and vector items hold the bytes of
/// their tables.
/// </summary>
public class FormatTests
{
    [Fact]
    public void SizedTablesHoldTheirLengthInTheHeader() =>
        CheckFormat(found =>
        {
            var layout = found.Table.Layout;
            if (layout.Sized)
            {
                int width = layout.OffsetType.Size;
                Assert.Equal(Bytes.SlotBytes(found.Bytes.Length, width), found.Bytes[..width]);
            }
        });

    [Fact]
    public void VersionedTablesHoldTheirVersionInTheHeader() =>
        CheckFormat(found =>
        {
            var layout = found.Table.Layout;
            if (layout.Versioned)
            {
                int width = layout.OffsetType.Size;
                int position = layout.Sized ? width : 0;
                Assert.Equal(Bytes.SlotBytes(found.Table.Version, width), found.Bytes[position..(position + width)]);
            }
        });

    /// <summary>
    /// Every offset of every table lies after its header and offsets, and its field ends within the
    /// table. A table nested with no fields is empty, and can end its parent.
    /// </summary>
    [Fact]
    public void OffsetsLieWithinTheirTable() =>
        CheckFormat(found =>
        {
            foreach (var field in found.Fields)
            {
                Assert.InRange(field.Position, found.OffsetsEnd, found.Bytes.Length);
                Assert.InRange(field.Position + field.Length, field.Position, found.Bytes.Length);
            }
        });

    /// <summary>
    /// From the start of the table that holds them, which is aligned in turn when it's nested: an
    /// array of scalars at a multiple of their size, and an array of structs of their largest scalar's
    /// </summary>
    [Fact]
    public void ArraysStartAtAMultipleOfTheirAlignment() =>
        CheckFormat(found =>
        {
            int width = found.Table.Layout.OffsetType.Size;
            foreach (var field in found.Fields)
            {
                if (ValueOf(field.Field) is { IsArray: true } value)
                {
                    Assert.Equal(0, (field.Position + width) % value.Alignment);
                }
            }
        });

    /// <summary>
    /// Scalars hold their values' little-endian bytes, structs their scalars', arrays and strings their
    /// counts' and their elements', and vectors their counts' and their items' ends', which count from
    /// the vector's position, after the ends. A vector of strings holds its strings' UTF-8 after its
    /// ends, one after another, without padding.
    /// </summary>
    [Fact]
    public void ValuesAreLittleEndian() =>
        CheckFormat(found =>
        {
            int width = found.Table.Layout.OffsetType.Size;
            foreach (var field in found.Fields)
            {
                var bytes = found.Bytes[field.Position..(field.Position + field.Length)];

                if (ValueOf(field.Field) is { } value)
                {
                    if (value.IsArray)
                    {
                        Assert.Equal(Bytes.SlotBytes(value.Count, width), bytes[..width]);
                        Assert.Equal(value.Bytes, bytes[width..]);
                    }
                    else
                    {
                        Assert.Equal(value.Bytes, bytes);
                    }
                }
                else if (field.Field is StringVectorField strings)
                {
                    Assert.Equal(Bytes.SlotBytes(strings.Values.Count, width), bytes[..width]);

                    int end = (strings.Values.Count + 1) * width;
                    for (var index = 0; index < strings.Values.Count; index++)
                    {
                        var utf8 = Encoding.UTF8.GetBytes(strings.Values[index]);
                        Assert.Equal(utf8, bytes[end..(end + utf8.Length)]);

                        end += utf8.Length;
                        Assert.Equal(Bytes.SlotBytes(end, width), bytes[((index + 1) * width)..((index + 2) * width)]);
                    }
                }
                else if (field.Field is VectorField vector)
                {
                    Assert.Equal(Bytes.SlotBytes(vector.Items.Count, width), bytes[..width]);

                    long end = (vector.Items.Count + 1) * width;
                    for (var index = 0; index < vector.Items.Count; index++)
                    {
                        end += vector.Items[index].Table.Bytes().Length;
                        Assert.Equal(Bytes.SlotBytes(end, width), bytes[((index + 1) * width)..((index + 2) * width)]);
                    }
                }
            }
        });

    /// <summary>
    /// Values follow the header and offsets in field order, each after the previous one, with only the
    /// zeroed padding that aligns an array's elements and a nested table to their alignment between
    /// them, and the last one ends the table
    /// </summary>
    [Fact]
    public void ValuesFollowEachOtherWithOnlyZeroedPaddingToAlignThem() =>
        CheckFormat(found =>
        {
            int width = found.Table.Layout.OffsetType.Size;
            int end = found.OffsetsEnd;

            foreach (var field in found.Fields)
            {
                int position = end + Padding(field.Field, end, width);
                Assert.Equal(position, field.Position);
                Assert.All(found.Bytes[end..position], padding => Assert.Equal(0, padding));

                end = position + field.Length;
            }

            Assert.Equal(found.Bytes.Length, end);
        });

    /// <summary>The bytes that a builder writes are the same nested in another table as on their own</summary>
    [Fact]
    public void NestedTablesAndVectorItemsHoldTheBytesOfTheirTables() =>
        CheckFormat(found =>
        {
            if (found.OwnBytes is { } own)
            {
                Assert.Equal(own, found.Bytes);
            }
        });

    /// <summary>Checks a property of every table in the bytes of a table that any description builds</summary>
    private static void CheckFormat(Action<TableIn> property) =>
        Strategy.Tables.Check(table =>
        {
            foreach (var found in TablesIn(table.Bytes(), table))
            {
                property(found);
            }
        });

    /// <summary>
    /// A table in the bytes of a table: the table itself, a table nested in it or one of its vector
    /// items, or a table in one of those, with the bytes that its builder writes on its own when it's
    /// in another
    /// </summary>
    private sealed record TableIn(byte[] Bytes, Table Table, List<FieldIn> Fields, byte[]? OwnBytes)
    {
        /// <summary>The length of the header and offsets, where the values start</summary>
        public int OffsetsEnd => Table.Layout.ToTableLayout().HeaderSize + Fields.Count * Table.Layout.OffsetType.Size;
    }

    /// <summary>A field and where it lies in its table's bytes</summary>
    private sealed record FieldIn(Field Field, int Position, int Length);

    /// <summary>
    /// Finds every table in bytes that hold a table, and the fields of each. Fields lie at the
    /// positions that their offsets hold, and are as long as the values that the description gives,
    /// and as the tables that their builders write on their own.
    /// </summary>
    private static List<TableIn> TablesIn(byte[] bytes, Table table)
    {
        var found = new List<TableIn>();
        FindTables(bytes, table, null, found);

        return found;
    }

    private static void FindTables(byte[] bytes, Table table, byte[]? ownBytes, List<TableIn> found)
    {
        int width = table.Layout.OffsetType.Size;
        int offsets = table.Layout.ToTableLayout().HeaderSize;
        var fields = new List<FieldIn>();

        for (var index = 0; index < table.Fields.Count; index++)
        {
            var field = table.Fields[index];
            var position = (int)Bytes.Slot(bytes, offsets + index * width, width);

            int length;
            switch (field)
            {
                case TableField nested:
                    length = FindNested(bytes, position, nested.Nested, found);
                    break;

                case VectorField vector:
                    // The items follow the count and the ends
                    int end = position + (vector.Items.Count + 1) * width;
                    foreach (var item in vector.Items)
                    {
                        end += FindNested(bytes, end, item, found);
                    }

                    length = end - position;
                    break;

                case StringVectorField strings:
                    // The count and the ends, and then the strings
                    length = (strings.Values.Count + 1) * width + strings.Values.Sum(Encoding.UTF8.GetByteCount);
                    break;

                default:
                    length = ValueOf(field)!.ByteLength(width);
                    break;
            }

            fields.Add(new FieldIn(field, position, length));
        }

        found.Add(new TableIn(bytes, table, fields, ownBytes));
    }

    /// <summary>Finds a nested table or vector item at a position, and the tables in it, and returns its length</summary>
    private static int FindNested(byte[] bytes, int position, Nested nested, List<TableIn> found)
    {
        var ownBytes = nested.Table.Bytes();
        Assert.InRange(position + ownBytes.Length, position, bytes.Length);

        FindTables(bytes[position..(position + ownBytes.Length)], nested.Table, ownBytes, found);
        return ownBytes.Length;
    }

    /// <summary>
    /// The value of a scalar, array, string or struct field: the little-endian bytes of a scalar, or
    /// of a struct's scalars, or the count of an array's elements, of a string's bytes or of an
    /// array's structs, and their little-endian bytes, which start at a multiple of an alignment from
    /// the table's start
    /// </summary>
    private sealed record Value(bool IsArray, int Alignment, int Count, byte[] Bytes)
    {
        /// <summary>The value's length in a table of offsets and counts of <paramref name="width"/> bytes</summary>
        public int ByteLength(int width) => IsArray ? width + Bytes.Length : Bytes.Length;
    }

    /// <summary>Returns the value of a field that holds one rather than tables</summary>
    private static Value? ValueOf(Field field) =>
        field switch
        {
            Scalar scalar => new Value(false, 1, 1, Scalars.StructBytes([scalar])),
            Bool value => new Value(false, 1, 1, [value.Value ? (byte)1 : (byte)0]),
            ArrayField array => new Value(true, array.Type.Size(), array.Bits.Count, Scalars.StructBytes(array.Bits.Select(bits => new Scalar(array.Type, bits)))),
            StringField value => new Value(true, 1, Encoding.UTF8.GetByteCount(value.Value), Encoding.UTF8.GetBytes(value.Value)),
            StructField value => new Value(false, 1, 1, Scalars.StructBytes(value.Scalars)),
            StructArray array => new Value(true, Scalars.StructAlignment(array.Types), array.Items.Count, [.. array.Items.SelectMany(item => Scalars.StructBytes(item))]),
            _ => null,
        };

    /// <summary>The padding before a field that would start at <paramref name="end"/>, which aligns an array's elements, after its count, and a nested table's start</summary>
    private static int Padding(Field field, int end, int width)
    {
        int start = ValueOf(field) is { IsArray: true } ? end + width : end;
        int alignment = AlignmentOf(field);

        return (alignment - start % alignment) % alignment;
    }

    /// <summary>
    /// The multiple of bytes from the start of its table that a field starts at, after its count when
    /// it's an array: an array's alignment, a nested table's alignment, and 1 for any other
    /// </summary>
    private static int AlignmentOf(Field field) =>
        field switch
        {
            _ when ValueOf(field) is { IsArray: true } value => value.Alignment,
            TableField { Alignment: { } alignment } => alignment,
            TableField { Nested.From: Source.Bytes } => TableBuilder.DefaultTableAlignment,
            // A builder's is the largest alignment of its fields
            TableField nested => nested.Nested.Table.Fields.Select(AlignmentOf).DefaultIfEmpty(1).Max(),
            _ => 1,
        };
}
