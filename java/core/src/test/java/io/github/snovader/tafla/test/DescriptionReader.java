package io.github.snovader.tafla.test;

import io.github.snovader.tafla.*;
import io.github.snovader.tafla.test.FieldDescription.*;

import java.util.*;

import static org.junit.jupiter.api.Assertions.*;

/**
 * Reads the bytes of a table as descriptions of its fields set them out, and returns those
 * descriptions with the values that were read in place of theirs, so that they equal the
 * descriptions when the bytes hold their values. Every read goes through {@link TableReader}, and
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
 *
 * <p>The cases of presence describe absent fields, whose offsets a strict reader checks are 0, and
 * tables read by a reader of a later revision, which counts the fields that it finds missing, as
 * {@code conformance/README.md} describes.</p>
 */
final class DescriptionReader {
    private final TableReader reader;
    private final LayoutDescription layout;
    private final boolean strict;

    private DescriptionReader(byte[] bytes, LayoutDescription layout, boolean strict) {
        this.reader = new TableReader(bytes, layout.tableLayout());
        this.layout = layout;
        this.strict = strict;
    }

    static DescriptionReader strict(byte[] bytes, LayoutDescription layout) {
        return new DescriptionReader(bytes, layout, true);
    }

    static DescriptionReader lenient(byte[] bytes, LayoutDescription layout) {
        return new DescriptionReader(bytes, layout, false);
    }

    /**
     * Reads the header and every field of the table, which must have the reader's layout, and
     * counts the fields that it's missing when it describes some
     */
    TableDescription readTable(TableDescription table) {
        var version = readHeader();
        return new TableDescription(layout, version, readRootFields(table.fields()), countMissing(0, table));
    }

    /** Reads the header's slots, checking a strict reader's size, and returns the version */
    OptionalLong readHeader() {
        if (layout.sized()) {
            var size = reader.getSize();
            if (strict) {
                assertEquals(reader.bytes().length, size, "The size slot doesn't hold the table's length");
            }
        }

        return readVersion();
    }

    FieldDescription readRootField(int index, FieldDescription field) {
        return readField(0, reader.rootOffset(index), field);
    }

    private OptionalLong readVersion() {
        return layout.versioned() ? OptionalLong.of(reader.getVersion()) : OptionalLong.empty();
    }

    private List<FieldDescription> readRootFields(List<FieldDescription> fields) {
        var read = new ArrayList<FieldDescription>(fields.size());
        for (int index = 0; index < fields.size(); index++) {
            read.add(readRootField(index, fields.get(index)));
        }

        return read;
    }

    /**
     * Reads the field at {@code position} of the table at {@code tablePosition}, whose offsets count
     * from there, which is 0 for the root
     */
    private FieldDescription readField(int tablePosition, int position, FieldDescription field) {
        return switch (field) {
            case AbsentField absent -> readAbsent(position - tablePosition, absent);
            case ScalarField scalar -> new ScalarField(scalar.type(), scalar.type().read(reader, position));
            case BoolField ignored -> new BoolField(reader.getUint8(position) != 0);
            case ArrayField array -> new ArrayField(array.type(), array.type().readArray(reader, position));
            case StringField ignored -> new StringField(reader.getString(position));
            case StructField struct -> StructBytes.read(reader, position, struct.types());
            case StructArrayField array -> StructBytes.readArray(reader, position, array.fields());
            case TableField table -> new TableField(table.from(), readNested(position, table.table()), table.alignment());
            case VectorField vector -> new VectorField(readVector(position, vector.items()));
        };
    }

    /**
     * Reads a field described as absent, whose offset from its table's start a strict reader
     * checks is 0, as the format writes it. A lenient reader reads the offset alone.
     */
    private AbsentField readAbsent(int offset, AbsentField field) {
        if (strict) {
            assertEquals(0, offset, "The offset of a field described as absent");
        }

        return field;
    }

    /**
     * Counts the fields after the table's own that it doesn't hold, of those that a reader of a
     * later revision knows there, when the description has them: a field whose offset would end
     * after the table's first value starts, which the first field's offset gives, or whose offset
     * is 0, as the padding before an aligned first value reads. A table that holds one of them
     * counts fewer than the description's.
     *
     * @param position the table's position, 0 for the root, whose offsets follow its header, and
     *     otherwise that of a table read in place, whose offsets count from its start
     */
    private OptionalInt countMissing(int position, TableDescription table) {
        if (table.missing().isEmpty()) {
            return OptionalInt.empty();
        }

        var root = position == 0;
        var headerSize = root ? layout.headerSize() : 0;
        var firstValue = root ? reader.rootOffset(0) : reader.fieldOffset(position, 0) - position;

        var fields = table.fields().size();
        var count = 0;
        for (int index = fields; index < fields + table.missing().getAsInt(); index++) {
            // An offset that would end after the first value starts is never read
            var holdsOffset = headerSize + (index + 1L) * layout.width() <= firstValue;
            if (!holdsOffset || relativeOffset(position, index) == 0) {
                count++;
            }
        }

        return OptionalInt.of(count);
    }

    /** {@return the offset of field {@code index} of the table at {@code position}, from its start} */
    private int relativeOffset(int position, int index) {
        return position == 0 ? reader.rootOffset(index) : reader.fieldOffset(position, index) - position;
    }

    /** Reads a table nested with {@link TableBuilder#setTable} */
    private TableDescription readNested(int position, TableDescription table) {
        if (readsInPlace(table.layout())) {
            return readInPlace(position, table);
        }

        // Its length isn't written, so it's read up to the end of its parent, and its size slot, if
        // any, isn't that length. A position past the end leaves no bytes, which the first read
        // throws for.
        var bytes = reader.bytes();
        var rest = Arrays.copyOfRange(bytes, Math.min(position, bytes.length), bytes.length);
        var nestedReader = new DescriptionReader(rest, table.layout(), strict);

        var version = nestedReader.readVersion();
        var fields = nestedReader.readRootFields(table.fields());

        return new TableDescription(table.layout(), version, fields, nestedReader.countMissing(0, table));
    }

    private List<NestedTable> readVector(int position, List<NestedTable> items) {
        var offsets = reader.getVectorOffsets(position);
        var vector = reader.getVector(position);
        if (strict) {
            assertEquals(items.size(), offsets.length, "The vector doesn't hold its items");
        }

        // Finding each item from its ends alone finds the same item as reading every item, whether
        // the count is read again or given
        assertEquals(offsets.length, reader.getVectorLength(position), "The vector's length");
        for (int index = 0; index < offsets.length; index++) {
            assertEquals(offsets[index], reader.getVectorOffset(position, index), "Item " + index + " from its ends");
            assertEquals(
                offsets[index],
                reader.getVectorItemOffset(position, offsets.length, index),
                "Item " + index + " from its ends and the count"
            );
        }

        var read = new ArrayList<NestedTable>(items.size());
        for (int index = 0; index < Math.min(items.size(), offsets.length); index++) {
            var item = items.get(index);
            var table = readsInPlace(item.table().layout())
                ? readInPlace(offsets[index], item.table())
                : new DescriptionReader(vector[index], item.table().layout(), strict).readTable(item.table());

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
            read.add(readField(position, reader.fieldOffset(position, index), fields.get(index)));
        }

        return new TableDescription(table.layout(), OptionalLong.empty(), read, countMissing(position, table));
    }
}
