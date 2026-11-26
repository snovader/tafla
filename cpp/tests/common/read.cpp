#include "common/read.hpp"

#include <algorithm>
#include <optional>
#include <string>
#include <type_traits>
#include <vector>

namespace tafla::test {

namespace {

/// What reading an item returned: its bytes or its error
struct ItemResult {
    std::optional<std::span<const std::uint8_t>> bytes;
    std::optional<DecodeError> error;

    friend bool operator==(const ItemResult& left, const ItemResult& right) {
        if (left.bytes.has_value() != right.bytes.has_value()) {
            return false;
        }
        if (left.bytes) {
            return left.bytes->data() == right.bytes->data() && left.bytes->size() == right.bytes->size();
        }
        return left.error->kind() == right.error->kind() && left.error->position() == right.error->position();
    }
};

template <class Read>
ItemResult try_item(Read read) {
    try {
        return {read(), std::nullopt};
    } catch (const DecodeError& error) {
        return {std::nullopt, error};
    }
}

std::vector<Field> read_struct(std::span<const std::uint8_t> bytes, const std::vector<ScalarType>& types);

/// Reads a field described as absent, from its offset counted from its
/// table's start, which must be 0
Field read_absent(std::size_t offset) {
    if (offset != 0) {
        throw ReadMismatch("A field described as absent has the offset " + std::to_string(offset));
    }
    return {Absent{}};
}

template <bool Versioned>
class Reader {
public:
    explicit Reader(TableReader<Versioned> reader) : reader_(reader) {}

    void check_size(const Layout& layout) const {
        const auto length = reader_.bytes().size();
        if (layout.sized) {
            const auto size = reader_.size();
            if (size != length) {
                throw ReadMismatch("The header holds the size " + std::to_string(size) + " of a table of " +
                                   std::to_string(length) + " bytes");
            }
        }
    }

    std::vector<Field> read_root_fields(const std::vector<Field>& fields) const {
        std::vector<Field> read;
        for (std::size_t index = 0; index < fields.size(); ++index) {
            read.push_back(read_root_field(index, fields[index]));
        }
        return read;
    }

    Field read_root_field(std::size_t index, const Field& field) const {
        return read_field(reader_.root_offset(index), field);
    }

    /// Reads `field` at `position`, or an absent field, whose position is its
    /// offset in the root, which must be 0
    Field read_field(std::size_t position, const Field& field) const {
        return std::visit(
            [this, position](const auto& value) -> Field {
                using T = std::decay_t<decltype(value)>;
                const auto& reader = reader_;

                if constexpr (std::is_same_v<T, Absent>) {
                    return read_absent(position);
                } else if constexpr (std::is_same_v<T, std::int8_t>) {
                    return {reader.get_int8(position)};
                } else if constexpr (std::is_same_v<T, std::uint8_t>) {
                    return {reader.get_uint8(position)};
                } else if constexpr (std::is_same_v<T, std::int16_t>) {
                    return {reader.get_int16(position)};
                } else if constexpr (std::is_same_v<T, std::uint16_t>) {
                    return {reader.get_uint16(position)};
                } else if constexpr (std::is_same_v<T, std::int32_t>) {
                    return {reader.get_int32(position)};
                } else if constexpr (std::is_same_v<T, std::uint32_t>) {
                    return {reader.get_uint32(position)};
                } else if constexpr (std::is_same_v<T, std::int64_t>) {
                    return {reader.get_int64(position)};
                } else if constexpr (std::is_same_v<T, std::uint64_t>) {
                    return {reader.get_uint64(position)};
                } else if constexpr (std::is_same_v<T, float>) {
                    return {reader.get_float32(position)};
                } else if constexpr (std::is_same_v<T, double>) {
                    return {reader.get_float64(position)};
                } else if constexpr (std::is_same_v<T, Bool>) {
                    return {Bool{reader.get_uint8(position) != 0}};
                } else if constexpr (std::is_same_v<T, std::vector<std::int8_t>>) {
                    return {reader.get_int8_array(position).to_vector()};
                } else if constexpr (std::is_same_v<T, std::vector<std::uint8_t>>) {
                    const auto bytes = reader.get_uint8_array(position);
                    return {std::vector<std::uint8_t>(bytes.begin(), bytes.end())};
                } else if constexpr (std::is_same_v<T, std::vector<std::int16_t>>) {
                    return {reader.get_int16_array(position).to_vector()};
                } else if constexpr (std::is_same_v<T, std::vector<std::uint16_t>>) {
                    return {reader.get_uint16_array(position).to_vector()};
                } else if constexpr (std::is_same_v<T, std::vector<std::int32_t>>) {
                    return {reader.get_int32_array(position).to_vector()};
                } else if constexpr (std::is_same_v<T, std::vector<std::uint32_t>>) {
                    return {reader.get_uint32_array(position).to_vector()};
                } else if constexpr (std::is_same_v<T, std::vector<std::int64_t>>) {
                    return {reader.get_int64_array(position).to_vector()};
                } else if constexpr (std::is_same_v<T, std::vector<std::uint64_t>>) {
                    return {reader.get_uint64_array(position).to_vector()};
                } else if constexpr (std::is_same_v<T, std::vector<float>>) {
                    return {reader.get_float32_array(position).to_vector()};
                } else if constexpr (std::is_same_v<T, std::vector<double>>) {
                    return {reader.get_float64_array(position).to_vector()};
                } else if constexpr (std::is_same_v<T, std::string>) {
                    return {std::string(reader.get_string(position))};
                } else if constexpr (std::is_same_v<T, Struct>) {
                    const auto types = struct_types(value.scalars);
                    const auto bytes = reader.get_struct(position, struct_size(types));
                    return {Struct{read_struct(bytes, types)}};
                } else if constexpr (std::is_same_v<T, StructArray>) {
                    const auto size = struct_size(value.types);
                    const auto bytes = reader.get_struct_array(position, size);

                    StructArray array{value.types, {}};
                    for (std::size_t start = 0; start + size <= bytes.size(); start += size) {
                        array.items.push_back(read_struct(bytes.subspan(start, size), value.types));
                    }
                    return {array};
                } else if constexpr (std::is_same_v<T, TableField>) {
                    const Nested nested{value.nested.from, read_nested(position, value.nested.table)};
                    return {TableField{nested, value.alignment}};
                } else {
                    static_assert(std::is_same_v<T, VectorField>);
                    return {VectorField{read_vector(position, value.items)}};
                }
            },
            field.value);
    }

