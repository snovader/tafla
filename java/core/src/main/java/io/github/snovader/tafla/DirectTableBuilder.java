package io.github.snovader.tafla;

import org.jspecify.annotations.*;

import java.util.*;
import java.util.function.*;

/**
 * <b>A prototype, not for release:</b> a {@link TableBuilder} that writes each field when it's set,
 * rather than recording it and writing every field in {@link #bytes()}. It takes the same calls,
 * writes the same bytes and throws the same exceptions, from the same setters, which leave the
 * builder as it was. It's here to be measured against {@code TableBuilder}; see the draft pull
 * request that adds it.
 *
 * <p>Fields are written forward into bytes that grow, by copying them into bytes twice as long,
 * as each field's offset is written into its table's header. A vector writes its count and room
 * for its items' ends, each of which is written when its item is done. A table that a fill records
 * in place is written where the parent's fields have reached, unpadded, and moved forward by its
 * padding once its fields are set and its alignment is known. Its offsets count from its own start,
 * so none of them changes.</p>
 *
 * <p>Runs of 256 bytes or more, such as long arrays and strings, aren't
 * copied into the growing bytes. They're kept as segments, which {@link #bytes()} writes into the
 * table, so a large column is copied once, as {@code TableBuilder} copies it, and arrays must not
 * change until then either. Shorter ones are copied when they're set.</p>
 */
public final class DirectTableBuilder {
    /** Runs of at least this many bytes are kept as segments, rather than copied when they're set */
    static final int SEGMENT_LENGTH = 256;

    /**
     * The bytes that a builder starts with: 128, which hold the benchmarks' record of 60 bytes with
     * room to encode its string without counting it first, and the 96 bytes of their document
     * besides its columns. A table longer than that grows them, to twice their length or the
     * field's end, whichever is longer. The prototype reads it from
     * {@code tafla.prototype.initialCapacity}, so that other capacities can be measured.
     */
    private static final int INITIAL_CAPACITY = Integer.getInteger("tafla.prototype.initialCapacity", 128);

    /**
     * Whether a vector in place makes room for its other items once its first is written, each as
     * long as the first, rather than growing as they're written. The prototype reads it from
     * {@code tafla.prototype.reserveItems}, so that growing can be measured.
     */
    private static final boolean RESERVE_ITEMS = !"false".equals(System.getProperty("tafla.prototype.reserveItems"));

    private static final byte[] NO_BYTES = {};
    private static final Segment[] NO_SEGMENTS = {};

    // The types of a segment's values, each written by its Elements setter
    private static final int INT8 = 0;
    private static final int UINT8 = 1;
    private static final int INT16 = 2;
    private static final int UINT16 = 3;
    private static final int INT32 = 4;
    private static final int UINT32 = 5;
    private static final int INT64 = 6;
    private static final int FLOAT32 = 7;
    private static final int FLOAT64 = 8;

    /**
     * A run of {@code byteLength} bytes that goes before byte {@code at} of the builder's bytes:
     * {@code values}, an array of the type that {@code type} names
     */
    private record Segment(int at, int type, Object values, int byteLength) {
        Segment movedBy(int distance) {
            return new Segment(at + distance, type, values, byteLength);
        }
    }

    /** The table's bytes, once written, after which it takes no more fields */
    private byte @Nullable [] written;

    private final TableLayout layout;
    private final int offsetSize;
    private final int maxLength;
    private int segmentLength = SEGMENT_LENGTH;

    /** The table so far, without its segments, and with a sized table's length still 0 */
    private byte[] buffer;
    /** The number of {@link #buffer}'s bytes that hold the table */
    private int length;
    /**
     * The table's length so far, segments included, which is where the next field goes. It may
     * pass an {@code int} while a vector in place records items that make it too long.
     */
    private long position;
    private Segment[] segments = NO_SEGMENTS;
    private int segmentCount;

    // The table whose fields are being set: the builder's own, or one that a fill records in place

    /** Where the table starts, which its offsets count from */
    private long tableStart = 0;
    /** The index in {@link #buffer} of the table's first offset */
    private int slots;
    private int fields;
    private int fieldsSet;
    /**
     * The largest alignment that a field needs from the table's start, which a parent pads this
     * table to when it nests it
     */
    private int alignment = Byte.BYTES;

    /** Builds a table of {@link TableLayout#DEFAULT}, such as one to nest in another */
    public DirectTableBuilder(int fields) {
        this(fields, TableLayout.DEFAULT);
    }

