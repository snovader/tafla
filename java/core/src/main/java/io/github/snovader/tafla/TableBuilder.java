package io.github.snovader.tafla;

import org.jspecify.annotations.*;

import java.util.*;
import java.util.function.*;

/**
 * Builds a table: a header with one offset per field, after the table's total length and version
 * when its layout has them, followed by the fields in the order they're set. Arrays, strings and
 * vectors start with their count, and a vector then holds the end of each item. Arrays of scalars
 * and of structs are padded so that their elements are aligned from the start of the table. Fields
 * are read back by index with {@link ByteBuffer#rootOffset}, or {@link ByteBuffer#fieldOffset} for
 * a table nested in another.
 *
 * <p>Setting a field records it and counts its bytes. {@link #bytes()} then allocates the table at
 * its exact size and writes every field in one pass, with a {@link TableWriter} as the generated
 * code does, so array elements must not change in between. Tables nested from their builders are
 * written in the same pass, straight into this one. {@link #setTable(int, Consumer)} and
 * {@link #setVector(int, int, ObjIntConsumer)} record a nested table, or each item of a vector,
 * among this table's own fields instead, without a builder of its own.</p>
 *
 * <p>A setter throws, and leaves the builder as it was, when the table already holds all its
 * fields, or when its field would make the table longer than the layout's offset type addresses:
 * 255 bytes, 64 KiB, or for {@code uint32} offsets, as long as a Java array can be.</p>
 */
public final class TableBuilder {
    private static final FieldWrite[] NO_WRITES = {};

    /** Writes a field that a setter recorded, with its index, through the writer's setter */
    @FunctionalInterface
    private interface FieldWrite {
        void write(TableWriter writer, int index);
    }

    /** The table's bytes, once written, after which it takes no more fields */
    private byte @Nullable [] written;

    private final TableLayout layout;
    private final long version;
    /** The type of offsets, lengths and header slots */
    private final OffsetType offsetType;
    /**
     * The longest table that the layout's offsets and lengths address, which a Java array's length
     * limits to an {@code int}
     */
    private final int maxLength;

    // What the builder counts of the table whose fields it's recording: its own, or one that it
    // records in place, as a field or a vector's item, while that table's fill runs. Every nested
    // builder is one of these objects, which holds these and what every table needs in 64 bytes
    // with compressed references, as it did before tables were recorded in place.

    /** Where the table's fields go in {@link #writes} */
    private int tableStart = 0;
    /** The number of fields that the table holds */
    private int fields;
    private int fieldsSet;
    /** The table's length so far, which is where the next field goes */
    private int offset;
    /**
     * The largest alignment that a field needs from the table's start, which a parent pads this
     * table to when it nests it
     */
    private int alignment = Byte.BYTES;

    /**
     * What writes each field, which captures its value, a scalar without boxing it: this table's
     * own fields first, and then those of the tables recorded in place in it, each table's
     * together. The writer works out each field's padding and offset as it writes it, so they
     * aren't recorded. A switch over recorded kinds instead was faster to record, but C2 compiled
     * a string's encoding slowly within it.
     */
    private FieldWrite[] writes;
    /**
     * The number of {@link #writes} that hold or are kept for fields: this table's own, and then
     * those of the tables recorded in place in it
     */
    private int recorded;

    /** Builds a table of {@link TableLayout#DEFAULT}, such as one to nest in another */
    public TableBuilder(int fields) {
        this(fields, TableLayout.DEFAULT);
    }

    /**
     * @param fields the number of fields that the table will hold
     * @param layout the table's header and offset type, which its readers must use too
     * @throws IllegalArgumentException if the layout is versioned, or the header alone is longer
     *     than the layout addresses
     */
    public TableBuilder(int fields, TableLayout layout) {
        this(fields, layout, false, 0);
    }

    /**
     * @param version the version to write to the header. Readers can check it with
     *     {@link ByteBuffer#getVersion()} before reading any field.
     * @throws IllegalArgumentException if the layout isn't versioned, the version doesn't fit in
     *     the layout's offset type, or the header alone is longer than the layout addresses
     */
    public TableBuilder(int fields, TableLayout layout, long version) {
        this(fields, layout, true, version);
    }

