package io.github.snovader.tafla;

import java.util.*;

/**
 * The header and offset type of a table. A format creates its layout once and passes it to both
 * the {@link TableBuilder} that writes the table and the {@link TableReader} that reads it, so that
 * the two can't disagree. Nested tables use {@link #DEFAULT}.
 *
 * {@snippet :
 * // The response's length and format version, then its field offsets
 * static final TableLayout LAYOUT = TableLayout.builder()
 *     .sized(true)
 *     .version(1)
 *     .build();
 * }
 *
 * <p>A layout with a version is the layout of that version of the format: its builders and
 * writers write the version, and its readers read tables of any version, so that an application
 * can read a table's version before it chooses how to read the fields. The layouts of a format's
 * versions should differ in the version only, as {@link #withVersion(long)} makes them.
 */
public final class TableLayout {
    /** The version of a layout that has none, which no header slot holds */
    private static final long NO_VERSION = -1;

    /** No header slots, and offsets and lengths of the offset type {@code uint32} */
    public static final TableLayout DEFAULT = new TableLayout(false, NO_VERSION, OffsetType.UINT32);
    private static final TableLayout HEADERLESS_UINT8 = new TableLayout(false, NO_VERSION, OffsetType.UINT8);
    private static final TableLayout HEADERLESS_UINT16 = new TableLayout(false, NO_VERSION, OffsetType.UINT16);

    /**
     * The longest array that the JDK's own collections allocate, which limits a table of
     * {@code uint32} offsets to about 2 GiB rather than their 4 GiB
     */
    static final int MAX_ARRAY_LENGTH = Integer.MAX_VALUE - 8;

    private final boolean sized;
    private final long version;
    private final OffsetType offsetType;
    private final int headerSize;

    private TableLayout(boolean sized, long version, OffsetType offsetType) {
        this.sized = sized;
        this.version = version;
        this.offsetType = offsetType;

        var headerSlots = (sized ? 1 : 0) + (version == NO_VERSION ? 0 : 1);
        this.headerSize = headerSlots * offsetType.size();
    }

    /** {@return a builder of a layout, which starts as {@link #DEFAULT}} */
    public static Builder builder() {
        return new Builder();
    }

    /** {@return whether the header starts with the table's total byte length} */
    public boolean sized() {
        return sized;
    }

    /** {@return whether the header has a slot for a version, after the length when sized} */
    public boolean versioned() {
        return version != NO_VERSION;
    }

    /** {@return the version that builders and writers write to the header, when it has a slot} */
    public OptionalLong version() {
        return version == NO_VERSION ? OptionalLong.empty() : OptionalLong.of(version);
    }

    /**
     * {@return the layout with {@code version} as its version}, for another version of the same
     * format. A layout without a version gains the slot for one, so its header is a slot longer.
     *
     * @throws IllegalArgumentException if the version doesn't fit in the offset type
     */
    public TableLayout withVersion(long version) {
        return new TableLayout(sized, checkedVersion(version, offsetType), offsetType);
    }

    /** {@return the version to write to the header}, of a layout that has one */
    long headerVersion() {
        return version;
    }

    private static long checkedVersion(long version, OffsetType offsetType) {
        if (version < 0 || version > offsetType.maxValue()) {
            throw new IllegalArgumentException(
                String.format("Version %d doesn't fit in a %s slot", version, offsetType)
            );
        }

        return version;
    }

    /**
     * {@return the offset type, the unsigned integer type of the header slots, field offsets,
     * counts, lengths and item ends}
     */
    public OffsetType offsetType() {
        return offsetType;
    }

    /** {@return the offset size, the width in bytes of the offset type: 1, 2 or 4} */
    public int offsetSize() {
        return offsetType.size();
    }

    /** {@return the byte length of the header slots before the field offsets} */
    public int headerSize() {
        return headerSize;
    }

    // The rules of the layout that TableBuilder, TableSizer and TableWriter share

