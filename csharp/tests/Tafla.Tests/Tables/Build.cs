namespace Tafla.Tests.Tables;

/// <summary>Builds tables from their descriptions with <see cref="TableBuilder"/>.</summary>
public static class Build
{
    /// <summary>Starts a builder of the table's layout, version and number of fields, with no field set.</summary>
    public static TableBuilder Builder(this Table table) =>
        table.Layout.Versioned
            ? new TableBuilder(table.Fields.Count, table.Layout.ToTableLayout(), table.Version)
            : new TableBuilder(table.Fields.Count, table.Layout.ToTableLayout());

    /// <summary>Builds the table, with every field set.</summary>
    public static TableBuilder Built(this Table table)
    {
        var builder = table.Builder();
        foreach (var field in table.Fields)
        {
            Set(builder, field);
        }

        return builder;
    }

    /// <summary>Writes the table with a <see cref="TableBuilder"/>.</summary>
    public static byte[] Bytes(this Table table) => table.Built().ToArray();

    /// <summary>Sets a field on the builder with the setter of its type.</summary>
    public static void Set(TableBuilder builder, Field field)
    {
        switch (field)
        {
            case Scalar scalar:
                Scalars.Set(builder, scalar);
                break;

            case Bool value:
                builder.SetUInt8(value.Value ? (byte)1 : (byte)0);
                break;

            case ArrayField array:
                Scalars.SetArray(builder, array);
                break;

            case StringField value:
                builder.SetString(value.Value);
                break;

            case StructField value:
                builder.SetStruct(Scalars.StructBytes(value.Scalars));
                break;

            case StructArray array:
                var bytes = array.Items.SelectMany(item => Scalars.StructBytes(item)).ToArray();
                builder.SetStructArray(bytes, Scalars.StructSize(array.Types), Scalars.StructAlignment(array.Types));
                break;

            case TableField { Nested.From: Source.Builder } table:
                var nested = table.Nested.Table.Built();
                if (table.Alignment is { } alignment)
                {
                    builder.SetTable(nested, alignment);
                }
                else
                {
                    builder.SetTable(nested);
                }

                break;

            case TableField table:
                builder.SetTable(table.Nested.Table.Bytes(), table.Alignment ?? TableBuilder.DefaultTableAlignment);
                break;

            case VectorField vector:
                builder.SetVector([.. vector.Items.Select(NestedTable)]);
                break;

            default:
                throw new ArgumentException($"Unknown field {field}", nameof(field));
        }
    }

    /// <summary>Builds a table to nest, as its builder or its bytes.</summary>
    public static NestedTable NestedTable(Nested nested) =>
        nested.From == Source.Builder ? nested.Table.Built() : nested.Table.Bytes();
}