    /** As {@link TableBuilder#TableBuilder(int, TableLayout)}. */
    public DirectTableBuilder(int fields, TableLayout layout) {
        this(fields, layout, false, 0);
    }

    /** As {@link TableBuilder#TableBuilder(int, TableLayout, long)}. */
    public DirectTableBuilder(int fields, TableLayout layout, long version) {
        this(fields, layout, true, version);
    }

    private DirectTableBuilder(int fields, TableLayout layout, boolean versioned, long version) {
        var header = layout.headerLength(fields, versioned, version);
        this.layout = layout;
        this.offsetSize = layout.offsetSize();
        this.maxLength = (int) layout.maxLength();

        this.buffer = new byte[Math.max(INITIAL_CAPACITY, header)];
        this.length = header;
        this.position = header;
        if (versioned) {
            store(buffer, layout.sized() ? offsetSize : 0, offsetSize, version);
        }

        this.slots = layout.headerSize();
        this.fields = fields;
    }

    /**
     * Keeps runs of at least {@code byteLength} bytes as segments, rather than
     * 256, for the prototype's tests and comparison: 1 makes every array a
     * segment, and {@link Integer#MAX_VALUE} copies every one. It applies to the fields set after it.
     *
     * @return this builder
     */
    public DirectTableBuilder segmentsFrom(int byteLength) {
        this.segmentLength = Math.max(byteLength, 1);
        return this;
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

        // Every field checked that the table fits in an array
        written = new byte[(int) position];
        copyInto(written, 0);

        // Release the segments, whose values may be large arrays
        this.buffer = NO_BYTES;
        this.segments = NO_SEGMENTS;
        this.segmentCount = 0;
        this.written = written;

        return written;
    }

    public void setInt8(byte value) {
        var at = startField(0, Byte.BYTES, Byte.BYTES);
        buffer[at] = value;
    }

    public void setUint8(short value) {
        setInt8((byte) value);
    }

    public void setInt16(short value) {
        var at = startField(0, Short.BYTES, Short.BYTES);
        Elements.INT16.set(buffer, at, value);
    }

    public void setUint16(int value) {
        setInt16((short) value);
    }

    public void setInt32(int value) {
        var at = startField(0, Integer.BYTES, Integer.BYTES);
        Elements.INT32.set(buffer, at, value);
    }

    public void setUint32(long value) {
        setInt32((int) value);
    }

    public void setInt64(long value) {
        var at = startField(0, Long.BYTES, Long.BYTES);
        Elements.INT64.set(buffer, at, value);
    }

    /** Sets a {@code uint64} from a {@code long} with its bits */
    public void setUint64(long value) {
        setInt64(value);
    }

    public void setFloat32(float value) {
        var at = startField(0, Float.BYTES, Float.BYTES);
        Elements.FLOAT32.set(buffer, at, value);
    }

    public void setFloat64(double value) {
        var at = startField(0, Double.BYTES, Double.BYTES);
        Elements.FLOAT64.set(buffer, at, value);
    }

    public void setInt8Array(byte[] values) {
        var byteLength = (long) values.length;
        var segment = byteLength >= segmentLength;
        var at = startArray(values.length, byteLength, Byte.BYTES, segment);
        if (segment) {
            addSegment(at, INT8, values, values.length);
        } else {
            Elements.setInt8(buffer, at, values);
        }
    }

    public void setUint8Array(short[] values) {
        var byteLength = (long) values.length;
        var segment = byteLength >= segmentLength;
        var at = startArray(values.length, byteLength, Byte.BYTES, segment);
        if (segment) {
            addSegment(at, UINT8, values, values.length);
        } else {
            Elements.setUint8(buffer, at, values);
        }
    }

    public void setInt16Array(short[] values) {
        var byteLength = (long) values.length * Short.BYTES;
        var segment = byteLength >= segmentLength;
        var at = startArray(values.length, byteLength, Short.BYTES, segment);
        if (segment) {
            addSegment(at, INT16, values, (int) byteLength);
        } else {
            Elements.setInt16(buffer, at, values);
        }
    }

    public void setUint16Array(int[] values) {
        var byteLength = (long) values.length * Short.BYTES;
        var segment = byteLength >= segmentLength;
        var at = startArray(values.length, byteLength, Short.BYTES, segment);
        if (segment) {
            addSegment(at, UINT16, values, (int) byteLength);
        } else {
            Elements.setUint16(buffer, at, values);
        }
    }

