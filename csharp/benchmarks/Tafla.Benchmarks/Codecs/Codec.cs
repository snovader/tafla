namespace Tafla.Benchmarks.Codecs;

// The workloads' shapes that a codec encodes, each as its library's documentation shows, and as the
// other languages' comparisons use their counterparts. Each codec writes a workload's value into new
// bytes, and reads it back with every field. Codecs are structs, so that the JIT compiles a timing
// loop of its own for each, a generic method instantiated with the codec's type, which calls the
// codec's methods directly.
//
// Each operation of a codec is marked NoInlining, so that the JIT compiles it as a method of its own,
// as an application's call of it is, rather than into the small loop that times it. Inlined into the
// loop, or into a small method that the loop calls, the same code took longer: TableWriter wrote the
// array of structs in 284 or 488 ns rather than 122 ns, and the TableReader before PR 263 read the
// vector in 1.40 µs rather than 1.04 µs, likely because the JIT's budget for inlining grows with the size of the method
// that it compiles, so that less of what the code calls was inlined.

/// <summary>A library that encodes some of the workloads.</summary>
internal interface ICodec
{
    /// <summary>The library's name, which names its variants in the report.</summary>
    string Name { get; }
}

/// <summary>Encodes the record.</summary>
internal interface IRecordCodec : ICodec
{
    byte[] SerializeRecord(Record record);

    Record DeserializeRecord(byte[] bytes);
}

/// <summary>Encodes the documents, and reads a column one element at a time.</summary>
internal interface IDocumentCodec : ICodec
{
    byte[] SerializeDocument(Document document);

    Document DeserializeDocument(byte[] bytes);

    /// <summary>
    /// Sums column a through the library's own per-element accessor, in the order of
    /// <see cref="Workloads.SumColumn"/>.
    /// </summary>
    double SumA(byte[] bytes);
}

/// <summary>Writes the documents into bytes that the caller keeps between calls.</summary>
internal interface IDocumentIntoCodec : ICodec
{
    /// <summary>
    /// Writes <paramref name="document"/> from the start of <paramref name="bytes"/>, which have room
    /// for it, and returns its length.
    /// </summary>
    int SerializeDocumentInto(Document document, byte[] bytes);
}

/// <summary>Encodes the vector of nested tables.</summary>
internal interface IListCodec : ICodec
{
    byte[] SerializeList(ItemList list);

    ItemList DeserializeList(byte[] bytes);
}

/// <summary>Encodes the same items as an array of structs.</summary>
internal interface IStructsCodec : ICodec
{
    byte[] SerializeStructs(ItemList list);

    ItemList DeserializeStructs(byte[] bytes);
}

/// <summary>Encodes the fixed-size record.</summary>
internal interface ISampleCodec : ICodec
{
    byte[] SerializeSample(Sample sample);

    Sample DeserializeSample(byte[] bytes);
}
