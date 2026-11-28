package io.github.snovader.tafla.test;

import io.github.snovader.tafla.test.FieldDescription.*;
import io.github.snovader.tafla.test.generated.all_types.*;
import net.jqwik.api.*;

import java.util.List;
import java.util.Map;

import static io.github.snovader.tafla.test.FieldValues.*;
import static io.github.snovader.tafla.test.GeneratedDescriptions.*;

/**
 * The message of enums that the code generated from {@code schemas/all_types.tafla} writes: the
 * values of its enums, how tests read the message back and describe it, in the integer types that
 * store its enums, to build its table by hand and to compare it, and arbitrary values of its fields,
 * over the whole range of each type, most of which the enums don't name.
 */
final class EnumsTables {
    /** The integer types that store the scalars of a {@code Mark}, those of its {@code Turn} last */
    static final List<ScalarType> MARK_TYPES = List.of(
        ScalarType.UINT8,
        ScalarType.INT16,
        ScalarType.FLOAT32,
        ScalarType.INT64,
        ScalarType.INT8,
        ScalarType.UINT64
    );

    /**
     * The enums of all_types.tafla, each with the Java type of its constants, which its fields read
     * as, and the values that the schema names, by their constants' names
     */
    static final List<EnumValues> ENUM_VALUES = List.of(
        new EnumValues(Level.class, short.class, Map.of("LOW", 0L, "MEDIUM", 1L, "HIGH", 2L)),
        new EnumValues(
            Direction.class,
            byte.class,
            Map.of("BACKWARD", -1L, "NONE", 0L, "FORWARD", 1L, "FURTHEST_BACK", -128L)
        ),
        new EnumValues(Unit.class, int.class, Map.of("LAST", 65_535L, "FIRST", 0L, "V_2", 2L)),
        new EnumValues(Change.class, short.class, Map.of("LEAST", -32_768L, "GREATEST", 32_767L)),
        new EnumValues(Status.class, long.class, Map.of("OK", 0L, "NOT_FOUND", 404L, "GREATEST", 4_294_967_295L)),
        new EnumValues(Sign.class, int.class, Map.of("LEAST", -2_147_483_648L, "GREATEST", 2_147_483_647L)),
        // A uint64 beyond a long's greatest is the long of its bits
        new EnumValues(
            Mask.class,
            long.class,
            Map.ofEntries(
                Map.entry("NONE", 0L),
                Map.entry("ONE", 1L),
                Map.entry("BEYOND_FLOAT64", 9_007_199_254_740_993L),
                Map.entry("ALL", Long.parseUnsignedLong("18446744073709551615"))
            )
        ),
        new EnumValues(
            Extent.class,
            long.class,
            Map.of("LEAST", -9_223_372_036_854_775_808L, "ZERO", 0L, "GREATEST", 9_223_372_036_854_775_807L)
        ),
        new EnumValues(
            Word.class,
            short.class,
            Map.ofEntries(
                Map.entry("CLASS", 0L),
                Map.entry("DEFAULT", 1L),
                Map.entry("IN", 2L),
                Map.entry("MATCH", 3L),
                Map.entry("SELF", 4L),
                Map.entry("TYPE", 5L),
                Map.entry("TRUE", 6L),
                Map.entry("NULL", 7L),
                Map.entry("NONE", 8L),
                Map.entry("INIT", 9L),
                Map.entry("NEW", 10L),
                Map.entry("NAME", 11L),
                Map.entry("VALUE", 12L)
            )
        )
    );

    /** An enum's class, the type of its constants, and their values by their names, as longs */
    record EnumValues(Class<?> enumClass, Class<?> type, Map<String, Long> values) {
        @Override
        public String toString() {
            return enumClass.getSimpleName();
        }
    }

    private EnumsTables() {}

    // Reading every field, the structs of arrays and the tables of vectors included