    /// Reads the fields of a table nested at `position`, from offsets counted
    /// from its start, within its parent's bytes
    Table read_in_place(std::size_t position, const Table& table) const {
        Table read{table.layout, std::nullopt, {}};
        for (std::size_t index = 0; index < table.fields.size(); ++index) {
            const auto& field = table.fields[index];
            const auto offset = reader_.field_offset(position, index);

            // An absent field's offset counts from the nested table's start
            if (std::holds_alternative<Absent>(field.value)) {
                read.fields.push_back(read_absent(offset - position));
            } else {
                read.fields.push_back(read_field(offset, field));
            }
        }

        if (table.missing) {
            read.missing = count_nested_missing(position, table);
        }
        return read;
    }

    /// Counts the fields after the root's own that it doesn't hold, of the
    /// `missing` fields that a reader of a later revision knows there, as
    /// `count_nested_missing()` counts them
    [[nodiscard]] std::size_t count_root_missing(const Table& table) const {
        const auto offset = [this](std::size_t index) { return reader_.root_offset(index); };
        return count_missing(table, table.layout.header_size(), offset);
    }

    /// Counts the fields after the nested table's own that it doesn't hold,
    /// of the `missing` fields that a reader of a later revision knows there:
    /// a field whose offset would end after the table's first value starts,
    /// or whose offset is 0, as padding before the first value reads. A table
    /// that holds one of them counts fewer than `missing`.
    [[nodiscard]] std::size_t count_nested_missing(std::size_t position, const Table& table) const {
        // A nested table's offsets count from its own start, where it has no
        // header
        const auto offset = [this, position](std::size_t index) {
            return reader_.field_offset(position, index) - position;
        };
        return count_missing(table, 0, offset);
    }

private:
    /// Counts the missing fields of a table whose offsets follow a header of
    /// `header_size` bytes, and whose field `index` lies at `offset(index)`
    /// from its start
    template <class Offset>
    std::size_t count_missing(const Table& table, std::size_t header_size, Offset offset) const {
        const auto width = offset_size(reader_.layout().offset_type());
        const auto first_value = offset(0);

        std::size_t count = 0;
        for (auto index = table.fields.size(); index < table.fields.size() + table.missing.value_or(0); ++index) {
            const bool holds_offset = header_size + (index + 1) * width <= first_value;
            if (!holds_offset || offset(index) == 0) {
                ++count;
            }
        }
        return count;
    }

    /// Reads a table nested with `set_table()`
    Table read_nested(std::size_t position, const Table& table) const {
        if (reads_in_place(table)) {
            return read_in_place(position, table);
        }

        // A position past the end leaves no bytes, which the first read fails
        // for
        const auto bytes = reader_.bytes();
        return read_table(bytes.subspan(std::min(position, bytes.size())), table);
    }

