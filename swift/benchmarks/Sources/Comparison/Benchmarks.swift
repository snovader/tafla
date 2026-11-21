import Codecs
import Harness
import Workloads

/// A row of the report: a workload's operation, and each library's variant of
/// it
struct Benchmark {
    let name: String
    var variants: [Variant]
}

/// Every benchmark, in the order that the report lists them, each with the
/// variants of the libraries in the order that it lists them. Adding a codec
/// checks that it reads back exactly what it wrote, before any is timed.
///
/// Each codec's variants are made by generic functions called with the
/// codec's own type, which the compiler specializes for it, so that every
/// variant's loop is code of its own that calls the library directly.
struct Benchmarks {
    private(set) var all: [Benchmark] = []

    /// Adds the variants of a library that encodes every workload
    mutating func addSuite<C: SuiteCodec>(_ codec: C) throws {
        try checkReadBack(codec)

        try addRecord(codec)
        for rows in documentRows {
            try addDocument(codec, Document(rows: rows))
        }
        try addList(codec)
        try addStructs(codec)
        try addSample(codec)
    }

    mutating func addRecord<C: RecordCodec>(_ codec: C) throws {
        let record = Record.workload
        let bytes = try codec.serializeRecord(record)

        add("record: serialize", codec.name) { calls in
            for _ in 0..<calls {
                keep(try codec.serializeRecord(record))
            }
        }
        add("record: deserialize", codec.name) { calls in
            for _ in 0..<calls {
                keep(try codec.deserializeRecord(bytes))
            }
        }
    }

    mutating func addDocument<C: DocumentCodec>(_ codec: C, _ document: Document) throws {
        let rows = "document of \(thousands(document.a.count)) rows"
        let bytes = try codec.serializeDocument(document)

        add("\(rows): serialize", codec.name) { calls in
            for _ in 0..<calls {
                keep(try codec.serializeDocument(document))
            }
        }
        add("\(rows): deserialize", codec.name) { calls in
            for _ in 0..<calls {
                keep(try codec.deserializeDocument(bytes))
            }
        }
        add("\(rows): sum a column per element", codec.name) { calls in
            for _ in 0..<calls {
                keep(try codec.sumA(bytes))
            }
        }
    }

    mutating func addList<C: ListCodec>(_ codec: C) throws {
        let list = List(items: listItems)
        let bytes = try codec.serializeList(list)

        add("vector of \(listItems) nested tables: serialize", codec.name) { calls in
            for _ in 0..<calls {
                keep(try codec.serializeList(list))
            }
        }
        add("vector of \(listItems) nested tables: deserialize", codec.name) { calls in
            for _ in 0..<calls {
                keep(try codec.deserializeList(bytes))
            }
        }
    }

    mutating func addStructs<C: StructsCodec>(_ codec: C) throws {
        let list = List(items: listItems)
        let bytes = try codec.serializeStructs(list)

        add("array of \(listItems) structs: serialize", codec.name) { calls in
            for _ in 0..<calls {
                keep(try codec.serializeStructs(list))
            }
        }
        add("array of \(listItems) structs: deserialize", codec.name) { calls in
            for _ in 0..<calls {
                keep(try codec.deserializeStructs(bytes))
            }
        }
    }

    /// Adds the variants of a library that encodes the fixed-size record,
    /// alone or with the others
    mutating func addSample<C: SampleCodec>(_ codec: C) throws {
        try checkSample(codec)

        let sample = Sample.workload
        let bytes = try codec.serializeSample(sample)

        add("fixed-size record: serialize", codec.name) { calls in
            for _ in 0..<calls {
                keep(try codec.serializeSample(sample))
            }
        }
        add("fixed-size record: deserialize", codec.name) { calls in
            for _ in 0..<calls {
                keep(try codec.deserializeSample(bytes))
            }
        }
    }

    /// Adds the groups that read tafla's items, with `access` named `name`
    mutating func addItemAccess<A: ItemAccess>(_ access: A, name: String) throws {
        try checkItemAccess(access, name: name)

        let list = List(items: listItems)
        let tables = try TaflaCodec().serializeList(list)
        let structs = try TaflaCodec().serializeStructs(list)

        add("vector of \(listItems) nested tables: read the last item", name) { calls in
            for _ in 0..<calls {
                keep(try access.lastTable(tables))
            }
        }
        add("vector of \(listItems) nested tables: sum every item", name) { calls in
            for _ in 0..<calls {
                keep(try access.everyTable(tables))
            }
        }
        add("array of \(listItems) structs: read the last item", name) { calls in
            for _ in 0..<calls {
                keep(try access.lastStruct(structs))
            }
        }
        add("array of \(listItems) structs: sum every item", name) { calls in
            for _ in 0..<calls {
                keep(try access.everyStruct(structs))
            }
        }
    }

    /// Adds a variant to the benchmark of that name, which the first variant
    /// of the name appends
    private mutating func add(_ benchmark: String, _ variant: String, _ loop: @escaping (Int) throws -> Void) {
        let variant = Variant(name: variant, loop: loop)
        if let index = all.firstIndex(where: { $0.name == benchmark }) {
            all[index].variants.append(variant)
        } else {
            all.append(Benchmark(name: benchmark, variants: [variant]))
        }
    }
}
