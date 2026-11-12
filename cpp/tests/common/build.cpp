#include "common/build.hpp"

#include <type_traits>
#include <utility>
#include <vector>

namespace tafla::test {

TableBuilder builder_of(const Table& table) {
    const auto layout = table.layout.unversioned();
    const auto fields = table.fields.size();

    if (table.layout.versioned) {
        return {fields, layout.versioned(), table.version.value()};
    }
    return {fields, layout};
}

TableBuilder build(const Table& table) {
    auto builder = builder_of(table);
    for (const auto& field : table.fields) {
        set_field(field, builder);
    }
    return builder;
}

Bytes to_bytes(const Table& table) {
    return build(table).to_bytes();
}

void set_field(const Field& field, TableBuilder& builder) {
    std::visit(
        [&builder](const auto& value) {
            using T = std::decay_t<decltype(value)>;
            if constexpr (std::is_same_v<T, std::int8_t>) {
                builder.set_int8(value);
            } else if constexpr (std::is_same_v<T, std::uint8_t>) {
                builder.set_uint8(value);
            } else if constexpr (std::is_same_v<T, std::int16_t>) {
                builder.set_int16(value);
            } else if constexpr (std::is_same_v<T, std::uint16_t>) {
                builder.set_uint16(value);
            } else if constexpr (std::is_same_v<T, std::int32_t>) {
                builder.set_int32(value);
            } else if constexpr (std::is_same_v<T, std::uint32_t>) {
                builder.set_uint32(value);
            } else if constexpr (std::is_same_v<T, std::int64_t>) {
                builder.set_int64(value);
            } else if constexpr (std::is_same_v<T, std::uint64_t>) {
                builder.set_uint64(value);
            } else if constexpr (std::is_same_v<T, float>) {
                builder.set_float32(value);
            } else if constexpr (std::is_same_v<T, double>) {
                builder.set_float64(value);
            } else if constexpr (std::is_same_v<T, Bool>) {
                builder.set_uint8(value.value ? 1 : 0);
            } else if constexpr (std::is_same_v<T, std::vector<std::int8_t>>) {
                builder.set_int8_array(std::span(value));
            } else if constexpr (std::is_same_v<T, std::vector<std::uint8_t>>) {
                builder.set_uint8_array(std::span(value));
            } else if constexpr (std::is_same_v<T, std::vector<std::int16_t>>) {
                builder.set_int16_array(std::span(value));
            } else if constexpr (std::is_same_v<T, std::vector<std::uint16_t>>) {
                builder.set_uint16_array(std::span(value));
            } else if constexpr (std::is_same_v<T, std::vector<std::int32_t>>) {
                builder.set_int32_array(std::span(value));
            } else if constexpr (std::is_same_v<T, std::vector<std::uint32_t>>) {
                builder.set_uint32_array(std::span(value));
            } else if constexpr (std::is_same_v<T, std::vector<std::int64_t>>) {
                builder.set_int64_array(std::span(value));
            } else if constexpr (std::is_same_v<T, std::vector<std::uint64_t>>) {
                builder.set_uint64_array(std::span(value));
            } else if constexpr (std::is_same_v<T, std::vector<float>>) {
                builder.set_float32_array(std::span(value));
            } else if constexpr (std::is_same_v<T, std::vector<double>>) {
                builder.set_float64_array(std::span(value));
            } else if constexpr (std::is_same_v<T, std::string>) {
                builder.set_string(std::string_view(value));
            } else if constexpr (std::is_same_v<T, Struct>) {
                builder.set_struct(struct_bytes(value.scalars));
            } else if constexpr (std::is_same_v<T, StructArray>) {
                std::vector<std::uint8_t> bytes;
                for (const auto& item : value.items) {
                    const auto item_bytes = struct_bytes(item);
                    bytes.insert(bytes.end(), item_bytes.begin(), item_bytes.end());
                }
                builder.set_struct_array(std::move(bytes), struct_size(value.types), struct_alignment(value.types));
            } else if constexpr (std::is_same_v<T, TableField>) {
                auto table = build_nested(value.nested);
                if (value.alignment) {
                    builder.set_table(std::move(table), *value.alignment);
                } else {
                    builder.set_table(std::move(table));
                }
            } else {
                static_assert(std::is_same_v<T, VectorField>);
                std::vector<NestedTable> items;
                items.reserve(value.items.size());
                for (const auto& item : value.items) {
                    items.push_back(build_nested(item));
                }
                builder.set_vector(std::move(items));
            }
        },
        field.value);
}

NestedTable build_nested(const Nested& nested) {
    auto builder = build(nested.table);
    if (nested.from == Source::builder) {
        return builder;
    }
    return builder.to_bytes();
}

}  // namespace tafla::test
