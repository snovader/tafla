package io.github.snovader.tafla;

import java.util.*;

/**
 * The header and offset type of a table. A format creates its layout once and passes it to both
 * the {@link TableBuilder} that writes the table and the {@link ByteBuffer} that reads it, so that
 * the two can't disagree. Nested tables use {@link #DEFAULT}.
 *
 * {@snippet :
 * // The response's length and format version, then its field offsets
 * static final TableLayout LAYOUT = TableLayout.builder()
 *     .sized(true)
 *     .versioned(true)
 *     .build();
 * }
 */
public final class TableLayout {
    /** No header slots, and offsets and lengths of the offset type {@code uint32} */
    public static final TableLayout DEFAULT = new TableLayout(false, false, OffsetType.UINT32);
    private static final TableLayout HEADERLESS_UINT8 = new TableLayout(false, false, OffsetType.UINT8);
    private static final TableLayout HEADERLESS_UINT16 = new TableLayout(false, false, OffsetType.UINT16);

    /**
     * The longest array that the JDK's own collections allocate, which limits a table of
     * {@code uint32} offsets to about 2 GiB rather than their 4 GiB
     */
    static final int MAX_ARRAY_LENGTH = Integer.MAX_VALUE - 8;

    private final boolean sized;
    private final boolean versioned;
    private final OffsetType offsetType;
    private final int headerSize;

    private TableLayout(boolean sized, boolean versioned, OffsetType offsetType) {
        this.sized = sized;
        this.versioned = versioned;
        this.offsetType = offsetType;

        var headerSlots = (sized ? 1 : 0) + (versioned ? 1 : 0);
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
        return versioned;
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
     * {@link TableBuilder} records in place have
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
     * fields}, whose writer puts {@code version} in the header when {@code versioned}
     *
     * @throws IllegalArgumentException if the layout is versioned and no version is given, or the
     *     other way round, the version doesn't fit in the offset type, the number of fields is
     *     negative, or the header alone is longer than the layout addresses
     */
    int headerLength(int fields, boolean versioned, long version) {
        if (this.versioned != versioned) {
            throw new IllegalArgumentException(
                this.versioned ? "A versioned layout needs a version" : "The layout has no version slot"
            );
        }

        if (versioned && (version < 0 || version > offsetType.maxValue())) {
            throw new IllegalArgumentException(
                String.format("Version %d doesn't fit in a %s slot", version, offsetType)
            );
        }

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
            "TableLayout[sized=%s, versioned=%s, offsetType=%s]",
            sized,
            versioned,
            offsetType
        );
    }

    /**
     * Sets the options of a {@link TableLayout}, as the options object of the TypeScript
     * {@code TableLayout.create()} and the keyword arguments of the Python constructor do.
     */
    public static final class Builder {
        private boolean sized = false;
        private boolean versioned = false;
        private OffsetType offsetType = OffsetType.UINT32;

        private Builder() {}

        /** Whether the header starts with the table's total byte length, which is false by default */
        public Builder sized(boolean sized) {
            this.sized = sized;
            return this;
        }

        /**
         * Whether the header has a slot for a version, after the length when sized, which is false by
         * default. Readers can check it before reading any field, and a {@link TableBuilder} of the
         * layout needs a version to write to it.
         */
        public Builder versioned(boolean versioned) {
            this.versioned = versioned;
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

        public TableLayout build() {
            return new TableLayout(sized, versioned, offsetType);
        }
    }
}
