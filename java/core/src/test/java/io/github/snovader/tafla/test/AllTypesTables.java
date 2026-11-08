package io.github.snovader.tafla.test;

import io.github.snovader.tafla.test.FieldDescription.*;
import io.github.snovader.tafla.test.generated.all_types.*;
import net.jqwik.api.*;

import java.util.ArrayList;
import java.util.Collections;
import java.util.List;

import static io.github.snovader.tafla.test.FieldValues.*;
import static io.github.snovader.tafla.test.GeneratedDescriptions.*;

/**
 * The code generated from {@code schemas/all_types.tafla}: how tests read its messages back into
 * their fields, describe their fields in the conformance suite's model to build their tables by hand
 * and to compare them, and arbitrary values of their fields. Its {@code Arrays} hides
 * {@link java.util.Arrays} here.
 */
final class AllTypesTables {
    /** How many levels of {@code Tree}s a tree holds below itself at most */
    private static final int TREE_DEPTH = 2;

    // The types of each struct's scalars in the order of their offsets, with the scalars of a struct
    // within a struct in its place

    /**
     * Every scalar type, in the order of {@link ScalarType}'s constants, and then the uint8 that
     * stores a bool
     */
    static final List<ScalarType> PACKED_TYPES = java.util.stream.Stream.concat(
        java.util.Arrays.stream(ScalarType.values()),
        java.util.stream.Stream.of(ScalarType.UINT8)
    ).toList();
    static final List<ScalarType> DOCUMENTED_TYPES = List.of(ScalarType.UINT8, ScalarType.UINT16, ScalarType.UINT32);
    static final List<ScalarType> POINT_TYPES = List.of(ScalarType.FLOAT32, ScalarType.FLOAT32);
    /** A start point and an end point */
    static final List<ScalarType> SEGMENT_TYPES = Collections.nCopies(2, POINT_TYPES).stream()
        .flatMap(List::stream)
        .toList();

    private AllTypesTables() {}

    // Reading every field, nested tables and vector items included

    static Scalars.Fields readScalars(Scalars scalars) {
        return new Scalars.Fields(
            scalars.int8(),
            scalars.uint8(),
            scalars.int16(),
            scalars.uint16(),
            scalars.int32(),
            scalars.uint32(),
            scalars.int64(),
            scalars.uint64(),
            scalars.float32(),
            scalars.float64(),
            scalars.bool()
        );
    }

    static Arrays.Fields readArrays(Arrays arrays) {
        return new Arrays.Fields(
            arrays.int8(),
            arrays.uint8(),
            arrays.int16(),
            arrays.uint16(),
            arrays.int32(),
            arrays.uint32(),
            arrays.int64(),
            arrays.uint64(),
            arrays.float32(),
            arrays.float64()
        );
    }

    static Nested.Fields readNested(Nested nested) {
        var trees = nested.trees().stream().map(AllTypesTables::readTree).toList();
        return new Nested.Fields(nested.label(), readOuter(nested.outer()), trees);
    }

    private static Outer.Fields readOuter(Outer outer) {
        return new Outer.Fields(outer.count(), readInner(outer.inner()));
    }

    private static Inner.Fields readInner(Inner inner) {
        return new Inner.Fields(inner.name(), inner.values());
    }

    private static Tree.Fields readTree(Tree tree) {
        var children = tree.children().stream().map(AllTypesTables::readTree).toList();
        return new Tree.Fields(tree.value(), children);
    }

    static Reserved.Fields readReserved(Reserved reserved) {
        return new Reserved.Fields(readKeywords(reserved.keywords()));
    }

    static Structs.Fields readStructs(Structs structs) {
        return new Structs.Fields(
            structs.packed(),
            structs.documented(),
            structs.segment(),
            structs.segments(),
            readPath(structs.path())
        );
    }

    private static Path.Fields readPath(Path path) {
        return new Path.Fields(path.start(), path.points());
    }

    // Reading the vectors, arrays of structs and arrays of scalars through their views instead

