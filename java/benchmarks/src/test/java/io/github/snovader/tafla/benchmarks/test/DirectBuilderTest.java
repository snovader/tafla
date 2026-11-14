package io.github.snovader.tafla.benchmarks.test;

import io.github.snovader.tafla.benchmarks.*;
import io.github.snovader.tafla.benchmarks.codecs.*;
import net.jqwik.api.*;

import static org.junit.jupiter.api.Assertions.*;

/**
 * Checks that the prototype {@code DirectTableBuilder} writes every workload byte for byte as
 * {@code TableBuilder} writes it, with segments and copying every value
 */
public class DirectBuilderTest {
    private static final Codec TAFLA = TaflaCodec.create();

    @Provide
    Arbitrary<Codec> directCodecs() {
        return Arbitraries.of(TaflaDirectCodec.create(), TaflaDirectCodec.copying());
    }

    @Provide
    Arbitrary<Integer> documentRows() {
        return Arbitraries.of(Workloads.DOCUMENT_ROWS);
    }

    @Property(generation = GenerationMode.EXHAUSTIVE)
    void should_write_the_record_as_table_builder_does(@ForAll("directCodecs") Codec codec) {
        assertArrayEquals(TAFLA.record().serialize(Workloads.RECORD), codec.record().serialize(Workloads.RECORD));
    }

    @Property(generation = GenerationMode.EXHAUSTIVE)
    void should_write_documents_as_table_builder_does(@ForAll("directCodecs") Codec codec, @ForAll("documentRows") int rows) {
        var input = Workloads.documentInput(rows);
        assertArrayEquals(TAFLA.document().serialize(input), codec.document().serialize(input));
    }

    @Property(generation = GenerationMode.EXHAUSTIVE)
    void should_write_the_vector_and_the_array_of_structs_as_table_builder_does(@ForAll("directCodecs") Codec codec) {
        var input = Workloads.listInput(Workloads.LIST_ITEMS);
        assertArrayEquals(TAFLA.list().serialize(input), codec.list().serialize(input));
        assertArrayEquals(TAFLA.structs().serialize(input), codec.structs().serialize(input));
    }
}
