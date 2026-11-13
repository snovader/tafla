// Tables that `TableBuilder` writes follow the format's rules, as `SPEC.md` at
// the root of the repository states them: the header holds the length and
// version, offsets lie within the table, values follow each other in field
// order with only the padding that aligns arrays and nested tables, multi-byte
// values are little-endian, and nested tables and vector items hold the bytes
// of their tables.

#include <gtest/gtest.h>
#include <rapidcheck/gtest.h>

#include <algorithm>
#include <cstddef>
#include <cstdint>
#include <optional>
#include <span>
#include <vector>

#include "common/build.hpp"
#include "common/strategy.hpp"

namespace tafla::test {
namespace {

/// The value of a scalar, array, string or struct field: the little-endian
/// bytes of a scalar, or of a struct's scalars, or the count of an array's
/// elements, of a string's bytes or of an array's structs and their
/// little-endian bytes, which start at a multiple of `alignment` from the
/// table's start
struct Value {
    bool array = false;
    std::size_t alignment = 1;
    std::size_t count = 0;
    std::vector<std::uint8_t> bytes;

    /// The value's length in a table of `width`-byte offsets and counts
    [[nodiscard]] std::size_t byte_length(std::size_t width) const {
        return (array ? width : 0) + bytes.size();
    }
};

template <class T>
Value array_value(const std::vector<T>& values) {
    Value value{true, sizeof(T), values.size(), std::vector<std::uint8_t>(values.size() * sizeof(T))};
    for (std::size_t index = 0; index < values.size(); ++index) {
        detail::store(value.bytes.data() + index * sizeof(T), values[index]);
    }
    return value;
}

/// Returns the value of a field that holds one rather than tables
std::optional<Value> value_of(const Field& field) {
    return std::visit(
        [&field](const auto& value) -> std::optional<Value> {
            using T = std::decay_t<decltype(value)>;
            if constexpr (std::is_arithmetic_v<T>) {
                return Value{false, 1, 0, field.scalar_bytes()};
            } else if constexpr (std::is_same_v<T, Bool>) {
                return Value{false, 1, 0, {static_cast<std::uint8_t>(value.value ? 1 : 0)}};
            } else if constexpr (std::is_same_v<T, std::string>) {
                return Value{true, 1, value.size(), std::vector<std::uint8_t>(value.begin(), value.end())};
            } else if constexpr (std::is_same_v<T, Struct>) {
                return Value{false, 1, 0, struct_bytes(value.scalars)};
            } else if constexpr (std::is_same_v<T, StructArray>) {
                Value array{true, alignment_bytes(struct_alignment(value.types)), value.items.size(), {}};
                for (const auto& item : value.items) {
                    const auto bytes = struct_bytes(item);
                    array.bytes.insert(array.bytes.end(), bytes.begin(), bytes.end());
                }
                return array;
            } else if constexpr (std::is_same_v<T, TableField> || std::is_same_v<T, VectorField>) {
                return std::nullopt;
            } else {
                return array_value(value);
            }
        },
        field.value);
}

/// The alignment of a table nested without one: a builder's is the largest
/// alignment of its fields, which is that of its arrays, of scalars and of
/// structs, and nested tables, and bytes' is 4
std::size_t alignment_of(const Field& field);

std::size_t default_alignment(const Nested& nested) {
    if (nested.from == Source::bytes) {
        return 4;
    }

    std::size_t alignment = 1;
    for (const auto& field : nested.table.fields) {
        alignment = std::max(alignment, alignment_of(field));
    }
    return alignment;
}

/// The multiple of bytes from the start of its table that a field starts at,
/// after its count when it's an array: an array's alignment, a nested table's
/// alignment, and 1 for any other
std::size_t alignment_of(const Field& field) {
    if (const auto value = value_of(field); value && value->array) {
        return value->alignment;
    }
    if (const auto* table = std::get_if<TableField>(&field.value)) {
        return table->alignment ? alignment_bytes(*table->alignment) : default_alignment(table->nested);
    }
    return 1;
}

/// The padding before `field` when it would start at `end`, which aligns an
/// array's elements, after its count, and a nested table's start
std::size_t padding_of(const Field& field, std::size_t end, std::size_t width) {
    const auto value = value_of(field);
    const std::size_t start = value && value->array ? end + width : end;
    const std::size_t alignment = alignment_of(field);
    return (alignment - start % alignment) % alignment;
}

/// Reads a little-endian slot of `width` bytes at `position`
std::size_t slot(std::span<const std::uint8_t> bytes, std::size_t position, std::size_t width) {
    if (position + width > bytes.size()) {
        throw std::out_of_range("A slot leaves the bytes");
    }

    std::size_t value = 0;
    for (std::size_t byte = width; byte > 0; --byte) {
        value = value << 8 | bytes[position + byte - 1];
    }
    return value;
}

/// The little-endian bytes of `value` in a slot of `width` bytes
std::vector<std::uint8_t> slot_bytes(std::size_t value, std::size_t width) {
    std::vector<std::uint8_t> bytes(width);
    for (std::size_t byte = 0; byte < width; ++byte) {
        bytes[byte] = static_cast<std::uint8_t>(value >> (8 * byte));
    }
    if (width < sizeof(std::size_t) && (value >> (8 * width)) != 0) {
        throw std::out_of_range("The value doesn't fit in the slot");
    }
    return bytes;
}

std::vector<std::uint8_t> range(std::span<const std::uint8_t> bytes, std::size_t start, std::size_t end) {
    return {bytes.begin() + static_cast<std::ptrdiff_t>(start), bytes.begin() + static_cast<std::ptrdiff_t>(end)};
}

/// A field and where it lies in its table's bytes
struct FieldIn {
    const Field* field;
    std::size_t position;
    std::size_t length;
};

/// A table in the bytes of a table: the table itself, a table nested in it or
/// one of its vector items, or a table in one of those
struct TableIn {
    std::span<const std::uint8_t> bytes;
    const Table* table;
    std::vector<FieldIn> fields;
    /// The bytes that the builder of a table in another writes on its own
    std::optional<std::vector<std::uint8_t>> own_bytes;