    private TableBuilder(int fields, TableLayout layout, boolean versioned, long version) {
        this.offset = layout.headerLength(fields, versioned, version);
        this.layout = layout;
        this.version = version;
        this.offsetType = layout.offsetType();
        this.maxLength = (int) layout.maxLength();

        this.fields = fields;
        this.writes = new FieldWrite[fields];
        this.recorded = fields;
    }

    /**
     * Writes the table, once; later calls return the same bytes.
     *
     * @throws IllegalStateException if fewer fields were set than the table holds
     */
    public byte[] bytes() {
        var written = this.written;
        if (written != null) {
            return written;
        }

        if (tableStart != 0) {
            throw new IllegalStateException("The table can't be written while a fill records a table in it");
        }
        if (fieldsSet != fields) {
            throw new IllegalStateException(
                String.format("Only %d of the table's %d fields were set", fieldsSet, fields)
            );
        }

        // The constructor checked the header, as the sizer's writer() does
        var writer = new TableWriter(new byte[offset], offset, fields, layout, version, List.of());
        writeFields(writer, 0, fields);
        written = writer.bytes();

        // Release the writes, whose values may be large arrays
        this.writes = NO_WRITES;
        this.written = written;

        return written;
    }

    public void setInt8(byte value) {
        addField(Byte.BYTES, 0, (writer, index) -> writer.setInt8(index, value));
    }

    public void setUint8(short value) {
        addField(Byte.BYTES, 0, (writer, index) -> writer.setUint8(index, value));
    }

    public void setInt16(short value) {
        addField(Short.BYTES, 0, (writer, index) -> writer.setInt16(index, value));
    }

    public void setUint16(int value) {
        addField(Short.BYTES, 0, (writer, index) -> writer.setUint16(index, value));
    }

    public void setInt32(int value) {
        addField(Integer.BYTES, 0, (writer, index) -> writer.setInt32(index, value));
    }

    public void setUint32(long value) {
        addField(Integer.BYTES, 0, (writer, index) -> writer.setUint32(index, value));
    }

    public void setInt64(long value) {
        addField(Long.BYTES, 0, (writer, index) -> writer.setInt64(index, value));
    }

    /** Sets a {@code uint64} from a {@code long} with its bits */
    public void setUint64(long value) {
        addField(Long.BYTES, 0, (writer, index) -> writer.setUint64(index, value));
    }

    public void setFloat32(float value) {
        addField(Float.BYTES, 0, (writer, index) -> writer.setFloat32(index, value));
    }

    public void setFloat64(double value) {
        addField(Double.BYTES, 0, (writer, index) -> writer.setFloat64(index, value));
    }

    public void setInt8Array(byte[] values) {
        addArray(values.length, Byte.BYTES, (writer, index) -> writer.setInt8Array(index, values));
    }

    public void setUint8Array(short[] values) {
        addArray(values.length, Byte.BYTES, (writer, index) -> writer.setUint8Array(index, values));
    }

    public void setInt16Array(short[] values) {
        addArray(values.length, Short.BYTES, (writer, index) -> writer.setInt16Array(index, values));
    }

    public void setUint16Array(int[] values) {
        addArray(values.length, Short.BYTES, (writer, index) -> writer.setUint16Array(index, values));
    }

    public void setInt32Array(int[] values) {
        addArray(values.length, Integer.BYTES, (writer, index) -> writer.setInt32Array(index, values));
    }

    public void setUint32Array(long[] values) {
        addArray(values.length, Integer.BYTES, (writer, index) -> writer.setUint32Array(index, values));
    }

    public void setInt64Array(long[] values) {
        addArray(values.length, Long.BYTES, (writer, index) -> writer.setInt64Array(index, values));
    }

    /** Sets {@code uint64}s from {@code long}s with their bits */
    public void setUint64Array(long[] values) {
        addArray(values.length, Long.BYTES, (writer, index) -> writer.setUint64Array(index, values));
    }

    public void setFloat32Array(float[] values) {
        addArray(values.length, Float.BYTES, (writer, index) -> writer.setFloat32Array(index, values));
    }

    public void setFloat64Array(double[] values) {
        addArray(values.length, Double.BYTES, (writer, index) -> writer.setFloat64Array(index, values));
    }

