package io.github.snovader.tafla.test;

import io.github.snovader.tafla.*;
import io.github.snovader.tafla.test.FieldDescription.*;
import net.jqwik.api.*;

import java.util.*;

import static org.junit.jupiter.api.Assertions.*;

/** Tables read back what they were built from */
public class RoundTripTest {
    @Property
    void a_table_built_from_any_description_reads_back_the_same_values(@ForAll("tables") TableDescription table) {
        var bytes = Tables.build(table).bytes();

        assertEquals(table, DescriptionReader.strict(bytes, table.layout()).readTable(table));
    }

    @Example
    void a_table_reads_back_the_limits_of_every_type_as_scalars_and_in_arrays() {
        var fields = new ArrayList<FieldDescription>();
        for (var type : ScalarType.values()) {
            var limits = Descriptions.limits(type);

            limits.forEach(limit -> fields.add(new ScalarField(type, limit)));
            fields.add(new ArrayField(type, limits.stream().mapToLong(Long::longValue).toArray()));
        }

        var table = new TableDescription(LayoutDescription.headerless(OffsetType.UINT32), OptionalLong.empty(), fields);
        var bytes = Tables.build(table).bytes();

        assertEquals(table, DescriptionReader.strict(bytes, table.layout()).readTable(table));
    }

    // Tables of 32 KiB and more, so fewer of them
    @Property(tries = 250)
    void fields_at_offsets_with_the_top_bit_of_the_offset_type_set_read_back(
        @ForAll("narrowTables") TableDescription table
    ) {
        // An array that ends past 2^7 or 2^15 bytes, before the table's fields
        var offsetType = table.layout().offsetType();
        var array = new ArrayField(ScalarType.UINT8, new long[(int) (offsetType.maxValue() / 2) + 1]);

        var fields = new ArrayList<FieldDescription>();
        fields.add(array);
        fields.addAll(table.fields());

        var withArray = new TableDescription(table.layout(), table.version(), fields);
        Assume.that(Descriptions.maxLength(withArray) <= offsetType.maxValue());

        var bytes = Tables.build(withArray).bytes();
        assertEquals(withArray, DescriptionReader.strict(bytes, table.layout()).readTable(withArray));
    }

    @Provide
    Arbitrary<TableDescription> tables() {
        return Descriptions.tables();
    }

    @Provide
    Arbitrary<TableDescription> narrowTables() {
        return Descriptions.narrowTables();
    }
}
