namespace Tafla.Tests.Tables;

/// <summary>
/// What a table's fields are set on: a <see cref="TableBuilder"/>, or the
/// <see cref="TableBuilder.Item"/> of a table that a builder records in place, whose setters are the
/// builder's, so that the tests set fields on both alike.
/// </summary>
public interface ITarget
{
    void SetInt8(sbyte value);

    void SetUInt8(byte value);

    void SetInt16(short value);

    void SetUInt16(ushort value);

    void SetInt32(int value);

    void SetUInt32(uint value);

    void SetInt64(long value);

    void SetUInt64(ulong value);

    void SetFloat32(float value);

    void SetFloat64(double value);

    void SetInt8Array(ReadOnlySpan<sbyte> values);

    void SetUInt8Array(ReadOnlySpan<byte> values);

    void SetInt16Array(ReadOnlySpan<short> values);

    void SetUInt16Array(ReadOnlySpan<ushort> values);

    void SetInt32Array(ReadOnlySpan<int> values);

    void SetUInt32Array(ReadOnlySpan<uint> values);

    void SetInt64Array(ReadOnlySpan<long> values);

    void SetUInt64Array(ReadOnlySpan<ulong> values);

    void SetFloat32Array(ReadOnlySpan<float> values);

    void SetFloat64Array(ReadOnlySpan<double> values);

    void SetBoolArray(ReadOnlySpan<bool> values);

    void SetString(string value);

    void SetStruct(ReadOnlySpan<byte> bytes);

    void SetStructArray(ReadOnlySpan<byte> bytes, int size, int alignment);

    void SetTable(TableBuilder table);

    void SetTable(TableBuilder table, int alignment);

    void SetTable(ReadOnlySpan<byte> table, int alignment);

    void SetTable<TState>(int fields, TState state, Action<TableBuilder.Item, TState> fill);

    void SetVector(ReadOnlySpan<NestedTable> items);

    void SetVector<TState>(int count, int fields, TState state, Action<TableBuilder.Item, int, TState> fill);
}

/// <summary>The fields of a builder.</summary>
public readonly struct BuilderTarget(TableBuilder builder) : ITarget
{
    public void SetInt8(sbyte value) => builder.SetInt8(value);

    public void SetUInt8(byte value) => builder.SetUInt8(value);

    public void SetInt16(short value) => builder.SetInt16(value);

    public void SetUInt16(ushort value) => builder.SetUInt16(value);

    public void SetInt32(int value) => builder.SetInt32(value);

    public void SetUInt32(uint value) => builder.SetUInt32(value);

    public void SetInt64(long value) => builder.SetInt64(value);

    public void SetUInt64(ulong value) => builder.SetUInt64(value);

    public void SetFloat32(float value) => builder.SetFloat32(value);

    public void SetFloat64(double value) => builder.SetFloat64(value);

    public void SetInt8Array(ReadOnlySpan<sbyte> values) => builder.SetInt8Array(values);

    public void SetUInt8Array(ReadOnlySpan<byte> values) => builder.SetUInt8Array(values);

    public void SetInt16Array(ReadOnlySpan<short> values) => builder.SetInt16Array(values);

    public void SetUInt16Array(ReadOnlySpan<ushort> values) => builder.SetUInt16Array(values);

    public void SetInt32Array(ReadOnlySpan<int> values) => builder.SetInt32Array(values);

    public void SetUInt32Array(ReadOnlySpan<uint> values) => builder.SetUInt32Array(values);

    public void SetInt64Array(ReadOnlySpan<long> values) => builder.SetInt64Array(values);

    public void SetUInt64Array(ReadOnlySpan<ulong> values) => builder.SetUInt64Array(values);

    public void SetFloat32Array(ReadOnlySpan<float> values) => builder.SetFloat32Array(values);

    public void SetFloat64Array(ReadOnlySpan<double> values) => builder.SetFloat64Array(values);

    public void SetBoolArray(ReadOnlySpan<bool> values) => builder.SetBoolArray(values);

    public void SetString(string value) => builder.SetString(value);

    public void SetStruct(ReadOnlySpan<byte> bytes) => builder.SetStruct(bytes);

    public void SetStructArray(ReadOnlySpan<byte> bytes, int size, int alignment) =>
        builder.SetStructArray(bytes, size, alignment);

    public void SetTable(TableBuilder table) => builder.SetTable(table);

    public void SetTable(TableBuilder table, int alignment) => builder.SetTable(table, alignment);

    public void SetTable(ReadOnlySpan<byte> table, int alignment) => builder.SetTable(table, alignment);

    public void SetTable<TState>(int fields, TState state, Action<TableBuilder.Item, TState> fill) =>
        builder.SetTable(fields, state, fill);

    public void SetVector(ReadOnlySpan<NestedTable> items) => builder.SetVector(items);

    public void SetVector<TState>(int count, int fields, TState state, Action<TableBuilder.Item, int, TState> fill) =>
        builder.SetVector(count, fields, state, fill);
}

