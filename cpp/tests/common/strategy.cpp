#include "common/strategy.hpp"

#include <array>
#include <bit>
#include <iterator>
#include <limits>
#include <string>
#include <utility>
#include <vector>

namespace tafla::test::strategy {

namespace {

/// How deeply tables nest: the root's fields can hold tables whose fields
/// hold tables
constexpr unsigned kDepth = 2;

constexpr std::array kOffsetTypes = {OffsetType::uint8, OffsetType::uint16, OffsetType::uint32};

/// The most fields, array elements, characters, structs in an array and
/// vector items that a table holds. A table of `uint8` offsets, or nested in
/// one, holds few enough to fit in its 255 bytes more often than not, and
/// others hold enough for `uint16` lengths and offsets past 255.
struct Limits {
    std::size_t fields;
    std::size_t elements;
    std::size_t characters;
    std::size_t structs;
    std::size_t items;

    static Limits of(bool small) {
        if (small) {
            return {5, 6, 6, 2, 2};
        }
        return {8, 24, 16, 6, 4};
    }
};

/// Vectors of `0..=max` values of `values`
template <class T>
rc::Gen<std::vector<T>> vectors(std::size_t max, rc::Gen<T> values) {
    return rc::gen::mapcat(rc::gen::inRange<std::size_t>(0, max + 1),
                           [values](std::size_t count) { return rc::gen::container<std::vector<T>>(count, values); });
}

/// Integers across their range, and their limits and 0 more often than chance
template <class T>
rc::Gen<T> integers() {
    using Numbers = std::numeric_limits<T>;
    return rc::gen::weightedOneOf<T>({
        {1, rc::gen::element<T>(Numbers::min(), T{0}, Numbers::max())},
        {3, rc::gen::arbitrary<T>()},
    });
}

template <class T>
rc::Gen<T> floats() {
    using Bits = detail::BitsOf<T>;
    using Numbers = std::numeric_limits<T>;
    constexpr Bits sign = Bits{1} << (sizeof(T) * 8 - 1);

    const std::vector<T> special = {
        -T{0},
        Numbers::quiet_NaN(),
        std::bit_cast<T>(static_cast<Bits>(std::bit_cast<Bits>(Numbers::quiet_NaN()) | sign)),
        Numbers::infinity(),
        -Numbers::infinity(),
        Numbers::lowest(),
        Numbers::max(),
        Numbers::min(),
        Numbers::denorm_min(),
    };

    // Any bits make a float of every class, signaling NaNs among them
    return rc::gen::weightedOneOf<T>({
        {1, rc::gen::elementOf(special)},
        {3, rc::gen::map(rc::gen::arbitrary<Bits>(), [](Bits bits) { return std::bit_cast<T>(bits); })},
    });
}

template <class T>
rc::Gen<Field> fields_of(rc::Gen<T> values) {
    return rc::gen::map(std::move(values), [](T value) { return Field{std::move(value)}; });
}

/// Scalar fields of a type
rc::Gen<Field> scalar(ScalarType type) {
    switch (type) {
        case ScalarType::int8:
            return fields_of(integers<std::int8_t>());
        case ScalarType::uint8:
            return fields_of(integers<std::uint8_t>());
        case ScalarType::int16:
            return fields_of(integers<std::int16_t>());
        case ScalarType::uint16:
            return fields_of(integers<std::uint16_t>());
        case ScalarType::int32:
            return fields_of(integers<std::int32_t>());
        case ScalarType::uint32:
            return fields_of(integers<std::uint32_t>());
        case ScalarType::int64:
            return fields_of(integers<std::int64_t>());
        case ScalarType::uint64:
            return fields_of(integers<std::uint64_t>());
        case ScalarType::float32:
            return fields_of(float32());
        case ScalarType::float64:
            break;
    }
    return fields_of(float64());
}

void append_utf8(std::string& string, char32_t character) {
    const auto code = static_cast<std::uint32_t>(character);
    if (code < 0x80) {
        string += static_cast<char>(code);
    } else if (code < 0x800) {
        string += static_cast<char>(0xc0 | (code >> 6));
        string += static_cast<char>(0x80 | (code & 0x3f));
    } else if (code < 0x10000) {
        string += static_cast<char>(0xe0 | (code >> 12));
        string += static_cast<char>(0x80 | ((code >> 6) & 0x3f));
        string += static_cast<char>(0x80 | (code & 0x3f));
    } else {
        string += static_cast<char>(0xf0 | (code >> 18));
        string += static_cast<char>(0x80 | ((code >> 12) & 0x3f));
        string += static_cast<char>(0x80 | ((code >> 6) & 0x3f));
        string += static_cast<char>(0x80 | (code & 0x3f));
    }
}

}  // namespace

rc::Gen<std::string> strings(std::size_t characters) {
    // Three bytes, skipping the surrogates, which aren't characters
    const auto three_bytes = rc::gen::map(rc::gen::inRange<char32_t>(0x800, 0x10000 - 0x800), [](char32_t code) {
        return code >= 0xd800 ? static_cast<char32_t>(code + 0x800) : code;
    });
    const auto character = rc::gen::weightedOneOf<char32_t>({
        {3, rc::gen::inRange<char32_t>(U'a', U'z' + 1)},
        {1, rc::gen::just(U'﻿')},
        {1, rc::gen::inRange<char32_t>(0x80, 0x800)},
        {1, three_bytes},
        {1, rc::gen::inRange<char32_t>(0x10000, 0x110000)},
    });

    return rc::gen::map(vectors(characters, character), [](const std::vector<char32_t>& codes) {
        std::string string;
        for (auto code : codes) {
            append_utf8(string, code);
        }
        return string;
    });
}

namespace {

/// The types of the scalars of a struct, of one to four
rc::Gen<std::vector<ScalarType>> struct_scalar_types() {
    return rc::gen::mapcat(rc::gen::inRange<std::size_t>(1, 5), [](std::size_t count) {
        const std::vector<ScalarType> types(std::begin(kScalarTypes), std::end(kScalarTypes));
        return rc::gen::container<std::vector<ScalarType>>(count, rc::gen::elementOf(types));
    });
}

/// The scalars of a struct of `types`
rc::Gen<std::vector<Field>> struct_scalars(const std::vector<ScalarType>& types) {
    return rc::gen::exec([types] {
        std::vector<Field> scalars;
        for (auto type : types) {
            scalars.push_back(*scalar(type));
        }
        return scalars;
    });
}

/// Structs of one to four scalars of any types
rc::Gen<Field> struct_field() {
    return rc::gen::mapcat(struct_scalar_types(), [](const std::vector<ScalarType>& types) {
        return rc::gen::map(struct_scalars(types),
                            [](std::vector<Field> scalars) { return Field{Struct{std::move(scalars)}}; });
    });
}

/// Arrays of up to `structs` structs of the same scalars, empty too
rc::Gen<Field> struct_array(std::size_t structs) {
    return rc::gen::mapcat(struct_scalar_types(), [structs](const std::vector<ScalarType>& types) {
        return rc::gen::map(vectors(structs, struct_scalars(types)), [types](std::vector<std::vector<Field>> items) {
            return Field{StructArray{types, std::move(items)}};
        });
    });
}

/// Scalars, arrays, strings and structs, which hold values rather than tables
rc::Gen<Field> value(Limits limits) {
    const auto elements = limits.elements;
    const auto scalars = rc::gen::oneOf(
        scalar(ScalarType::int8), scalar(ScalarType::uint8), scalar(ScalarType::int16), scalar(ScalarType::uint16),
        scalar(ScalarType::int32), scalar(ScalarType::uint32), scalar(ScalarType::int64), scalar(ScalarType::uint64),
        scalar(ScalarType::float32), scalar(ScalarType::float64));
    const auto arrays = rc::gen::oneOf(
        fields_of(vectors(elements, integers<std::int8_t>())), fields_of(vectors(elements, integers<std::uint8_t>())),
        fields_of(vectors(elements, integers<std::int16_t>())), fields_of(vectors(elements, integers<std::uint16_t>())),
        fields_of(vectors(elements, integers<std::int32_t>())), fields_of(vectors(elements, integers<std::uint32_t>())),
        fields_of(vectors(elements, integers<std::int64_t>())), fields_of(vectors(elements, integers<std::uint64_t>())),
        fields_of(vectors(elements, float32())), fields_of(vectors(elements, float64())));

    // Each type of scalar and of array as likely as any other, and strings,
    // structs and arrays of structs each as likely as three of them
    return rc::gen::weightedOneOf<Field>({
        {10, scalars},
        {10, arrays},
        {3, fields_of(strings(limits.characters))},
        {3, struct_field()},
        {3, struct_array(limits.structs)},
    });
}

/// The generators of scalars, arrays, strings and structs, which a generator
/// of tables creates once for all its fields
struct Values {
    rc::Gen<Field> large = value(Limits::of(false));
    rc::Gen<Field> small = value(Limits::of(true));