    public void setInt32Array(int[] values) {
        var byteLength = (long) values.length * Integer.BYTES;
        var segment = byteLength >= segmentLength;
        var at = startArray(values.length, byteLength, Integer.BYTES, segment);
        if (segment) {
            addSegment(at, INT32, values, (int) byteLength);
        } else {
            Elements.setInt32(buffer, at, values);
        }
    }

    public void setUint32Array(long[] values) {
        var byteLength = (long) values.length * Integer.BYTES;
        var segment = byteLength >= segmentLength;
        var at = startArray(values.length, byteLength, Integer.BYTES, segment);
        if (segment) {
            addSegment(at, UINT32, values, (int) byteLength);
        } else {
            Elements.setUint32(buffer, at, values);
        }
    }

    public void setInt64Array(long[] values) {
        var byteLength = (long) values.length * Long.BYTES;
        var segment = byteLength >= segmentLength;
        var at = startArray(values.length, byteLength, Long.BYTES, segment);
        if (segment) {
            addSegment(at, INT64, values, (int) byteLength);
        } else {
            Elements.setInt64(buffer, at, values);
        }
    }

    /** Sets {@code uint64}s from {@code long}s with their bits */
    public void setUint64Array(long[] values) {
        setInt64Array(values);
    }

    public void setFloat32Array(float[] values) {
        var byteLength = (long) values.length * Float.BYTES;
        var segment = byteLength >= segmentLength;
        var at = startArray(values.length, byteLength, Float.BYTES, segment);
        if (segment) {
            addSegment(at, FLOAT32, values, (int) byteLength);
        } else {
            Elements.setFloat32(buffer, at, values);
        }
    }

    public void setFloat64Array(double[] values) {
        var byteLength = (long) values.length * Double.BYTES;
        var segment = byteLength >= segmentLength;
        var at = startArray(values.length, byteLength, Double.BYTES, segment);
        if (segment) {
            addSegment(at, FLOAT64, values, (int) byteLength);
        } else {
            Elements.setFloat64(buffer, at, values);
        }
    }

    /** Writes {@code string} as UTF-8, with lone surrogates replaced by U+FFFD */
    public void setString(String string) {
        // A short string is encoded straight into the bytes, and a long one is encoded once, as
        // TableBuilder encodes it, and copied or kept as a segment
        if (string.length() < Utf8.SHORT_LENGTH) {
            setShortString(string);
        } else {
            setBytes(Utf8.encode(string), true);
        }
    }

    /** As {@link TableBuilder#setStruct(byte[])}. */
    public void setStruct(byte[] bytes) {
        setBytes(bytes, false);
    }

    /** As {@link TableBuilder#setStructArray(byte[], int, int)}. */
    public void setStructArray(byte[] bytes, int size, int alignment) {
        var count = TableWriter.structCount(bytes.length, size);

        var segment = bytes.length >= segmentLength;
        var at = startArray(count, bytes.length, alignment, segment);
        if (segment) {
            addSegment(at, INT8, bytes, bytes.length);
        } else {
            System.arraycopy(bytes, 0, buffer, at, bytes.length);
        }
    }

    /** As {@link TableBuilder#setTable(TableBuilder)}. */
    public void setTable(DirectTableBuilder table) {
        setTable(table, table.alignment);
    }

    /**
     * Copies a table from its builder, as {@link TableBuilder#setTable(TableBuilder, int)} nests
     * one: its bytes, and its segments, which aren't copied until this table is written. The
     * builder can still be written itself or nested elsewhere too, since a complete builder
     * doesn't change.
     */
    public void setTable(DirectTableBuilder table, int alignment) {
        var padding = TableLayout.padding(position - tableStart, alignment);
        var tableLength = lengthOf(table);

        var at = startField(padding, tableLength, bufferLengthOf(table));
        append(table, at);
        alignTo(alignment);
    }

    /** As {@link TableBuilder#setTable(byte[])}. */
    public void setTable(byte[] table) {
        setTable(table, Integer.BYTES);
    }

    /** As {@link TableBuilder#setTable(byte[], int)}. */
    public void setTable(byte[] table, int alignment) {
        var padding = TableLayout.padding(position - tableStart, alignment);

        var segment = table.length >= segmentLength;
        var at = startField(padding, table.length, segment ? 0 : table.length);
        append(table, at, segment);
        alignTo(alignment);
    }