/// <summary>The fields of a table that a builder records in place.</summary>
public readonly ref struct ItemTarget : ITarget
{
    private readonly TableBuilder.Item _item;

    public ItemTarget(TableBuilder.Item item)
    {
        _item = item;
    }

    public void SetInt8(sbyte value) => _item.SetInt8(value);

    public void SetUInt8(byte value) => _item.SetUInt8(value);

    public void SetInt16(short value) => _item.SetInt16(value);

    public void SetUInt16(ushort value) => _item.SetUInt16(value);

    public void SetInt32(int value) => _item.SetInt32(value);

    public void SetUInt32(uint value) => _item.SetUInt32(value);

    public void SetInt64(long value) => _item.SetInt64(value);

    public void SetUInt64(ulong value) => _item.SetUInt64(value);

    public void SetFloat32(float value) => _item.SetFloat32(value);

    public void SetFloat64(double value) => _item.SetFloat64(value);

    public void SetInt8Array(ReadOnlySpan<sbyte> values) => _item.SetInt8Array(values);

    public void SetUInt8Array(ReadOnlySpan<byte> values) => _item.SetUInt8Array(values);

    public void SetInt16Array(ReadOnlySpan<short> values) => _item.SetInt16Array(values);

    public void SetUInt16Array(ReadOnlySpan<ushort> values) => _item.SetUInt16Array(values);

    public void SetInt32Array(ReadOnlySpan<int> values) => _item.SetInt32Array(values);

    public void SetUInt32Array(ReadOnlySpan<uint> values) => _item.SetUInt32Array(values);

    public void SetInt64Array(ReadOnlySpan<long> values) => _item.SetInt64Array(values);

    public void SetUInt64Array(ReadOnlySpan<ulong> values) => _item.SetUInt64Array(values);

    public void SetFloat32Array(ReadOnlySpan<float> values) => _item.SetFloat32Array(values);

    public void SetFloat64Array(ReadOnlySpan<double> values) => _item.SetFloat64Array(values);

    public void SetBoolArray(ReadOnlySpan<bool> values) => _item.SetBoolArray(values);

    public void SetString(string value) => _item.SetString(value);

    public void SetStruct(ReadOnlySpan<byte> bytes) => _item.SetStruct(bytes);

    public void SetStructArray(ReadOnlySpan<byte> bytes, int size, int alignment) =>
        _item.SetStructArray(bytes, size, alignment);

    public void SetTable(TableBuilder table) => _item.SetTable(table);

    public void SetTable(TableBuilder table, int alignment) => _item.SetTable(table, alignment);

    public void SetTable(ReadOnlySpan<byte> table, int alignment) => _item.SetTable(table, alignment);

    public void SetTable<TState>(int fields, TState state, Action<TableBuilder.Item, TState> fill) =>
        _item.SetTable(fields, state, fill);

    public void SetVector(ReadOnlySpan<NestedTable> items) => _item.SetVector(items);

    public void SetVector<TState>(int count, int fields, TState state, Action<TableBuilder.Item, int, TState> fill) =>
        _item.SetVector(count, fields, state, fill);
}
