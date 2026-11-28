package io.github.snovader.tafla.test;

import io.github.snovader.tafla.test.generated.shapes.*;
import jakarta.json.*;
import net.jqwik.api.*;

import java.io.*;
import java.nio.charset.*;
import java.nio.file.*;
import java.util.*;

import static io.github.snovader.tafla.test.ShapesTables.*;
import static org.junit.jupiter.api.Assertions.*;

/**
 * The code generated from {@code schemas/shapes.tafla} writes the tables and the struct of the
 * cases in {@code conformance/generated}, which the generated TypeScript wrote, byte for byte, and
 * reads their values back. See {@code conformance/generated/README.md} at the root of the
 * repository for the format of {@code shapes.json}, whose values {@link DescriptionJson} reads as
 * it reads those of {@code cases.json}.
 */
public class ShapesCasesTest {
    private static final Path DIRECTORY = Path.of(System.getProperty("tafla.conformance"), "generated");
    private static final JsonObject SHAPES = readShapes();

    /** A case of {@code shapes.json}, which shows as its name when a property fails */
    record ShapesCase(JsonObject description) {
        String name() {
            return description.getString("name");
        }

        /** {@return the message or struct of the schema whose values the case holds} */
        String type() {
            return description.getString("type");
        }

        JsonObject fields() {
            return description.getJsonObject("fields");
        }

        byte[] bytes() {
            try {
                return Files.readAllBytes(DIRECTORY.resolve(description.getString("file")));
            } catch (IOException ex) {
                throw new UncheckedIOException(ex);
            }
        }

        @Override
        public String toString() {
            return name();
        }
    }

    private static JsonObject readShapes() {
        try (var reader = Json.createReader(
            Files.newBufferedReader(DIRECTORY.resolve("shapes.json"), StandardCharsets.UTF_8)
        )) {
            return reader.readObject();
        } catch (IOException ex) {
            throw new UncheckedIOException(ex);
        }
    }

    @Provide
    Arbitrary<ShapesCase> cases() {
        var cases = SHAPES.getJsonArray("cases").stream().map(value -> new ShapesCase(value.asJsonObject())).toList();
        return Arbitraries.of(cases);
    }

    @Property(generation = GenerationMode.EXHAUSTIVE, shrinking = ShrinkingMode.OFF)
    void should_write_every_case_byte_for_byte(@ForAll("cases") ShapesCase shapesCase) {
        assertArrayEquals(shapesCase.bytes(), serialize(shapesCase));
    }

    @Property(generation = GenerationMode.EXHAUSTIVE, shrinking = ShrinkingMode.OFF)
    void should_read_the_values_of_every_case(@ForAll("cases") ShapesCase shapesCase) {
        assertEquals(describeFields(shapesCase), describeBytes(shapesCase));
    }

    /** {@return the case's fields, written with the generated code} */
    private static byte[] serialize(ShapesCase shapesCase) {
        var fields = shapesCase.fields();

        return switch (shapesCase.type()) {
            case "Point" -> Point.serialize(pointOf(fields));
            case "Document" -> Document.serialize(documentOf(fields));
            case "ItemList" -> ItemList.serialize(itemListOf(fields));
            case "Sample" -> sampleOf(fields).serialize();
            case "Task" -> Task.serialize(taskOf(fields));
            default -> throw unknownType(shapesCase);
        };
    }

    /**
     * {@return the description of the case's fields}, which compares floats by their bits, as
     * {@link #describeBytes} describes the values read
     */
    private static Object describeFields(ShapesCase shapesCase) {
        var fields = shapesCase.fields();

        return switch (shapesCase.type()) {
            case "Point" -> describePoint(pointOf(fields));
            case "Document" -> describeDocument(documentOf(fields));
            case "ItemList" -> describeItemList(itemListOf(fields));
            case "Sample" -> describeSample(sampleOf(fields));
            case "Task" -> describeTask(taskOf(fields));
            default -> throw unknownType(shapesCase);
        };
    }