    /** As {@link TableBuilder#setVector(List)}. */
    public void setVector(List<DirectTableBuilder> tables) {
        var items = tables.toArray(new DirectTableBuilder[0]);

        var slotBytes = (items.length + 1L) * offsetSize;
        var byteLength = slotBytes;
        var bufferBytes = slotBytes;
        for (var item : items) {
            byteLength += lengthOf(item);
            bufferBytes += bufferLengthOf(item);
        }

        var vector = startField(0, byteLength, bufferBytes);
        store(buffer, vector, offsetSize, items.length);

        var at = vector + (int) slotBytes;
        var end = slotBytes;
        for (int i = 0; i < items.length; i++) {
            var item = items[i];
            append(item, at);

            at += bufferLengthOf(item);
            end += item.position;
            store(buffer, vector + (i + 1) * offsetSize, offsetSize, end);
        }
    }

    /** As {@link TableBuilder#setVector(byte[][])}. */
    public void setVector(byte[][] tables) {
        var items = tables.clone();

        var slotBytes = (items.length + 1L) * offsetSize;
        var byteLength = slotBytes;
        var bufferBytes = slotBytes;
        for (var item : items) {
            byteLength += item.length;
            bufferBytes += item.length >= segmentLength ? 0 : item.length;
        }

        var vector = startField(0, byteLength, bufferBytes);
        store(buffer, vector, offsetSize, items.length);

        var at = vector + (int) slotBytes;
        var end = slotBytes;
        for (int i = 0; i < items.length; i++) {
            var item = items[i];
            var segment = item.length >= segmentLength;
            append(item, at, segment);

            at += segment ? 0 : item.length;
            end += item.length;
            store(buffer, vector + (i + 1) * offsetSize, offsetSize, end);
        }
    }

    /**
     * Writes a table of {@code fields} fields nested in this one, whose fields {@code fill} sets on
     * the {@link Item} that it's given, as {@link TableBuilder#setTable(int, Consumer)} records
     * one. It's written in place, and then moved forward by its padding.
     */
    public void setTable(int fields, Consumer<Item> fill) {
        var index = nextField();
        var header = headerOf(fields);

        // What the table and its parent restore if the fill throws
        var parentStart = tableStart;
        var parentSlots = slots;
        var parentFields = this.fields;
        var parentAlignment = alignment;
        var startLength = length;
        var start = position;
        var startSegments = segmentCount;
        try {
            startInPlace(fields, header);
            fill.accept(new Item());
            checkComplete();

            var tableAlignment = alignment;
            endInPlace(parentStart, parentSlots, parentFields, index, parentAlignment);
            nestInPlace(index, start, startLength, startSegments, tableAlignment);
        } catch (Throwable failure) {
            endInPlace(parentStart, parentSlots, parentFields, index, parentAlignment);
            truncate(startLength, start, startSegments);
            throw failure;
        }
    }

    /**
     * Writes a vector of {@code count} tables of {@code fields} fields each, as
     * {@link TableBuilder#setVector(int, int, ObjIntConsumer)} records one: {@code fill} sets the
     * fields of each item, which are written in place, one after another.
     */
    public void setVector(int count, int fields, ObjIntConsumer<Item> fill) {
        var index = nextField();
        var header = headerOf(fields);

        var slotBytes = (TableSizer.checkCount(count) + 1L) * offsetSize;
        var offset = position - tableStart;
        var fewest = offset + slotBytes + (long) count * header;
        if (fewest > maxLength) {
            throw tooLong(index, fewest);
        }

        var parentStart = tableStart;
        var parentSlots = slots;
        var parentFields = this.fields;
        var parentAlignment = alignment;
        var vector = length;
        var start = position;
        var startSegments = segmentCount;
        try {
            startVector(count, (int) slotBytes);

            // The first item, after which the others have room as long as it, if the table does
            var item = new Item();
            if (count > 0) {
                var itemLength = length;
                writeItem(item, 0, fill, fields, header, vector, start);
                if (RESERVE_ITEMS) {
                    reserveItems(count - 1, length - itemLength, parentStart);
                }
            }
            for (int i = 1; i < count; i++) {
                writeItem(item, i, fill, fields, header, vector, start);
            }
            endInPlace(parentStart, parentSlots, parentFields, index, parentAlignment);

            var end = offset + (position - start);
            if (end > maxLength) {
                throw tooLong(index, end);
            }

            setOffset(index, offset);
            fieldsSet = index + 1;
        } catch (Throwable failure) {
            endInPlace(parentStart, parentSlots, parentFields, index, parentAlignment);
            truncate(vector, start, startSegments);
            throw failure;
        }
    }