    /**
     * {@return the layout without a header of {@code offsetType}}, which the tables that a
     * {@link TableBuilder} writes in place have
     */
    static TableLayout headerless(OffsetType offsetType) {
        return switch (offsetType) {
            case UINT8 -> HEADERLESS_UINT8;
            case UINT16 -> HEADERLESS_UINT16;
            case UINT32 -> DEFAULT;
        };
    }

    /**
     * {@return the longest table that the offsets address and a Java array holds}: 255 bytes, 64 KiB,
     * or for {@code uint32} offsets, about 2 GiB rather than their 4 GiB
     */
    long maxLength() {
        return Math.min(offsetType.maxValue(), MAX_ARRAY_LENGTH);
    }

    /**
     * {@return the byte length of the header slots and field offsets of a table of {@code fields}
     * fields}
     *
     * @throws IllegalArgumentException if the number of fields is negative, or the header alone is
     *     longer than the layout addresses
     */
    int headerLength(int fields) {
        if (fields < 0) {
            throw new IllegalArgumentException(String.format("A table can't hold %d fields", fields));
        }

        var length = headerSize + (long) fields * offsetType.size();
        if (length > maxLength()) {
            throw new IllegalArgumentException(
                String.format("A header of %d fields doesn't fit in %s offsets", fields, offsetType)
            );
        }

        return (int) length;
    }

    /**
     * The padding that brings {@code offset} from a table's start to a multiple of
     * {@code alignment}
     *
     * @throws IllegalArgumentException if the alignment isn't 1, 2, 4 or 8
     */
    static int padding(long offset, int alignment) {
        if (alignment != 1 && alignment != 2 && alignment != 4 && alignment != 8) {
            throw new IllegalArgumentException(
                String.format("An alignment of %d isn't 1, 2, 4 or 8 bytes", alignment)
            );
        }

        // The alignment is a power of 2, so its mask is the remainder's
        return (int) (-offset & (alignment - 1));
    }

    @Override
    public String toString() {
        return String.format(
            "TableLayout[sized=%s, version=%s, offsetType=%s]",
            sized,
            version == NO_VERSION ? "none" : Long.toString(version),
            offsetType
        );
    }

    /**
     * Sets the options of a {@link TableLayout}, as the options object of the TypeScript
     * {@code TableLayout.create()} and the keyword arguments of the Python constructor do.
     */
    public static final class Builder {
        private boolean sized = false;
        private long version = NO_VERSION;
        private boolean versioned = false;
        private OffsetType offsetType = OffsetType.UINT32;

        private Builder() {}

        /** Whether the header starts with the table's total byte length, which is false by default */
        public Builder sized(boolean sized) {
            this.sized = sized;
            return this;
        }

        /**
         * The version that the header holds, after the length when sized, which gives it a slot for
         * one; a layout has none by default. Builders and writers write it, and readers can check
         * the one that a table holds, with {@link TableReader#getVersion()}, before reading any
         * field. {@link #build()} checks that it fits in the offset type.
         */
        public Builder version(long version) {
            this.version = version;
            this.versioned = true;
            return this;
        }

        /**
         * The offset type, the unsigned integer type of the header slots, field offsets, counts,
         * lengths and item ends, which limits the table's length: {@link OffsetType#UINT8} for up to
         * 255 bytes, {@link OffsetType#UINT16} for 65,535, and {@link OffsetType#UINT32}, the
         * default, for as long as a Java array can be, about 2 GiB.
         */
        public Builder offsetType(OffsetType offsetType) {
            this.offsetType = Objects.requireNonNull(offsetType);
            return this;
        }

        /**
         * {@return the layout that the options set out}
         *
         * @throws IllegalArgumentException if the version doesn't fit in the offset type
         */
        public TableLayout build() {
            return new TableLayout(
                sized,
                versioned ? checkedVersion(version, offsetType) : NO_VERSION,
                offsetType
            );
        }
    }
}
