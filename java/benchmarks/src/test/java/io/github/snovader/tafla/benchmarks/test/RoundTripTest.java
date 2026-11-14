package io.github.snovader.tafla.benchmarks.test;

import io.github.snovader.tafla.benchmarks.*;
import io.github.snovader.tafla.benchmarks.codecs.*;
import net.jqwik.api.*;

import java.util.*;

import static org.junit.jupiter.api.Assertions.*;

/**
 * Checks that every codec reads back exactly what it wrote for every workload, so that the
 * benchmarks can't time a codec that skips or corrupts part of the work.
 */
public class RoundTripTest {
    @Provide
    Arbitrary<Codec> codecs() {
        return Arbitraries.of(Codecs.all());
    }

    @Provide
    Arbitrary<String> sampleCodecs() {
        return Arbitraries.of(List.copyOf(Codecs.samples().keySet()));
    }

    @Provide
    Arbitrary<String> structsCodecs() {
        return Arbitraries.of(List.copyOf(Codecs.structs().keySet()));
    }

    @Provide
    Arbitrary<String> reusedBytesCodecs() {
        return Arbitraries.of(List.copyOf(Codecs.reusedBytesDocuments().keySet()));
    }

    @Provide
    Arbitrary<Integer> documentRows() {
        return Arbitraries.of(Workloads.DOCUMENT_ROWS);
    }

    @Property(generation = GenerationMode.EXHAUSTIVE)
    void should_read_back_the_record(@ForAll("codecs") Codec codec) {
        assertReadBack(codec.record(), Workloads.RECORD);
    }

    @Property(generation = GenerationMode.EXHAUSTIVE)
    void should_read_back_documents(@ForAll("codecs") Codec codec, @ForAll("documentRows") int rows) {
        assertReadBack(codec.document(), Workloads.documentInput(rows));
    }

    @Property(generation = GenerationMode.EXHAUSTIVE)
    void should_write_documents_into_reused_bytes_as_into_new_ones(
        @ForAll("reusedBytesCodecs") String codec,
        @ForAll("documentRows") int rows
    ) {
        var workload = Objects.requireNonNull(Codecs.reusedBytesDocuments().get(codec));
        var input = Workloads.documentInput(rows);

        assertArrayEquals(workload.serialize(input), Codecs.serializeIntoChecked(workload, input));
    }

    @Property(generation = GenerationMode.EXHAUSTIVE)
    void should_sum_a_document_column_in_order(@ForAll("codecs") Codec codec, @ForAll("documentRows") int rows) {
        var input = Workloads.documentInput(rows);

        double expected = 0;
        for (var value : input.a()) {
            expected += value;
        }

        var bytes = codec.document().serialize(input);
        assertEquals(expected, codec.document().sumA(bytes));
    }

    @Property(generation = GenerationMode.EXHAUSTIVE)
    void should_read_back_the_vector_of_nested_tables(@ForAll("codecs") Codec codec) {
        assertReadBack(codec.list(), Workloads.listInput(Workloads.LIST_ITEMS));
    }

    @Property(generation = GenerationMode.EXHAUSTIVE)
    void should_read_back_the_array_of_structs(@ForAll("structsCodecs") String codec) {
        assertReadBack(Codecs.structs(codec), Workloads.listInput(Workloads.LIST_ITEMS));
    }

    @Property(generation = GenerationMode.EXHAUSTIVE)
    void should_read_back_the_fixed_size_record(@ForAll("sampleCodecs") String codec) {
        assertReadBack(Codecs.sample(codec), Workloads.SAMPLE);
    }

    @Provide
    Arbitrary<String> accesses() {
        return Arbitraries.of(List.copyOf(ItemAccesses.all().keySet()));
    }

    @Property(generation = GenerationMode.EXHAUSTIVE)
    void should_sum_the_last_item_and_every_item_in_order(@ForAll("accesses") String name) {
        var access = ItemAccesses.named(name);
        var input = Workloads.listInput(Workloads.LIST_ITEMS);
        var tafla = TaflaCodec.create();
        var tables = tafla.list().serialize(input);
        var structs = tafla.structs().serialize(input);

        double every = 0;
        for (var item : input.items()) {
            every += ItemAccesses.sum(item);
        }
        var last = ItemAccesses.sum(input.items().getLast());

        assertEquals(last, access.lastTable(tables));
        assertEquals(every, access.everyTable(tables));
        assertEquals(last, access.lastStruct(structs));
        assertEquals(every, access.everyStruct(structs));
    }

    @Example
    void should_write_the_builders_tables_from_generated_code() {
        var built = TaflaCodec.create();
        var generated = TaflaGenCodec.create();
        var list = Workloads.listInput(Workloads.LIST_ITEMS);

        assertArrayEquals(built.record().serialize(Workloads.RECORD), generated.record().serialize(Workloads.RECORD));
        for (var rows : Workloads.DOCUMENT_ROWS) {
            var document = Workloads.documentInput(rows);
            assertArrayEquals(built.document().serialize(document), generated.document().serialize(document));
        }
        assertArrayEquals(built.list().serialize(list), generated.list().serialize(list));
        assertArrayEquals(built.structs().serialize(list), generated.structs().serialize(list));
        assertArrayEquals(built.sample().serialize(Workloads.SAMPLE), generated.sample().serialize(Workloads.SAMPLE));
    }

    @Example
    void should_fail_the_check_of_a_codec_that_loses_signed_zero() {
        var input = new SampleInput(1, -0.0f, 1);
        var lossy = new Workload<SampleInput>() {
            @Override
            public byte[] serialize(SampleInput sample) {
                return new byte[0];
            }

            @Override
            public SampleInput deserialize(byte[] bytes) {
                return new SampleInput(1, 0.0f, 1);
            }
        };

        assertThrows(IllegalStateException.class, () -> Codecs.serializeChecked(lossy, input));
    }

    private static <T> void assertReadBack(Workload<T> workload, T input) {
        var bytes = workload.serialize(input);

        assertEquals(input, workload.deserialize(bytes));
        // The check that each benchmark runs before timing
        assertArrayEquals(bytes, Codecs.serializeChecked(workload, input));
    }
}