    /** Writes a vector's count, and makes room for its items' ends */
    private void startVector(int count, int slotBytes) {
        ensure(slotBytes);
        store(buffer, length, offsetSize, count);
        length += slotBytes;
        position += slotBytes;
    }

    /**
     * Writes item {@code i} of a vector of tables in place, whose count is at {@code vector} in the
     * bytes, and {@code start} in the table, and then its end
     */
    private void writeItem(
        Item item,
        int i,
        ObjIntConsumer<Item> fill,
        int fields,
        int header,
        int vector,
        long start
    ) {
        startInPlace(fields, header);
        fill.accept(item, i);
        checkComplete();

        store(buffer, vector + (i + 1) * offsetSize, offsetSize, position - start);
    }

    /**
     * Checks a field of {@code byteLength} bytes after {@code padding} bytes, of which the first
     * {@code bufferBytes} go in the builder's bytes, and the rest in segments that its setter adds.
     * Then writes the padding and the field's offset, makes room for those bytes, and moves past
     * them, so the setter writes the field's bytes {@return from where the padding ends}.
     *
     * @throws IllegalStateException if the table is already written, or holds all its fields
     * @throws IllegalArgumentException if the field would make the table longer than the layout
     *     addresses
     */
    private int startField(int padding, long byteLength, long bufferBytes) {
        var index = nextField();

        var offset = position - tableStart + padding;
        var end = offset + byteLength;
        if (end > maxLength) {
            throw tooLong(index, end);
        }

        // The field fits in the table, which fits in an array
        var room = padding + (int) bufferBytes;
        ensure(room);

        var at = length;
        if (padding != 0) {
            Arrays.fill(buffer, at, at + padding, (byte) 0);
        }
        setOffset(index, offset);
        fieldsSet = index + 1;

        length = at + room;
        position += room;

        return at + padding;
    }

    /**
     * Starts an array of {@code byteLength} bytes of {@code count} elements, padded so that they
     * start at a multiple of {@code alignment}, after their count, {@return where they go}, which is
     * the end of the builder's bytes when they're a segment
     *
     * @throws IllegalArgumentException if the alignment isn't 1, 2, 4 or 8, or as
     *     {@link #startField} throws
     * @throws IllegalStateException as {@link #startField} throws
     */
    private int startArray(int count, long byteLength, int alignment, boolean segment) {
        var padding = TableLayout.padding(position - tableStart + offsetSize, alignment);
        var bufferBytes = offsetSize + (segment ? 0 : byteLength);

        var at = startField(padding, offsetSize + byteLength, bufferBytes);
        store(buffer, at, offsetSize, count);
        alignTo(alignment);

        return at + offsetSize;
    }

    /**
     * Writes a string of fewer than {@link Utf8#SHORT_LENGTH} {@code char}s, encoding it into the
     * room after the bytes, and keeping it only if the table holds it
     */
    private void setShortString(String string) {
        var index = nextField();
        var offset = position - tableStart;

        // Each char takes at most 3 bytes; if that doesn't fit, the string is counted, so that
        // the bytes grow only as far as they need to
        if (offsetSize + 3 * string.length() > buffer.length - length) {
            ensure(offsetSize + (int) Utf8.length(string));
        }
        var at = length;
        var utf8Length = Utf8.encodeInto(string, buffer, at + offsetSize);

        var byteLength = offsetSize + utf8Length;
        var end = offset + byteLength;
        if (end > maxLength) {
            throw tooLong(index, end);
        }

        setOffset(index, offset);
        store(buffer, at, offsetSize, utf8Length);
        fieldsSet = index + 1;

        length = at + byteLength;
        position += byteLength;
    }

    /** Writes {@code bytes}, after their length if {@code counted}, copied or as a segment */
    private void setBytes(byte[] bytes, boolean counted) {
        var head = counted ? offsetSize : 0;
        var segment = bytes.length >= segmentLength;

        var at = startField(0, head + (long) bytes.length, head + (segment ? 0 : bytes.length));
        if (counted) {
            store(buffer, at, offsetSize, bytes.length);
        }
        append(bytes, at + head, segment);
    }

