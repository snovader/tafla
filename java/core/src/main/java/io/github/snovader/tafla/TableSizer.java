package io.github.snovader.tafla;

import org.jspecify.annotations.*;

import java.util.*;

/**
 * Counts the bytes of a table, so that a {@link TableWriter} can write it into new bytes of exactly
 * that length, or into the start of bytes that the caller reuses. It's for code that knows a
 * table's fields before it writes them, such as the code that tafla-gen generates, which writes
 * tables without recording their fields as {@link TableBuilder} does.
 *
 * <p>The fields are added in order, each with its type's method, which takes what decides its
 * length: nothing for a scalar, the count of an array, the string itself. A nested table is added
 * with {@link #addTable}, and a vector with {@link #addVector}, and the fields of each such table
 * then follow between {@link #startTable} and {@link #endTable}. The sizer pads arrays and nested
 * tables as {@code TableBuilder} does, so a writer that takes the same fields in the same order
 * writes each where the sizer counted it.</p>
 *
 * {@snippet :
 * var sizer = new TableSizer(2);
 * sizer.addFloat64();
 * sizer.addString("Europe/Stockholm");
 *
 * var writer = sizer.writer();
 * writer.setFloat64(0, 59.3293);
 * writer.setString(1, "Europe/Stockholm");
 * byte[] bytes = writer.bytes();
 * }
 *
 * <p>The length is counted in a {@code long}, and checked once all the fields are added, by
 * {@link #byteLength()} and the writers, against the longest table that the layout's offset type
 * addresses: 255 bytes, 64 KiB, or for {@code uint32} offsets, as long as a Java array can be.</p>
 */
public final class TableSizer {
    private final TableLayout layout;
    private final long version;
    private final int fields;
    /** The offset size, the width in bytes of offsets, lengths and header slots */
    private final int offsetSize;
    /** The table's length so far, which is where the next field goes */
    private long position;
    /** The start of the table being sized, which arrays are aligned from */
    private long table = 0;
    /** The long strings added, in order, with their bytes, which the writer copies */
    private @Nullable List<EncodedString> encodedStrings;

    /** A string of at least {@link Utf8#SHORT_LENGTH} {@code char}s, with its UTF-8 bytes */
    // Never compared or hashed, so its array's identity doesn't matter
    @SuppressWarnings("ArrayRecordComponent")
    record EncodedString(String string, byte[] utf8) {}

    /** Sizes a table of {@link TableLayout#DEFAULT} */
    public TableSizer(int fields) {
        this(fields, TableLayout.DEFAULT);
    }

    /**
     * Sizes a table of a layout without a version, with the checks of {@link TableBuilder}'s
     * constructor.
     *
     * @param fields the number of fields that the table holds
     * @param layout the table's header and offset type, which its readers must use too
     * @throws IllegalArgumentException if the layout is versioned, the number of fields is
     *     negative, or the header alone is longer than the layout addresses
     */
    public TableSizer(int fields, TableLayout layout) {
        this(fields, layout, false, 0);
    }

    /**
     * Sizes a table of a versioned layout, with the checks of {@link TableBuilder}'s constructor.
     *
     * @param version the version that the writer writes to the header
     * @throws IllegalArgumentException if the layout isn't versioned, the version doesn't fit in
     *     the layout's offset type, the number of fields is negative, or the header alone is longer
     *     than the layout addresses
     */
    public TableSizer(int fields, TableLayout layout, long version) {
        this(fields, layout, true, version);
    }

    private TableSizer(int fields, TableLayout layout, boolean versioned, long version) {
        this.position = layout.headerLength(fields, versioned, version);
        this.layout = layout;
        this.version = version;
        this.fields = fields;
        this.offsetSize = layout.offsetSize();
    }

    /**
     * Returns the byte length of the table, with every field added.
     *
     * @throws IllegalArgumentException if the table is too long for its offset type, or longer than
     *     a Java array can be
     */
    public int byteLength() {
        var maxLength = layout.maxLength();
        if (position > maxLength) {
            throw new IllegalArgumentException(tooLong(maxLength));
        }

        return (int) position;
    }