    /** Ways to read a view's items: by index from the last, and in order through its iterator */
    enum ViewOrder {
        /** As a reader that skips items would */
        FROM_LAST {
            @Override
            <T> List<T> read(List<T> view) {
                var items = new ArrayList<T>(view.size());
                for (int i = view.size() - 1; i >= 0; i--) {
                    items.add(view.get(i));
                }

                return items.reversed();
            }
        },
        ITERATED {
            @Override
            <T> List<T> read(List<T> view) {
                var items = new ArrayList<T>(view.size());
                for (var item : view) {
                    items.add(item);
                }

                return items;
            }
        };

        /** {@return the view's items, in order} */
        abstract <T> List<T> read(List<T> view);
    }

    static Nested.Fields readNestedViews(Nested nested, ViewOrder order) {
        var trees = order.read(nested.treesView()).stream().map(tree -> readTreeViews(tree, order)).toList();
        return new Nested.Fields(nested.label(), readOuter(nested.outer()), trees);
    }

    private static Tree.Fields readTreeViews(Tree tree, ViewOrder order) {
        var children = order.read(tree.childrenView()).stream().map(child -> readTreeViews(child, order)).toList();
        return new Tree.Fields(tree.value(), children);
    }

    static Structs.Fields readStructsViews(Structs structs, ViewOrder order) {
        var path = structs.path();

        return new Structs.Fields(
            structs.packed(),
            order.read(structs.documentedView()).toArray(Documented[]::new),
            structs.segment(),
            order.read(structs.segmentsView()).toArray(Segment[]::new),
            new Path.Fields(path.start(), order.read(path.pointsView()).toArray(Point[]::new))
        );
    }

    /**
     * Reads every array through its view, element by element, with unsigned elements widened from
     * their bits as the getters widen them
     */
    static Arrays.Fields readArraysViews(Arrays arrays) {
        var int8 = arrays.int8View();
        var int8s = new byte[int8.remaining()];
        for (int i = 0; i < int8s.length; i++) {
            int8s[i] = int8.get(i);
        }

        var uint8 = arrays.uint8View();
        var uint8s = new short[uint8.remaining()];
        for (int i = 0; i < uint8s.length; i++) {
            uint8s[i] = (short) Byte.toUnsignedInt(uint8.get(i));
        }

        var int16 = arrays.int16View();
        var int16s = new short[int16.remaining()];
        for (int i = 0; i < int16s.length; i++) {
            int16s[i] = int16.get(i);
        }

        var uint16 = arrays.uint16View();
        var uint16s = new int[uint16.remaining()];
        for (int i = 0; i < uint16s.length; i++) {
            uint16s[i] = Short.toUnsignedInt(uint16.get(i));
        }

        var int32 = arrays.int32View();
        var int32s = new int[int32.remaining()];
        for (int i = 0; i < int32s.length; i++) {
            int32s[i] = int32.get(i);
        }

        var uint32 = arrays.uint32View();
        var uint32s = new long[uint32.remaining()];
        for (int i = 0; i < uint32s.length; i++) {
            uint32s[i] = Integer.toUnsignedLong(uint32.get(i));
        }

        var int64 = arrays.int64View();
        var int64s = new long[int64.remaining()];
        for (int i = 0; i < int64s.length; i++) {
            int64s[i] = int64.get(i);
        }

        var uint64 = arrays.uint64View();
        var uint64s = new long[uint64.remaining()];
        for (int i = 0; i < uint64s.length; i++) {
            uint64s[i] = uint64.get(i);
        }

        var float32 = arrays.float32View();
        var float32s = new float[float32.remaining()];
        for (int i = 0; i < float32s.length; i++) {
            float32s[i] = float32.get(i);
        }

        var float64 = arrays.float64View();
        var float64s = new double[float64.remaining()];
        for (int i = 0; i < float64s.length; i++) {
            float64s[i] = float64.get(i);
        }

        return new Arrays.Fields(int8s, uint8s, int16s, uint16s, int32s, uint32s, int64s, uint64s, float32s, float64s);
    }

    private static Keywords.Fields readKeywords(Keywords keywords) {
        return new Keywords.Fields(
            keywords.class_(),
            keywords.default_(),
            keywords.in(),
            keywords.match(),
            keywords.self(),
            keywords.type()
        );
    }

    // The tables that the generated code writes, in the conformance suite's model