    /** Copies {@code bytes} to {@code at}, or adds them as a segment there */
    private void append(byte[] bytes, int at, boolean segment) {
        if (segment) {
            addSegment(at, INT8, bytes, bytes.length);
        } else {
            System.arraycopy(bytes, 0, buffer, at, bytes.length);
        }
    }

    /**
     * Copies a nested builder's bytes to {@code at}, which has room for them, with a sized
     * table's length, and adds its segments
     */
    private void append(DirectTableBuilder table, int at) {
        var tableBytes = table.written;
        if (tableBytes != null) {
            append(tableBytes, at, tableBytes.length >= segmentLength);
            return;
        }

        System.arraycopy(table.buffer, 0, buffer, at, table.length);
        if (table.layout.sized()) {
            store(buffer, at, table.offsetSize, table.position);
        }

        for (int i = 0; i < table.segmentCount; i++) {
            var segment = table.segments[i];
            addSegment(at + segment.at(), segment.type(), segment.values(), segment.byteLength());
        }
    }

    /** {@return the number of a nested builder's bytes that go in this builder's bytes} */
    private int bufferLengthOf(DirectTableBuilder table) {
        var tableBytes = table.written;
        if (tableBytes != null) {
            return tableBytes.length >= segmentLength ? 0 : tableBytes.length;
        }

        return table.length;
    }

    /**
     * Adds a segment of {@code byteLength} bytes before byte {@code at}, the end of the builder's
     * bytes so far, which a setter checked
     */
    private void addSegment(int at, int type, Object values, int byteLength) {
        if (segmentCount == segments.length) {
            segments = Arrays.copyOf(segments, Math.max(4, 2 * segments.length));
        }

        segments[segmentCount++] = new Segment(at, type, values, byteLength);
        position += byteLength;
    }

    /** Writes the table, with its segments and a sized table's length, into {@code bytes} at {@code at} */
    private void copyInto(byte[] bytes, int at) {
        var from = 0;
        var to = at;
        for (int i = 0; i < segmentCount; i++) {
            var segment = segments[i];
            var run = segment.at() - from;
            System.arraycopy(buffer, from, bytes, to, run);
            from = segment.at();
            to += run;

            writeSegment(segment, bytes, to);
            to += segment.byteLength();
        }
        System.arraycopy(buffer, from, bytes, to, length - from);

        if (layout.sized()) {
            store(bytes, at, offsetSize, position);
        }
    }

    private static void writeSegment(Segment segment, byte[] bytes, int at) {
        var values = segment.values();
        switch (segment.type()) {
            case INT8 -> Elements.setInt8(bytes, at, (byte[]) values);
            case UINT8 -> Elements.setUint8(bytes, at, (short[]) values);
            case INT16 -> Elements.setInt16(bytes, at, (short[]) values);
            case UINT16 -> Elements.setUint16(bytes, at, (int[]) values);
            case INT32 -> Elements.setInt32(bytes, at, (int[]) values);
            case UINT32 -> Elements.setUint32(bytes, at, (long[]) values);
            case INT64 -> Elements.setInt64(bytes, at, (long[]) values);
            case FLOAT32 -> Elements.setFloat32(bytes, at, (float[]) values);
            case FLOAT64 -> Elements.setFloat64(bytes, at, (double[]) values);
            default -> throw new AssertionError("A segment of type " + segment.type());
        }
    }

    /**
     * Starts the fields of a table written in place, at the end of the bytes: {@code fields}
     * fields after offsets of {@code header} bytes
     */
    private void startInPlace(int fields, int header) {
        ensure(header);

        tableStart = position;
        slots = length;
        this.fields = fields;
        fieldsSet = 0;
        alignment = Byte.BYTES;

        length += header;
        position += header;
    }

    /** Goes back to setting field {@code index} of the parent of a table written in place */
    private void endInPlace(long parentStart, int parentSlots, int parentFields, int index, int parentAlignment) {
        tableStart = parentStart;
        slots = parentSlots;
        fields = parentFields;
        fieldsSet = index;
        alignment = parentAlignment;
    }

