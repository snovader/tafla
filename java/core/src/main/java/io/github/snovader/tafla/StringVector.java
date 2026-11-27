package io.github.snovader.tafla;

import java.util.*;

/**
 * A vector of strings in a table, which reads each string when it's asked for, from its end and
 * the end of the string before it, so that a reader of some strings doesn't decode the others, and
 * a string whose bytes are corrupt throws a {@link DecodeException} when it's read. The code that
 * tafla-gen generates returns one from the getter of a {@code [string]} field.
 *
 * <p>It's a random-access, unmodifiable {@link List}, as the generated views of vectors of tables
 * are. Its iterator reads each string's end once, as the next string's start, so a loop over it
 * reads every string as {@link TableReader#getStringVector} does, and so do
 * {@code List.copyOf()} and {@code toArray(String[]::new)}. Like the reader, it holds the bytes,
 * and reads what they hold when a string is read.</p>
 */
public final class StringVector extends AbstractList<String> implements RandomAccess {
    private final TableReader reader;
    private final int position;
    private final int size;

    /**
     * Views the vector of strings at {@code position}, as {@link TableReader#rootOffset} or
     * {@link TableReader#fieldOffset} finds it.
     *
     * @throws DecodeException if the vector's count or the ends of its strings aren't within the
     *     bytes
     */
    public StringVector(TableReader reader, int position) {
        this.reader = reader;
        this.position = position;
        this.size = reader.getVectorLength(position);
    }

    /** {@return the number of strings} */
    @Override
    public int size() {
        return size;
    }

    /**
     * Reads string {@code index}, without reading the others.
     *
     * @throws DecodeException if the string ends before it starts or past the bytes, or isn't valid
     *     UTF-8
     * @throws IndexOutOfBoundsException if the vector has no string {@code index}
     */
    @Override
    public String get(int index) {
        // The count that the constructor checked, rather than reading and checking it again
        return reader.getStringVectorItem(position, size, index);
    }

    /**
     * {@return an iterator of the strings in order}, which reads each string's end once, as the
     * next string's start, where {@code get()} reads the ends of a string and the one before it.
     * Its {@code next()} throws a {@code DecodeException} if the string ends before it starts or
     * past the bytes, or isn't valid UTF-8, and the {@code next()} after it then reads the next
     * string, as {@code get()} reads it.
     */
    @Override
    public Iterator<String> iterator() {
        return new Iterator<>() {
            private int index = 0;
            // Where string index starts, from the vector's position, which is where the string
            // before it ends
            private long start = reader.firstItemStart(size);

            @Override
            public boolean hasNext() {
                return index < size;
            }

            @Override
            public String next() {
                if (index >= size) {
                    throw new NoSuchElementException();
                }

                // The next string starts at this one's end, as get() finds it, even if this one
                // ends before it starts or past the bytes, or isn't valid UTF-8, so the end is
                // kept before it's checked
                var string = start;
                start = reader.itemEnd(position, index++);

                return reader.vectorString(position, string, start);
            }
        };
    }
}
