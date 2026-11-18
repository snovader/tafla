using System;

namespace Tafla;

/// <summary>
/// A vector read from a table: its items, which are tables. <see cref="TableReader.GetVector"/> reads
/// it, after checking that its count and the ends of its items lie within the bytes, and each item is
/// checked when it's read, so reading one can throw a <see cref="DecodeException"/>.
/// </summary>
/// <remarks>
/// <para>
/// The vector holds its count, the end of each item, counted from the vector's position, and then the
/// items. <see cref="Get"/> and <see cref="GetInPlace"/> find any item from its end and the one before
/// it, without reading the others, and <see cref="Items"/> and <see cref="ItemsInPlace"/> read each
/// item's end once, in order.
/// </para>
/// <para>
/// An item that has no header and its parent's offset type is read in place: from the bytes that
/// <see cref="GetInPlace"/> returns, which start where it does and end where its parent's do, with a
/// <see cref="TableReader"/> of its parent's offset type, at position 0. Its reads may reach past its
/// end, as those of any table read in place may. Any other item is read as a table of its own, from
/// the bytes that <see cref="Get"/> returns, with a <see cref="TableReader"/> of its own layout.
/// </para>
/// <para>The default value is an empty vector.</para>
/// </remarks>
public readonly struct Vector
{
    /// <summary>The bytes of the buffer that the vector lies in</summary>
    private readonly ReadOnlyMemory<byte> _bytes;

    /// <summary>The vector's position in the bytes, which its ends count from</summary>
    private readonly int _position;

    private readonly int _count;
    private readonly OffsetType _offsetType;

    /// <summary>
    /// A vector of <paramref name="count"/> items at <paramref name="position"/> in
    /// <paramref name="bytes"/>, whose count and ends lie within them
    /// </summary>
    internal Vector(ReadOnlyMemory<byte> bytes, int position, int count, OffsetType offsetType)
    {
        _bytes = bytes;
        _position = position;
        _count = count;
        _offsetType = offsetType;
    }

    /// <summary>The number of items.</summary>
    public int Count => _count;

    /// <summary>
    /// Returns the bytes of item <paramref name="index"/>, from its start to its end, to read it as a
    /// table of its own. Only the ends of the item and the one before it are read.
    /// </summary>
    /// <exception cref="DecodeException">
    /// The item ends before the item before it, or past the bytes.
    /// </exception>
    /// <exception cref="ArgumentOutOfRangeException"><paramref name="index"/> isn't an item's.</exception>
    public ReadOnlyMemory<byte> Get(int index)
    {
        var (start, end) = Bounds(index);
        return Item(start, end, inPlace: false);
    }

    /// <summary>
    /// Returns the bytes of item <paramref name="index"/> to read it in place, from its start to the
    /// end of the buffer's bytes. Only the ends of the item and the one before it are read.
    /// </summary>
    /// <exception cref="DecodeException">
    /// The item ends before the item before it, or past the bytes.
    /// </exception>
    /// <exception cref="ArgumentOutOfRangeException"><paramref name="index"/> isn't an item's.</exception>
    public ReadOnlyMemory<byte> GetInPlace(int index)
    {
        var (start, end) = Bounds(index);
        return Item(start, end, inPlace: true);
    }

    /// <summary>
    /// Returns the items' bytes, in order, as <see cref="Get"/> returns them, reading each item's end
    /// once. The enumerator throws the <see cref="DecodeException"/> of an item that fails from
    /// <see cref="VectorItemEnumerator.MoveNext"/>, and can move on to the next item after it.
    /// </summary>
    public VectorItems Items() => new(this, inPlace: false);

    /// <summary>
    /// Returns the items' bytes, in order, as <see cref="GetInPlace"/> returns them, reading each item's
    /// end once, as <see cref="Items"/> does.
    /// </summary>
    public VectorItems ItemsInPlace() => new(this, inPlace: true);

    /// <summary>Returns where item <paramref name="index"/> starts and ends, from the vector's position.</summary>
    private (long Start, long End) Bounds(int index)
    {
        if ((uint)index >= (uint)_count)
        {
            throw new ArgumentOutOfRangeException(
                nameof(index),
                index,
                $"Not an item of a vector of {_count}"
            );
        }

        long start = index == 0 ? ItemsStart : End(index - 1);
        return (start, End(index));
    }

    /// <summary>Where item 0 starts, from the vector's position: after the count and the ends</summary>
    internal long ItemsStart => ((long)_count + 1) * _offsetType.Size;

    /// <summary>
    /// Reads the end of item <paramref name="index"/>, from the vector's position, which the vector's
    /// reader checked lies within the bytes
    /// </summary>
    internal long End(int index)
    {
        int width = _offsetType.Size;
        var slot = _bytes.Span.Slice(_position + (index + 1) * width, width);

        return TableReader.ReadUnsigned(slot, _offsetType);
    }

    /// <summary>
    /// Returns the bytes of the item from <paramref name="start"/> to <paramref name="end"/>, from the
    /// vector's position, or from its start to the end of the bytes when it's read in place
    /// </summary>
    internal ReadOnlyMemory<byte> Item(long start, long end, bool inPlace)
    {
        if (start > end)
        {
            DecodeException.ThrowItemEndsBeforeStart(_position + start, _position + end);
        }

        if (_position + end > _bytes.Length)
        {
            DecodeException.ThrowOutOfRange(_position + start, end - start, _bytes.Length);
        }

        int itemStart = (int)(_position + start);
        return inPlace ? _bytes[itemStart..] : _bytes[itemStart..(int)(_position + end)];
    }
}

/// <summary>
/// The items of a <see cref="Vector"/>, in order, which <c>foreach</c> reads with a
/// <see cref="VectorItemEnumerator"/>.
/// </summary>
public readonly struct VectorItems
{
    private readonly Vector _vector;
    private readonly bool _inPlace;

    internal VectorItems(Vector vector, bool inPlace)
    {
        _vector = vector;
        _inPlace = inPlace;
    }

    /// <summary>Returns an enumerator of the items, from the first.</summary>
    public VectorItemEnumerator GetEnumerator() => new(_vector, _inPlace);
}

/// <summary>Reads the items of a <see cref="Vector"/> in order, each from its end and the one before it.</summary>
public struct VectorItemEnumerator
{
    private readonly Vector _vector;
    private readonly bool _inPlace;
    private int _index;

    /// <summary>Where the next item starts, from the vector's position: where the item before it ends</summary>
    private long _start;

    internal VectorItemEnumerator(Vector vector, bool inPlace)
    {
        _vector = vector;
        _inPlace = inPlace;
        _index = -1;
        _start = vector.ItemsStart;
        Current = default;
    }

    /// <summary>The bytes of the item that <see cref="MoveNext"/> moved to.</summary>
    public ReadOnlyMemory<byte> Current { get; private set; }

    /// <summary>
    /// Moves to the next item, and returns false after the last. The next item starts where this one
    /// ends, even when this one throws.
    /// </summary>
    /// <exception cref="DecodeException">
    /// The item ends before the item before it, or past the bytes.
    /// </exception>
    public bool MoveNext()
    {
        if (_index + 1 >= _vector.Count)
        {
            return false;
        }

        _index++;
        long start = _start;
        long end = _vector.End(_index);
        _start = end;

        Current = _vector.Item(start, end, _inPlace);
        return true;
    }
}
