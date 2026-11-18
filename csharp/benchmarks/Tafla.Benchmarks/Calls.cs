using System;
using Tafla.Benchmarks.Codecs;

namespace Tafla.Benchmarks;

/// <summary>
/// A benchmark's call: one codec's operation on one workload, which stores its result in
/// <see cref="Sink"/>, so that the JIT can't leave the work out. Calls are structs, generic over their
/// codec's struct, so that <see cref="Loops"/> runs each in a loop compiled for it alone.
/// </summary>
internal interface ICall
{
    /// <summary>The variant's name in the report: its codec's.</summary>
    string Name { get; }

    /// <summary>Does the operation once, and stores its result in <see cref="Sink"/>.</summary>
    void Call();

    /// <summary>
    /// Whether the result that <see cref="Call"/> stored is right: a value with the workload's every
    /// field, by their bits, or bytes from which the codec reads it back.
    /// </summary>
    bool Check();
}

/// <summary>
/// Which copy of a call a loop runs: <see cref="Original"/>, or <see cref="Again"/>, its A/A copy, a
/// type of its own, so that its loop is compiled and profiled apart from the original's. Both loops
/// call the same compiled codec method, so the pair shows the harness's noise, but not the variance
/// of compiling the code a second time.
/// </summary>
internal interface ICopy
{
    static abstract string Suffix { get; }
}

internal readonly struct Original : ICopy
{
    public static string Suffix => "";
}

internal readonly struct Again : ICopy
{
    public static string Suffix => " A/A";
}

/// <summary>
/// Makes each call a variant whose loop is a generic method instantiated with the call's struct and
/// its copy's, so that the JIT compiles a loop of its own for every variant, as it does for every
/// instantiation over structs, with the codec's methods called directly, and dynamic PGO profiles
/// each loop apart from the others'.
/// </summary>
internal static class Loops
{
    public static Variant Of<TCall, TCopy>(TCall call)
        where TCall : struct, ICall
        where TCopy : struct, ICopy => new(call.Name + TCopy.Suffix, n => Run<TCall, TCopy>(call, n), call.Check);

    /// <summary>A variant of the call and its A/A copy, in that order.</summary>
    public static Variant[] WithCopy<TCall>(TCall call)
        where TCall : struct, ICall => [Of<TCall, Original>(call), Of<TCall, Again>(call)];

    private static void Run<TCall, TCopy>(TCall call, long n)
        where TCall : struct, ICall
        where TCopy : struct, ICopy
    {
        for (long index = 0; index < n; index++)
        {
            call.Call();
        }
    }
}

/// <summary>The results of the timed calls, which the loops store so that the JIT can't leave the calls out.</summary>
internal static class Sink
{
    public static byte[]? Bytes;
    public static int Length;
    public static Record? Record;
    public static Document? Document;
    public static ItemList? List;
    public static Sample Sample;
    public static double Number;

    /// <summary>Forgets every result, so that a check sees only what the next call stores.</summary>
    public static void Clear()
    {
        Bytes = null;
        Length = -1;
        Record = null;
        Document = null;
        List = null;
        Sample = default;
        Number = double.NaN;
    }

    /// <summary>Keeps the last results alive to the end of the run.</summary>
    public static void KeepAlive()
    {
        GC.KeepAlive(Bytes);
        GC.KeepAlive(Length);
        GC.KeepAlive(Record);
        GC.KeepAlive(Document);
        GC.KeepAlive(List);
        GC.KeepAlive(Sample);
        GC.KeepAlive(Number);
    }
}

internal readonly struct SerializeRecord<TCodec>(Record record) : ICall
    where TCodec : struct, IRecordCodec
{
    public string Name => default(TCodec).Name;

    public void Call() => Sink.Bytes = default(TCodec).SerializeRecord(record);

    public bool Check() => Sink.Bytes is { } bytes && default(TCodec).DeserializeRecord(bytes).Same(record);
}

internal readonly struct DeserializeRecord<TCodec>(byte[] bytes, Record record) : ICall
    where TCodec : struct, IRecordCodec
{
    public string Name => default(TCodec).Name;

    public void Call() => Sink.Record = default(TCodec).DeserializeRecord(bytes);

    public bool Check() => Sink.Record?.Same(record) == true;
}