    /** Writes {@code string} as UTF-8, with lone surrogates replaced by U+FFFD */
    public void setString(String string) {
        // As the sizer adds strings: a short one is counted, and the writer encodes it in place,
        // and a long one is encoded once, faster than counting it, and the writer copies it
        if (string.length() < Utf8.SHORT_LENGTH) {
            var byteLength = offsetType.size() + Utf8.length(string);
            addField(byteLength, 0, (writer, index) -> writer.setString(index, string));
        } else {
            var utf8 = Utf8.encode(string);
            var byteLength = offsetType.size() + (long) utf8.length;
            addField(byteLength, 0, (writer, index) -> writer.setEncodedString(index, utf8));
        }
    }

    /**
     * Sets a struct field from its bytes: its scalars, little-endian, one after another, which are
     * written as they are, without padding. {@link ByteBuffer#getStruct} reads them back.
     */
    public void setStruct(byte[] bytes) {
        addField(bytes.length, 0, (writer, index) -> writer.setStruct(index, bytes));
    }

    /**
     * Sets an array of structs of {@code size} bytes each, from their bytes one after another,
     * after their count. It's padded so that the structs start at a multiple of {@code alignment}
     * from the start of the table, which is the size of the structs' largest scalar.
     * {@link ByteBuffer#getStructArray} reads them back.
     *
     * @param alignment what the structs' start is padded to a multiple of: 1, 2, 4 or 8
     * @throws IllegalArgumentException if {@code size} is less than 1, {@code bytes} aren't a whole
     *     number of structs, or the alignment isn't 1, 2, 4 or 8
     */
    public void setStructArray(byte[] bytes, int size, int alignment) {
        TableWriter.structCount(bytes.length, size);

        FieldWrite write = (writer, index) -> writer.setStructArray(index, bytes, size, alignment);
        addElements(bytes.length, alignment, write);
    }

    /**
     * Embeds a table from its builder, padded to the largest alignment of its arrays, arrays of
     * structs and nested tables, or not at all if it has none of them.
     *
     * @throws IllegalArgumentException if the builder has fields left to set
     * @see #setTable(TableBuilder, int)
     */
    public void setTable(TableBuilder table) {
        setTable(table, table.alignment);
    }

    /**
     * Embeds a table from its builder, without a length prefix. Readers find its fields with
     * {@link ByteBuffer#fieldOffset} from the field's offset, so its layout must be
     * {@link TableLayout#DEFAULT}.
     *
     * <p>The builder must have all its fields set. {@link #bytes()} writes it straight into this
     * table, without bytes of its own, so its arrays must not change until then either. It can
     * still be written itself or nested elsewhere too, since a complete builder doesn't change.
     * One that was written already, or whose layout has a header or another offset type, is
     * written to its own bytes, which are copied.</p>
     *
     * @param alignment what the table's start is padded to a multiple of: 1, 2, 4 or 8
     * @throws IllegalArgumentException if the builder has fields left to set
     */
    public void setTable(TableBuilder table, int alignment) {
        var padding = TableLayout.padding(offset, alignment);
        addField(lengthOf(table), padding, (writer, index) -> {
            writer.setTable(index, alignment);
            writeTable(writer, table);
        });
        alignTo(alignment);
    }

    /**
     * Embeds a table from its bytes, padded to start at a multiple of 4, which suits tables of 8-,
     * 16- and 32-bit elements.
     *
     * @see #setTable(byte[], int)
     */
    public void setTable(byte[] table) {
        setTable(table, Integer.BYTES);
    }

    /**
     * Embeds a table from its bytes, without a length prefix. Readers find its fields with
     * {@link ByteBuffer#fieldOffset} from the field's offset, so its layout must be
     * {@link TableLayout#DEFAULT}.
     *
     * <p>The nested table's arrays are aligned from its own start, so it's padded to start at a
     * multiple of {@code alignment}, which keeps them aligned in this table too. Pass 8 for a table
     * with 64-bit arrays.</p>
     *
     * @param alignment what the table's start is padded to a multiple of: 1, 2, 4 or 8
     */
    public void setTable(byte[] table, int alignment) {
        var padding = TableLayout.padding(offset, alignment);
        addField(table.length, padding, (writer, index) -> {
            writer.setTable(index, alignment);
            writer.copyTable(table);
        });
        alignTo(alignment);
    }

