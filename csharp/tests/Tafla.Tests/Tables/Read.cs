using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;

namespace Tafla.Tests.Tables;

/// <summary>
/// The error of bytes that read without a decode error, but otherwise than their description: a
/// sized table whose header holds another length, a vector of another number of items, or one whose
/// items <see cref="Vector.Get"/> finds otherwise than reading them in order does.
/// </summary>
public sealed class MismatchException(string message) : Exception(message);

/// <summary>
/// Reads tables with <see cref="TableReader"/> as their descriptions set them out, and returns the
/// descriptions with the values read in place of theirs, so that they equal the descriptions when
/// the bytes hold the described values.
/// </summary>
/// <remarks>
/// A nested table is read in place when it has no header and its parent's offset type: a field from
/// its position in its parent's bytes, and a vector item from the bytes that
/// <see cref="Vector.ItemsInPlace"/> returns, which end where its parent's do. Any other is read as a
/// table of its own: a field from its position to the end of its parent, since its length isn't
/// written, and a vector item from the bytes that <see cref="Vector.Items"/> returns. The scalars of
/// structs are read at their offsets in the bytes that GetStruct and GetStructArray return.
/// </remarks>
public static class Read
{
    /// <summary>
    /// Reads a table from its own bytes, as the root or a vector item is read: its header, whose size
    /// must be the table's byte length when it's sized, and then its fields.
    /// </summary>
    public static Table Table(ReadOnlySpan<byte> bytes, Table table)
    {
        var reader = new TableReader(bytes, table.Layout.ToTableLayout());

        if (table.Layout.Sized && reader.GetSize() != bytes.Length)
        {
            throw new MismatchException($"The header holds the size {reader.GetSize()} of {bytes.Length} bytes");
        }

        var version = table.Layout.Versioned ? reader.GetVersion() : 0;

        var fields = new List<Field>();
        for (var index = 0; index < table.Fields.Count; index++)
        {
            fields.Add(Field(reader, reader.RootOffset(index), table.Fields[index]));
        }

        return new Table(table.Layout, version, fields);
    }

    /// <summary>Reads root field <paramref name="index"/> of a table of a layout as the field, without reading the header.</summary>
    public static Field RootField(ReadOnlySpan<byte> bytes, Layout layout, int index, Field field)
    {
        var reader = new TableReader(bytes, layout.ToTableLayout());
        return Field(reader, reader.RootOffset(index), field);
    }

    /// <summary>Reads a field at a position of a table as the field describes it.</summary>
    public static Field Field(TableReader reader, int position, Field field) =>
        field switch
        {
            Scalar scalar => Scalars.Read(reader, scalar.Type, position),
            Bool => new Bool(reader.GetUInt8(position) != 0),
            ArrayField array => Scalars.ReadArray(reader, array.Type, position),
            StringField => new StringField(reader.GetString(position)),
            StructField value => new StructField(Struct(reader, position, value)),
            StructArray array => StructArray(reader, position, array),
            TableField table => new TableField(new Nested(table.Nested.From, NestedTable(reader, position, table.Nested.Table)), table.Alignment),
            VectorField vector => Vector(reader, position, vector),
            _ => throw new ArgumentException($"Unknown field {field}", nameof(field)),
        };

    private static Seq<Scalar> Struct(TableReader reader, int position, StructField value)
    {
        var types = value.Scalars.Select(scalar => scalar.Type).ToArray();
        return Scalars.ReadStruct(reader.GetStruct(position, Scalars.StructSize(types)), types);
    }

    private static StructArray StructArray(TableReader reader, int position, StructArray array)
    {
        int size = Scalars.StructSize(array.Types);
        var bytes = reader.GetStructArray(position, size);

        var items = new List<Seq<Scalar>>();
        for (var start = 0; start < bytes.Length; start += size)
        {
            items.Add(Scalars.ReadStruct(bytes.Slice(start, size), array.Types));
        }

        return new StructArray(array.Types, items);
    }

    /// <summary>Reads a table nested with SetTable</summary>
    private static Table NestedTable(TableReader reader, int position, Table table)
    {
        if (ReadsInPlace(reader, table))
        {
            return InPlace(reader, position, table);
        }

        // A position past the end leaves no bytes, which the first read fails for
        var bytes = reader.Bytes;
        return Table(bytes[Math.Clamp(position, 0, bytes.Length)..], table);
    }

