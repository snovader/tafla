using System.Runtime.CompilerServices;

namespace Tafla.Tests.Tables;

/// <summary>
/// Writes tables from their descriptions with <see cref="TableSizer"/> and <see cref="TableWriter"/>,
/// which write nested tables in place, so those must have no header and their parent's offset type,
/// as <see cref="NestedInPlace"/> makes them.
/// </summary>
public static class Write
{
    /// <summary>Returns a sizer of the table's layout, version and number of fields, with every field added.</summary>
    public static TableSizer Sized(this Table table)
    {
        var sizer = new TableSizer(table.Fields.Count, table.Layout.ToTableLayout(table.Version));

        foreach (var field in table.Fields)
        {
            Add(ref sizer, field);
        }

        return sizer;
    }

    /// <summary>Sizes and writes the table into new bytes.</summary>
    public static byte[] Written(this Table table)
    {
        var writer = table.Sized().CreateWriter();
        SetFields(ref writer, table, index => index);

        return writer.Bytes;
    }

    /// <summary>
    /// Writes the table's fields, each with its index and each item end with its item passed through
    /// <paramref name="indices"/>, which can change them.
    /// </summary>
    public static void SetFields(ref TableWriter writer, Table table, Func<int, int> indices)
    {
        for (var index = 0; index < table.Fields.Count; index++)
        {
            Set(ref writer, indices(index), table.Fields[index], indices);
        }
    }

    /// <summary>
    /// Returns the description with every table nested in it made one that a writer writes in place:
    /// from its builder, without a header, and with the root's offset type.
    /// </summary>
    public static Table NestedInPlace(this Table table) =>
        table with { Fields = new Seq<Field>(table.Fields.Select(field => InPlace(field, table.Layout.OffsetType))) };

    /// <summary>
    /// Returns the alignment that SPEC.md gives a nested table: its own, or by default 4 for bytes,
    /// and for a builder the largest alignment of its arrays, arrays of structs and own nested tables,
    /// or 1 if it has none of them.
    /// </summary>
    public static int Alignment(TableField table)
    {
        if (table.Alignment is { } alignment)
        {
            return alignment;
        }

        if (table.Nested.From == Source.Bytes)
        {
            return TableBuilder.DefaultTableAlignment;
        }

        return table.Nested.Table.Fields.Select(FieldAlignment).DefaultIfEmpty(1).Max();
    }

    private static int FieldAlignment(Field field) =>
        field switch
        {
            TableField table => Alignment(table),
            ArrayField array => array.Type.Size(),
            StructArray array => Scalars.StructAlignment(array.Types),
            _ => 1,
        };

    private static Field InPlace(Field field, OffsetType offsetType) =>
        field switch
        {
            TableField table => new TableField(InPlace(table.Nested, offsetType), table.Alignment),
            VectorField vector => new VectorField(new Seq<Nested>(vector.Items.Select(item => InPlace(item, offsetType)))),
            _ => field,
        };

    private static Nested InPlace(Nested nested, OffsetType offsetType)
    {
        var fields = nested.Table.Fields.Select(field => InPlace(field, offsetType));
        return new Nested(Source.Builder, new Table(Layout.Headerless(offsetType), 0, new Seq<Field>(fields)));
    }

    /// <summary>Adds the field to the sizer with the method of its type, and then a nested table's fields.</summary>
    private static void Add(ref TableSizer sizer, Field field)
    {
        switch (field)
        {
            case Scalar scalar:
                AddScalar(ref sizer, scalar.Type);
                break;

            case Bool:
                sizer.AddUInt8();
                break;

            case ArrayField array:
                AddArray(ref sizer, array.Type, array.Bits.Count);
                break;

            case StringField value:
                sizer.AddString(value.Value);
                break;

            case StringVectorField value:
                sizer.AddStringVector([.. value.Values]);
                break;

            case StructField value:
                sizer.AddStruct(Scalars.StructSize(value.Scalars.Select(scalar => scalar.Type)));
                break;

            case StructArray array:
                sizer.AddStructArray(array.Items.Count, Scalars.StructSize(array.Types), Scalars.StructAlignment(array.Types));
                break;

            case TableField table:
                sizer.AddTable(Alignment(table));
                AddNested(ref sizer, table.Nested.Table);
                break;

            case VectorField vector:
                sizer.AddVector(vector.Items.Count);
                foreach (var item in vector.Items)
                {
                    AddNested(ref sizer, item.Table);
                }

                break;

            default:
                throw new ArgumentException($"Unknown field {field}", nameof(field));
        }
    }

    private static void AddNested(ref TableSizer sizer, Table table)
    {
        var parent = sizer.StartTable(table.Fields.Count);
        foreach (var field in table.Fields)
        {
            Add(ref sizer, field);
        }

        sizer.EndTable(parent);
    }

