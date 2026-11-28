using System.Runtime.CompilerServices;
using Gen = global::Workloads;

namespace Tafla.Benchmarks;

// The reads of TaflaReads through the code that tafla-gen generates: the record's scalars with its
// getters, and the items of the vector of nested tables and of the array of structs through the views
// that the generated getters return, whose indexer reads one item and whose enumerator reads each in
// turn. A column's generated getter returns its span, which TaflaReads.ColumnByGetter's getters read
// past, so it has no counterpart here.
internal static class TaflaGenReads
{
    /// <summary>Sums the record's two float64s and its int64, without its string.</summary>
    internal readonly struct RecordScalars : ITaflaRead
    {
        public static string Name => "tafla-gen";

        [MethodImpl(MethodImplOptions.NoInlining)]
        public static double Read(byte[] bytes)
        {
            var record = new Gen.Record(bytes);

            return record.X + record.Y + record.Id;
        }
    }

    /// <summary>Sums the last item of the vector, from the view's indexer.</summary>
    internal readonly struct LastTable : ITaflaRead
    {
        public static string Name => "tafla-gen";

        [MethodImpl(MethodImplOptions.NoInlining)]
        public static double Read(byte[] bytes)
        {
            var view = new Gen.ItemList(bytes).Items;
            var item = view[view.Count - 1];
            var pair = item.Pair;

            return Workloads.SumItem(item.Value, pair.First, pair.Second);
        }
    }

    /// <summary>Sums every item of the vector, in order, through the view's enumerator.</summary>
    internal readonly struct EveryTable : ITaflaRead
    {
        public static string Name => "tafla-gen";

        [MethodImpl(MethodImplOptions.NoInlining)]
        public static double Read(byte[] bytes)
        {
            var sum = 0.0;
            foreach (var item in new Gen.ItemList(bytes).Items)
            {
                var pair = item.Pair;
                sum += Workloads.SumItem(item.Value, pair.First, pair.Second);
            }

            return sum;
        }
    }

    /// <summary>Sums the last struct of the array, from the view's indexer.</summary>
    internal readonly struct LastStruct : ITaflaRead
    {
        public static string Name => "tafla-gen";

        [MethodImpl(MethodImplOptions.NoInlining)]
        public static double Read(byte[] bytes)
        {
            var view = new Gen.Triples(bytes).Items;
            var triple = view[view.Count - 1];

            return Workloads.SumItem(triple.Value, triple.First, triple.Second);
        }
    }

    /// <summary>Sums every struct of the array, in order, through the view's enumerator.</summary>
    internal readonly struct EveryStruct : ITaflaRead
    {
        public static string Name => "tafla-gen";

        [MethodImpl(MethodImplOptions.NoInlining)]
        public static double Read(byte[] bytes)
        {
            var sum = 0.0;
            foreach (var triple in new Gen.Triples(bytes).Items)
            {
                sum += Workloads.SumItem(triple.Value, triple.First, triple.Second);
            }

            return sum;
        }
    }
}