    /**
     * Nests the table written in place from {@code start}, which is {@code startLength} in the
     * bytes, as field {@code index}, padded as a nested builder is, and moved by its padding
     *
     * @throws IllegalArgumentException if it makes its parent too long
     */
    private void nestInPlace(int index, long start, int startLength, int startSegments, int tableAlignment) {
        var padding = TableLayout.padding(start - tableStart, tableAlignment);
        var offset = start - tableStart + padding;
        var end = offset + (position - start);
        if (end > maxLength) {
            throw tooLong(index, end);
        }

        if (padding != 0) {
            moveBy(padding, startLength, startSegments);
        }

        setOffset(index, offset);
        fieldsSet = index + 1;
        alignTo(tableAlignment);
    }

    /**
     * Moves the bytes from {@code startLength}, and the segments from {@code startSegments}, forward
     * by {@code distance} bytes, which are then 0
     */
    private void moveBy(int distance, int startLength, int startSegments) {
        ensure(distance);

        System.arraycopy(buffer, startLength, buffer, startLength + distance, length - startLength);
        Arrays.fill(buffer, startLength, startLength + distance, (byte) 0);
        for (int i = startSegments; i < segmentCount; i++) {
            segments[i] = segments[i].movedBy(distance);
        }

        length += distance;
        position += distance;
    }

    /**
     * Makes room for {@code count} more items of a vector of tables in place, each of
     * {@code itemLength} bytes, as long as its first, up to what the table can address
     */
    private void reserveItems(int count, int itemLength, long parentStart) {
        var room = maxLength - (position - parentStart);
        ensure((int) Math.max(0, Math.min((long) count * itemLength, room)));
    }

    /** Undoes what a fill wrote, from where it started */
    private void truncate(int length, long position, int segmentCount) {
        this.length = length;
        this.position = position;

        Arrays.fill(segments, segmentCount, this.segmentCount, null);
        this.segmentCount = segmentCount;
    }

    /** Makes room for {@code byteLength} more bytes */
    private void ensure(int byteLength) {
        if (byteLength > buffer.length - length) {
            grow(byteLength);
        }
    }

    /** Copies the bytes into bytes with room for {@code byteLength} more, and twice as long or more */
    private void grow(int byteLength) {
        var needed = (long) length + byteLength;
        if (needed > TableLayout.MAX_ARRAY_LENGTH) {
            throw new OutOfMemoryError("The table's bytes would be longer than an array can be: " + needed);
        }

        var doubled = Math.min(2L * buffer.length, TableLayout.MAX_ARRAY_LENGTH);
        var grown = new byte[(int) Math.max(needed, doubled)];
        System.arraycopy(buffer, 0, grown, 0, length);
        buffer = grown;
    }

    private void setOffset(int index, long offset) {
        store(buffer, slots + index * offsetSize, offsetSize, offset);
    }

    /** Writes an offset, length, count or header slot of {@code offsetSize} bytes */
    private static void store(byte[] bytes, int at, int offsetSize, long value) {
        switch (offsetSize) {
            case Byte.BYTES -> bytes[at] = (byte) value;
            case Short.BYTES -> Elements.INT16.set(bytes, at, (short) value);
            default -> Elements.INT32.set(bytes, at, (int) value);
        }
    }

    private void alignTo(int alignment) {
        this.alignment = Math.max(this.alignment, alignment);
    }

    /**
     * The byte length of a table to nest, which a builder knows once all its fields are set
     *
     * @throws IllegalArgumentException if the builder has fields left to set
     */
    private static long lengthOf(DirectTableBuilder table) {
        if (table.fieldsSet != table.fields) {
            throw new IllegalArgumentException(incomplete(table.fieldsSet, table.fields));
        }

        return table.position;
    }

    private static String incomplete(int fieldsSet, int fields) {
        return String.format("Only %d of the nested table's %d fields were set", fieldsSet, fields);
    }

    /**
     * {@return the byte length of the offsets of a table of {@code fields} fields written in
     * place}, which has this table's offset type and no header
     *
     * @throws IllegalArgumentException if the number of fields is negative, or their offsets don't
     *     fit in the offset type
     */
    private int headerOf(int fields) {
        return TableLayout.headerless(layout.offsetType()).headerLength(fields, false, 0);
    }

    /** @throws IllegalArgumentException if the table written in place has fields left to set */
    private void checkComplete() {
        if (fieldsSet != fields) {
            throw new IllegalArgumentException(incomplete(fieldsSet, fields));
        }
    }

    /**
     * {@return the index of the next field of the table being written}
     *
     * @throws IllegalStateException if the table is already written, or holds all its fields
     */
    private int nextField() {
        var index = fieldsSet;
        if (index == fields || written != null) {
            throw full();
        }

        return index;
    }