internal readonly struct SerializeDocument<TCodec>(Document document) : ICall
    where TCodec : struct, IDocumentCodec
{
    public string Name => default(TCodec).Name;

    public void Call() => Sink.Bytes = default(TCodec).SerializeDocument(document);

    public bool Check() => Sink.Bytes is { } bytes && default(TCodec).DeserializeDocument(bytes).Same(document);
}

/// <summary>
/// Writes the document into the start of bytes that the call keeps, which start as <c>0xff</c>, so
/// that its check sees every byte that it writes: the bytes that it writes into new ones.
/// </summary>
internal readonly struct SerializeDocumentInto<TCodec>(Document document, byte[] reused, byte[] expected) : ICall
    where TCodec : struct, IDocumentIntoCodec
{
    public string Name => default(TCodec).Name;

    public void Call()
    {
        Sink.Length = default(TCodec).SerializeDocumentInto(document, reused);
        Sink.Bytes = reused;
    }

    public bool Check() =>
        Sink.Bytes == reused && Sink.Length == expected.Length && reused.AsSpan(0, Sink.Length).SequenceEqual(expected);
}

internal readonly struct DeserializeDocument<TCodec>(byte[] bytes, Document document) : ICall
    where TCodec : struct, IDocumentCodec
{
    public string Name => default(TCodec).Name;

    public void Call() => Sink.Document = default(TCodec).DeserializeDocument(bytes);

    public bool Check() => Sink.Document?.Same(document) == true;
}

internal readonly struct SumA<TCodec>(byte[] bytes, double sum) : ICall
    where TCodec : struct, IDocumentCodec
{
    public string Name => default(TCodec).Name;

    public void Call() => Sink.Number = default(TCodec).SumA(bytes);

    public bool Check() => Bits.Same(Sink.Number, sum);
}

internal readonly struct SerializeList<TCodec>(ItemList list) : ICall
    where TCodec : struct, IListCodec
{
    public string Name => default(TCodec).Name;

    public void Call() => Sink.Bytes = default(TCodec).SerializeList(list);

    public bool Check() => Sink.Bytes is { } bytes && default(TCodec).DeserializeList(bytes).Same(list);
}

internal readonly struct DeserializeList<TCodec>(byte[] bytes, ItemList list) : ICall
    where TCodec : struct, IListCodec
{
    public string Name => default(TCodec).Name;

    public void Call() => Sink.List = default(TCodec).DeserializeList(bytes);

    public bool Check() => Sink.List?.Same(list) == true;
}

internal readonly struct SerializeStructs<TCodec>(ItemList list) : ICall
    where TCodec : struct, IStructsCodec
{
    public string Name => default(TCodec).Name;

    public void Call() => Sink.Bytes = default(TCodec).SerializeStructs(list);

    public bool Check() => Sink.Bytes is { } bytes && default(TCodec).DeserializeStructs(bytes).Same(list);
}

internal readonly struct DeserializeStructs<TCodec>(byte[] bytes, ItemList list) : ICall
    where TCodec : struct, IStructsCodec
{
    public string Name => default(TCodec).Name;

    public void Call() => Sink.List = default(TCodec).DeserializeStructs(bytes);

    public bool Check() => Sink.List?.Same(list) == true;
}

internal readonly struct SerializeSample<TCodec>(Sample sample) : ICall
    where TCodec : struct, ISampleCodec
{
    public string Name => default(TCodec).Name;

    public void Call() => Sink.Bytes = default(TCodec).SerializeSample(sample);

    public bool Check() => Sink.Bytes is { } bytes && default(TCodec).DeserializeSample(bytes).Same(sample);
}

internal readonly struct DeserializeSample<TCodec>(byte[] bytes, Sample sample) : ICall
    where TCodec : struct, ISampleCodec
{
    public string Name => default(TCodec).Name;

    public void Call() => Sink.Sample = default(TCodec).DeserializeSample(bytes);

    public bool Check() => Sink.Sample.Same(sample);
}

/// <summary>One of tafla's reads of <see cref="TaflaReads"/>, which sums what it reads.</summary>
internal readonly struct Read<TRead>(byte[] bytes, double sum) : ICall
    where TRead : struct, ITaflaRead
{
    public string Name => "tafla";

    public void Call() => Sink.Number = TRead.Read(bytes);

    public bool Check() => Bits.Same(Sink.Number, sum);
}
