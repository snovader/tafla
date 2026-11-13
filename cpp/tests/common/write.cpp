#include "common/write.hpp"

#include <type_traits>
#include <vector>

namespace tafla::test {

namespace {

void add_nested(const Table& table, TableSizer& sizer) {
    const auto parent = sizer.start_table(table.fields.size());
    for (const auto& field : table.fields) {
        add_field(field, sizer);
    }
    sizer.end_table(parent);
}

void write_nested(const Table& table, TableWriter& writer) {
    const auto parent = writer.start_table(table.fields.size());
    for (std::size_t index = 0; index < table.fields.size(); ++index) {
        write_field(table.fields[index], writer, index);
    }
    writer.end_table(parent);
}

Field field_in_place(const Field& field, const Layout& layout);

Nested in_place(const Nested& nested, const Layout& layout) {
    Nested result{Source::builder, {layout, std::nullopt, {}}};
    for (const auto& field : nested.table.fields) {
        result.table.fields.push_back(field_in_place(field, layout));
    }
    return result;
}

Field field_in_place(const Field& field, const Layout& layout) {
    if (const auto* table = std::get_if<TableField>(&field.value)) {
        return {TableField{in_place(table->nested, layout), table->alignment}};
    }

    if (const auto* vector = std::get_if<VectorField>(&field.value)) {
        VectorField items;
        for (const auto& item : vector->items) {
            items.items.push_back(in_place(item, layout));
        }
        return {items};
    }

    return field;
}

/// Writes the table's fields, in order
void write_fields(const Table& table, TableWriter& writer) {
    for (std::size_t index = 0; index < table.fields.size(); ++index) {
        write_field(table.fields[index], writer, index);
    }
}

std::vector<std::uint8_t> struct_array_bytes(const StructArray& array) {
    std::vector<std::uint8_t> bytes;
    for (const auto& item : array.items) {
        const auto item_bytes = struct_bytes(item);
        bytes.insert(bytes.end(), item_bytes.begin(), item_bytes.end());
    }
    return bytes;
}

}  // namespace

TableSizer sizer_of(const Table& table) {
    const auto layout = table.layout.unversioned();
    const auto fields = table.fields.size();

    if (table.layout.versioned) {
        return {fields, layout.versioned(), table.version.value()};
    }
    return {fields, layout};
}

namespace {

/// A sizer of the table with its fields added
TableSizer sized_of(const Table& table) {
    auto sizer = sizer_of(table);
    for (const auto& field : table.fields) {
        add_field(field, sizer);
    }
    return sizer;
}

}  // namespace

Bytes write(const Table& table) {
    return write_as(table, table);
}

Bytes write_as(const Table& sized, const Table& written) {
    auto writer = sized_of(sized).writer();
    write_fields(written, writer);
    return writer.finish();
}

std::size_t write_into(const Table& table, std::span<std::uint8_t> bytes) {
    auto writer = sized_of(table).writer(bytes);
    write_fields(table, writer);
    return writer.finish_into();
}

Table nested_in_place(const Table& table) {
    const auto layout = Layout::headerless(table.layout.offset_type);
    Table result{table.layout, table.version, {}};
    for (const auto& field : table.fields) {
        result.fields.push_back(field_in_place(field, layout));
    }
    return result;
}

void add_field(const Field& field, TableSizer& sizer) {
    std::visit(
        [&sizer, &field](const auto& value) {
            using T = std::decay_t<decltype(value)>;
            if constexpr (std::is_same_v<T, std::int8_t>) {
                sizer.add_int8();
            } else if constexpr (std::is_same_v<T, std::uint8_t> || std::is_same_v<T, Bool>) {
                sizer.add_uint8();
            } else if constexpr (std::is_same_v<T, std::int16_t>) {
                sizer.add_int16();
            } else if constexpr (std::is_same_v<T, std::uint16_t>) {
                sizer.add_uint16();
            } else if constexpr (std::is_same_v<T, std::int32_t>) {
                sizer.add_int32();
            } else if constexpr (std::is_same_v<T, std::uint32_t>) {
                sizer.add_uint32();
            } else if constexpr (std::is_same_v<T, std::int64_t>) {
                sizer.add_int64();
            } else if constexpr (std::is_same_v<T, std::uint64_t>) {
                sizer.add_uint64();
            } else if constexpr (std::is_same_v<T, float>) {
                sizer.add_float32();
            } else if constexpr (std::is_same_v<T, double>) {
                sizer.add_float64();
            } else if constexpr (std::is_same_v<T, std::vector<std::int8_t>>) {
                sizer.add_int8_array(value.size());
            } else if constexpr (std::is_same_v<T, std::vector<std::uint8_t>>) {
                sizer.add_uint8_array(value.size());
            } else if constexpr (std::is_same_v<T, std::vector<std::int16_t>>) {
                sizer.add_int16_array(value.size());
            } else if constexpr (std::is_same_v<T, std::vector<std::uint16_t>>) {
                sizer.add_uint16_array(value.size());
            } else if constexpr (std::is_same_v<T, std::vector<std::int32_t>>) {
                sizer.add_int32_array(value.size());
            } else if constexpr (std::is_same_v<T, std::vector<std::uint32_t>>) {
                sizer.add_uint32_array(value.size());
            } else if constexpr (std::is_same_v<T, std::vector<std::int64_t>>) {
                sizer.add_int64_array(value.size());
            } else if constexpr (std::is_same_v<T, std::vector<std::uint64_t>>) {
                sizer.add_uint64_array(value.size());
            } else if constexpr (std::is_same_v<T, std::vector<float>>) {
                sizer.add_float32_array(value.size());
            } else if constexpr (std::is_same_v<T, std::vector<double>>) {
                sizer.add_float64_array(value.size());
            } else if constexpr (std::is_same_v<T, std::string>) {
                sizer.add_string(value);
            } else if constexpr (std::is_same_v<T, Struct>) {
                sizer.add_struct(struct_size(struct_types(value.scalars)));
            } else if constexpr (std::is_same_v<T, StructArray>) {
                sizer.add_struct_array(value.items.size(), struct_size(value.types), struct_alignment(value.types));
            } else if constexpr (std::is_same_v<T, TableField>) {
                sizer.add_table(field.alignment());
                add_nested(value.nested.table, sizer);
            } else {
                static_assert(std::is_same_v<T, VectorField>);
                sizer.add_vector(value.items.size());
                for (const auto& item : value.items) {
                    add_nested(item.table, sizer);
                }
            }
        },
        field.value);
}

void write_field(const Field& field, TableWriter& writer, std::size_t index) {
    std::visit(
        [&writer, &field, index](const auto& value) {
            using T = std::decay_t<decltype(value)>;
            if constexpr (std::is_same_v<T, std::int8_t>) {
                writer.set_int8(index, value);
            } else if constexpr (std::is_same_v<T, std::uint8_t>) {
                writer.set_uint8(index, value);
            } else if constexpr (std::is_same_v<T, std::int16_t>) {
                writer.set_int16(index, value);
            } else if constexpr (std::is_same_v<T, std::uint16_t>) {
                writer.set_uint16(index, value);
            } else if constexpr (std::is_same_v<T, std::int32_t>) {
                writer.set_int32(index, value);
            } else if constexpr (std::is_same_v<T, std::uint32_t>) {
                writer.set_uint32(index, value);
            } else if constexpr (std::is_same_v<T, std::int64_t>) {
                writer.set_int64(index, value);
            } else if constexpr (std::is_same_v<T, std::uint64_t>) {
                writer.set_uint64(index, value);
            } else if constexpr (std::is_same_v<T, float>) {
                writer.set_float32(index, value);
            } else if constexpr (std::is_same_v<T, double>) {
                writer.set_float64(index, value);
            } else if constexpr (std::is_same_v<T, Bool>) {
                writer.set_uint8(index, value.value ? 1 : 0);
            } else if constexpr (std::is_same_v<T, std::vector<std::int8_t>>) {
                writer.set_int8_array(index, value);
            } else if constexpr (std::is_same_v<T, std::vector<std::uint8_t>>) {
                writer.set_uint8_array(index, value);
            } else if constexpr (std::is_same_v<T, std::vector<std::int16_t>>) {
                writer.set_int16_array(index, value);
            } else if constexpr (std::is_same_v<T, std::vector<std::uint16_t>>) {
                writer.set_uint16_array(index, value);
            } else if constexpr (std::is_same_v<T, std::vector<std::int32_t>>) {
                writer.set_int32_array(index, value);
            } else if constexpr (std::is_same_v<T, std::vector<std::uint32_t>>) {
                writer.set_uint32_array(index, value);
            } else if constexpr (std::is_same_v<T, std::vector<std::int64_t>>) {
                writer.set_int64_array(index, value);
            } else if constexpr (std::is_same_v<T, std::vector<std::uint64_t>>) {
                writer.set_uint64_array(index, value);
            } else if constexpr (std::is_same_v<T, std::vector<float>>) {
                writer.set_float32_array(index, value);
            } else if constexpr (std::is_same_v<T, std::vector<double>>) {
                writer.set_float64_array(index, value);
            } else if constexpr (std::is_same_v<T, std::string>) {
                writer.set_string(index, value);
            } else if constexpr (std::is_same_v<T, Struct>) {
                writer.set_struct(index, struct_bytes(value.scalars));
            } else if constexpr (std::is_same_v<T, StructArray>) {
                writer.set_struct_array(index, struct_array_bytes(value), struct_size(value.types),
                                        struct_alignment(value.types));
            } else if constexpr (std::is_same_v<T, TableField>) {
                writer.set_table(index, field.alignment());
                write_nested(value.nested.table, writer);
            } else {
                static_assert(std::is_same_v<T, VectorField>);
                const auto vector = writer.set_vector(index, value.items.size());
                for (std::size_t item = 0; item < value.items.size(); ++item) {
                    write_nested(value.items[item].table, writer);
                    writer.set_item_end(vector, item);
                }
            }
        },
        field.value);
}

}  // namespace tafla::test
