#include "common/build.hpp"

#include <cstddef>
#include <type_traits>
#include <utility>
#include <vector>

namespace tafla::test {

TableBuilder builder_of(const Table& table) {
    return start_table(table, [](std::size_t fields, auto layout) { return TableBuilder(fields, layout); });
}

Alignment alignment_of(const Table& table) {
    try {
        return build(table).alignment();
    } catch (const BuildError&) {
        return Alignment::one;
    }
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
    set_field_on(field, builder, std::nullopt);
}

Bytes to_bytes_in_place(const Table& table) {
    auto builder = builder_of(table);
    for (const auto& field : table.fields) {
        set_field_on(field, builder, table.layout.offset_type);
    }
    return builder.to_bytes();
}

bool writes_in_place(const Nested& nested, std::optional<OffsetType> in_place) {
    const auto& layout = nested.table.layout;
    return in_place && nested.from == Source::builder && !layout.sized && !layout.versioned &&
           layout.offset_type == *in_place;
}

bool writes_in_place(const std::vector<Nested>& items, std::optional<OffsetType> in_place) {
    return std::ranges::all_of(items, [&](const Nested& item) {
        return writes_in_place(item, in_place) && item.table.fields.size() == items.front().table.fields.size();
    });
}

NestedTable build_nested(const Nested& nested) {
    auto builder = build(nested.table);
    if (nested.from == Source::builder) {
        return builder;
    }
    return builder.to_bytes();
}

}  // namespace tafla::test