    /** {@return the description of every field that the generated code reads from the case's bytes} */
    private static Object describeBytes(ShapesCase shapesCase) {
        var bytes = shapesCase.bytes();

        return switch (shapesCase.type()) {
            case "Point" -> describePoint(readPoint(new Point(bytes)));
            case "Document" -> describeDocument(readDocument(new Document(bytes)));
            case "ItemList" -> describeItemList(readItemList(new ItemList(bytes)));
            case "Sample" -> describeSample(Sample.read(bytes, 0));
            case "Task" -> describeTask(readTask(new Task(bytes)));
            default -> throw unknownType(shapesCase);
        };
    }

    private static IllegalArgumentException unknownType(ShapesCase shapesCase) {
        return new IllegalArgumentException(shapesCase.type() + " isn't a message or struct of shapes.tafla");
    }

    // The fields of each type, from the JSON of a case

    private static Point.Fields pointOf(JsonObject json) {
        return new Point.Fields(float64(json, "x"), float64(json, "y"), int64(json, "id"), json.getString("name"));
    }

    private static Document.Fields documentOf(JsonObject json) {
        return new Document.Fields(
            float64(json, "x"),
            float64(json, "y"),
            json.getString("name"),
            columnsOf(json.getJsonObject("columns"))
        );
    }

    private static Columns.Fields columnsOf(JsonObject json) {
        return new Columns.Fields(
            int64(json, "base"),
            DescriptionJson.readValues(ScalarType.UINT32, json.getJsonArray("keys")),
            float32Array(json, "a"),
            float32Array(json, "b")
        );
    }

    private static ItemList.Fields itemListOf(JsonObject json) {
        var items = json.getJsonArray("items").stream().map(item -> itemOf(item.asJsonObject())).toList();
        return new ItemList.Fields(items);
    }

    private static Item.Fields itemOf(JsonObject json) {
        return new Item.Fields(float32(json, "value"), pairOf(json.getJsonObject("pair")));
    }

    private static Pair.Fields pairOf(JsonObject json) {
        return new Pair.Fields(float32(json, "first"), float32(json, "second"));
    }

    private static Sample sampleOf(JsonObject json) {
        return new Sample(uint32(json, "key"), float32(json, "a"), float32(json, "b"));
    }

    private static Task.Fields taskOf(JsonObject json) {
        return new Task.Fields(
            int8(json, "priority"),
            ScalarType.toInts(DescriptionJson.readValues(ScalarType.UINT16, json.getJsonArray("history"))),
            stepOf(json.getJsonObject("step")),
            json.getString("name")
        );
    }

    private static Step stepOf(JsonObject json) {
        return new Step(uint16(json, "state"), int8(json, "priority"), uint32(json, "seconds"));
    }

    // Values, which JSON holds as cases.json does, and enums as their integers

    private static byte int8(JsonObject json, String name) {
        return (byte) DescriptionJson.readValue(ScalarType.INT8, valueOf(json, name));
    }

    private static int uint16(JsonObject json, String name) {
        return (int) DescriptionJson.readValue(ScalarType.UINT16, valueOf(json, name));
    }

    private static long uint32(JsonObject json, String name) {
        return DescriptionJson.readValue(ScalarType.UINT32, valueOf(json, name));
    }

    private static long int64(JsonObject json, String name) {
        return DescriptionJson.readValue(ScalarType.INT64, valueOf(json, name));
    }

    private static float float32(JsonObject json, String name) {
        return Float.intBitsToFloat((int) DescriptionJson.readValue(ScalarType.FLOAT32, valueOf(json, name)));
    }

    private static double float64(JsonObject json, String name) {
        return Double.longBitsToDouble(DescriptionJson.readValue(ScalarType.FLOAT64, valueOf(json, name)));
    }

    private static float[] float32Array(JsonObject json, String name) {
        return ScalarType.toFloats(DescriptionJson.readValues(ScalarType.FLOAT32, json.getJsonArray(name)));
    }

    private static JsonValue valueOf(JsonObject json, String name) {
        return Objects.requireNonNull(json.get(name), () -> "The fields have no " + name);
    }
}
