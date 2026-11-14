// Builds tables as their descriptions set them out, with any builder that has
// `tafla::TableBuilder`'s setters: the runtime's, or the prototype's. It is
// `tests/common/build.hpp`, generic over the builder.

#ifndef TAFLA_PROTOTYPE_BUILD_WITH_HPP
#define TAFLA_PROTOTYPE_BUILD_WITH_HPP

#include <tafla/tafla.hpp>

#include <optional>
#include <span>
#include <string_view>
#include <type_traits>
#include <utility>
#include <vector>

#include "common/build.hpp"
#include "common/description.hpp"
#include "direct_table_builder.hpp"

namespace tafla::prototype {

/// The type of a table that a builder of type `Builder` nests
template <class Builder>
struct NestedOf {
    using type = typename Builder::NestedTable;
};

template <>
struct NestedOf<TableBuilder> {
    using type = NestedTable;
};

/// Starts a builder of the layout, `fields` fields and the version, with no
/// field set
template <class Builder>
Builder builder_with(const test::Layout& layout, std::size_t fields, std::optional<std::uint32_t> version) {
    const auto unversioned = layout.unversioned();
    if (layout.versioned) {
        return {fields, unversioned.versioned(), version.value_or(0)};
    }
    return {fields, unversioned};
}

template <class Builder>
void set_field_with(const test::Field& field, Builder& builder, std::optional<OffsetType> in_place);

/// Builds the table, borrowing its values
template <class Builder>
Builder build_with(const test::Table& table, std::optional<OffsetType> in_place = std::nullopt) {
    auto builder = builder_with<Builder>(table.layout, table.fields.size(), table.version);
    for (const auto& field : table.fields) {
        set_field_with(field, builder, in_place);
    }
    return builder;
}

/// Builds a table to nest, as its builder or its bytes
template <class Builder>
typename NestedOf<Builder>::type nested_with(const test::Nested& nested) {
    auto builder = build_with<Builder>(nested.table);
    if (nested.from == test::Source::builder) {
        return builder;
    }
    return builder.to_bytes();
}

/// Sets the field on `target`, a builder or an item that one writes in
/// place, as `test::set_field_on()` sets it on a `tafla::TableBuilder`
template <class Builder, class Target>
void set_field_on_with(const test::Field& field, Target& target, std::optional<OffsetType> in_place) {
    using Nested = typename NestedOf<Builder>::type;

    std::visit(
        [&target, in_place](const auto& value) {
            using T = std::decay_t<decltype(value)>;
            if constexpr (std::is_same_v<T, std::int8_t>) {
                target.set_int8(value);
            } else if constexpr (std::is_same_v<T, std::uint8_t>) {
                target.set_uint8(value);
            } else if constexpr (std::is_same_v<T, std::int16_t>) {
                target.set_int16(value);
            } else if constexpr (std::is_same_v<T, std::uint16_t>) {
                target.set_uint16(value);
            } else if constexpr (std::is_same_v<T, std::int32_t>) {
                target.set_int32(value);
            } else if constexpr (std::is_same_v<T, std::uint32_t>) {
                target.set_uint32(value);
            } else if constexpr (std::is_same_v<T, std::int64_t>) {
                target.set_int64(value);
            } else if constexpr (std::is_same_v<T, std::uint64_t>) {
                target.set_uint64(value);
            } else if constexpr (std::is_same_v<T, float>) {
                target.set_float32(value);
            } else if constexpr (std::is_same_v<T, double>) {
                target.set_float64(value);
            } else if constexpr (std::is_same_v<T, test::Bool>) {
                target.set_uint8(value.value ? 1 : 0);
            } else if constexpr (std::is_same_v<T, std::vector<std::int8_t>>) {
                target.set_int8_array(std::span<const std::int8_t>(value));
            } else if constexpr (std::is_same_v<T, std::vector<std::uint8_t>>) {
                target.set_uint8_array(std::span<const std::uint8_t>(value));
            } else if constexpr (std::is_same_v<T, std::vector<std::int16_t>>) {
                target.set_int16_array(std::span<const std::int16_t>(value));
            } else if constexpr (std::is_same_v<T, std::vector<std::uint16_t>>) {
                target.set_uint16_array(std::span<const std::uint16_t>(value));
            } else if constexpr (std::is_same_v<T, std::vector<std::int32_t>>) {
                target.set_int32_array(std::span<const std::int32_t>(value));
            } else if constexpr (std::is_same_v<T, std::vector<std::uint32_t>>) {
                target.set_uint32_array(std::span<const std::uint32_t>(value));
            } else if constexpr (std::is_same_v<T, std::vector<std::int64_t>>) {
                target.set_int64_array(std::span<const std::int64_t>(value));
            } else if constexpr (std::is_same_v<T, std::vector<std::uint64_t>>) {
                target.set_uint64_array(std::span<const std::uint64_t>(value));
            } else if constexpr (std::is_same_v<T, std::vector<float>>) {
                target.set_float32_array(std::span<const float>(value));
            } else if constexpr (std::is_same_v<T, std::vector<double>>) {
                target.set_float64_array(std::span<const double>(value));
            } else if constexpr (std::is_same_v<T, std::string>) {
                target.set_string(std::string_view(value));
            } else if constexpr (std::is_same_v<T, test::Struct>) {
                target.set_struct(test::struct_bytes(value.scalars));
            } else if constexpr (std::is_same_v<T, test::StructArray>) {
                std::vector<std::uint8_t> bytes;
                for (const auto& item : value.items) {
                    const auto item_bytes = test::struct_bytes(item);
                    bytes.insert(bytes.end(), item_bytes.begin(), item_bytes.end());
                }
                target.set_struct_array(std::move(bytes), test::struct_size(value.types),
                                        test::struct_alignment(value.types));
            } else if constexpr (std::is_same_v<T, test::TableField>) {
                if (!value.alignment && test::records_in_place(value.nested, in_place)) {
                    const auto& fields = value.nested.table.fields;
                    target.set_table(fields.size(), [&](auto& item) {
                        for (const auto& nested : fields) {
                            set_field_on_with<Builder>(nested, item, in_place);
                        }
                    });
                    return;
                }

                auto table = nested_with<Builder>(value.nested);
                if (value.alignment) {
                    target.set_table(std::move(table), *value.alignment);
                } else {
                    target.set_table(std::move(table));
                }
            } else {
                static_assert(std::is_same_v<T, test::VectorField>);
                if (test::records_in_place(value.items, in_place)) {
                    const auto fields = value.items.empty() ? 0 : value.items.front().table.fields.size();
                    target.set_vector(value.items.size(), fields, [&](std::size_t index, auto& item) {
                        for (const auto& nested : value.items[index].table.fields) {
                            set_field_on_with<Builder>(nested, item, in_place);
                        }
                    });
                    return;
                }

                std::vector<Nested> items;
                items.reserve(value.items.size());
                for (const auto& item : value.items) {
                    items.push_back(nested_with<Builder>(item));
                }
                target.set_vector(std::move(items));
            }
        },
        field.value);
}

template <class Builder>
void set_field_with(const test::Field& field, Builder& builder, std::optional<OffsetType> in_place) {
    set_field_on_with<Builder>(field, builder, in_place);
}

}  // namespace tafla::prototype

#endif