    static TableDescription describeScalars(Scalars.Fields scalars) {
        return message(
            DEFAULT_LAYOUT,
            int8(scalars.int8()),
            uint8(scalars.uint8()),
            int16(scalars.int16()),
            uint16(scalars.uint16()),
            int32(scalars.int32()),
            uint32(scalars.uint32()),
            int64(scalars.int64()),
            uint64(scalars.uint64()),
            float32(scalars.float32()),
            float64(scalars.float64()),
            new BoolField(scalars.bool())
        );
    }

    static TableDescription describeArrays(Arrays.Fields arrays) {
        return message(
            DEFAULT_LAYOUT,
            int8Array(arrays.int8()),
            uint8Array(arrays.uint8()),
            int16Array(arrays.int16()),
            uint16Array(arrays.uint16()),
            int32Array(arrays.int32()),
            uint32Array(arrays.uint32()),
            int64Array(arrays.int64()),
            uint64Array(arrays.uint64()),
            float32Array(arrays.float32()),
            float64Array(arrays.float64())
        );
    }

    static TableDescription describeNested(Nested.Fields nested) {
        var trees = nested.trees().stream().map(AllTypesTables::describeTree).toList();
        return message(DEFAULT_LAYOUT, string(nested.label()), describeOuter(nested.outer()), vector(trees));
    }

    private static TableField describeOuter(Outer.Fields outer) {
        return table(uint32(outer.count()), describeInner(outer.inner()));
    }

    private static TableField describeInner(Inner.Fields inner) {
        return table(string(inner.name()), float64Array(inner.values()));
    }

    private static NestedTable describeTree(Tree.Fields tree) {
        var children = tree.children().stream().map(AllTypesTables::describeTree).toList();
        return vectorItem(int32(tree.value()), vector(children));
    }

    static TableDescription describeReserved(Reserved.Fields reserved) {
        return message(DEFAULT_LAYOUT, describeKeywords(reserved.keywords()));
    }

    private static TableField describeKeywords(Keywords.Fields keywords) {
        return table(
            uint8(keywords.class_()),
            uint8(keywords.default_()),
            uint8(keywords.in()),
            uint8(keywords.match()),
            uint8(keywords.self()),
            string(keywords.type())
        );
    }

    static TableDescription describeStructs(Structs.Fields structs) {
        var documented = List.of(structs.documented()).stream().map(AllTypesTables::describeDocumented).toList();
        var segments = List.of(structs.segments()).stream().map(AllTypesTables::describeSegment).toList();

        return message(
            DEFAULT_LAYOUT,
            struct(describePacked(structs.packed())),
            structArray(DOCUMENTED_TYPES, documented),
            struct(describeSegment(structs.segment())),
            structArray(SEGMENT_TYPES, segments),
            describePath(structs.path())
        );
    }

    private static TableField describePath(Path.Fields path) {
        var points = List.of(path.points()).stream().map(AllTypesTables::describePoint).toList();
        return table(struct(describePoint(path.start())), structArray(POINT_TYPES, points));
    }

    /**
     * {@return the fields of a {@code Packed} in the order of their offsets, with the bool as the
     * uint8 that stores it}
     */
    static List<ScalarField> describePacked(Packed packed) {
        return List.of(
            int8(packed.int8()),
            uint8(packed.uint8()),
            int16(packed.int16()),
            uint16(packed.uint16()),
            int32(packed.int32()),
            uint32(packed.uint32()),
            int64(packed.int64()),
            uint64(packed.uint64()),
            float32(packed.float32()),
            float64(packed.float64()),
            uint8((short) (packed.bool() ? 1 : 0))
        );
    }

    /** {@return the fields of a {@code Documented} in the order of their offsets} */
    static List<ScalarField> describeDocumented(Documented documented) {
        return List.of(uint8(documented.one()), uint16(documented.two()), uint32(documented.undocumented()));
    }

    /** {@return the scalars of a {@code Segment}'s points in the order of their offsets} */
    static List<ScalarField> describeSegment(Segment segment) {
        var scalars = new ArrayList<>(describePoint(segment.start()));
        scalars.addAll(describePoint(segment.end()));

        return scalars;
    }

