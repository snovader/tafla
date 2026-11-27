package io.github.snovader.tafla.test;

import io.github.snovader.tafla.TableLayout;
import java.util.*;

/**
 * The description of a table in the shape of the conformance suite's {@code cases.json}, see
 * {@code conformance/README.md} at the root of the repository. The conformance test reads it from
 * the suite, and {@link Descriptions} generates it.
 *
 * @param version the version in the header, present exactly when the layout is versioned
 * @param fields the fields in the order that they're set, which is the order of their offsets
 * @param missing how many fields a reader of a later revision of the table knows after
 *     {@code fields}, each of which it must find missing from the table. Only the cases of
 *     presence have it.
 */
record TableDescription(
    LayoutDescription layout,
    OptionalLong version,
    List<FieldDescription> fields,
    OptionalInt missing
) {
    TableDescription {
        fields = List.copyOf(fields);
    }

    /** A table that a reader of its own revision reads, as every table but a case of presence */
    TableDescription(LayoutDescription layout, OptionalLong version, List<FieldDescription> fields) {
        this(layout, version, fields, OptionalInt.empty());
    }

    /**
     * {@return the layout that the table's writers take, with its version when it's versioned}
     *
     * @throws IllegalStateException if the layout is versioned and the description has no version,
     *     which would otherwise be written as 0
     */
    TableLayout writtenLayout() {
        if (layout.versioned() && version.isEmpty()) {
            throw new IllegalStateException("A versioned table needs a version");
        }

        return layout.tableLayout(version.orElse(0));
    }

    /** {@return this table with {@code field} set after its fields} */
    TableDescription withField(FieldDescription field) {
        var withField = new ArrayList<>(fields);
        withField.add(field);

        return new TableDescription(layout, version, withField, missing);
    }

    /** {@return this table with {@code version} in its header, which its layout must have a slot for} */
    TableDescription withVersion(long version) {
        return new TableDescription(layout, OptionalLong.of(version), fields, missing);
    }

    /** {@return this table read by a reader of a later revision, which knows {@code missing} more fields} */
    TableDescription withMissing(int missing) {
        return new TableDescription(layout, version, fields, OptionalInt.of(missing));
    }

    @Override
    public String toString() {
        return DescriptionJson.write(this).toString();
    }
}