    /**
     * Writes a vector of tables from their builders, after the end of each, which lets readers find
     * any item without reading the others. Builders must have all their fields set, and are written
     * straight into this table as {@link #setTable(TableBuilder)} writes them, except for those
     * that were written already, or whose layout has a header or another offset type, which are
     * written to their own bytes and copied. Items aren't padded.
     *
     * @throws IllegalArgumentException if a builder has fields left to set
     */
    public void setVector(List<TableBuilder> tables) {
        // A copy, so that the items can't change before the table is written
        var items = tables.toArray(new TableBuilder[0]);

        var byteLength = offsetType.size() + (long) items.length * offsetType.size();
        for (var item : items) {
            byteLength += lengthOf(item);
        }

        addField(byteLength, 0, (writer, index) -> writeVector(writer, index, items));
    }

    /** Writes a vector of tables from their bytes, after the end of each */
    public void setVector(byte[][] tables) {
        var items = tables.clone();

        var byteLength = offsetType.size() + (long) items.length * offsetType.size();
        for (var item : items) {
            byteLength += item.length;
        }

        addField(byteLength, 0, (writer, index) -> writeVector(writer, index, items));
    }

    /**
     * Records a table of {@code fields} fields nested in this one, as a nested builder would be, but
     * without a builder of its own: {@code fill} sets its fields, in order, on the {@link Item} that
     * it's given, which records each straight among this table's fields. The nested table has this
     * table's offset type and no header, and is padded to start at a multiple of its largest
     * alignment, as {@link #setTable(TableBuilder)} pads a builder, so it's written as a nested
     * builder of that layout is.
     *
     * <p>While {@code fill} runs, this builder records the nested table's fields rather than its
     * own, so it's used only through the item. What {@code fill} throws is passed on, and whatever
     * is thrown, the builder is left as it was.</p>
     *
     * @throws IllegalArgumentException if the number of fields is negative or their offsets don't fit
     *     in the offset type, if {@code fill} leaves fields unset, or as the setters do
     * @throws IllegalStateException as the setters do
     */
    public void setTable(int fields, Consumer<Item> fill) {
        nextField();
        var header = headerOf(fields);

        var parent = counters();
        var start = keep(fields);
        try {
            startInPlace(start, fields, header);
            fill.accept(new Item());
            checkComplete();

            // Padded from this table's length, as a nested builder is
            var length = offset;
            var tableAlignment = alignment;
            restore(parent);
            addField(length, TableLayout.padding(offset, tableAlignment), (writer, index) -> {
                writer.setTable(index, tableAlignment);
                writeInPlace(writer, start, fields);
            });
            alignTo(tableAlignment);
        } catch (Throwable failure) {
            restore(parent);
            release(start);
            throw failure;
        }
    }

    /**
     * Records a vector of {@code count} tables of {@code fields} fields each, as
     * {@link #setTable(int, Consumer)} records one: {@code fill} sets the fields of each item, in
     * order, on the {@link Item} that it's given with the item's index. Items aren't padded, as
     * {@link #setVector(List)} doesn't pad them.
     *
     * {@snippet :
     * // Items of a float32 and a nested table of two float32s
     * builder.setVector(points.size(), 2, (item, index) -> {
     *     var point = points.get(index);
     *     item.setFloat32(point.value());
     *     item.setTable(2, position -> {
     *         position.setFloat32(point.x());
     *         position.setFloat32(point.y());
     *     });
     * });
     * }
     *
     * <p>The vector's count, its items' ends and their offsets, which every item takes whatever its
     * fields, are checked before any item is recorded. A vector that they alone make too long fails
     * without calling {@code fill}, as a vector of as many tables of their offsets' bytes would.
     * What {@code fill} throws is passed on, and whatever is thrown, the builder is left as it
     * was.</p>
     *
     * @throws IllegalArgumentException if the count or the number of fields is negative, if an
     *     item's offsets don't fit in the offset type, if the vector would make the table too long
     *     for it, if {@code fill} leaves an item's fields unset, or as the setters do
     * @throws IllegalStateException as the setters do
     */
    public void setVector(int count, int fields, ObjIntConsumer<Item> fill) {
        var field = nextField();
        var header = headerOf(fields);

        // Checked first, which also bounds the writes that the items' fields are kept in
        var slots = (TableSizer.checkCount(count) + 1L) * offsetType.size();
        var fewest = offset + slots + (long) count * header;
        if (fewest > maxLength) {
            throw new IllegalArgumentException(tooLong(field, fewest));
        }

        // The items' writes, one item's after another's, before those of the tables recorded in
        // place in them
        var parent = counters();
        var start = keep(count * fields);
        try {
            var item = new Item();
            long itemBytes = 0;
            for (int i = 0; i < count; i++) {
                startInPlace(start + i * fields, fields, header);
                fill.accept(item, i);
                checkComplete();

                itemBytes += offset;
                restore(parent);
            }

            addField(slots + itemBytes, 0, (writer, index) -> writeVector(writer, index, start, count, fields));
        } catch (Throwable failure) {
            restore(parent);
            release(start);
            throw failure;
        }
    }

