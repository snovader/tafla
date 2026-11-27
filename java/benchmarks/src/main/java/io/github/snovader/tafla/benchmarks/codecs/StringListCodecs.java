package io.github.snovader.tafla.benchmarks.codecs;

import io.github.snovader.tafla.benchmarks.generated.lists.Name;
import io.github.snovader.tafla.benchmarks.generated.lists.NameList;
import io.github.snovader.tafla.benchmarks.generated.lists.StringList;

import java.util.*;

/**
 * The lists suite's encodings of a list of strings, which the code that tafla-gen generates from
 * {@code src/main/tafla/lists.tafla} writes and reads: the vector of strings of {@code StringList},
 * the vector of tables of one string each of {@code NameList}, which is how a schema holds a list
 * of strings without a vector of strings, and an A/A copy of the first, whose times differ from it
 * only by the measurement's noise.
 *
 * <p>Each writes from the generated {@code Fields}, which it creates from the list in each call, as
 * the comparison's codecs do, and reads every string in order through its getter's view, into an
 * array that {@code Arrays.asList()} wraps, and the last string by its index.</p>
 */
public final class StringListCodecs {
    /** tafla-gen's {@code StringList}, a {@code [string]}, read with its getter's {@code StringVector} */
    public static final String STRING_VECTOR = "string-vector";
    /** tafla-gen's {@code NameList}, a {@code [Name]} of tables of one string each */
    public static final String NAME_VECTOR = "name-vector";
    /** A copy of {@link #STRING_VECTOR} */
    public static final String STRING_VECTOR_AA = "string-vector-aa";

    private StringListCodecs() {}

    /** {@return a new encoding of each variant, by name} */
    public static SequencedMap<String, StringListWorkload> all() {
        var codecs = new LinkedHashMap<String, StringListWorkload>();
        codecs.put(STRING_VECTOR, new StringVectors());
        codecs.put(NAME_VECTOR, new NameVectors());
        codecs.put(STRING_VECTOR_AA, new StringVectors());

        return codecs;
    }

    /**
     * {@return a new encoding of the variant of that name}
     *
     * @throws IllegalArgumentException if no variant has that name
     */
    public static StringListWorkload named(String name) {
        var codec = all().get(name);
        if (codec == null) {
            throw new IllegalArgumentException("No codec of lists is named " + name);
        }

        return codec;
    }

    /**
     * Serializes the strings after checking that the workload reads back exactly what it wrote, and
     * reads the last string, so that a broken codec isn't timed.
     *
     * @throws IllegalStateException if it doesn't
     */
    public static byte[] serializeChecked(StringListWorkload workload, List<String> strings) {
        var bytes = Codecs.serializeChecked(workload, strings);

        var last = workload.readLast(bytes);
        if (!last.equals(strings.getLast())) {
            throw new IllegalStateException(
                String.format("%s read the last string as %s", workload.getClass().getName(), last)
            );
        }

        return bytes;
    }

    /** {@code StringList}'s vector of strings */
    private static final class StringVectors implements StringListWorkload {
        @Override
        public byte[] serialize(List<String> strings) {
            return StringList.serialize(new StringList.Fields(strings));
        }

        /** Reads every string through the view's iterator, which reads each string's end once */
        @Override
        public List<String> deserialize(byte[] bytes) {
            var items = new StringList(bytes).items();
            var strings = new String[items.size()];

            var index = 0;
            for (var string : items) {
                strings[index++] = string;
            }

            return Arrays.asList(strings);
        }

        @Override
        public String readLast(byte[] bytes) {
            var items = new StringList(bytes).items();
            return items.get(items.size() - 1);
        }
    }

    /** {@code NameList}'s vector of tables of one string each */
    private static final class NameVectors implements StringListWorkload {
        @Override
        public byte[] serialize(List<String> strings) {
            var items = new ArrayList<Name.Fields>(strings.size());
            for (var string : strings) {
                items.add(new Name.Fields(string));
            }

            return NameList.serialize(new NameList.Fields(items));
        }

        @Override
        public List<String> deserialize(byte[] bytes) {
            var items = new NameList(bytes).items();
            var strings = new String[items.size()];

            var index = 0;
            for (var name : items) {
                strings[index++] = name.value();
            }

            return Arrays.asList(strings);
        }

        @Override
        public String readLast(byte[] bytes) {
            var items = new NameList(bytes).items();
            return items.get(items.size() - 1).value();
        }
    }
}