    [[nodiscard]] rc::Gen<Field> of(bool is_small) const {
        return is_small ? small : large;
    }
};

const Values& values() {
    static const Values values;
    return values;
}

rc::Gen<Table> tables_of(rc::Gen<Layout> layouts, unsigned depth, bool small);

rc::Gen<Alignment> alignment() {
    return rc::gen::element(Alignment::one, Alignment::two, Alignment::four, Alignment::eight);
}

rc::Gen<Nested> nested(rc::Gen<Layout> layouts, unsigned depth, bool small) {
    const auto sources = rc::gen::element(Source::builder, Source::bytes);
    return rc::gen::map(
        rc::gen::tuple(sources, tables_of(std::move(layouts), depth, small)),
        [](std::tuple<Source, Table> nested) { return Nested{std::get<0>(nested), std::move(std::get<1>(nested))}; });
}

/// Tables nested with `set_table()`, which have no header, from their
/// builders or bytes, aligned by default or explicitly. They're read in place
/// when their offsets are as wide as their parent's, as they are more often
/// than not.
rc::Gen<Field> nested_table(OffsetType parent, unsigned depth, bool small) {
    const auto offset_types = rc::gen::weightedOneOf<OffsetType>({{2, rc::gen::just(parent)}, {1, offset_type()}});
    const auto layouts = rc::gen::map(offset_types, Layout::headerless);
    const auto alignments = rc::gen::weightedOneOf<std::optional<Alignment>>({
        {1, rc::gen::just(std::optional<Alignment>())},
        {1, rc::gen::map(alignment(), [](Alignment value) { return std::optional<Alignment>(value); })},
    });

    return rc::gen::map(rc::gen::tuple(nested(layouts, depth, small), alignments),
                        [](std::tuple<Nested, std::optional<Alignment>> table) {
                            return Field{TableField{std::move(std::get<0>(table)), std::get<1>(table)}};
                        });
}

/// Vectors of tables of any layout, from their builders or bytes
rc::Gen<Field> vector(unsigned depth, bool small) {
    return rc::gen::map(vectors(Limits::of(small).items, nested(layout(), depth, small)),
                        [](std::vector<Nested> items) { return Field{VectorField{std::move(items)}}; });
}

/// Fields of every type, and tables nested `depth` deep
rc::Gen<Field> field(OffsetType offset_type, unsigned depth, bool small) {
    if (depth == 0) {
        return values().of(small);
    }

    return rc::gen::weightedOneOf<Field>({
        {8, values().of(small)},
        {1, nested_table(offset_type, depth - 1, small)},
        {1, vector(depth - 1, small)},
    });
}

/// Versions that fit in the layout's slot, from 0 to its limit, for a
/// versioned layout
rc::Gen<std::optional<std::uint32_t>> version(const Layout& layout) {
    if (!layout.versioned) {
        return rc::gen::just(std::optional<std::uint32_t>());
    }

    const auto max = max_value(layout.offset_type);
    const auto versions = rc::gen::weightedOneOf<std::uint32_t>({
        {1, rc::gen::element<std::uint32_t>(0, max)},
        {1, rc::gen::map(rc::gen::arbitrary<std::uint32_t>(),
                         [max](std::uint32_t version) { return max == UINT32_MAX ? version : version % (max + 1); })},
    });
    return rc::gen::map(versions, [](std::uint32_t value) { return std::optional<std::uint32_t>(value); });
}

/// The longest that a field can be in a table of `width`-byte offsets, with
/// the most padding that it can need
std::uint64_t longest(const Field& field, std::uint64_t width);

std::uint64_t longest_table(const Table& table) {
    const std::uint64_t width = offset_size(table.layout.offset_type);
    std::uint64_t length = table.layout.header_size();
    for (const auto& field : table.fields) {
        length += width + longest(field, width);
    }
    return length;
}

std::uint64_t longest(const Field& field, std::uint64_t width) {
    return std::visit(
        [width](const auto& value) -> std::uint64_t {
            using T = std::decay_t<decltype(value)>;
            if constexpr (std::is_arithmetic_v<T>) {
                return sizeof(T);
            } else if constexpr (std::is_same_v<T, Bool>) {
                return 1;
            } else if constexpr (std::is_same_v<T, std::string>) {
                return width + value.size();
            } else if constexpr (std::is_same_v<T, Struct>) {
                return struct_size(struct_types(value.scalars));
            } else if constexpr (std::is_same_v<T, StructArray>) {
                const std::uint64_t size = struct_size(value.types);
                const std::uint64_t alignment = alignment_bytes(struct_alignment(value.types));
                return alignment - 1 + width + value.items.size() * size;
            } else if constexpr (std::is_same_v<T, TableField>) {
                return 7 + longest_table(value.nested.table);
            } else if constexpr (std::is_same_v<T, VectorField>) {
                std::uint64_t length = width;
                for (const auto& item : value.items) {
                    length += width + longest_table(item.table);
                }
                return length;
            } else {
                const std::uint64_t size = sizeof(typename T::value_type);
                return size - 1 + width + value.size() * size;
            }
        },
        field.value);
}

/// Keeps the table's first fields that surely fit in its offset type, as many
/// as its header and their longest lengths leave room for. The tables nested
/// in it were fitted to theirs.
Table fitted(Table table) {
    const std::uint64_t width = offset_size(table.layout.offset_type);
    const std::uint64_t max = max_value(table.layout.offset_type);

    std::uint64_t length = table.layout.header_size();
    std::size_t fits = 0;
    for (const auto& field : table.fields) {
        length += width + longest(field, width);
        if (length > max) {
            break;
        }
        ++fits;
    }

    table.fields.resize(fits);
    return table;
}

/// Tables of `layouts` whose fields hold tables `depth` deep, and hold few
/// values when `small`
rc::Gen<Table> tables_of(rc::Gen<Layout> layouts, unsigned depth, bool small) {
    // The fields of each offset type, created once rather than for each table
    std::vector<rc::Gen<std::vector<Field>>> fields_of_type;
    for (auto type : kOffsetTypes) {
        const bool is_small = small || type == OffsetType::uint8;
        fields_of_type.push_back(vectors(Limits::of(is_small).fields, field(type, depth, is_small)));
    }

    return rc::gen::mapcat(std::move(layouts), [fields_of_type](const Layout& layout) {
        const auto index = static_cast<std::size_t>(layout.offset_type);
        return rc::gen::map(rc::gen::tuple(version(layout), fields_of_type[index]),
                            [layout](std::tuple<std::optional<std::uint32_t>, std::vector<Field>> parts) {
                                return fitted(Table{layout, std::get<0>(parts), std::move(std::get<1>(parts))});
                            });
    });
}

}  // namespace

rc::Gen<Table> table() {
    return tables(layout());
}

rc::Gen<Table> tables(rc::Gen<Layout> layouts) {
    // At RapidCheck's nominal size, which its counts and numbers would
    // otherwise scale down in early tests, so that every test draws from the
    // whole of each range, as proptest does
    return rc::gen::resize(rc::kNominalSize, tables_of(std::move(layouts), kDepth, false));
}

rc::Gen<Layout> layout() {
    return layouts(offset_type());
}

rc::Gen<Layout> layouts(rc::Gen<OffsetType> offset_types) {
    return rc::gen::map(rc::gen::tuple(rc::gen::arbitrary<bool>(), rc::gen::arbitrary<bool>(), offset_types),
                        [](std::tuple<bool, bool, OffsetType> parts) {
                            return Layout{std::get<0>(parts), std::get<1>(parts), std::get<2>(parts)};
                        });
}

rc::Gen<OffsetType> offset_type() {
    return rc::gen::elementOf(kOffsetTypes);
}

rc::Gen<float> float32() {
    return rc::gen::resize(rc::kNominalSize, floats<float>());
}

rc::Gen<double> float64() {
    return rc::gen::resize(rc::kNominalSize, floats<double>());
}

}  // namespace tafla::test::strategy