    /// Reads a vector's items, after checking that finding each from its ends
    /// alone finds what iterating finds
    std::vector<Nested> read_vector(std::size_t position, const std::vector<Nested>& items) const {
        const auto vector = reader_.get_vector(position);

        std::vector<ItemResult> iterated;
        std::vector<ItemResult> in_place;
        const auto all = vector.items();
        for (auto it = all.begin(); it != all.end(); ++it) {
            iterated.push_back(try_item([&it] { return *it; }));
        }
        const auto all_in_place = vector.items_in_place();
        for (auto it = all_in_place.begin(); it != all_in_place.end(); ++it) {
            in_place.push_back(try_item([&it] { return *it; }));
        }

        for (std::size_t index = 0; index < iterated.size(); ++index) {
            const auto got = try_item([&] { return vector.get(index); });
            const auto got_in_place = try_item([&] { return vector.get_in_place(index); });
            if (!(got == iterated[index]) || !(got_in_place == in_place[index])) {
                throw ReadMismatch("Finding item " + std::to_string(index) + " found another than iterating");
            }
        }

        for (std::size_t index = 0; index < iterated.size(); ++index) {
            if (!iterated[index].bytes) {
                throw *iterated[index].error;
            }
            if (!in_place[index].bytes) {
                throw *in_place[index].error;
            }
        }

        if (vector.size() != items.size() || iterated.size() != items.size()) {
            throw ReadMismatch("A vector holds " + std::to_string(vector.size()) + " items rather than " +
                               std::to_string(items.size()));
        }

        std::vector<Nested> read;
        for (std::size_t index = 0; index < items.size(); ++index) {
            const auto& item = items[index];
            if (reads_in_place(item.table)) {
                // At its start, with a reader of its offset type, which is its
                // parent's
                const TableReader reader(*in_place[index].bytes, item.table.layout.unversioned());
                read.push_back({item.from, Reader<false>(reader).read_in_place(0, item.table)});
            } else {
                read.push_back({item.from, read_table(*iterated[index].bytes, item.table)});
            }
        }
        return read;
    }

    /// Whether a nested table is read in place: it has no header, and its
    /// parent's offset type
    bool reads_in_place(const Table& table) const {
        const auto& layout = table.layout;
        return !layout.sized && !layout.versioned && layout.offset_type == reader_.layout().offset_type();
    }

    TableReader<Versioned> reader_;
};

Field read_scalar(const TableReader<false>& reader, ScalarType type, std::size_t position) {
    switch (type) {
        case ScalarType::int8:
            return {reader.get_int8(position)};
        case ScalarType::uint8:
            return {reader.get_uint8(position)};
        case ScalarType::int16:
            return {reader.get_int16(position)};
        case ScalarType::uint16:
            return {reader.get_uint16(position)};
        case ScalarType::int32:
            return {reader.get_int32(position)};
        case ScalarType::uint32:
            return {reader.get_uint32(position)};
        case ScalarType::int64:
            return {reader.get_int64(position)};
        case ScalarType::uint64:
            return {reader.get_uint64(position)};
        case ScalarType::float32:
            return {reader.get_float32(position)};
        case ScalarType::float64:
            break;
    }
    return {reader.get_float64(position)};
}

/// Reads the scalars of a struct of `types` at their offsets in its bytes,
/// which a `TableReader` reads in any layout
std::vector<Field> read_struct(std::span<const std::uint8_t> bytes, const std::vector<ScalarType>& types) {
    const TableReader reader(bytes, TableLayout());
    std::vector<Field> scalars;

    std::size_t offset = 0;
    for (auto type : types) {
        scalars.push_back(read_scalar(reader, type, offset));
        offset += size_of(type);
    }
    return scalars;
}

template <class Function>
auto with_reader(std::span<const std::uint8_t> bytes, const Layout& layout, Function function) {
    const auto unversioned = layout.unversioned();
    if (layout.versioned) {
        return function(Reader<true>(TableReader(bytes, unversioned.with_version(0))));
    }
    return function(Reader<false>(TableReader(bytes, unversioned)));
}

}  // namespace

Table read_table(std::span<const std::uint8_t> bytes, const Table& table) {
    const auto unversioned = table.layout.unversioned();

    if (table.layout.versioned) {
        const Reader<true> reader(TableReader(bytes, unversioned.with_version(0)));
        reader.check_size(table.layout);
        const auto version = TableReader(bytes, unversioned.with_version(0)).version();
        Table read{table.layout, version, reader.read_root_fields(table.fields)};
        if (table.missing) {
            read.missing = reader.count_root_missing(table);
        }
        return read;
    }

    const Reader<false> reader(TableReader(bytes, unversioned));
    reader.check_size(table.layout);
    Table read{table.layout, std::nullopt, reader.read_root_fields(table.fields)};
    if (table.missing) {
        read.missing = reader.count_root_missing(table);
    }
    return read;
}

Field read_root_field(std::span<const std::uint8_t> bytes, const Layout& layout, std::size_t index,
                      const Field& field) {
    return with_reader(bytes, layout, [&](const auto& reader) { return reader.read_root_field(index, field); });
}

Field read_field(std::span<const std::uint8_t> bytes, const Layout& layout, std::size_t position, const Field& field) {
    return with_reader(bytes, layout, [&](const auto& reader) { return reader.read_field(position, field); });
}

}  // namespace tafla::test
