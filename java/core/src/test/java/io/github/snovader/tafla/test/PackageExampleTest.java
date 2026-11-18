package io.github.snovader.tafla.test;

import io.github.snovader.tafla.*;
import net.jqwik.api.*;

import static org.junit.jupiter.api.Assertions.*;

/** Runs the example in the package's documentation, which must be kept the same as this */
public class PackageExampleTest {
    @Example
    void should_read_back_the_example_table() {
        // A format's header, shared by its writer and its readers
        TableLayout layout = TableLayout.builder().sized(true).versioned(true).build();
        int formatVersion = 1;

        var series = new TableBuilder(2);
        series.setUint32Array(new long[] {0, 86400});
        series.setFloat32Array(new float[] {21.75f, 18.5f});

        var builder = new TableBuilder(3, layout, formatVersion);
        builder.setFloat64(59.3293);
        builder.setString("Europe/Stockholm");
        builder.setTable(series); // written straight into the table
        byte[] bytes = builder.bytes();

        var reader = new TableReader(bytes, layout);
        if (reader.getVersion() != formatVersion) {
            throw new IllegalStateException("Unsupported format version");
        }

        double latitude = reader.getFloat64(reader.rootOffset(0));
        String timeZone = reader.getString(reader.rootOffset(1));
        int table = reader.rootOffset(2);
        float[] values = reader.getFloat32Array(reader.fieldOffset(table, 1));

        assertEquals(59.3293, latitude);
        assertEquals("Europe/Stockholm", timeZone);
        assertArrayEquals(new float[] {21.75f, 18.5f}, values);
    }
}
