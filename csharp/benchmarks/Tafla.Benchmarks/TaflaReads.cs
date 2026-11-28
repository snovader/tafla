using System.Runtime.CompilerServices;
using Tafla.Benchmarks.Codecs;

namespace Tafla.Benchmarks;

/// <summary>
/// A read of tafla's bytes that sums what it reads, as doubles, rather than collecting values of its
/// own, so that it times the reads alone.
/// </summary>
internal interface ITaflaRead
{
    /// <summary>The variant's name in the report: the code that reads.</summary>
    static abstract string Name { get; }

    static abstract double Read(byte[] bytes);
}

// Reads of only some of a table's fields, which time TableReader's getters more closely than the
// comparison's reads of whole workloads: the record's scalars, a column with a getter at each
// element's position, and the items of the vector of nested tables and of the array of structs, the
// last alone, which a reader finds without reading the others, and every one, as Go's item reads do.
// They read the bytes of TaflaCodec, which TaflaGenCodec writes too.
internal static class TaflaReads
{
    /// <summary>Sums the record's two float64s and its int64, without its string.</summary>
    internal readonly struct RecordScalars : ITaflaRead
    {
        public static string Name => "tafla";

        [MethodImpl(MethodImplOptions.NoInlining)]
        public static double Read(byte[] bytes)
        {
            var reader = new TableReader(bytes);

            return reader.GetFloat64(reader.RootOffset(0))
                + reader.GetFloat64(reader.RootOffset(1))
                + reader.GetInt64(reader.RootOffset(2));
        }

        public static double Expected(Record record) => record.X + record.Y + record.Id;
    }

    /// <summary>
    /// Sums column a with <see cref="TableReader.GetFloat32"/> at each element's position, as Java's
    /// comparison sums it, which times a getter's checks once a row.
    /// </summary>
    internal readonly struct ColumnByGetter : ITaflaRead
    {
        public static string Name => "tafla";

        [MethodImpl(MethodImplOptions.NoInlining)]
        public static double Read(byte[] bytes)
        {
            var reader = TaflaCodec.OpenDocument(bytes);
            var a = reader.FieldOffset(reader.RootOffset(3), 2);

            // The elements follow their count, which is of the layout's offset type, uint32
            var length = (int)reader.GetUInt32(a);
            var elements = a + sizeof(uint);

            var sum = 0.0;
            for (var index = 0; index < length; index++)
            {
                sum += reader.GetFloat32(elements + index * sizeof(float));
            }

            return sum;
        }
    }

    /// <summary>Sums the last item of the vector of nested tables, read in place.</summary>
    internal readonly struct LastTable : ITaflaRead
    {
        public static string Name => "tafla";

        [MethodImpl(MethodImplOptions.NoInlining)]
        public static double Read(byte[] bytes)
        {
            var vector = VectorOf(bytes);
            var item = TaflaCodec.ReadItem(vector.GetInPlace(vector.Count - 1));

            return Workloads.SumItem(item.Value, item.First, item.Second);
        }

        public static double Expected(ItemList list)
        {
            var item = list.Items[^1];
            return Workloads.SumItem(item.Value, item.First, item.Second);
        }
    }

    /// <summary>Sums every item of the vector of nested tables, each read in place, in order.</summary>
    internal readonly struct EveryTable : ITaflaRead
    {
        public static string Name => "tafla";

        [MethodImpl(MethodImplOptions.NoInlining)]
        public static double Read(byte[] bytes)
        {
            var sum = 0.0;
            foreach (var inPlace in VectorOf(bytes).ItemsInPlace())
            {
                var item = TaflaCodec.ReadItem(inPlace);
                sum += Workloads.SumItem(item.Value, item.First, item.Second);
            }

            return sum;
        }

        public static double Expected(ItemList list)
        {
            var sum = 0.0;
            foreach (var item in list.Items)
            {
                sum += Workloads.SumItem(item.Value, item.First, item.Second);
            }

            return sum;
        }
    }

    /// <summary>Sums the last struct of the array, from its bytes in place.</summary>
    internal readonly struct LastStruct : ITaflaRead
    {
        public static string Name => "tafla";

        [MethodImpl(MethodImplOptions.NoInlining)]
        public static double Read(byte[] bytes)
        {
            var structs = TaflaCodec.GetStructs(bytes);
            var item = TaflaCodec.ReadTriple(structs[^TaflaCodec.TripleSize..]);

            return Workloads.SumItem(item.Value, item.First, item.Second);
        }
    }

    /// <summary>Sums every struct of the array, from their bytes in place, in order.</summary>
    internal readonly struct EveryStruct : ITaflaRead
    {
        public static string Name => "tafla";

        [MethodImpl(MethodImplOptions.NoInlining)]
        public static double Read(byte[] bytes)
        {
            var structs = TaflaCodec.GetStructs(bytes);

            var sum = 0.0;
            for (var start = 0; start + TaflaCodec.TripleSize <= structs.Length; start += TaflaCodec.TripleSize)
            {
                var item = TaflaCodec.ReadTriple(structs.Slice(start, TaflaCodec.TripleSize));
                sum += Workloads.SumItem(item.Value, item.First, item.Second);
            }

            return sum;
        }
    }

    private static Vector VectorOf(byte[] bytes)
    {
        var reader = new TableReader(bytes);

        return reader.GetVector(reader.RootOffset(0));
    }
}
