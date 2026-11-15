package io.github.snovader.tafla.test;

import org.jspecify.annotations.*;

import java.util.function.*;

/**
 * A fault to inject into one of the tables and vectors that fills write in place, the one at
 * index {@code at} in the order that their setters are called. It's paused while a field that
 * failed is set again.
 */
final class FillFaults {
    /** What goes wrong with the table or vector */
    enum Kind {
        /** Its fill, or its last item's, throws a {@link FillFailure} once it has set its fields */
        THROW,
        /** It's declared with a field more than its fill sets, which leaves it incomplete */
        MORE_FIELDS,
        /** It's declared with a field fewer than its fill sets, which sets one too many */
        FEWER_FIELDS
    }

    /** What a fill throws for {@link Kind#THROW} */
    static final class FillFailure extends RuntimeException {
        private static final long serialVersionUID = 1L;

        FillFailure() {
            super("The fill failed", null, false, false);
        }
    }

    private final Kind kind;
    private final int at;
    private int calls = 0;
    private boolean paused = false;

    FillFaults(Kind kind, int at) {
        this.kind = kind;
        this.at = at;
    }

    /** {@return faults that never happen} */
    static FillFaults none() {
        return new FillFaults(Kind.THROW, -1);
    }

    /** {@return the fault of the next table or vector in place, or null if it has none} */
    @Nullable Kind next() {
        if (paused) {
            return null;
        }

        return calls++ == at ? kind : null;
    }

    /** {@return the number of fields to declare for a table or items of {@code fields} fields} */
    static int declared(@Nullable Kind fault, int fields) {
        if (fault == Kind.MORE_FIELDS) {
            return fields + 1;
        }
        if (fault == Kind.FEWER_FIELDS) {
            return fields - 1;
        }

        return fields;
    }

    /** Throws after a fill set its fields, if it's the last of a table or vector that fails so */
    static void afterFill(@Nullable Kind fault, boolean last) {
        if (fault == Kind.THROW && last) {
            throw new FillFailure();
        }
    }

    /** {@return what {@code set} returns}, run without faults */
    <T> T paused(Supplier<T> set) {
        paused = true;
        try {
            return set.get();
        } finally {
            paused = false;
        }
    }
}
