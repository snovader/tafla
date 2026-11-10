package io.github.snovader.tafla.test;

import io.github.snovader.tafla.*;
import io.github.snovader.tafla.test.FieldDescription.*;

import java.util.*;

import static org.junit.jupiter.api.Assertions.*;

/**
 * Reads the bytes of a table as descriptions of its fields set them out, and returns those
 * descriptions with the values that were read in place of theirs, so that they equal the
 * descriptions when the bytes hold their values. Every read goes through {@link ByteBuffer}, and
 * throws its {@link DecodeException}.
 *
 * <p>A nested table, from {@link TableBuilder#setTable} or in a vector, is read in place when it
 * has no header and its parent's offset type. Any other is read as a table of its own: a vector
 * item from its bytes, and a field from its position to the end of its parent, since its length
 * isn't written.</p>
 *
 * <p>Descriptions don't hold a vector's item count or a table's size, so a strict reader checks
 * them: every vector must hold as many items as its description, and every sized table read from
 * its own bytes, the root or a vector item, must hold their length in its size slot. A lenient
 * reader, for corrupted bytes, reads them without checking, and reads as many of a vector's items
 * as both the bytes and the description hold.</p>
 */
final class TableReader {
    private final ByteBuffer buffer;
    private final LayoutDescription layout;
    private final boolean strict;

    private TableReader(byte[] bytes, LayoutDescription layout, boolean strict) {
        this.buffer = new ByteBuffer(bytes, layout.tableLayout());
        this.layout = layout;
        this.strict = strict;
    }

    static TableReader strict(byte[] bytes, LayoutDescription layout) {
        return new TableReader(bytes, layout, true);
    }

    static TableReader lenient(byte[] bytes, LayoutDescription layout) {
        return new TableReader(bytes, layout, false);
    }

    /** Reads the header and every field of the table, which must have the reader's layout */
    TableDescription readTable(TableDescription table) {
        var version = readHeader();
        return new TableDescription(layout, version, readRootFields(table.fields()));
    }

    /** Reads the header's slots, checking a strict reader's size, and returns the version */
    OptionalLong readHeader() {
        if (layout.sized()) {
            var size = buffer.getSize();
            if (strict) {
                assertEquals(buffer.bytes().length, size, "The size slot doesn't hold the table's length");
            }
        }

        return readVersion();
    }

    FieldDescription readRootField(int index, FieldDescription field) {
        return readField(buffer.rootOffset(index), field);
    }

    private OptionalLong readVersion() {
        return layout.versioned() ? OptionalLong.of(buffer.getVersion()) : OptionalLong.empty();
    }

    private List<FieldDescription> readRootFields(List<FieldDescription> fields) {
        var read = new ArrayList<FieldDescription>(fields.size());
        for (int index = 0; index < fields.size(); index++) {
            read.add(readRootField(index, fields.get(index)));
        }

        return read;
    }

    private FieldDescription readField(int position, FieldDescription field) {
        return switch (field) {
            case ScalarField scalar -> new ScalarField(scalar.type(), scalar.type().read(buffer, position));
            case BoolField ignored -> new BoolField(buffer.getUint8(position) != 0);
            case ArrayField array -> new ArrayField(array.type(), array.type().readArray(buffer, position));
            case StringField ignored -> new StringField(buffer.getString(position));
            case StructField struct -> StructBytes.read(buffer, position, struct.types());
            case StructArrayField array -> StructBytes.readArray(buffer, position, array.fields());
            case TableField table -> new TableField(table.from(), readNested(position, table.table()), table.alignment());
            case VectorField vector -> new VectorField(readVector(position, vector.items()));
        };
    }

    /** Reads a table nested with {@link TableBuilder#setTable} */
    private TableDescription readNested(int position, TableDescription table) {
        if (readsInPlace(table.layout())) {
            return readInPlace(position, table);
        }

        // Its length isn't written, so it's read up to the end of its parent, and its size slot, if
        // any, isn't that length. A position past the end leaves no bytes, which the first read
        // throws for.
        var bytes = buffer.bytes();
        var rest = Arrays.copyOfRange(bytes, Math.min(position, bytes.length), bytes.length);
        var reader = new TableReader(rest, table.layout(), strict);

        return new TableDescription(table.layout(), reader.readVersion(), reader.readRootFields(table.fields()));
    }

    private List<NestedTable> readVector(int position, List<NestedTable> items) {
        var offsets = buffer.getVectorOffsets(position);
        var vector = buffer.getVector(position);
        if (strict) {
            assertEquals(items.size(), offsets.length, "The vector doesn't hold its items");
        }

        // Finding each item from its ends alone finds the same item as reading every item, whether
        // the count is read again or given
        assertEquals(offsets.length, buffer.getVectorLength(position), "The vector's length");
        for (int index = 0; index < offsets.length; index++) {
            assertEquals(offsets[index], buffer.getVectorOffset(position, index), "Item " + index + " from its ends");
            assertEquals(
                offsets[index],
                buffer.getVectorItemOffset(position, offsets.length, index),
                "Item " + index + " from its ends and the count"
            );
        }

        var read = new ArrayList<NestedTable>(items.size());
        for (int index = 0; index < Math.min(items.size(), offsets.length); index++) {
            var item = items.get(index);
            var table = readsInPlace(item.table().layout())
                ? readInPlace(offsets[index], item.table())
                : new TableReader(vector[index], item.table().layout(), strict).readTable(item.table());

            read.add(new NestedTable(item.from(), table));
        }

        return read;
    }

    private boolean readsInPlace(LayoutDescription nested) {
        return !nested.sized() && !nested.versioned() && nested.offsetType() == layout.offsetType();
    }

    private TableDescription readInPlace(int position, TableDescription table) {
        var fields = table.fields();

        var read = new ArrayList<FieldDescription>(fields.size());
        for (int index = 0; index < fields.size(); index++) {
            read.add(readField(buffer.fieldOffset(position, index), fields.get(index)));
        }

        return new TableDescription(table.layout(), OptionalLong.empty(), read);
    }
}
