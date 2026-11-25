package io.github.snovader.tafla.test;

import io.github.snovader.tafla.*;
import net.jqwik.api.*;

import java.util.*;

import static org.junit.jupiter.api.Assertions.*;

public class TableLayoutTest {
    @Example
    void should_default_to_no_header_slots_and_uint32_offsets() {
        for (var layout : List.of(TableLayout.DEFAULT, TableLayout.builder().build())) {
            assertFalse(layout.sized());
            assertFalse(layout.versioned());
            assertEquals(OffsetType.UINT32, layout.offsetType());
            assertEquals(0, layout.headerSize());
        }
    }

    @Property(generation = GenerationMode.EXHAUSTIVE)
    void the_header_has_a_slot_of_the_offset_size_for_the_length_and_for_the_version(
        @ForAll boolean sized,
        @ForAll boolean versioned,
        @ForAll OffsetType offsetType
    ) {
        var builder = TableLayout.builder().sized(sized).offsetType(offsetType);
        var layout = (versioned ? builder.version(1) : builder).build();
        var slots = (sized ? 1 : 0) + (versioned ? 1 : 0);

        assertEquals(slots * offsetType.size(), layout.headerSize());
        assertEquals(versioned, layout.versioned());
        assertEquals(versioned ? OptionalLong.of(1) : OptionalLong.empty(), layout.version());
    }

    @Example
    void a_layout_of_another_version_differs_in_the_version_only() {
        var first = TableLayout.builder().sized(true).version(1).offsetType(OffsetType.UINT16).build();
        var second = first.withVersion(2);

        assertEquals(OptionalLong.of(2), second.version());
        assertTrue(second.sized());
        assertEquals(OffsetType.UINT16, second.offsetType());
        assertEquals(OptionalLong.of(1), first.version());
        assertEquals(OptionalLong.of(7), TableLayout.DEFAULT.withVersion(7).version());
    }

    @Example
    void the_layout_has_the_offset_type_and_its_size_in_bytes() {
        var sizes = Map.of(OffsetType.UINT8, 1, OffsetType.UINT16, 2, OffsetType.UINT32, 4);

        for (var entry : sizes.entrySet()) {
            var layout = TableLayout.builder().offsetType(entry.getKey()).build();

            assertEquals(entry.getKey(), layout.offsetType());
            assertEquals(entry.getValue(), layout.offsetSize());
        }
    }

    @Example
    void offset_types_are_named_as_the_spec_names_them() {
        assertEquals("uint8", OffsetType.UINT8.toString());
        assertEquals("uint16", OffsetType.UINT16.toString());
        assertEquals("uint32", OffsetType.UINT32.toString());
    }

    @Example
    void the_layout_names_its_offset_type() {
        var layout = TableLayout.builder().sized(true).offsetType(OffsetType.UINT8).build();

        assertEquals("TableLayout[sized=true, version=none, offsetType=uint8]", layout.toString());
    }
}
