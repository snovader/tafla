package io.github.snovader.tafla.test;

import io.github.snovader.tafla.TableReader;
import io.github.snovader.tafla.test.generated.all_types.*;
import net.jqwik.api.*;
import net.jqwik.api.constraints.*;

import java.lang.reflect.Modifier;
import java.util.HashMap;

import static io.github.snovader.tafla.test.DecodeAssertions.*;
import static io.github.snovader.tafla.test.EnumsTables.*;
import static io.github.snovader.tafla.test.GeneratedDescriptions.*;
import static org.junit.jupiter.api.Assertions.*;

/**
 * The enums of {@code schemas/all_types.tafla}: names for some of the integers of a type, which the
 * generated code holds as constants of the primitive type that a field of the enum reads as, so a
 * field reads any integer of its type, named or not, and compares with the constants as it is.
 */
public class EnumsTest {
    // The constants

    @Property(generation = GenerationMode.EXHAUSTIVE)
    void an_enum_holds_the_values_that_the_schema_names_as_constants_of_its_fields_type(
        @ForAll("enumValues") EnumValues expected
    ) throws IllegalAccessException {
        var constants = new HashMap<String, Long>();
        for (var field : expected.enumClass().getFields()) {
            assertTrue(Modifier.isStatic(field.getModifiers()) && Modifier.isFinal(field.getModifiers()), field::toString);
            assertEquals(expected.type(), field.getType(), field::toString);

            constants.put(field.getName(), ((Number) field.get(null)).longValue());
        }

        assertEquals(expected.values(), constants);
        assertEquals(0, expected.enumClass().getConstructors().length, "An enum's class has no public constructor");
    }

    @Example
    void the_constants_are_written_and_read_back_as_the_integers_that_the_schema_names() {
        var mark = new Mark(Level.HIGH, Change.LEAST, 0.5f, Extent.GREATEST, new Turn(Direction.BACKWARD, Mask.ALL));

        var read = Mark.read(mark.serialize(), 0);

        assertEquals(2, read.level());
        assertEquals(-32_768, read.change());
        assertEquals(9_223_372_036_854_775_807L, read.extent());
        assertEquals(-1, read.turn().direction());
        assertEquals("18446744073709551615", Long.toUnsignedString(read.turn().mask()));
        // The values compare with the constants without a cast
        assertTrue(read.level() == Level.HIGH && read.turn().mask() == Mask.ALL);
    }

    @Example
    void a_switch_takes_the_constants_as_its_labels_and_any_other_value_as_its_default() {
        var named = Turn.read(new Turn(Direction.FURTHEST_BACK, Mask.NONE).serialize(), 0);
        var unnamed = Turn.read(new Turn((byte) 2, Mask.NONE).serialize(), 0);

        assertEquals("furthest back", directionName(named));
        assertEquals("unnamed", directionName(unnamed));
    }

    private static String directionName(Turn turn) {
        return switch (turn.direction()) {
            case Direction.BACKWARD -> "backward";
            case Direction.NONE -> "none";
            case Direction.FORWARD -> "forward";
            case Direction.FURTHEST_BACK -> "furthest back";
            default -> "unnamed";
        };
    }

    // Round trips of any integer of each enum's type, most of which the enum doesn't name

    @Property
    void enums_read_back_any_integer_of_each_enum_as_a_field_in_an_array_in_structs_and_in_a_vector(
        @ForAll("enums") Enums.Fields fields
    ) {
        var bytes = Enums.serialize(fields);

        assertEquals(describeEnums(fields), describeEnums(readEnums(new Enums(bytes))));
    }

    @Property
    void a_mark_is_written_in_its_24_bytes_little_endian_at_fixed_offsets_and_read_back(@ForAll("marks") Mark mark) {
        var bytes = mark.serialize();

        assertEquals(24, Mark.SIZE);
        assertArrayEquals(structBytes(describeMark(mark)), bytes);
        assertEquals(describeMark(mark), describeMark(Mark.read(bytes, 0)));
    }

    @Property
    void any_byte_of_an_enum_of_one_byte_reads_as_its_value(
        @ForAll("enums") Enums.Fields fields,
        @ForAll byte level,
        @ForAll byte direction
    ) {
        var bytes = Enums.serialize(fields);
        var reader = new TableReader(bytes);
        bytes[reader.rootOffset(0)] = level;
        bytes[reader.rootOffset(2)] = direction;

        var enums = new Enums(bytes);

        assertEquals(Byte.toUnsignedInt(level), enums.level());
        assertEquals(direction, enums.direction());
    }

    // Tables built by hand

    @Property
    void enums_are_written_as_TableBuilder_writes_their_integer_types(@ForAll("enums") Enums.Fields fields) {
        var byHand = Tables.build(describeEnums(fields)).bytes();

        assertArrayEquals(byHand, Enums.serialize(fields), () -> describeEnums(fields).toString());
    }

    // Decode exceptions

    @Property
    void corrupted_enums_read_as_values_or_throw_DecodeException(
        @ForAll("enums") Enums.Fields fields,
        @ForAll("corruptions") Corruption corruption
    ) {
        var bytes = corruption.applyTo(Enums.serialize(fields));

        assertValuesOrDecodeException(() -> readEnums(new Enums(bytes)));
    }

    @Property
    void any_bytes_read_as_enums_or_throw_DecodeException(@ForAll @Size(max = 64) byte[] bytes) {
        assertValuesOrDecodeException(() -> readEnums(new Enums(bytes)));
    }

    @Provide
    Arbitrary<EnumValues> enumValues() {
        return Arbitraries.of(ENUM_VALUES);
    }

    @Provide
    Arbitrary<Enums.Fields> enums() {
        return enumsFields();
    }

    @Provide
    Arbitrary<Mark> marks() {
        return markFields();
    }

    @Provide
    Arbitrary<Corruption> corruptions() {
        return Corruption.corruptions();
    }
}
