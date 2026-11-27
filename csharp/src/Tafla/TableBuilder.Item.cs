using System;

namespace Tafla;

public sealed partial class TableBuilder
{
    /// <summary>
    /// A table that a <see cref="TableBuilder"/> records in place: one of its fields, with
    /// <see cref="SetTable(int, Action{Item})"/>, or an item of a vector, with
    /// <see cref="SetVector(int, int, Action{Item, int})"/>, which give it to the fill that sets its
    /// fields. Its setters are the builder's, taking the same values and throwing the same
    /// exceptions, and write each field straight among the builder's bytes, so the table needs no
    /// builder of its own. They count the table's own fields and length, as a builder of it would.
    /// </summary>
    /// <remarks>
    /// <para>
    /// An item comes only from a fill, and is valid only while that fill runs. It's a
    /// <see langword="ref struct"/>, so that it can't be kept after that, nor captured by a lambda,
    /// such as the fill of a table recorded in place in it, which is given an item of its own.
    /// </para>
    /// <para>
    /// An item holds only its builder, whose setters set the fields of the table that it's recording
    /// at the time, the innermost. So an item passed on as the state of the fill of a table recorded
    /// in place in its own, which a <c>TState</c> that allows a ref struct can be, sets that inner
    /// table's fields, as the inner table's own item does. The default item has no builder, and its
    /// setters throw <see cref="NullReferenceException"/>.
    /// </para>
    /// </remarks>
    public readonly ref struct Item
    {
        private readonly TableBuilder _builder;

        internal Item(TableBuilder builder)
        {
            _builder = builder;
        }

        /// <inheritdoc cref="TableBuilder.SetInt8"/>
        public void SetInt8(sbyte value) => _builder.SetInt8(value);

        /// <inheritdoc cref="TableBuilder.SetUInt8"/>
        public void SetUInt8(byte value) => _builder.SetUInt8(value);

        /// <inheritdoc cref="TableBuilder.SetInt16"/>
        public void SetInt16(short value) => _builder.SetInt16(value);

        /// <inheritdoc cref="TableBuilder.SetUInt16"/>
        public void SetUInt16(ushort value) => _builder.SetUInt16(value);

        /// <inheritdoc cref="TableBuilder.SetInt32"/>
        public void SetInt32(int value) => _builder.SetInt32(value);

        /// <inheritdoc cref="TableBuilder.SetUInt32"/>
        public void SetUInt32(uint value) => _builder.SetUInt32(value);

        /// <inheritdoc cref="TableBuilder.SetInt64"/>
        public void SetInt64(long value) => _builder.SetInt64(value);

        /// <inheritdoc cref="TableBuilder.SetUInt64"/>
        public void SetUInt64(ulong value) => _builder.SetUInt64(value);

        /// <inheritdoc cref="TableBuilder.SetFloat32"/>
        public void SetFloat32(float value) => _builder.SetFloat32(value);

        /// <inheritdoc cref="TableBuilder.SetFloat64"/>
        public void SetFloat64(double value) => _builder.SetFloat64(value);

        /// <inheritdoc cref="TableBuilder.SetInt8Array"/>
        public void SetInt8Array(ReadOnlySpan<sbyte> values) => _builder.SetInt8Array(values);

        /// <inheritdoc cref="TableBuilder.SetUInt8Array"/>
        public void SetUInt8Array(ReadOnlySpan<byte> values) => _builder.SetUInt8Array(values);

        /// <inheritdoc cref="TableBuilder.SetInt16Array"/>
        public void SetInt16Array(ReadOnlySpan<short> values) => _builder.SetInt16Array(values);

        /// <inheritdoc cref="TableBuilder.SetUInt16Array"/>
        public void SetUInt16Array(ReadOnlySpan<ushort> values) => _builder.SetUInt16Array(values);

        /// <inheritdoc cref="TableBuilder.SetInt32Array"/>
        public void SetInt32Array(ReadOnlySpan<int> values) => _builder.SetInt32Array(values);

        /// <inheritdoc cref="TableBuilder.SetUInt32Array"/>
        public void SetUInt32Array(ReadOnlySpan<uint> values) => _builder.SetUInt32Array(values);

        /// <inheritdoc cref="TableBuilder.SetInt64Array"/>
        public void SetInt64Array(ReadOnlySpan<long> values) => _builder.SetInt64Array(values);

        /// <inheritdoc cref="TableBuilder.SetUInt64Array"/>
        public void SetUInt64Array(ReadOnlySpan<ulong> values) => _builder.SetUInt64Array(values);

        /// <inheritdoc cref="TableBuilder.SetFloat32Array"/>
        public void SetFloat32Array(ReadOnlySpan<float> values) => _builder.SetFloat32Array(values);

        /// <inheritdoc cref="TableBuilder.SetFloat64Array"/>
        public void SetFloat64Array(ReadOnlySpan<double> values) => _builder.SetFloat64Array(values);

        /// <inheritdoc cref="TableBuilder.SetBoolArray"/>
        public void SetBoolArray(ReadOnlySpan<bool> values) => _builder.SetBoolArray(values);

        /// <inheritdoc cref="TableBuilder.SetString"/>
        public void SetString(string value) => _builder.SetString(value);

        /// <inheritdoc cref="TableBuilder.SetStruct"/>
        public void SetStruct(ReadOnlySpan<byte> bytes) => _builder.SetStruct(bytes);

        /// <inheritdoc cref="TableBuilder.SetStructArray"/>
        public void SetStructArray(ReadOnlySpan<byte> bytes, int size, int alignment) =>
            _builder.SetStructArray(bytes, size, alignment);

        /// <inheritdoc cref="TableBuilder.SetTable(TableBuilder)"/>
        public void SetTable(TableBuilder table) => _builder.SetTable(table);

        /// <inheritdoc cref="TableBuilder.SetTable(TableBuilder, int)"/>
        public void SetTable(TableBuilder table, int alignment) => _builder.SetTable(table, alignment);

        /// <inheritdoc cref="TableBuilder.SetTable(ReadOnlySpan{byte}, int)"/>
        public void SetTable(ReadOnlySpan<byte> table, int alignment = DefaultTableAlignment) =>
            _builder.SetTable(table, alignment);

        /// <inheritdoc cref="TableBuilder.SetTable(int, Action{Item})"/>
        public void SetTable(int fields, Action<Item> fill) => _builder.SetTable(fields, fill);

        /// <inheritdoc cref="TableBuilder.SetTable{TState}(int, TState, Action{Item, TState})"/>
        public void SetTable<TState>(int fields, TState state, Action<Item, TState> fill)
            where TState : allows ref struct => _builder.SetTable(fields, state, fill);

        /// <inheritdoc cref="TableBuilder.SetVector(ReadOnlySpan{NestedTable})"/>
        public void SetVector(params ReadOnlySpan<NestedTable> items) => _builder.SetVector(items);

        /// <inheritdoc cref="TableBuilder.SetVector(int, int, Action{Item, int})"/>
        public void SetVector(int count, int fields, Action<Item, int> fill) =>
            _builder.SetVector(count, fields, fill);

        /// <inheritdoc cref="TableBuilder.SetVector{TState}(int, int, TState, Action{Item, int, TState})"/>
        public void SetVector<TState>(int count, int fields, TState state, Action<Item, int, TState> fill)
            where TState : allows ref struct => _builder.SetVector(count, fields, state, fill);
    }
}