    /** Pads the array so that its elements, after the count, are aligned to their size */
    private void addArray(int length, int elementSize, FieldWrite write) {
        addElements((long) length * elementSize, elementSize, write);
    }

    /**
     * Records a count and then {@code byteLength} bytes of elements, padded so that the elements
     * start at a multiple of {@code alignment}
     *
     * @throws IllegalArgumentException if the alignment isn't 1, 2, 4 or 8
     */
    private void addElements(long byteLength, int alignment, FieldWrite write) {
        var padding = TableLayout.padding(offset + offsetType.size(), alignment);
        addField(offsetType.size() + byteLength, padding, write);
        alignTo(alignment);
    }

    private void alignTo(int alignment) {
        this.alignment = Math.max(this.alignment, alignment);
    }

    /**
     * The byte length of a table to nest, which a builder knows once all its fields are set
     *
     * @throws IllegalArgumentException if the builder has fields left to set
     */
    private static int lengthOf(TableBuilder table) {
        if (table.fieldsSet != table.fields) {
            throw new IllegalArgumentException(incomplete(table.fieldsSet, table.fields));
        }

        return table.offset;
    }

    private static String incomplete(int fieldsSet, int fields) {
        return String.format("Only %d of the nested table's %d fields were set", fieldsSet, fields);
    }

    /** What the builder counts of the table whose fields it's recording */
    private record Counters(int tableStart, int fields, int fieldsSet, int offset, int alignment) {}

    private Counters counters() {
        return new Counters(tableStart, fields, fieldsSet, offset, alignment);
    }

    private void restore(Counters counters) {
        tableStart = counters.tableStart();
        fields = counters.fields();
        fieldsSet = counters.fieldsSet();
        offset = counters.offset();
        alignment = counters.alignment();
    }

    /**
     * Starts counting the fields of a table recorded in place rather than those of the table that
     * holds it: {@code fields} fields, whose writes go from {@code start}, after offsets of
     * {@code header} bytes
     */
    private void startInPlace(int start, int fields, int header) {
        tableStart = start;
        this.fields = fields;
        fieldsSet = 0;
        offset = header;
        alignment = Byte.BYTES;
    }

    /**
     * {@return the byte length of the offsets of a table of {@code fields} fields recorded in
     * place}, which has this table's offset type and no header, as a nested builder's layout
     * checks them
     *
     * @throws IllegalArgumentException if the number of fields is negative, or their offsets don't
     *     fit in the offset type
     */
    private int headerOf(int fields) {
        return TableLayout.headerless(offsetType).headerLength(fields, false, 0);
    }

    /** @throws IllegalArgumentException if the table recorded in place has fields left to set */
    private void checkComplete() {
        if (fieldsSet != fields) {
            throw new IllegalArgumentException(incomplete(fieldsSet, fields));
        }
    }