    /// <summary>
    /// Where a vector's item lies in its reader's bytes, its start and length, or the error of reading
    /// it, since a list can't hold the item's span
    /// </summary>
    private readonly record struct Item(int Start, int Length, DecodeException? Error)
    {
        /// <summary>Where <paramref name="item"/>, a span of <paramref name="bytes"/>, lies in them</summary>
        public static Item Of(ReadOnlySpan<byte> bytes, ReadOnlySpan<byte> item)
        {
            var start = Unsafe.ByteOffset(ref MemoryMarshal.GetReference(bytes), ref MemoryMarshal.GetReference(item));
            return new Item((int)start, item.Length, null);
        }

        public static Item Failed(DecodeException error) => new(0, 0, error);

        /// <summary>The item's bytes, within <paramref name="bytes"/></summary>
        public ReadOnlySpan<byte> In(ReadOnlySpan<byte> bytes) => bytes.Slice(Start, Length);
    }

    /// <summary>Reads a vector's items, after checking that finding each from its ends alone finds what reading them in order finds</summary>
    private static VectorField Vector(TableReader reader, int position, VectorField items)
    {
        var bytes = reader.Bytes;
        var vector = reader.GetVector(position);
        var inOrder = Items(bytes, vector.Items());
        var inPlace = Items(bytes, vector.ItemsInPlace());

        if (inOrder.Count != vector.Count || inPlace.Count != vector.Count)
        {
            throw new MismatchException($"A vector of {vector.Count} items reads {inOrder.Count} and {inPlace.Count} in order");
        }

        for (var index = 0; index < vector.Count; index++)
        {
            var item = Find(bytes, vector, index, inPlace: false);
            var itemInPlace = Find(bytes, vector, index, inPlace: true);
            if (!SameItem(item, inOrder[index]) || !SameItem(itemInPlace, inPlace[index]))
            {
                throw new MismatchException($"Get finds item {index} otherwise than reading in order does");
            }
        }

        foreach (var error in inOrder.Concat(inPlace).Select(item => item.Error))
        {
            if (error is not null)
            {
                throw error;
            }
        }

        if (vector.Count != items.Items.Count)
        {
            throw new MismatchException($"A vector of {vector.Count} items, rather than {items.Items.Count}");
        }

        var read = new List<Nested>();
        for (var index = 0; index < items.Items.Count; index++)
        {
            var nested = items.Items[index];
            var table = ReadsInPlace(reader, nested.Table)
                // At its start, in a reader of its offset type, which is its parent's
                ? InPlace(new TableReader(inPlace[index].In(bytes), nested.Table.Layout.ToTableLayout()), 0, nested.Table)
                : Table(inOrder[index].In(bytes), nested.Table);

            read.Add(new Nested(nested.From, table));
        }

        return new VectorField(read);
    }

    /// <summary>Reads the items in order, each item's bytes or error, as an enumerator of them moves on past an item that fails</summary>
    private static List<Item> Items(ReadOnlySpan<byte> bytes, VectorItems items)
    {
        var all = new List<Item>();
        var enumerator = items.GetEnumerator();
        while (true)
        {
            try
            {
                if (!enumerator.MoveNext())
                {
                    return all;
                }

                all.Add(Item.Of(bytes, enumerator.Current));
            }
            catch (DecodeException error)
            {
                all.Add(Item.Failed(error));
            }
        }
    }

    /// <summary>Finds item <paramref name="index"/> from its ends, as a table of its own or in place</summary>
    private static Item Find(ReadOnlySpan<byte> bytes, Vector vector, int index, bool inPlace)
    {
        try
        {
            return Item.Of(bytes, inPlace ? vector.GetInPlace(index) : vector.Get(index));
        }
        catch (DecodeException error)
        {
            return Item.Failed(error);
        }
    }

    /// <summary>Whether two items are the same bytes, from the same position, or the same error</summary>
    private static bool SameItem(Item a, Item b)
    {
        if (a.Error is not null || b.Error is not null)
        {
            return a.Error is { } x
                && b.Error is { } y
                && (x.Kind, x.Position, x.ByteLength, x.Available, x.End) == (y.Kind, y.Position, y.ByteLength, y.Available, y.End);
        }

        return a.Length == b.Length && (a.Length == 0 || a.Start == b.Start);
    }

    /// <summary>Whether a nested table is read in place: it has no header, and its parent's offset type</summary>
    private static bool ReadsInPlace(TableReader reader, Table table) =>
        table.Layout is { Sized: false, Versioned: false } && table.Layout.OffsetType == reader.Layout.OffsetType;

    /// <summary>Reads the fields of a table nested at a position, from offsets counted from its start, within its parent's bytes</summary>
    private static Table InPlace(TableReader reader, int position, Table table)
    {
        var fields = new List<Field>();
        for (var index = 0; index < table.Fields.Count; index++)
        {
            fields.Add(Field(reader, reader.FieldOffset(position, index), table.Fields[index]));
        }

        return new Table(table.Layout, 0, fields);
    }
}