    /** {@return the fields of a {@code Point} in the order of their offsets} */
    static List<ScalarField> describePoint(Point point) {
        return List.of(float32(point.x()), float32(point.y()));
    }

    // Arbitrary values of the fields

    static Arbitrary<Scalars.Fields> scalarsFields() {
        // Combinators take eight arbitraries at most
        var integers = Combinators.combine(
            int8s(),
            uint8s(),
            int16s(),
            uint16s(),
            int32s(),
            uint32s(),
            int64s(),
            uint64s()
        ).as(Tuple::of);

        return Combinators.combine(integers, float32s(), float64s(), Arbitraries.of(false, true)).as(
            (ints, float32, float64, bool) -> new Scalars.Fields(
                ints.get1(),
                ints.get2(),
                ints.get3(),
                ints.get4(),
                ints.get5(),
                ints.get6(),
                ints.get7(),
                ints.get8(),
                float32,
                float64,
                bool
            )
        );
    }

    static Arbitrary<Arrays.Fields> arraysFields() {
        // Combinators take eight arbitraries at most
        var integers = Combinators.combine(
            int8Arrays(),
            uint8Arrays(),
            int16Arrays(),
            uint16Arrays(),
            int32Arrays(),
            uint32Arrays(),
            int64Arrays(),
            uint64Arrays()
        ).as(Tuple::of);

        return Combinators.combine(integers, float32Arrays(), float64Arrays()).as(
            (ints, float32, float64) -> new Arrays.Fields(
                ints.get1(),
                ints.get2(),
                ints.get3(),
                ints.get4(),
                ints.get5(),
                ints.get6(),
                ints.get7(),
                ints.get8(),
                float32,
                float64
            )
        );
    }

    static Arbitrary<Nested.Fields> nestedFields() {
        var inner = Combinators.combine(strings(), float64Arrays()).as(Inner.Fields::new);
        var outer = Combinators.combine(uint32s(), inner).as(Outer.Fields::new);

        return Combinators.combine(strings(), outer, vectorsOf(trees(TREE_DEPTH))).as(Nested.Fields::new);
    }

    /** {@return trees whose children hold children {@code depth} levels deep at most, and none below} */
    private static Arbitrary<Tree.Fields> trees(int depth) {
        var children = depth == 0 ? Arbitraries.just(List.<Tree.Fields>of()) : vectorsOf(trees(depth - 1));
        return Combinators.combine(int32s(), children).as(Tree.Fields::new);
    }

    static Arbitrary<Reserved.Fields> reservedFields() {
        var keywords = Combinators.combine(uint8s(), uint8s(), uint8s(), uint8s(), uint8s(), strings())
            .as(Keywords.Fields::new);

        return keywords.map(Reserved.Fields::new);
    }

    static Arbitrary<Packed> packedFields() {
        return scalarsFields().map(AllTypesTables::packedOf);
    }

    /** {@return a {@code Packed} of the values of {@code Scalars}, which it holds in the same order} */
    static Packed packedOf(Scalars.Fields scalars) {
        return new Packed(
            scalars.int8(),
            scalars.uint8(),
            scalars.int16(),
            scalars.uint16(),
            scalars.int32(),
            scalars.uint32(),
            scalars.int64(),
            scalars.uint64(),
            scalars.float32(),
            scalars.float64(),
            scalars.bool()
        );
    }

    static Arbitrary<Documented> documentedFields() {
        return Combinators.combine(uint8s(), uint16s(), uint32s()).as(Documented::new);
    }

    static Arbitrary<Structs.Fields> structsFields() {
        var path = Combinators.combine(pointFields(), structArraysOf(pointFields(), Point[].class)).as(Path.Fields::new);

        return Combinators.combine(
            packedFields(),
            structArraysOf(documentedFields(), Documented[].class),
            segmentFields(),
            structArraysOf(segmentFields(), Segment[].class),
            path
        ).as(Structs.Fields::new);
    }

    static Arbitrary<Segment> segmentFields() {
        return Combinators.combine(pointFields(), pointFields()).as(Segment::new);
    }

    static Arbitrary<Point> pointFields() {
        return Combinators.combine(float32s(), float32s()).as(Point::new);
    }
}