    /**
     * Keeps {@code slots} writes for the fields of tables recorded in place, {@return where they
     * start}. The writes grow to twice the room they need, so that tables recorded one after
     * another take amortized constant time, and the tables nested in a vector's items usually fit
     * in the room that the vector's items made.
     */
    private int keep(int slots) {
        var start = recorded;
        var end = start + (long) slots;
        if (end > writes.length) {
            if (end > TableLayout.MAX_ARRAY_LENGTH) {
                throw new OutOfMemoryError("Too many fields to record: " + end);
            }

            writes = Arrays.copyOf(writes, (int) Math.min(2 * end, TableLayout.MAX_ARRAY_LENGTH));
        }

        recorded = (int) end;
        return start;
    }

    /**
     * Releases the writes kept from {@code start}, by a fill that failed, so that the values they
     * capture can be collected
     */
    private void release(int start) {
        Arrays.fill(writes, start, recorded, null);
        recorded = start;
    }

    /**
     * Records a field of {@code byteLength} bytes after {@code padding} bytes. Checking the
     * table's length here, rather than in {@link #bytes()}, reports a field too long for the layout
     * from its own setter and leaves the builder as it was. Every offset and length is shorter
     * than the table, so the table's length is the one value to check.
     *
     * @throws IllegalStateException if the table is already written, or holds all its fields
     * @throws IllegalArgumentException if the field would make the table longer than the layout
     *     addresses
     */
    private void addField(long byteLength, int padding, FieldWrite write) {
        var index = nextField();

        var length = offset + padding + byteLength;
        if (length > maxLength) {
            throw new IllegalArgumentException(tooLong(index, length));
        }

        writes[tableStart + index] = write;
        fieldsSet = index + 1;

        offset = (int) length;
    }

    /**
     * {@return the index of the next field of the table being recorded}
     *
     * @throws IllegalStateException if the table is already written, or holds all its fields
     */
    private int nextField() {
        if (written != null) {
            throw new IllegalStateException("The table was already written");
        }

        var index = fieldsSet;
        if (index == fields) {
            throw new IllegalStateException(
                String.format("Can't add field %d to a table of %d", fields + 1, fields)
            );
        }

        return index;
    }

    private String tooLong(int index, long length) {
        if (maxLength == TableLayout.MAX_ARRAY_LENGTH) {
            return String.format(
                "Field %d makes the table %d bytes, longer than a Java array can be",
                index + 1,
                length
            );
        }

        return String.format(
            "Field %d makes the table %d bytes, too long for %s offsets",
            index + 1,
            length,
            offsetType
        );
    }

    /**
     * Writes the {@code fields} fields of a table, whose writes start at {@code start}, with
     * {@code writer}, which has just written the table's offsets' room
     */
    private void writeFields(TableWriter writer, int start, int fields) {
        var writes = this.writes;
        for (int i = 0; i < fields; i++) {
            writes[start + i].write(writer, i);
        }
    }

    /** Writes a table recorded in place, as a nested builder is written */
    private void writeInPlace(TableWriter writer, int start, int fields) {
        var parent = writer.startTable(fields);
        writeFields(writer, start, fields);
        writer.endTable(parent);
    }

    /** Writes a vector of tables recorded in place, whose items' writes follow one another */
    private void writeVector(TableWriter writer, int index, int start, int count, int fields) {
        var vector = writer.setVector(index, count);
        for (int i = 0; i < count; i++) {
            writeInPlace(writer, start + i * fields, fields);
            writer.setItemEnd(vector, i);
        }
    }

    private void writeVector(TableWriter writer, int index, TableBuilder[] items) {
        var vector = writer.setVector(index, items.length);
        for (int i = 0; i < items.length; i++) {
            writeTable(writer, items[i]);
            writer.setItemEnd(vector, i);
        }
    }

    private static void writeVector(TableWriter writer, int index, byte[][] items) {
        var vector = writer.setVector(index, items.length);
        for (int i = 0; i < items.length; i++) {
            writer.copyTable(items[i]);
            writer.setItemEnd(vector, i);
        }
    }

    /**
     * Writes a nested builder straight into the writer, unless it was written already, or its
     * layout has a header, which {@link TableWriter#startTable} doesn't write, or another offset
     * type than this table's, whose offsets and lengths the writer can't write. Those tables are
     * written to their own bytes and copied.
     */
    private void writeTable(TableWriter writer, TableBuilder table) {
        if (table.written == null && table.offsetType == offsetType && table.layout.headerSize() == 0) {
            var parent = writer.startTable(table.fields);
            table.writeFields(writer, 0, table.fields);
            writer.endTable(parent);
        } else {
            writer.copyTable(table.bytes());
        }
    }

