package io.github.snovader.tafla.test;

import io.github.snovader.tafla.*;
import io.github.snovader.tafla.test.FieldDescription.*;
import io.github.snovader.tafla.test.generated.shapes.*;
import net.jqwik.api.*;

import java.util.*;

import static io.github.snovader.tafla.test.FieldValues.*;
import static io.github.snovader.tafla.test.GeneratedDescriptions.*;

/**
 * The code generated from {@code schemas/shapes.tafla}: how tests read its messages back into their
 * fields, describe their fields in the conformance suite's model to build their tables by hand and
 * to compare them, and arbitrary values of their fields
 */
final class ShapesTables {
    /** The layout of a {@code Document}, from its attributes: sized, and of version 1 */
    static final LayoutDescription DOCUMENT_LAYOUT = new LayoutDescription(true, true, OffsetType.UINT32);
    static final long DOCUMENT_VERSION = 1;

    private ShapesTables() {}

    // Reading every field, nested tables and vector items included

    static Point.Fields readPoint(Point point) {
        return new Point.Fields(point.x(), point.y(), point.id(), point.name());
    }

    static Document.Fields readDocument(Document document) {
        return new Document.Fields(document.x(), document.y(), document.name(), readColumns(document.columns()));
    }

    private static Columns.Fields readColumns(Columns columns) {
        return new Columns.Fields(columns.base(), columns.keys(), columns.a(), columns.b());
    }

    static ItemList.Fields readItemList(ItemList itemList) {
        return new ItemList.Fields(itemList.items().stream().map(ShapesTables::readItem).toList());
    }

    static Item.Fields readItem(Item item) {
        return new Item.Fields(item.value(), readPair(item.pair()));
    }

    private static Pair.Fields readPair(Pair pair) {
        return new Pair.Fields(pair.first(), pair.second());
    }

    static Task.Fields readTask(Task task) {
        return new Task.Fields(task.priority(), task.history(), task.step(), task.name());
    }

    // The tables that the generated code writes, in the conformance suite's model

    static TableDescription describePoint(Point.Fields point) {
        return message(DEFAULT_LAYOUT, float64(point.x()), float64(point.y()), int64(point.id()), string(point.name()));
    }

    static TableDescription describeDocument(Document.Fields document) {
        return message(
            DOCUMENT_LAYOUT,
            DOCUMENT_VERSION,
            float64(document.x()),
            float64(document.y()),
            string(document.name()),
            describeColumns(document.columns())
        );
    }

    private static TableField describeColumns(Columns.Fields columns) {
        return table(
            int64(columns.base()),
            uint32Array(columns.keys()),
            float32Array(columns.a()),
            float32Array(columns.b())
        );
    }

    static TableDescription describeItemList(ItemList.Fields itemList) {
        return message(DEFAULT_LAYOUT, vector(itemList.items().stream().map(ShapesTables::describeItem).toList()));
    }

    private static NestedTable describeItem(Item.Fields item) {
        return vectorItem(float32(item.value()), describePair(item.pair()));
    }

    private static TableField describePair(Pair.Fields pair) {
        return table(float32(pair.first()), float32(pair.second()));
    }

    /** {@return the fields of a {@code Sample} in the order of their offsets} */
    static List<ScalarField> describeSample(Sample sample) {
        return List.of(uint32(sample.key()), float32(sample.a()), float32(sample.b()));
    }

    /** {@return the table of a {@code Task}, whose enums are the integer types that store them} */
    static TableDescription describeTask(Task.Fields task) {
        var step = task.step();

        return message(
            DEFAULT_LAYOUT,
            int8(task.priority()),
            uint16Array(task.history()),
            struct(List.of(uint16(step.state()), int8(step.priority()), uint32(step.seconds()))),
            string(task.name())
        );
    }

    // Arbitrary values of the fields

    static Arbitrary<Point.Fields> pointFields() {
        return Combinators.combine(float64s(), float64s(), int64s(), strings()).as(Point.Fields::new);
    }

    static Arbitrary<Document.Fields> documentFields() {
        var columns = Combinators.combine(int64s(), uint32Arrays(), float32Arrays(), float32Arrays())
            .as(Columns.Fields::new);

        return Combinators.combine(float64s(), float64s(), strings(), columns).as(Document.Fields::new);
    }

    static Arbitrary<ItemList.Fields> itemListFields() {
        var pairs = Combinators.combine(float32s(), float32s()).as(Pair.Fields::new);
        var items = Combinators.combine(float32s(), pairs).as(Item.Fields::new);

        return vectorsOf(items).map(ItemList.Fields::new);
    }

    static Arbitrary<Sample> sampleFields() {
        return Combinators.combine(uint32s(), float32s(), float32s()).as(Sample::new);
    }
}
