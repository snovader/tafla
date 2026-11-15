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
 * <p>Setting a field writes it, and its offset, at once, after the fields before it, into bytes
 * that grow to twice their length when a field doesn't fit. {@link #bytes()} then copies them into
 * an array of the table's exact length. Arrays, strings, structs and nested tables of 256 bytes or
 * more aren't copied when they're set: {@code bytes()} copies them straight into the table, so
 * array elements must not change in between. A nested builder is copied into this table when it's
 * set, apart from its own runs of 256 bytes or more, which are copied when this table is written.
 * {@link #setTable(int, Consumer)} and {@link #setVector(int, int, ObjIntConsumer)} write a nested
 * table, or each item of a vector, in place among this table's own fields instead, without a
 * builder of its own.</p>
 *
 * <p>A setter throws, and leaves the builder as it was, when the table already holds all its
 * fields, or when its field would make the table longer than the layout's offset type addresses:
 * 255 bytes, 64 KiB, or for {@code uint32} offsets, as long as a Java array can be.</p>
 */
public final class TableBuilder {
    /**
     * Runs of at least this many bytes, such as long arrays and strings, are kept as segments that
     * {@link #bytes()} copies into the table, rather than copied when they're set and again when
     * the table is written. So a large column is copied once, and shorter runs, whose copies cost
     * less than keeping them, are copied when they're set. Copying every run when it's set made the
     * benchmarks' document of 100,000 rows take 3.3 times as long.
     */
    private static final int SEGMENT_LENGTH = 256;

    /**
     * The bytes that a builder starts with: 128, which hold the benchmarks' record of 60 bytes with
     * room to encode its string without counting it first, and the 96 bytes of their document
     * besides its columns. With 64, the record took 26 ns rather than 20, and the document grew its
     * bytes.
     */
    private static final int INITIAL_CAPACITY = 128;

    /**
     * The most room that a vector in place makes for its other items once its first is written,
     * each as long as the first, so that a long first item doesn't make room for many more of it
     * than the table holds
     */
    private static final int MAX_ITEM_ROOM = 64 * 1024;

    /** The segments that a builder makes room for with its first */
    private static final int INITIAL_SEGMENTS = 4;

    private static final String WRITING_IN_PLACE = "The table can't be written while a fill writes a table in it";

    private static final byte[] NO_BYTES = {};
    /** The values of a segment that isn't set */
    private static final Object NO_VALUES = NO_BYTES;
    private static final Object[] NO_OBJECTS = {};
    private static final long[] NO_PLACES = {};

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

    /** The table's bytes, once written, after which it takes no more fields */
    private byte @Nullable [] written;

    private final TableLayout layout;
    /** The width in bytes of offsets, lengths and header slots */
    private final int offsetSize;
    /**
     * The longest table that the layout's offsets and lengths address, which a Java array's length
     * limits to an {@code int}
     */
    private final int maxLength;

    /** The table so far, without its segments, and with a sized table's length still 0 */
    private byte[] buffer;
    /** The number of {@link #buffer}'s bytes that hold the table */
    private int length;
    /**
     * The table's length so far, segments included, which is where the next field goes. It may
     * pass an {@code int} while a vector in place writes items that make it too long.
     */
    private long position;

    // The segments: runs that go between the buffer's bytes, each an array of values of a type, and
    // its place, which holds the index in the buffer that it goes before in its low 32 bits, and its
    // type in the high ones. Keeping the first 4 in fields, which escape analysis removes along with
    // the builder, took as long on every benchmark row, and saved 80 bytes on those with segments.

    private int segmentCount;
    private Object[] segmentValues = NO_OBJECTS;
    private long[] segmentPlaces = NO_PLACES;

    // The table whose fields are being set: the builder's own, or one that a fill writes in place

    /** Where the table starts, which its offsets count from */
    private long tableStart = 0;
    /** The index in {@link #buffer} of the table's first offset */
    private int slots;
    /** The number of fields that the table holds */
    private int fields;
    private int fieldsSet;
    /**
     * The largest alignment that a field needs from the table's start, which a parent pads this
     * table to when it nests it
     */
    private int alignment = Byte.BYTES;

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
            throw new IllegalStateException(WRITING_IN_PLACE);
        }
        if (fieldsSet != fields) {
            throw new IllegalStateException(
                String.format("Only %d of the table's %d fields were set", fieldsSet, fields)
            );
        }

        // Every field checked that the table fits in an array
        written = new byte[(int) position];
        copyInto(written, 0);

        // Release the bytes, and the segments, whose values may be large arrays
        this.buffer = NO_BYTES;
        clearSegments(0);
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
        var segment = byteLength >= SEGMENT_LENGTH;
        var at = startArray(values.length, byteLength, Byte.BYTES, segment);
        if (segment) {
            addSegment(at, INT8, values, values.length);
        } else {
            Elements.setInt8(buffer, at, values);
        }
    }

    public void setUint8Array(short[] values) {
        var byteLength = (long) values.length;
        var segment = byteLength >= SEGMENT_LENGTH;
        var at = startArray(values.length, byteLength, Byte.BYTES, segment);
        if (segment) {
            addSegment(at, UINT8, values, values.length);
        } else {
            Elements.setUint8(buffer, at, values);
        }
    }

    public void setInt16Array(short[] values) {
        var byteLength = (long) values.length * Short.BYTES;
        var segment = byteLength >= SEGMENT_LENGTH;
        var at = startArray(values.length, byteLength, Short.BYTES, segment);
        if (segment) {
            addSegment(at, INT16, values, (int) byteLength);
        } else {
            Elements.setInt16(buffer, at, values);
        }
    }

    public void setUint16Array(int[] values) {
        var byteLength = (long) values.length * Short.BYTES;
        var segment = byteLength >= SEGMENT_LENGTH;
        var at = startArray(values.length, byteLength, Short.BYTES, segment);
        if (segment) {
            addSegment(at, UINT16, values, (int) byteLength);
        } else {
            Elements.setUint16(buffer, at, values);
        }
    }

    public void setInt32Array(int[] values) {
        var byteLength = (long) values.length * Integer.BYTES;
        var segment = byteLength >= SEGMENT_LENGTH;
        var at = startArray(values.length, byteLength, Integer.BYTES, segment);
        if (segment) {
            addSegment(at, INT32, values, (int) byteLength);
        } else {
            Elements.setInt32(buffer, at, values);
        }
    }

    public void setUint32Array(long[] values) {
        var byteLength = (long) values.length * Integer.BYTES;
        var segment = byteLength >= SEGMENT_LENGTH;
        var at = startArray(values.length, byteLength, Integer.BYTES, segment);
        if (segment) {
            addSegment(at, UINT32, values, (int) byteLength);
        } else {
            Elements.setUint32(buffer, at, values);
        }
    }

    public void setInt64Array(long[] values) {
        var byteLength = (long) values.length * Long.BYTES;
        var segment = byteLength >= SEGMENT_LENGTH;
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
        var segment = byteLength >= SEGMENT_LENGTH;
        var at = startArray(values.length, byteLength, Float.BYTES, segment);
        if (segment) {
            addSegment(at, FLOAT32, values, (int) byteLength);
        } else {
            Elements.setFloat32(buffer, at, values);
        }
    }

    public void setFloat64Array(double[] values) {
        var byteLength = (long) values.length * Double.BYTES;
        var segment = byteLength >= SEGMENT_LENGTH;
        var at = startArray(values.length, byteLength, Double.BYTES, segment);
        if (segment) {
            addSegment(at, FLOAT64, values, (int) byteLength);
        } else {
            Elements.setFloat64(buffer, at, values);
        }
    }

    /** Writes {@code string} as UTF-8, with lone surrogates replaced by U+FFFD */
    public void setString(String string) {
        // As the sizer adds strings: a short one is encoded straight into the bytes, and a long one
        // is encoded once, faster than counting it, and then copied, or kept as a segment
        if (string.length() < Utf8.SHORT_LENGTH) {
            setShortString(string);
        } else {
            setBytes(Utf8.encode(string), true);
        }
    }

    /**
     * Sets a struct field from its bytes: its scalars, little-endian, one after another, which are
     * written as they are, without padding. {@link ByteBuffer#getStruct} reads them back.
     */
    public void setStruct(byte[] bytes) {
        setBytes(bytes, false);
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
        var count = TableWriter.structCount(bytes.length, size);

        var segment = bytes.length >= SEGMENT_LENGTH;
        var at = startArray(count, bytes.length, alignment, segment);
        if (segment) {
            addSegment(at, INT8, bytes, bytes.length);
        } else {
            System.arraycopy(bytes, 0, buffer, at, bytes.length);
        }
    }

    /**
     * Embeds a table from its builder, padded to the largest alignment of its arrays, arrays of
     * structs and nested tables, or not at all if it has none of them.
     *
     * @throws IllegalArgumentException if the builder has fields left to set, or a fill is writing
     *     a table in it
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
     * <p>The builder must have all its fields set. It's copied into this table now, apart from its
     * runs of 256 bytes or more, which {@link #bytes()} copies straight into this table, so its
     * arrays must not change until then either. It can still be written itself or nested elsewhere
     * too, since a complete builder doesn't change.</p>
     *
     * @param alignment what the table's start is padded to a multiple of: 1, 2, 4 or 8
     * @throws IllegalArgumentException if the builder has fields left to set, or a fill is writing
     *     a table in it
     */
    public void setTable(TableBuilder table, int alignment) {
        var padding = TableLayout.padding(position - tableStart, alignment);
        var tableLength = lengthOf(table);

        var at = startField(padding, tableLength, bufferLengthOf(table));
        append(table, at);
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
        var padding = TableLayout.padding(position - tableStart, alignment);

        var segment = table.length >= SEGMENT_LENGTH;
        var at = startField(padding, table.length, segment ? 0 : table.length);
        append(table, at, segment);
        alignTo(alignment);
    }

    /**
     * Writes a vector of tables from their builders, after the end of each, which lets readers find
     * any item without reading the others. Builders must have all their fields set, and are copied
     * into this table as {@link #setTable(TableBuilder)} copies them. Items aren't padded.
     *
     * @throws IllegalArgumentException if a builder has fields left to set, or a fill is writing a
     *     table in it
     */
    public void setVector(List<TableBuilder> tables) {
        var items = tables.toArray(new TableBuilder[0]);

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

    /** Writes a vector of tables from their bytes, after the end of each */
    public void setVector(byte[][] tables) {
        // A copy, so that the items can't change before the table is written
        var items = tables.clone();

        var slotBytes = (items.length + 1L) * offsetSize;
        var byteLength = slotBytes;
        var bufferBytes = slotBytes;
        for (var item : items) {
            byteLength += item.length;
            bufferBytes += item.length >= SEGMENT_LENGTH ? 0 : item.length;
        }

        var vector = startField(0, byteLength, bufferBytes);
        store(buffer, vector, offsetSize, items.length);

        var at = vector + (int) slotBytes;
        var end = slotBytes;
        for (int i = 0; i < items.length; i++) {
            var item = items[i];
            var segment = item.length >= SEGMENT_LENGTH;
            append(item, at, segment);

            at += segment ? 0 : item.length;
            end += item.length;
            store(buffer, vector + (i + 1) * offsetSize, offsetSize, end);
        }
    }

    /**
     * Writes a table of {@code fields} fields nested in this one, as a nested builder would be, but
     * without a builder of its own: {@code fill} sets its fields, in order, on the {@link Item} that
     * it's given, which writes each straight among this table's fields. The nested table has this
     * table's offset type and no header, and is padded to start at a multiple of its largest
     * alignment, as {@link #setTable(TableBuilder)} pads a builder, so it's written as a nested
     * builder of that layout is.
     *
     * <p>While {@code fill} runs, this builder writes the nested table's fields rather than its
     * own, so it's used only through the item. What {@code fill} throws is passed on, and whatever
     * is thrown, the builder is left as it was.</p>
     *
     * @throws IllegalArgumentException if the number of fields is negative or their offsets don't fit
     *     in the offset type, if {@code fill} leaves fields unset, or as the setters do
     * @throws IllegalStateException as the setters do
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
     * {@link #setTable(int, Consumer)} writes one: {@code fill} sets the fields of each item, in
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
     * fields, are checked before any item is written. A vector that they alone make too long fails
     * without calling {@code fill}, as a vector of as many tables of their offsets' bytes would.
     * Once the items make the vector longer than the table can address, the rest are still filled
     * and counted, so that the exception gives the vector's whole length, but not kept. What
     * {@code fill} throws is passed on, and whatever is thrown, the builder is left as it was.</p>
     *
     * @throws IllegalArgumentException if the count or the number of fields is negative, if an
     *     item's offsets don't fit in the offset type, if the vector would make the table too long
     *     for it, if {@code fill} leaves an item's fields unset, or as the setters do
     * @throws IllegalStateException as the setters do
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
            var counted = writeItems(count, fields, fill, header, vector, start, parentStart + maxLength);
            endInPlace(parentStart, parentSlots, parentFields, index, parentAlignment);

            var end = offset + (position - start) + counted;
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
     * Writes the {@code count} items of a vector of tables in place, whose count is at
     * {@code vector} in the bytes, and {@code start} in the table, {@return the bytes of the items
     * that it counted but didn't keep}. The first item is written before the others, so that room
     * for them is made outside their loop, which C2 compiles faster without the call. Once the
     * items pass {@code reach}, where the table's offsets end, the vector will be rejected, so the
     * rest are counted rather than kept, which bounds the bytes to what the table addresses.
     */
    private long writeItems(
        int count,
        int fields,
        ObjIntConsumer<Item> fill,
        int header,
        int vector,
        long start,
        long reach
    ) {
        var item = new Item();
        if (count > 0) {
            var itemLength = length;
            writeItem(item, 0, fill, fields, header, vector, start);
            reserveItems(count - 1, length - itemLength, reach);
        }
        for (int i = 1; i < count; i++) {
            if (position > reach) {
                return countItems(item, i, count, fill, fields, header, vector, start);
            }
            writeItem(item, i, fill, fields, header, vector, start);
        }

        return 0;
    }

    /**
     * Writes items {@code from} to {@code count} of a vector of tables in place, each over the one
     * before, {@return their bytes}, which aren't kept
     */
    private long countItems(
        Item item,
        int from,
        int count,
        ObjIntConsumer<Item> fill,
        int fields,
        int header,
        int vector,
        long start
    ) {
        long counted = 0;
        for (int i = from; i < count; i++) {
            var itemLength = length;
            var itemStart = position;
            var itemSegments = segmentCount;
            writeItem(item, i, fill, fields, header, vector, start);

            counted += position - itemStart;
            truncate(itemLength, itemStart, itemSegments);
        }

        return counted;
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
     * <p>Checking the table's length here, rather than in {@link #bytes()}, reports a field too long
     * for the layout from its own setter, before it changes the builder. Every offset and length is
     * shorter than the table, so the table's length is the one value to check.</p>
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
        var segment = bytes.length >= SEGMENT_LENGTH;

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
     * table's length, and adds its segments. A builder of any layout is copied so, as its own
     * {@link #bytes()} would write it.
     */
    private void append(TableBuilder table, int at) {
        var tableBytes = table.written;
        if (tableBytes != null) {
            append(tableBytes, at, tableBytes.length >= SEGMENT_LENGTH);
            return;
        }

        System.arraycopy(table.buffer, 0, buffer, at, table.length);
        if (table.layout.sized()) {
            store(buffer, at, table.offsetSize, table.position);
        }

        for (int i = 0; i < table.segmentCount; i++) {
            var place = table.segmentPlaces[i];
            var values = table.segmentValues[i];
            addSegment(at + atOf(place), typeOf(place), values, byteLengthOf(typeOf(place), values));
        }
    }

    /** {@return the number of a nested builder's bytes that go in this builder's bytes} */
    private static int bufferLengthOf(TableBuilder table) {
        var tableBytes = table.written;
        if (tableBytes != null) {
            return tableBytes.length >= SEGMENT_LENGTH ? 0 : tableBytes.length;
        }

        return table.length;
    }

    /**
     * Adds a segment of {@code byteLength} bytes before byte {@code at}, the end of the builder's
     * bytes so far, which a setter checked
     */
    private void addSegment(int at, int type, Object values, int byteLength) {
        var segment = segmentCount;
        if (segment == segmentValues.length) {
            growSegments();
        }

        segmentValues[segment] = values;
        segmentPlaces[segment] = (long) type << 32 | at;
        segmentCount = segment + 1;
        position += byteLength;
    }

    /** Copies the segments into arrays with room for twice as many, or {@link #INITIAL_SEGMENTS} */
    private void growSegments() {
        var length = Math.max(INITIAL_SEGMENTS, 2 * segmentValues.length);
        segmentValues = Arrays.copyOf(segmentValues, length);
        segmentPlaces = Arrays.copyOf(segmentPlaces, length);
    }

    /** Releases the values of the segments from {@code segment}, which may be large arrays */
    private void clearSegments(int segment) {
        Arrays.fill(segmentValues, segment, segmentCount, NO_VALUES);
        segmentCount = segment;
    }

    private static int atOf(long place) {
        return (int) place;
    }

    private static int typeOf(long place) {
        return (int) (place >>> 32);
    }

    private static int byteLengthOf(int type, Object values) {
        return switch (type) {
            case INT8 -> ((byte[]) values).length;
            case UINT8 -> ((short[]) values).length;
            case INT16 -> ((short[]) values).length * Short.BYTES;
            case UINT16 -> ((int[]) values).length * Short.BYTES;
            case INT32 -> ((int[]) values).length * Integer.BYTES;
            case UINT32 -> ((long[]) values).length * Integer.BYTES;
            case INT64 -> ((long[]) values).length * Long.BYTES;
            case FLOAT32 -> ((float[]) values).length * Float.BYTES;
            default -> ((double[]) values).length * Double.BYTES;
        };
    }

    /** Writes the table, with its segments and a sized table's length, into {@code bytes} at {@code at} */
    private void copyInto(byte[] bytes, int at) {
        var from = 0;
        var to = at;
        for (int i = 0; i < segmentCount; i++) {
            var place = segmentPlaces[i];
            var run = atOf(place) - from;
            System.arraycopy(buffer, from, bytes, to, run);
            from = atOf(place);
            to += run;

            to = writeSegment(typeOf(place), segmentValues[i], bytes, to);
        }
        System.arraycopy(buffer, from, bytes, to, length - from);

        if (layout.sized()) {
            store(bytes, at, offsetSize, position);
        }
    }

    /** Writes a segment's values at {@code at}, {@return where they end} */
    private static int writeSegment(int type, Object values, byte[] bytes, int at) {
        switch (type) {
            case INT8 -> Elements.setInt8(bytes, at, (byte[]) values);
            case UINT8 -> Elements.setUint8(bytes, at, (short[]) values);
            case INT16 -> Elements.setInt16(bytes, at, (short[]) values);
            case UINT16 -> Elements.setUint16(bytes, at, (int[]) values);
            case INT32 -> Elements.setInt32(bytes, at, (int[]) values);
            case UINT32 -> Elements.setUint32(bytes, at, (long[]) values);
            case INT64 -> Elements.setInt64(bytes, at, (long[]) values);
            case FLOAT32 -> Elements.setFloat32(bytes, at, (float[]) values);
            case FLOAT64 -> Elements.setFloat64(bytes, at, (double[]) values);
            default -> throw new AssertionError("A segment of type " + type);
        }

        return at + byteLengthOf(type, values);
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
     * bytes, as field {@code index}, padded as a nested builder is, and moved by its padding. Its
     * offsets count from its own start, so none of them changes.
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
            segmentPlaces[i] += distance;
        }

        length += distance;
        position += distance;
    }

    /**
     * Makes room for {@code count} more items of a vector of tables in place, each of
     * {@code itemLength} bytes, as long as its first, up to {@link #MAX_ITEM_ROOM} and what the
     * table can address. Growing as each item is written instead took as long on the benchmarks'
     * vector of nested tables, and allocated a fifth more.
     */
    private void reserveItems(int count, int itemLength, long reach) {
        var room = Math.min(Math.min((long) count * itemLength, reach - position), MAX_ITEM_ROOM);
        ensure((int) Math.max(0, room));
    }

    /** Undoes what a fill wrote, from where it started */
    private void truncate(int length, long position, int segmentCount) {
        this.length = length;
        this.position = position;

        clearSegments(segmentCount);
    }

    /** Makes room for {@code byteLength} more bytes */
    private void ensure(int byteLength) {
        if (byteLength > buffer.length - length) {
            grow(byteLength);
        }
    }

    /**
     * Copies the bytes into bytes with room for {@code byteLength} more, and twice as long or more
     *
     * @throws OutOfMemoryError if they'd be longer than an array can be, which only fields of a
     *     table written in place can ask for, before its parent is found too long
     */
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
     * @throws IllegalArgumentException if the builder has fields left to set, or a fill is writing
     *     a table in it
     */
    private static long lengthOf(TableBuilder table) {
        // Its counters are then those of the table that the fill writes
        if (table.tableStart != 0) {
            throw new IllegalArgumentException(WRITING_IN_PLACE);
        }
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
     * place}, which has this table's offset type and no header, as a nested builder's layout
     * checks them
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
     * {@return the index of the next field of the table being written}. Its exceptions are built
     * out of line, which keeps it, and the setters that C2 inlines it into, small.
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
     * The fields of a table that a {@code TableBuilder} writes in place, as one of its own fields
     * or an item of a vector, which {@link #setTable(int, Consumer)} and
     * {@link #setVector(int, int, ObjIntConsumer)} give to the fill that sets them. Its setters are
     * the builder's, taking the same values and throwing the same exceptions, and write each field
     * among the builder's own, so the table needs no builder of its own. It's valid only within
     * that fill.
     */
    public final class Item {
        /**
         * Each fill is given an item of its own rather than one that the builder keeps, which the
         * JIT removes when it inlines the fill, knowing that the item's builder is the one that
         * writes the table. An item kept in a field of every builder, which refers back to it,
         * kept escape analysis from removing builders: the benchmarks' record took 26.6 ns rather
         * than 19.7, and allocated 376 bytes rather than 224, and their vector of nested tables
         * took 2.41 µs rather than 1.87.
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
