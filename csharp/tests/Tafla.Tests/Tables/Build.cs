namespace Tafla.Tests.Tables;

/// <summary>Builds tables from their descriptions with <see cref="TableBuilder"/>.</summary>
public static class Build
{
    /// <summary>Starts a builder of the table's layout, version and number of fields, with no field set.</summary>
    public static TableBuilder Builder(this Table table) =>
        new(table.Fields.Count, table.Layout.ToTableLayout(table.Version));

    /// <summary>Builds the table, with every field set, and the tables nested in it from builders of their own.</summary>
    public static TableBuilder Built(this Table table) => Built(table, inPlace: false);

    /// <summary>
    /// Builds the table as <see cref="Built(Table)"/> does, but records each table nested from a
    /// builder that it would write in place in place: one without a header, of its parent's offset
    /// type, and padded to its own alignment rather than one given. It records each vector of them
    /// whose items hold as many fields in place too, and the tables nested in tables that it nests
    /// from their builders.
    /// </summary>
    public static TableBuilder BuiltInPlace(this Table table) => Built(table, inPlace: true);

    /// <summary>Writes the table with a <see cref="TableBuilder"/>.</summary>
    public static byte[] Bytes(this Table table) => table.Built().ToArray();

    /// <summary>Sets a field on the builder with the setter of its type.</summary>
    public static void Set(TableBuilder builder, Field field) => Set(new BuilderTarget(builder), field, null);

    /// <summary>
    /// Sets a field on the builder with the setter of its type, recording the tables nested in it in
    /// place as <see cref="BuiltInPlace"/> does.
    /// </summary>
    public static void SetInPlace(TableBuilder builder, Field field) =>
        Set(new BuilderTarget(builder), field, builder.Layout.OffsetType);

    /// <summary>
    /// Sets a field of a table recorded in place in a table of <paramref name="offsetType"/>, and
    /// records the tables nested in it in place too.
    /// </summary>
    public static void SetInPlace(TableBuilder.Item item, Field field, OffsetType offsetType) =>
        Set(new ItemTarget(item), field, offsetType);

    /// <summary>Builds a table to nest, as its builder or its bytes.</summary>
    public static NestedTable NestedTable(Nested nested) => NestedTable(nested, inPlace: false);

    private static TableBuilder Built(Table table, bool inPlace)
    {
        var builder = table.Builder();
        OffsetType? recorded = inPlace ? table.Layout.OffsetType : null;
        foreach (var field in table.Fields)
        {
            Set(new BuilderTarget(builder), field, recorded);
        }

        return builder;
    }

    private static NestedTable NestedTable(Nested nested, bool inPlace) =>
        nested.From == Source.Builder ? Built(nested.Table, inPlace) : nested.Table.Bytes();

    /// <summary>
    /// Sets a field with the setter of its type, and records the tables nested in it in place when
    /// <paramref name="inPlace"/> is the offset type of the table that the field is in
    /// </summary>
    private static void Set<T>(T target, Field field, OffsetType? inPlace)
        where T : ITarget, allows ref struct
    {
        switch (field)
        {
            case Scalar scalar:
                Scalars.Set(target, scalar);
                break;

            case Bool value:
                target.SetUInt8(value.Value ? (byte)1 : (byte)0);
                break;

            case ArrayField array:
                Scalars.SetArray(target, array);
                break;

            case StringField value:
                target.SetString(value.Value);
                break;

            case Absent:
                target.SetAbsent();
                break;

            case StructField value:
                target.SetStruct(Scalars.StructBytes(value.Scalars));
                break;

            case StructArray array:
                var bytes = array.Items.SelectMany(item => Scalars.StructBytes(item)).ToArray();
                target.SetStructArray(bytes, Scalars.StructSize(array.Types), Scalars.StructAlignment(array.Types));
                break;

            case TableField { Alignment: null } table when inPlace is { } offsetType && RecordsInPlace(table.Nested, offsetType):
                target.SetTable(table.Nested.Table.Fields.Count, (table.Nested.Table.Fields, offsetType), SetTableFields);
                break;

            case TableField { Nested.From: Source.Builder } table:
                var nested = Built(table.Nested.Table, inPlace is not null);
                if (table.Alignment is { } alignment)
                {
                    target.SetTable(nested, alignment);
                }
                else
                {
                    target.SetTable(nested);
                }

                break;

            case TableField table:
                target.SetTable(table.Nested.Table.Bytes(), table.Alignment ?? TableBuilder.DefaultTableAlignment);
                break;

            case VectorField vector when inPlace is { } offsetType && ItemsInPlace(vector.Items, offsetType):
                var fields = vector.Items.Count == 0 ? 0 : vector.Items[0].Table.Fields.Count;
                target.SetVector(vector.Items.Count, fields, (vector.Items, offsetType), SetItemFields);
                break;

            case VectorField vector:
                NestedTable[] items = [.. vector.Items.Select(item => NestedTable(item, inPlace is not null))];
                target.SetVector(items);
                break;

            default:
                throw new ArgumentException($"Unknown field {field}", nameof(field));
        }
    }

    /// <summary>Sets the fields of a table recorded in place, recording the tables nested in it in place too</summary>
    private static void SetTableFields(TableBuilder.Item table, (Seq<Field> Fields, OffsetType OffsetType) nested)
    {
        foreach (var field in nested.Fields)
        {
            Set(new ItemTarget(table), field, nested.OffsetType);
        }
    }

    /// <summary>Sets the fields of item <paramref name="index"/> of a vector recorded in place</summary>
    private static void SetItemFields(TableBuilder.Item item, int index, (Seq<Nested> Items, OffsetType OffsetType) vector) =>
        SetTableFields(item, (vector.Items[index].Table.Fields, vector.OffsetType));

    /// <summary>
    /// Whether a nested table is one that a builder would write in place in a table of
    /// <paramref name="offsetType"/>: from its builder, without a header, and with that offset type
    /// </summary>
    private static bool RecordsInPlace(Nested nested, OffsetType offsetType) =>
        nested.From == Source.Builder && nested.Table.Layout == Layout.Headerless(offsetType);

    /// <summary>Whether every item is recorded in place, and they hold as many fields</summary>
    private static bool ItemsInPlace(Seq<Nested> items, OffsetType offsetType) =>
        items.All(item => RecordsInPlace(item, offsetType) && item.Table.Fields.Count == items[0].Table.Fields.Count);
}