    /**
     * The fields of a table that a {@code TableBuilder} records in place, as one of its own fields
     * or an item of a vector, which {@link #setTable(int, Consumer)} and
     * {@link #setVector(int, int, ObjIntConsumer)} give to the fill that sets them. Its setters are
     * the builder's, taking the same values and throwing the same exceptions, and record each field
     * among the builder's own, so the table needs no builder of its own. It's valid only within
     * that fill.
     */
    public final class Item {
        /**
         * Each fill is given an item of its own rather than one that the builder keeps, which the
         * JIT removes when it inlines the fill, knowing that the item's builder is the one that
         * records the table. Recording the benchmarks' vector of nested tables took 4.4 µs with
         * an item kept in a field of the builder, and 3.8 µs so.
         */
        private Item() {}

        public void setInt8(byte value) {
            TableBuilder.this.setInt8(value);
        }

        public void setUint8(short value) {
            TableBuilder.this.setUint8(value);
        }

        public void setInt16(short value) {
            TableBuilder.this.setInt16(value);
        }

        public void setUint16(int value) {
            TableBuilder.this.setUint16(value);
        }

        public void setInt32(int value) {
            TableBuilder.this.setInt32(value);
        }

        public void setUint32(long value) {
            TableBuilder.this.setUint32(value);
        }

        public void setInt64(long value) {
            TableBuilder.this.setInt64(value);
        }

        /** Sets a {@code uint64} from a {@code long} with its bits */
        public void setUint64(long value) {
            TableBuilder.this.setUint64(value);
        }

        public void setFloat32(float value) {
            TableBuilder.this.setFloat32(value);
        }

        public void setFloat64(double value) {
            TableBuilder.this.setFloat64(value);
        }

        public void setInt8Array(byte[] values) {
            TableBuilder.this.setInt8Array(values);
        }

        public void setUint8Array(short[] values) {
            TableBuilder.this.setUint8Array(values);
        }

        public void setInt16Array(short[] values) {
            TableBuilder.this.setInt16Array(values);
        }

        public void setUint16Array(int[] values) {
            TableBuilder.this.setUint16Array(values);
        }

        public void setInt32Array(int[] values) {
            TableBuilder.this.setInt32Array(values);
        }

        public void setUint32Array(long[] values) {
            TableBuilder.this.setUint32Array(values);
        }

        public void setInt64Array(long[] values) {
            TableBuilder.this.setInt64Array(values);
        }

        /** Sets {@code uint64}s from {@code long}s with their bits */
        public void setUint64Array(long[] values) {
            TableBuilder.this.setUint64Array(values);
        }

        public void setFloat32Array(float[] values) {
            TableBuilder.this.setFloat32Array(values);
        }

        public void setFloat64Array(double[] values) {
            TableBuilder.this.setFloat64Array(values);
        }

        public void setString(String string) {
            TableBuilder.this.setString(string);
        }

        public void setStruct(byte[] bytes) {
            TableBuilder.this.setStruct(bytes);
        }

        public void setStructArray(byte[] bytes, int size, int alignment) {
            TableBuilder.this.setStructArray(bytes, size, alignment);
        }

        public void setTable(TableBuilder table) {
            TableBuilder.this.setTable(table);
        }

        public void setTable(TableBuilder table, int alignment) {
            TableBuilder.this.setTable(table, alignment);
        }

        public void setTable(byte[] table) {
            TableBuilder.this.setTable(table);
        }

        public void setTable(byte[] table, int alignment) {
            TableBuilder.this.setTable(table, alignment);
        }

        public void setTable(int fields, Consumer<Item> fill) {
            TableBuilder.this.setTable(fields, fill);
        }

        public void setVector(List<TableBuilder> tables) {
            TableBuilder.this.setVector(tables);
        }

        public void setVector(byte[][] tables) {
            TableBuilder.this.setVector(tables);
        }

        public void setVector(int count, int fields, ObjIntConsumer<Item> fill) {
            TableBuilder.this.setVector(count, fields, fill);
        }
    }
}
