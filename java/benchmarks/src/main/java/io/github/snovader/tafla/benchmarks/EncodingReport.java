package io.github.snovader.tafla.benchmarks;

import io.github.snovader.tafla.benchmarks.codecs.*;

import java.io.*;
import java.util.*;

/**
 * Prints what the timings don't show: each codec's encoded size of every workload and of the
 * strings suite's shapes, and what it does with a document cut short, as the TypeScript
 * comparison and strings suite print after their timings.
 */
public final class EncodingReport {
    private EncodingReport() {}

    public static void main(String[] args) {
        var documents = Workloads.DOCUMENT_ROWS.stream().map(Workloads::documentInput).toList();

        printSizes(documents);
        printStringSizes();
        printTruncation(documents.getLast());
    }

    private static void printSizes(List<DocumentInput> documents) {
        var columns = new ArrayList<String>();
        columns.add("record");
        for (var rows : Workloads.DOCUMENT_ROWS) {
            columns.add("doc " + rows);
        }
        columns.add("vector");
        columns.add("structs");
        columns.add("fixed");

        System.out.printf("Encoded size (bytes)%n  %-16s", "");
        columns.forEach(column -> System.out.printf("%12s", column));
        System.out.println();

        var list = Workloads.listInput(Workloads.LIST_ITEMS);
        for (var codec : Codecs.all()) {
            System.out.printf("  %-16s%12d", codec.name(), codec.record().serialize(Workloads.RECORD).length);
            for (var document : documents) {
                System.out.printf("%12d", codec.document().serialize(document).length);
            }
            System.out.printf("%12d", codec.list().serialize(list).length);
            System.out.printf("%12d", codec.structs().serialize(list).length);
            System.out.printf("%12d%n", codec.sample().serialize(Workloads.SAMPLE).length);
        }

        // SBE, which encodes only the array of structs and the fixed-size record
        var sbeStructs = Codecs.structs(Codecs.SBE).serialize(list);
        var sbeSample = Codecs.sample(Codecs.SBE).serialize(Workloads.SAMPLE);
        System.out.printf(
            "  %-16s%" + 12 * (columns.size() - 2) + "s%12d%12d%n",
            Codecs.SBE,
            "",
            sbeStructs.length,
            sbeSample.length
        );
    }

    /** Prints a row of each codec's encoded size of 16 strings for each shape */
    private static void printStringSizes() {
        var codecs = Codecs.all();

        System.out.printf("%nEncoded size of 16 strings (bytes)%n  %-16s", "");
        codecs.forEach(codec -> System.out.printf("%12s", codec.name()));
        System.out.println();

        for (var shape : StringShapes.ALL) {
            System.out.printf("  %-16s", shape.name());
            for (var codec : codecs) {
                System.out.printf("%12d", codec.strings().serialize(shape.strings()).length);
            }
            System.out.println();
        }
    }

    /**
     * Decodes copies of a document cut short, as a dropped connection would leave it. A copy,
     * since a view could read on into the full bytes.
     */
    private static void printTruncation(DocumentInput input) {
        System.out.printf("%nTruncated document of %d rows: deserialize()%n", input.a().length);

        for (var codec : Codecs.all()) {
            var bytes = codec.document().serialize(input);
            for (var length : List.of(bytes.length / 2, 24)) {
                var truncated = Arrays.copyOf(bytes, length);
                var outcome = describeOutcome(codec.document(), truncated, input.a().length);

                System.out.printf("  %-16s%8d B  %s%n", codec.name(), length, outcome);
            }
        }
    }

    private static String describeOutcome(Workload<DocumentInput> workload, byte[] bytes, int rows) {
        try {
            var output = workload.deserialize(bytes);
            return String.format("returned %d of %d values", output.a().length, rows);
        } catch (RuntimeException ex) {
            // The codecs rethrow checked exceptions, such as protobuf's, unchecked
            var cause = ex instanceof UncheckedIOException && ex.getCause() != null ? ex.getCause() : ex;
            var name = cause.getClass().getSimpleName();
            var message = cause.getMessage();

            if (message == null) {
                return "threw " + name;
            }

            var excerpt = message.substring(0, Math.min(message.length(), 50)).stripTrailing();
            return String.format("threw %s: %s", name, excerpt);
        }
    }
}
