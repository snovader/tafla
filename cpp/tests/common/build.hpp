// Builds tables as their descriptions set them out, with `TableBuilder`

#ifndef TAFLA_TEST_BUILD_HPP
#define TAFLA_TEST_BUILD_HPP

#include <tafla/tafla.hpp>

#include <algorithm>
#include <optional>
#include <type_traits>
#include <utility>
#include <vector>

#include "common/description.hpp"

namespace tafla::test {

/// Starts a builder of the table's layout, version and number of fields, with
/// no field set
TableBuilder builder_of(const Table& table);

/// Builds the table, borrowing its values, which must outlive the builder
TableBuilder build(const Table& table);

/// Writes the table with `TableBuilder`
Bytes to_bytes(const Table& table);

/// Sets the field on `builder`, which borrows its value, or owns the bytes of
/// structs
void set_field(const Field& field, TableBuilder& builder);

/// Builds the table to nest, as its builder or its bytes
NestedTable build_nested(const Nested& nested);

/// Writes the table with `TableBuilder`, recording each table nested from a
/// builder that it writes in place with `set_table(fields, fill)`, and each
/// vector of them, when its items hold as many fields, with
/// `set_vector(count, fields, fill)`, rather than from their builders. Its
/// bytes and errors are `to_bytes()`'s.
Bytes to_bytes_in_place(const Table& table);

/// Whether the table nested from `nested` is recorded in place in a table of
/// `in_place` offsets: from a builder without a header and of those offsets
bool records_in_place(const Nested& nested, std::optional<OffsetType> in_place);

/// Whether a vector of `items` is recorded in place: each of them is, and
/// they hold as many fields
bool records_in_place(const std::vector<Nested>& items, std::optional<OffsetType> in_place);

/// Sets the field on `target`, a builder or an item that one records in
/// place, as `set_field()` sets it, and with `in_place`, the offset type of
/// the builder's table, records the tables nested in it in place as
/// `to_bytes_in_place()` does
template <class Target>
void set_field_on(const Field& field, Target& target, std::optional<OffsetType> in_place) {
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
            } else if constexpr (std::is_same_v<T, Bool>) {
                target.set_uint8(value.value ? 1 : 0);
            } else if constexpr (std::is_same_v<T, std::vector<std::int8_t>>) {
                target.set_int8_array(std::span(value));
            } else if constexpr (std::is_same_v<T, std::vector<std::uint8_t>>) {
                target.set_uint8_array(std::span(value));
            } else if constexpr (std::is_same_v<T, std::vector<std::int16_t>>) {
                target.set_int16_array(std::span(value));
            } else if constexpr (std::is_same_v<T, std::vector<std::uint16_t>>) {
                target.set_uint16_array(std::span(value));
            } else if constexpr (std::is_same_v<T, std::vector<std::int32_t>>) {
                target.set_int32_array(std::span(value));
            } else if constexpr (std::is_same_v<T, std::vector<std::uint32_t>>) {
                target.set_uint32_array(std::span(value));
            } else if constexpr (std::is_same_v<T, std::vector<std::int64_t>>) {
                target.set_int64_array(std::span(value));
            } else if constexpr (std::is_same_v<T, std::vector<std::uint64_t>>) {
                target.set_uint64_array(std::span(value));
            } else if constexpr (std::is_same_v<T, std::vector<float>>) {
                target.set_float32_array(std::span(value));
            } else if constexpr (std::is_same_v<T, std::vector<double>>) {
                target.set_float64_array(std::span(value));
            } else if constexpr (std::is_same_v<T, std::string>) {
                target.set_string(std::string_view(value));
            } else if constexpr (std::is_same_v<T, Struct>) {
                target.set_struct(struct_bytes(value.scalars));
            } else if constexpr (std::is_same_v<T, StructArray>) {
                std::vector<std::uint8_t> bytes;
                for (const auto& item : value.items) {
                    const auto item_bytes = struct_bytes(item);
                    bytes.insert(bytes.end(), item_bytes.begin(), item_bytes.end());
                }
                target.set_struct_array(std::move(bytes), struct_size(value.types), struct_alignment(value.types));
            } else if constexpr (std::is_same_v<T, TableField>) {
                if (!value.alignment && records_in_place(value.nested, in_place)) {
                    const auto& fields = value.nested.table.fields;
                    target.set_table(fields.size(), [&](TableBuilder::Item& item) {
                        for (const auto& nested : fields) {
                            set_field_on(nested, item, in_place);
                        }
                    });
                    return;
                }

                auto table = build_nested(value.nested);
                if (value.alignment) {
                    target.set_table(std::move(table), *value.alignment);
                } else {
                    target.set_table(std::move(table));
                }
            } else {
                static_assert(std::is_same_v<T, VectorField>);
                if (records_in_place(value.items, in_place)) {
                    const auto fields = value.items.empty() ? 0 : value.items.front().table.fields.size();
                    target.set_vector(value.items.size(), fields, [&](std::size_t index, TableBuilder::Item& item) {
                        for (const auto& nested : value.items[index].table.fields) {
                            set_field_on(nested, item, in_place);
                        }
                    });
                    return;
                }

                std::vector<NestedTable> items;
                items.reserve(value.items.size());
                for (const auto& item : value.items) {
                    items.push_back(build_nested(item));
                }
                target.set_vector(std::move(items));
            }
        },
        field.value);
}

}  // namespace tafla::test

#endif
