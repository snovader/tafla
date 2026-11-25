package io.github.snovader.tafla.test;

import io.github.snovader.tafla.*;

/** The layout of a described table, as {@code cases.json} writes it out in full */
record LayoutDescription(boolean sized, boolean versioned, OffsetType offsetType) {
    /** {@return the layout with neither header slot and offsets of {@code offsetType}} */
    static LayoutDescription headerless(OffsetType offsetType) {
        return new LayoutDescription(false, false, offsetType);
    }

    /** {@return the layout, which reads tables of any version: its own is zero when it's versioned} */
    TableLayout tableLayout() {
        return tableLayout(0);
    }

    /** {@return the layout, with {@code version} when it's versioned} */
    TableLayout tableLayout(long version) {
        var builder = TableLayout.builder().sized(sized).offsetType(offsetType);
        return (versioned ? builder.version(version) : builder).build();
    }

    /** {@return the offset size: the width of offsets, header slots and length prefixes in bytes} */
    int width() {
        return offsetType.size();
    }

    /** {@return the byte length of the header slots before the field offsets} */
    int headerSize() {
        return ((sized ? 1 : 0) + (versioned ? 1 : 0)) * width();
    }

    @Override
    public String toString() {
        return DescriptionJson.write(this).toString();
    }
}
