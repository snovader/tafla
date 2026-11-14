package io.github.snovader.tafla.benchmarks.codecs;

import io.github.snovader.tafla.benchmarks.*;
import io.github.snovader.tafla.benchmarks.generated.tafla.Columns;
import io.github.snovader.tafla.benchmarks.generated.tafla.Document;
import io.github.snovader.tafla.benchmarks.generated.tafla.Item;
import io.github.snovader.tafla.benchmarks.generated.tafla.ItemList;
import io.github.snovader.tafla.benchmarks.generated.tafla.Pair;
import io.github.snovader.tafla.benchmarks.generated.tafla.Record;
import io.github.snovader.tafla.benchmarks.generated.tafla.Sample;
import io.github.snovader.tafla.benchmarks.generated.tafla.Triple;
import io.github.snovader.tafla.benchmarks.generated.tafla.Triples;

import java.util.ArrayList;
import java.util.Arrays;

/**
 * The code that tafla-gen generates from {@code src/main/tafla/workloads.tafla}, which writes each
 * message's table in one call with a {@code TableSizer} and a {@code TableWriter}, rather than
 * nesting {@code TableBuilder}s, and reads it with the generated getters.
 *
 * <p>The generated code writes from records of each table's fields and of the structs, which
 * {@code serialize()} creates from the workloads' inputs in each call, as the protobuf codec
 * creates its messages, except for the array of structs, whose {@code serialize(count, fill)}
 * writes each item's values straight into the table. Reading creates the workloads' records from
 * the generated readers' values. The getters of the vector and of the array of structs return
 * views, which read each item, a reader or a record, as it's asked for. Its getters of arrays of
 * scalars return copies, so it sums a column through the column's view, {@code aView()}, which
 * reads the elements in place, as the other languages' generated getters return them.</p>
 */
public final class TaflaGenCodec {
    private TaflaGenCodec() {}

    public static Codec create() {
        return new Codec("tafla-gen", new Records(), new Documents(), new Lists(), new Structs(), new Samples());
    }

    private static final class Records implements Workload<RecordInput> {
        @Override
        public byte[] serialize(RecordInput input) {
            return Record.serialize(new Record.Fields(input.x(), input.y(), input.id(), input.name()));
        }

        @Override
        public RecordInput deserialize(byte[] bytes) {
            var record = new Record(bytes);

            return new RecordInput(record.x(), record.y(), record.id(), record.name());
        }
    }

    private static final class Documents implements DocumentWorkload, ReusedBytesWorkload<DocumentInput> {
        @Override
        public byte[] serialize(DocumentInput input) {
            return Document.serialize(fieldsOf(input));
        }

        @Override
        public int serializeInto(DocumentInput input, byte[] bytes) {
            return Document.serializeInto(fieldsOf(input), bytes);
        }

        private static Document.Fields fieldsOf(DocumentInput input) {
            var columns = new Columns.Fields(input.base(), input.keys(), input.a(), input.b());

            return new Document.Fields(input.x(), input.y(), input.name(), columns);
        }

        @Override
        public DocumentInput deserialize(byte[] bytes) {
            var document = new Document(bytes);
            var columns = document.columns();

            return new DocumentInput(
                document.x(),
                document.y(),
                document.name(),
                columns.base(),
                columns.keys(),
                columns.a(),
                columns.b()
            );
        }

        @Override
        public double sumA(byte[] bytes) {
            var a = new Document(bytes).columns().aView();

            double sum = 0;
            for (int i = 0, length = a.remaining(); i < length; i++) {
                sum += a.get(i);
            }

            return sum;
        }
    }

    private static final class Lists implements Workload<ListInput> {
        @Override
        public byte[] serialize(ListInput input) {
            var items = input.items();
            var fields = new ArrayList<Item.Fields>(items.size());
            for (var item : items) {
                fields.add(new Item.Fields(item.value(), new Pair.Fields(item.first(), item.second())));
            }

            return ItemList.serialize(new ItemList.Fields(fields));
        }

        @Override
        public ListInput deserialize(byte[] bytes) {
            var tables = new ItemList(bytes).items();

            // Iterated, which reads each item's end once, where get(i) reads two
            var items = new ItemInput[tables.size()];
            var i = 0;
            for (var item : tables) {
                var pair = item.pair();
                items[i++] = new ItemInput(item.value(), pair.first(), pair.second());
            }

            return new ListInput(Arrays.asList(items));
        }
    }

    private static final class Structs implements Workload<ListInput> {
        @Override
        public byte[] serialize(ListInput input) {
            // Each item's values straight into the table, without a Triple of them
            var items = input.items();

            return Triples.serialize(items.size(), (index, bytes, offset) -> {
                var item = items.get(index);
                Triple.write(bytes, offset, item.value(), item.first(), item.second());
            });
        }

        @Override
        public ListInput deserialize(byte[] bytes) {
            var triples = new Triples(bytes).items();

            var items = new ItemInput[triples.size()];
            for (int i = 0; i < items.length; i++) {
                var triple = triples.get(i);
                items[i] = new ItemInput(triple.value(), triple.first(), triple.second());
            }

            return new ListInput(Arrays.asList(items));
        }
    }

    private static final class Samples implements Workload<SampleInput> {
        @Override
        public byte[] serialize(SampleInput input) {
            return new Sample(input.key(), input.a(), input.b()).serialize();
        }

        @Override
        public SampleInput deserialize(byte[] bytes) {
            var sample = Sample.read(bytes, 0);

            return new SampleInput(sample.key(), sample.a(), sample.b());
        }
    }
}