    /// The length of the header and offsets, where the values start
    [[nodiscard]] std::size_t offsets_end() const {
        return table->layout.header_size() + fields.size() * offset_size(table->layout.offset_type);
    }
};

void find_tables(std::span<const std::uint8_t> bytes, const Table& table,
                 std::optional<std::vector<std::uint8_t>> own_bytes, std::vector<TableIn>& tables);

/// Finds a nested table or vector item at `position`, and the tables in it,
/// and returns its length
std::size_t find_nested(std::span<const std::uint8_t> bytes, std::size_t position, const Nested& nested,
                        std::vector<TableIn>& tables) {
    auto own_bytes = to_bytes(nested.table).to_vector();
    const auto length = own_bytes.size();
    if (position + length > bytes.size()) {
        throw std::out_of_range("A nested table leaves its parent");
    }

    find_tables(bytes.subspan(position, length), nested.table, std::move(own_bytes), tables);
    return length;
}

/// Finds every table in `bytes`, which hold `table`, and the fields of each.
/// Fields lie at the positions that their offsets hold, and are as long as the
/// values that the description gives, and as the tables that their builders
/// write on their own.
void find_tables(std::span<const std::uint8_t> bytes, const Table& table,
                 std::optional<std::vector<std::uint8_t>> own_bytes, std::vector<TableIn>& tables) {
    const auto width = offset_size(table.layout.offset_type);
    const auto offsets = table.layout.header_size();
    std::vector<FieldIn> fields;

    for (std::size_t index = 0; index < table.fields.size(); ++index) {
        const auto& field = table.fields[index];
        const auto position = slot(bytes, offsets + index * width, width);

        std::size_t length = 0;
        if (const auto* nested = std::get_if<TableField>(&field.value)) {
            length = find_nested(bytes, position, nested->nested, tables);
        } else if (const auto* vector = std::get_if<VectorField>(&field.value)) {
            // The items follow the count and the ends
            auto end = position + (vector->items.size() + 1) * width;
            for (const auto& item : vector->items) {
                end += find_nested(bytes, end, item, tables);
            }
            length = end - position;
        } else {
            length = value_of(field)->byte_length(width);
        }

        fields.push_back({&field, position, length});
    }

    tables.push_back({bytes, &table, std::move(fields), std::move(own_bytes)});
}

std::vector<TableIn> tables_in(std::span<const std::uint8_t> bytes, const Table& table) {
    std::vector<TableIn> tables;
    find_tables(bytes, table, std::nullopt, tables);
    return tables;
}

RC_GTEST_PROP(Format, SizedTablesHoldTheirLengthInTheHeader, ()) {
    const auto table = *strategy::table();
    const auto bytes = to_bytes(table);

    for (const auto& found : tables_in(bytes, table)) {
        const auto& layout = found.table->layout;
        if (layout.sized) {
            const auto width = offset_size(layout.offset_type);
            RC_ASSERT(range(found.bytes, 0, width) == slot_bytes(found.bytes.size(), width));
        }
    }
}

RC_GTEST_PROP(Format, VersionedTablesHoldTheirVersionInTheHeader, ()) {
    const auto table = *strategy::table();
    const auto bytes = to_bytes(table);

    for (const auto& found : tables_in(bytes, table)) {
        const auto& layout = found.table->layout;
        if (found.table->version) {
            const auto width = offset_size(layout.offset_type);
            const auto position = layout.sized ? width : 0;
            RC_ASSERT(range(found.bytes, position, position + width) == slot_bytes(*found.table->version, width));
        }
    }
}

/// Every offset of every table lies after its header and offsets, and its
/// field ends within the table. A table nested with no fields is empty, and
/// can end its parent.
RC_GTEST_PROP(Format, OffsetsLieWithinTheirTable, ()) {
    const auto table = *strategy::table();
    const auto bytes = to_bytes(table);

    for (const auto& found : tables_in(bytes, table)) {
        const auto offsets_end = found.offsets_end();
        for (const auto& field : found.fields) {
            RC_ASSERT(offsets_end <= field.position);
            RC_ASSERT(field.position + field.length <= found.bytes.size());
        }
    }
}

/// From the start of the table that holds them, which is aligned in turn when
/// it's nested: an array of scalars at a multiple of their size, and an array
/// of structs of their largest scalar's
RC_GTEST_PROP(Format, ArraysStartAtAMultipleOfTheirAlignment, ()) {
    const auto table = *strategy::table();
    const auto bytes = to_bytes(table);

    for (const auto& found : tables_in(bytes, table)) {
        const auto width = offset_size(found.table->layout.offset_type);
        for (const auto& field : found.fields) {
            if (const auto value = value_of(*field.field); value && value->array) {
                RC_ASSERT((field.position + width) % value->alignment == 0U);
            }
        }
    }
}

/// Scalars hold their values' little-endian bytes, structs their scalars',
/// arrays and strings their counts' and their elements', and vectors their
/// counts' and their items' ends', which count from the vector's position,
/// after the ends
RC_GTEST_PROP(Format, ValuesAreLittleEndian, ()) {
    const auto table = *strategy::table();
    const auto bytes = to_bytes(table);

    for (const auto& found : tables_in(bytes, table)) {
        const auto width = offset_size(found.table->layout.offset_type);
        for (const auto& field : found.fields) {
            const auto value = range(found.bytes, field.position, field.position + field.length);

            if (const auto expected = value_of(*field.field)) {
                if (expected->array) {
                    RC_ASSERT(range(value, 0, width) == slot_bytes(expected->count, width));
                    RC_ASSERT(range(value, width, value.size()) == expected->bytes);
                } else {
                    RC_ASSERT(value == expected->bytes);
                }
            } else if (const auto* vector = std::get_if<VectorField>(&field.field->value)) {
                RC_ASSERT(range(value, 0, width) == slot_bytes(vector->items.size(), width));

                auto end = (vector->items.size() + 1) * width;
                for (std::size_t index = 0; index < vector->items.size(); ++index) {
                    end += to_bytes(vector->items[index].table).size();

                    const auto position = (index + 1) * width;
                    RC_ASSERT(range(value, position, position + width) == slot_bytes(end, width));
                }
            }
        }
    }
}

/// Values follow the header and offsets in field order, each after the
/// previous one, with only the zeroed padding that aligns an array's elements
/// and a nested table to their alignment between them, and the last one ends
/// the table
RC_GTEST_PROP(Format, ValuesFollowEachOtherWithOnlyZeroedPaddingToAlignThem, ()) {
    const auto table = *strategy::table();
    const auto bytes = to_bytes(table);

    for (const auto& found : tables_in(bytes, table)) {
        const auto width = offset_size(found.table->layout.offset_type);
        auto end = found.offsets_end();

        for (const auto& field : found.fields) {
            const auto position = end + padding_of(*field.field, end, width);
            RC_ASSERT(field.position == position);

            const auto padding = range(found.bytes, end, position);
            RC_ASSERT(std::ranges::all_of(padding, [](std::uint8_t byte) { return byte == 0; }));

            end = position + field.length;
        }

        RC_ASSERT(end == found.bytes.size());
    }
}

/// The bytes that a builder writes are the same nested in another table as on
/// their own
RC_GTEST_PROP(Format, NestedTablesAndVectorItemsHoldTheBytesOfTheirTables, ()) {
    const auto table = *strategy::table();
    const auto bytes = to_bytes(table);

    for (const auto& found : tables_in(bytes, table)) {
        if (found.own_bytes) {
            RC_ASSERT(range(found.bytes, 0, found.bytes.size()) == *found.own_bytes);
        }
    }
}

}  // namespace
}  // namespace tafla::test