    /**
     * Returns a writer of the table, into new bytes of its length, which takes its fields in the
     * order that they were added. Its {@link TableWriter#bytes()} returns them.
     *
     * @throws IllegalArgumentException if the table is too long for its offset type, or longer than
     *     a Java array can be
     */
    public TableWriter writer() {
        var length = byteLength();

        return new TableWriter(new byte[length], length, fields, layout, version, encodedStrings());
    }

    /**
     * Returns a writer of the table into the start of {@code bytes}, rather than into new bytes,
     * which takes its fields in the order that they were added. Its
     * {@link TableWriter#byteLength()} is the table's length, the number of bytes that hold it. It
     * writes every byte of the table, so the bytes needn't be zero, and leaves those after it as
     * they were.
     *
     * <p>The caller owns the bytes, and the writer keeps them only until the table is written. The
     * table stays in them until they're written again, such as by the next table written into
     * them, which also changes what a {@link ByteBuffer} or array view of them reads. To keep the
     * table, or to read exactly its bytes, copy them with
     * {@code Arrays.copyOf(bytes, writer.byteLength())}.</p>
     *
     * <p>Reusing bytes saves allocating, zeroing and collecting new bytes for each table. In the
     * comparison suite, on JDK 25, the generated code took 34% less time to write a document of
     * 1.2 MB into reused bytes than into new bytes, 27% less for one of 1.3 KB, and 17% less for a
     * record of 60 bytes. Where creating the fields' values costs more than the bytes, it saves
     * little: 1.5% for a vector of 100 nested tables, and nothing for an array of 100 structs.</p>
     *
     * @throws IllegalArgumentException if {@code bytes} are shorter than the table, before any of
     *     them is written, or if the table is too long for its offset type
     */
    public TableWriter writer(byte[] bytes) {
        var length = byteLength();
        if (bytes.length < length) {
            throw new IllegalArgumentException(
                String.format(
                    "The table is %d bytes, longer than the %d bytes given",
                    length,
                    bytes.length
                )
            );
        }

        return new TableWriter(bytes, length, fields, layout, version, encodedStrings());
    }

    /** {@return the long strings that were added, in order, with their bytes} */
    private List<EncodedString> encodedStrings() {
        return encodedStrings == null ? List.of() : encodedStrings;
    }

    /**
     * Starts the fields of a nested table or vector item of {@code fields} fields, after its
     * parent's {@link #addTable} or {@link #addVector}.
     *
     * @return the parent's start, which {@link #endTable} takes back
     * @throws IllegalArgumentException if {@code fields} is negative
     */
    public long startTable(int fields) {
        var offsets = (long) checkCount(fields) * offsetSize;

        var parent = table;
        table = position;
        position += offsets;

        return parent;
    }

    /** Ends the fields of the table that {@link #startTable} started */
    public void endTable(long parent) {
        table = parent;
    }

    public void addInt8() {
        position += Byte.BYTES;
    }

    public void addUint8() {
        position += Byte.BYTES;
    }

    public void addInt16() {
        position += Short.BYTES;
    }

    public void addUint16() {
        position += Short.BYTES;
    }

    public void addInt32() {
        position += Integer.BYTES;
    }

    public void addUint32() {
        position += Integer.BYTES;
    }

    public void addInt64() {
        position += Long.BYTES;
    }

    public void addUint64() {
        position += Long.BYTES;
    }

    public void addFloat32() {
        position += Float.BYTES;
    }

    public void addFloat64() {
        position += Double.BYTES;
    }

    // The array adders take the count of the elements, and throw an IllegalArgumentException for a
    // negative one

    public void addInt8Array(int count) {
        addArray(count, Byte.BYTES, Byte.BYTES);
    }

    public void addUint8Array(int count) {
        addArray(count, Byte.BYTES, Byte.BYTES);
    }

    public void addInt16Array(int count) {
        addArray(count, Short.BYTES, Short.BYTES);
    }