    static Enums.Fields readEnums(Enums enums) {
        return new Enums.Fields(
            enums.level(),
            enums.levels(),
            enums.direction(),
            enums.directions(),
            enums.unit(),
            enums.units(),
            enums.change(),
            enums.changes(),
            enums.status(),
            enums.statuses(),
            enums.sign(),
            enums.signs(),
            enums.mask(),
            enums.masks(),
            enums.extent(),
            enums.extents(),
            enums.word(),
            enums.mark(),
            enums.marks().toArray(Mark[]::new),
            enums.steps().stream().map(EnumsTables::readStep).toList()
        );
    }

    private static Step.Fields readStep(Step step) {
        return new Step.Fields(step.level(), step.mark(), step.name());
    }

    // The table that the generated code writes, in the conformance suite's model, where an enum is
    // the integer type that stores it

    static TableDescription describeEnums(Enums.Fields enums) {
        var marks = List.of(enums.marks()).stream().map(EnumsTables::describeMark).toList();
        var steps = enums.steps().stream().map(EnumsTables::describeStep).toList();

        return message(
            DEFAULT_LAYOUT,
            uint8(enums.level()),
            uint8Array(enums.levels()),
            int8(enums.direction()),
            int8Array(enums.directions()),
            uint16(enums.unit()),
            uint16Array(enums.units()),
            int16(enums.change()),
            int16Array(enums.changes()),
            uint32(enums.status()),
            uint32Array(enums.statuses()),
            int32(enums.sign()),
            int32Array(enums.signs()),
            uint64(enums.mask()),
            uint64Array(enums.masks()),
            int64(enums.extent()),
            int64Array(enums.extents()),
            uint8(enums.word()),
            struct(describeMark(enums.mark())),
            structArray(MARK_TYPES, marks),
            vector(steps)
        );
    }

    private static NestedTable describeStep(Step.Fields step) {
        return vectorItem(uint8(step.level()), struct(describeMark(step.mark())), string(step.name()));
    }

    /** {@return the scalars of a {@code Mark} in the order of their offsets, its turn's last} */
    static List<ScalarField> describeMark(Mark mark) {
        var turn = mark.turn();

        return List.of(
            uint8(mark.level()),
            int16(mark.change()),
            float32(mark.weight()),
            int64(mark.extent()),
            int8(turn.direction()),
            uint64(turn.mask())
        );
    }

    // Arbitrary values of the fields: any integer of the type that stores each enum

    static Arbitrary<Enums.Fields> enumsFields() {
        // Combinators take eight arbitraries at most: the enums of 1 and 2 bytes, and those of 4 and 8
        var narrowEnums = Combinators.combine(
            uint8s(),
            uint8Arrays(),
            int8s(),
            int8Arrays(),
            uint16s(),
            uint16Arrays(),
            int16s(),
            int16Arrays()
        ).as(Tuple::of);
        var wideEnums = Combinators.combine(
            uint32s(),
            uint32Arrays(),
            int32s(),
            int32Arrays(),
            uint64s(),
            uint64Arrays(),
            int64s(),
            int64Arrays()
        ).as(Tuple::of);
        var marks = structArraysOf(markFields(), Mark[].class);
        var steps = vectorsOf(Combinators.combine(uint8s(), markFields(), strings()).as(Step.Fields::new));

        return Combinators.combine(narrowEnums, wideEnums, uint8s(), markFields(), marks, steps).as(
            (narrow, wide, word, mark, markArray, stepList) -> new Enums.Fields(
                narrow.get1(),
                narrow.get2(),
                narrow.get3(),
                narrow.get4(),
                narrow.get5(),
                narrow.get6(),
                narrow.get7(),
                narrow.get8(),
                wide.get1(),
                wide.get2(),
                wide.get3(),
                wide.get4(),
                wide.get5(),
                wide.get6(),
                wide.get7(),
                wide.get8(),
                word,
                mark,
                markArray,
                stepList
            )
        );
    }

    static Arbitrary<Mark> markFields() {
        var turns = Combinators.combine(int8s(), uint64s()).as(Turn::new);
        return Combinators.combine(uint8s(), int16s(), float32s(), int64s(), turns).as(Mark::new);
    }
}