    /// <summary>Writes field <paramref name="index"/> with the setter of its type, and then a nested table's fields.</summary>
    private static void Set(ref TableWriter writer, int index, Field field, Func<int, int> indices)
    {
        switch (field)
        {
            case Scalar scalar:
                SetScalar(ref writer, index, scalar);
                break;

            case Bool value:
                writer.SetUInt8(index, value.Value ? (byte)1 : (byte)0);
                break;

            case ArrayField array:
                SetArray(ref writer, index, array);
                break;

            case StringField value:
                writer.SetString(index, value.Value);
                break;

            case StringVectorField value:
                writer.SetStringVector(index, [.. value.Values]);
                break;

            case StructField value:
                writer.SetStruct(index, Scalars.StructBytes(value.Scalars));
                break;

            case StructArray array:
                var bytes = array.Items.SelectMany(item => Scalars.StructBytes(item)).ToArray();
                writer.SetStructArray(index, bytes, Scalars.StructSize(array.Types), Scalars.StructAlignment(array.Types));
                break;

            case TableField table:
                writer.SetTable(index, Alignment(table));
                SetNested(ref writer, table.Nested.Table, indices);
                break;

            case VectorField vector:
                var position = writer.SetVector(index, vector.Items.Count);
                for (var item = 0; item < vector.Items.Count; item++)
                {
                    SetNested(ref writer, vector.Items[item].Table, indices);
                    writer.SetItemEnd(position, indices(item));
                }

                break;

            default:
                throw new ArgumentException($"Unknown field {field}", nameof(field));
        }
    }

    private static void SetNested(ref TableWriter writer, Table table, Func<int, int> indices)
    {
        var parent = writer.StartTable(table.Fields.Count);
        SetFields(ref writer, table, indices);
        writer.EndTable(parent);
    }

    private static void AddScalar(ref TableSizer sizer, ScalarType type)
    {
        switch (type)
        {
            case ScalarType.Int8:
                sizer.AddInt8();
                break;
            case ScalarType.UInt8:
                sizer.AddUInt8();
                break;
            case ScalarType.Int16:
                sizer.AddInt16();
                break;
            case ScalarType.UInt16:
                sizer.AddUInt16();
                break;
            case ScalarType.Int32:
                sizer.AddInt32();
                break;
            case ScalarType.UInt32:
                sizer.AddUInt32();
                break;
            case ScalarType.Int64:
                sizer.AddInt64();
                break;
            case ScalarType.UInt64:
                sizer.AddUInt64();
                break;
            case ScalarType.Float32:
                sizer.AddFloat32();
                break;
            default:
                sizer.AddFloat64();
                break;
        }
    }

    private static void AddArray(ref TableSizer sizer, ScalarType type, int count)
    {
        switch (type)
        {
            case ScalarType.Int8:
                sizer.AddInt8Array(count);
                break;
            case ScalarType.UInt8:
                sizer.AddUInt8Array(count);
                break;
            case ScalarType.Int16:
                sizer.AddInt16Array(count);
                break;
            case ScalarType.UInt16:
                sizer.AddUInt16Array(count);
                break;
            case ScalarType.Int32:
                sizer.AddInt32Array(count);
                break;
            case ScalarType.UInt32:
                sizer.AddUInt32Array(count);
                break;
            case ScalarType.Int64:
                sizer.AddInt64Array(count);
                break;
            case ScalarType.UInt64:
                sizer.AddUInt64Array(count);
                break;
            case ScalarType.Float32:
                sizer.AddFloat32Array(count);
                break;
            default:
                sizer.AddFloat64Array(count);
                break;
        }
    }

    private static void SetScalar(ref TableWriter writer, int index, Scalar scalar)
    {
        var bits = scalar.Bits;
        switch (scalar.Type)
        {
            case ScalarType.Int8:
                writer.SetInt8(index, (sbyte)bits);
                break;
            case ScalarType.UInt8:
                writer.SetUInt8(index, (byte)bits);
                break;
            case ScalarType.Int16:
                writer.SetInt16(index, (short)bits);
                break;
            case ScalarType.UInt16:
                writer.SetUInt16(index, (ushort)bits);
                break;
            case ScalarType.Int32:
                writer.SetInt32(index, (int)bits);
                break;
            case ScalarType.UInt32:
                writer.SetUInt32(index, (uint)bits);
                break;
            case ScalarType.Int64:
                writer.SetInt64(index, (long)bits);
                break;
            case ScalarType.UInt64:
                writer.SetUInt64(index, bits);
                break;
            case ScalarType.Float32:
                // From its bits in memory, as Scalars makes floats
                var single = (uint)bits;
                writer.SetFloat32(index, Unsafe.As<uint, float>(ref single));
                break;
            default:
                writer.SetFloat64(index, Unsafe.As<ulong, double>(ref bits));
                break;
        }
    }

    private static void SetArray(ref TableWriter writer, int index, ArrayField array)
    {
        var bits = array.Bits;
        switch (array.Type)
        {
            case ScalarType.Int8:
                writer.SetInt8Array(index, Values(bits, value => (sbyte)value));
                break;
            case ScalarType.UInt8:
                writer.SetUInt8Array(index, Values(bits, value => (byte)value));
                break;
            case ScalarType.Int16:
                writer.SetInt16Array(index, Values(bits, value => (short)value));
                break;
            case ScalarType.UInt16:
                writer.SetUInt16Array(index, Values(bits, value => (ushort)value));
                break;
            case ScalarType.Int32:
                writer.SetInt32Array(index, Values(bits, value => (int)value));
                break;
            case ScalarType.UInt32:
                writer.SetUInt32Array(index, Values(bits, value => (uint)value));
                break;
            case ScalarType.Int64:
                writer.SetInt64Array(index, Values(bits, value => (long)value));
                break;
            case ScalarType.UInt64:
                writer.SetUInt64Array(index, Values(bits, value => value));
                break;
            case ScalarType.Float32:
                writer.SetFloat32Array(index, Scalars.Float32s(bits));
                break;
            default:
                writer.SetFloat64Array(index, Scalars.Float64s(bits));
                break;
        }
    }

    private static T[] Values<T>(Seq<ulong> bits, Func<ulong, T> value) => [.. bits.Select(value)];
}