    public void addUint16Array(int count) {
        addArray(count, Short.BYTES, Short.BYTES);
    }

    public void addInt32Array(int count) {
        addArray(count, Integer.BYTES, Integer.BYTES);
    }

    public void addUint32Array(int count) {
        addArray(count, Integer.BYTES, Integer.BYTES);
    }

    public void addInt64Array(int count) {
        addArray(count, Long.BYTES, Long.BYTES);
    }

    public void addUint64Array(int count) {
        addArray(count, Long.BYTES, Long.BYTES);
    }

    public void addFloat32Array(int count) {
        addArray(count, Float.BYTES, Float.BYTES);
    }

    public void addFloat64Array(int count) {
        addArray(count, Double.BYTES, Double.BYTES);
    }

    /**
     * Adds a string, as UTF-8 with lone surrogates replaced by U+FFFD. A long string is encoded
     * here, and its writer copies those bytes when it's given the same string.
     */
    public void addString(String string) {
        if (string.length() < Utf8.SHORT_LENGTH) {
            position += offsetSize + Utf8.length(string);
            return;
        }

        var utf8 = Utf8.encode(string);
        if (encodedStrings == null) {
            encodedStrings = new ArrayList<>();
        }
        encodedStrings.add(new EncodedString(string, utf8));
        position += offsetSize + utf8.length;
    }

    /**
     * Adds a struct of {@code size} bytes
     *
     * @throws IllegalArgumentException if {@code size} is negative
     */
    public void addStruct(int size) {
        if (size < 0) {
            throw new IllegalArgumentException(String.format("A struct's size can't be negative: %d", size));
        }

        position += size;
    }

    /**
     * Adds {@code count} structs of {@code size} bytes, padded so that they start at a multiple of
     * {@code alignment} from the start of the table
     *
     * @param alignment what the structs' start is padded to a multiple of: 1, 2, 4 or 8
     * @throws IllegalArgumentException if {@code count} is negative, {@code size} is less than 1, or
     *     the alignment isn't 1, 2, 4 or 8
     */
    public void addStructArray(int count, int size, int alignment) {
        ByteBuffer.assertStructSize(size);
        addArray(count, size, alignment);
    }

    /**
     * Adds a nested table, padded to start at a multiple of {@code alignment}, whose fields then
     * follow between {@link #startTable} and {@link #endTable}
     *
     * @param alignment what the table's start is padded to a multiple of: 1, 2, 4 or 8
     * @throws IllegalArgumentException if the alignment isn't 1, 2, 4 or 8
     */
    public void addTable(int alignment) {
        position += TableLayout.padding(position - table, alignment);
    }

    /**
     * Adds a vector of {@code count} tables, whose fields then follow, each between
     * {@link #startTable} and {@link #endTable}
     *
     * @throws IllegalArgumentException if {@code count} is negative
     */
    public void addVector(int count) {
        position += (checkCount(count) + 1L) * offsetSize;
    }

    /**
     * Adds a count and then {@code count} elements of {@code elementSize} bytes, padded so that the
     * elements start at a multiple of {@code alignment} from the start of the table
     */
    private void addArray(int count, int elementSize, int alignment) {
        var start = position + offsetSize;
        var padding = TableLayout.padding(start - table, alignment);
        position = start + padding + (long) checkCount(count) * elementSize;
    }

    /**
     * {@return the count of fields, items or elements}, which {@link TableWriter} checks as well
     *
     * @throws IllegalArgumentException if it's negative
     */
    static int checkCount(int count) {
        if (count < 0) {
            throw new IllegalArgumentException(String.format("A count can't be negative: %d", count));
        }

        return count;
    }

    private String tooLong(long maxLength) {
        if (maxLength == TableLayout.MAX_ARRAY_LENGTH) {
            return String.format("The table's fields make it %d bytes, longer than a Java array can be", position);
        }

        return String.format("The table's fields make it %d bytes, too long for %s offsets", position, layout.offsetType());
    }
}