    /** {@return why the table takes no more fields}: it's written, or holds all its fields */
    private IllegalStateException full() {
        if (written != null) {
            return new IllegalStateException("The table was already written");
        }

        return new IllegalStateException(String.format("Can't add field %d to a table of %d", fields + 1, fields));
    }

    private IllegalArgumentException tooLong(int index, long length) {
        if (maxLength == TableLayout.MAX_ARRAY_LENGTH) {
            return new IllegalArgumentException(String.format(
                "Field %d makes the table %d bytes, longer than a Java array can be",
                index + 1,
                length
            ));
        }

        return new IllegalArgumentException(String.format(
            "Field %d makes the table %d bytes, too long for %s offsets",
            index + 1,
            length,
            layout.offsetType()
        ));
    }

    /**
     * The fields of a table that a {@code DirectTableBuilder} writes in place, as
     * {@link TableBuilder.Item} sets them
     */
    public final class Item {
        /** As {@link TableBuilder.Item}, each fill is given an item of its own */
        private Item() {}

        public void setInt8(byte value) {
            DirectTableBuilder.this.setInt8(value);
        }

        public void setUint8(short value) {
            DirectTableBuilder.this.setUint8(value);
        }

        public void setInt16(short value) {
            DirectTableBuilder.this.setInt16(value);
        }

        public void setUint16(int value) {
            DirectTableBuilder.this.setUint16(value);
        }

        public void setInt32(int value) {
            DirectTableBuilder.this.setInt32(value);
        }

        public void setUint32(long value) {
            DirectTableBuilder.this.setUint32(value);
        }

        public void setInt64(long value) {
            DirectTableBuilder.this.setInt64(value);
        }

        public void setUint64(long value) {
            DirectTableBuilder.this.setUint64(value);
        }

        public void setFloat32(float value) {
            DirectTableBuilder.this.setFloat32(value);
        }

        public void setFloat64(double value) {
            DirectTableBuilder.this.setFloat64(value);
        }

        public void setInt8Array(byte[] values) {
            DirectTableBuilder.this.setInt8Array(values);
        }

        public void setUint8Array(short[] values) {
            DirectTableBuilder.this.setUint8Array(values);
        }

        public void setInt16Array(short[] values) {
            DirectTableBuilder.this.setInt16Array(values);
        }

        public void setUint16Array(int[] values) {
            DirectTableBuilder.this.setUint16Array(values);
        }

        public void setInt32Array(int[] values) {
            DirectTableBuilder.this.setInt32Array(values);
        }

        public void setUint32Array(long[] values) {
            DirectTableBuilder.this.setUint32Array(values);
        }

        public void setInt64Array(long[] values) {
            DirectTableBuilder.this.setInt64Array(values);
        }

        public void setUint64Array(long[] values) {
            DirectTableBuilder.this.setUint64Array(values);
        }

        public void setFloat32Array(float[] values) {
            DirectTableBuilder.this.setFloat32Array(values);
        }

        public void setFloat64Array(double[] values) {
            DirectTableBuilder.this.setFloat64Array(values);
        }

        public void setString(String string) {
            DirectTableBuilder.this.setString(string);
        }

        public void setStruct(byte[] bytes) {
            DirectTableBuilder.this.setStruct(bytes);
        }

        public void setStructArray(byte[] bytes, int size, int alignment) {
            DirectTableBuilder.this.setStructArray(bytes, size, alignment);
        }

        public void setTable(DirectTableBuilder table) {
            DirectTableBuilder.this.setTable(table);
        }

        public void setTable(DirectTableBuilder table, int alignment) {
            DirectTableBuilder.this.setTable(table, alignment);
        }

        public void setTable(byte[] table) {
            DirectTableBuilder.this.setTable(table);
        }

        public void setTable(byte[] table, int alignment) {
            DirectTableBuilder.this.setTable(table, alignment);
        }

        public void setTable(int fields, Consumer<Item> fill) {
            DirectTableBuilder.this.setTable(fields, fill);
        }

        public void setVector(List<DirectTableBuilder> tables) {
            DirectTableBuilder.this.setVector(tables);
        }

        public void setVector(byte[][] tables) {
            DirectTableBuilder.this.setVector(tables);
        }

        public void setVector(int count, int fields, ObjIntConsumer<Item> fill) {
            DirectTableBuilder.this.setVector(count, fields, fill);
        }
    }
}
