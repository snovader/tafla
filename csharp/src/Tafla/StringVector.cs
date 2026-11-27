using System;

namespace Tafla;

/// <summary>
/// A vector of strings read from a table. <see cref="TableReader.GetStringVector"/> reads it, after
/// checking that its count and the ends of its strings lie within the bytes, and each string is
/// checked when it's read, so reading one can throw a <see cref="DecodeException"/>. The code that
/// tafla-gen generates returns one from the getter of a <c>[string]</c> field.
/// </summary>
/// <remarks>
/// <para>
/// The vector holds its count, the end of each string, counted from the vector's position, and then
/// the strings' UTF-8, one after another, as a <see cref="Vector"/> holds its items. The indexer finds
/// any string from its end and the one before it, without reading the others, and
/// <see cref="ToArray"/> and the enumerator read each string's end once, in order.
/// </para>
/// <para>
/// Each string is decoded into a new <see cref="string"/>, and checked as UTF-8 on its own, as
/// <see cref="TableReader.GetString"/> checks a string: an end that splits a character leaves both
/// strings around it invalid. Reading every string reads what reading each one does, and throws
/// where reading any one throws.
/// </para>
/// <para>
/// The view and its enumerator are <c>ref struct</c>s over the span of bytes that the
/// <see cref="TableReader"/> read it from, as the reader is, so neither can be stored where it could
/// outlive the span. The strings they return are the caller's to keep. The default value is an empty
/// vector.
/// </para>
/// </remarks>
public readonly ref struct StringVector
{
    private readonly Vector _vector;

    /// <summary>The vector of strings that <paramref name="vector"/> reads as a vector of items</summary>
    internal StringVector(Vector vector)
    {
        _vector = vector;
    }

    /// <summary>The number of strings.</summary>
    public int Count => _vector.Count;

    /// <summary>
    /// Reads string <paramref name="index"/> into a new <see cref="string"/>. Only its end and the end
    /// of the string before it are read.
    /// </summary>
    /// <exception cref="DecodeException">
    /// The string ends before the string before it, or past the bytes, or isn't valid UTF-8.
    /// </exception>
    /// <exception cref="ArgumentOutOfRangeException"><paramref name="index"/> isn't a string's.</exception>
    public string this[int index]
    {
        get
        {
            var (start, end) = _vector.Bounds(index);
            return _vector.String(start, end);
        }
    }

    /// <summary>
    /// Reads every string, in order, into a new array, reading each string's end once. It throws the
    /// error of the first string that fails, as reading that string with the indexer does.
    /// </summary>
    /// <exception cref="DecodeException">
    /// A string ends before the string before it, or past the bytes, or isn't valid UTF-8.
    /// </exception>
    public string[] ToArray()
    {
        // The vector's reader checked that every end lies within the bytes, so the count is at most
        // the bytes' length over the offset size
        var strings = new string[_vector.Count];

        long start = _vector.ItemsStart;
        for (var index = 0; index < strings.Length; index++)
        {
            long end = _vector.End(index);
            strings[index] = _vector.String(start, end);
            start = end;
        }

        return strings;
    }

    /// <summary>
    /// Returns an enumerator of the strings, in order, which reads each string's end once. It throws
    /// the <see cref="DecodeException"/> of a string that fails from
    /// <see cref="Enumerator.MoveNext"/>, and can move on to the next string after it.
    /// </summary>
    public Enumerator GetEnumerator() => new(_vector);

    /// <summary>Reads the strings of a <see cref="StringVector"/> in order, each from its end and the one before it.</summary>
    public ref struct Enumerator
    {
        private readonly Vector _vector;
        private int _index;

        /// <summary>Where the next string starts, from the vector's position: where the string before it ends</summary>
        private long _start;

        internal Enumerator(Vector vector)
        {
            _vector = vector;
            _index = -1;
            _start = vector.ItemsStart;
            Current = "";
        }

        /// <summary>The string that <see cref="MoveNext"/> moved to.</summary>
        public string Current { get; private set; }

        /// <summary>
        /// Moves to the next string, and returns false after the last. The next string starts where
        /// this one ends, even when this one throws.
        /// </summary>
        /// <exception cref="DecodeException">
        /// The string ends before the string before it, or past the bytes, or isn't valid UTF-8.
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

            Current = _vector.String(start, end);
            return true;
        }
    }
}
