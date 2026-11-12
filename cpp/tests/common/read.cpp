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

template <bool Versioned>
class Reader {
public:
    explicit Reader(ByteBuffer<Versioned> buffer) : buffer_(buffer) {}

    void check_size(const Layout& layout) const {
        const auto length = buffer_.bytes().size();
        if (layout.sized) {
            const auto size = buffer_.size();
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
        return read_field(buffer_.root_offset(index), field);
    }

    Field read_field(std::size_t position, const Field& field) const {
        return std::visit(
            [this, position](const auto& value) -> Field {
                using T = std::decay_t<decltype(value)>;
                const auto& buffer = buffer_;

                if constexpr (std::is_same_v<T, std::int8_t>) {
                    return {buffer.get_int8(position)};
                } else if constexpr (std::is_same_v<T, std::uint8_t>) {
                    return {buffer.get_uint8(position)};
                } else if constexpr (std::is_same_v<T, std::int16_t>) {
                    return {buffer.get_int16(position)};
                } else if constexpr (std::is_same_v<T, std::uint16_t>) {
                    return {buffer.get_uint16(position)};
                } else if constexpr (std::is_same_v<T, std::int32_t>) {
                    return {buffer.get_int32(position)};
                } else if constexpr (std::is_same_v<T, std::uint32_t>) {
                    return {buffer.get_uint32(position)};
                } else if constexpr (std::is_same_v<T, std::int64_t>) {
                    return {buffer.get_int64(position)};
                } else if constexpr (std::is_same_v<T, std::uint64_t>) {
                    return {buffer.get_uint64(position)};
                } else if constexpr (std::is_same_v<T, float>) {
                    return {buffer.get_float32(position)};
                } else if constexpr (std::is_same_v<T, double>) {
                    return {buffer.get_float64(position)};
                } else if constexpr (std::is_same_v<T, Bool>) {
                    return {Bool{buffer.get_uint8(position) != 0}};
                } else if constexpr (std::is_same_v<T, std::vector<std::int8_t>>) {
                    return {buffer.get_int8_array(position).to_vector()};
                } else if constexpr (std::is_same_v<T, std::vector<std::uint8_t>>) {
                    const auto bytes = buffer.get_uint8_array(position);
                    return {std::vector<std::uint8_t>(bytes.begin(), bytes.end())};
                } else if constexpr (std::is_same_v<T, std::vector<std::int16_t>>) {
                    return {buffer.get_int16_array(position).to_vector()};
                } else if constexpr (std::is_same_v<T, std::vector<std::uint16_t>>) {
                    return {buffer.get_uint16_array(position).to_vector()};
                } else if constexpr (std::is_same_v<T, std::vector<std::int32_t>>) {
                    return {buffer.get_int32_array(position).to_vector()};
                } else if constexpr (std::is_same_v<T, std::vector<std::uint32_t>>) {
                    return {buffer.get_uint32_array(position).to_vector()};
                } else if constexpr (std::is_same_v<T, std::vector<std::int64_t>>) {
                    return {buffer.get_int64_array(position).to_vector()};
                } else if constexpr (std::is_same_v<T, std::vector<std::uint64_t>>) {
                    return {buffer.get_uint64_array(position).to_vector()};
                } else if constexpr (std::is_same_v<T, std::vector<float>>) {
                    return {buffer.get_float32_array(position).to_vector()};
                } else if constexpr (std::is_same_v<T, std::vector<double>>) {
                    return {buffer.get_float64_array(position).to_vector()};
                } else if constexpr (std::is_same_v<T, std::string>) {
                    return {std::string(buffer.get_string(position))};
                } else if constexpr (std::is_same_v<T, Struct>) {
                    const auto types = struct_types(value.scalars);
                    const auto bytes = buffer.get_struct(position, struct_size(types));
                    return {Struct{read_struct(bytes, types)}};
                } else if constexpr (std::is_same_v<T, StructArray>) {
                    const auto size = struct_size(value.types);
                    const auto bytes = buffer.get_struct_array(position, size);

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
            read.fields.push_back(read_field(buffer_.field_offset(position, index), table.fields[index]));
        }
        return read;
    }

private:
    /// Reads a table nested with `set_table()`
    Table read_nested(std::size_t position, const Table& table) const {
        if (reads_in_place(table)) {
            return read_in_place(position, table);
        }

        // A position past the end leaves no bytes, which the first read fails
        // for
        const auto bytes = buffer_.bytes();
        return read_table(bytes.subspan(std::min(position, bytes.size())), table);
    }

    /// Reads a vector's items, after checking that finding each from its ends
    /// alone finds what iterating finds
    std::vector<Nested> read_vector(std::size_t position, const std::vector<Nested>& items) const {
        const auto vector = buffer_.get_vector(position);

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
                // At its start, in a buffer of its offset type, which is its
                // parent's
                const ByteBuffer buffer(*in_place[index].bytes, item.table.layout.unversioned());
                read.push_back({item.from, Reader<false>(buffer).read_in_place(0, item.table)});
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
        return !layout.sized && !layout.versioned && layout.offset_type == buffer_.layout().offset_type();
    }

    ByteBuffer<Versioned> buffer_;
};

Field read_scalar(const ByteBuffer<false>& buffer, ScalarType type, std::size_t position) {
    switch (type) {
        case ScalarType::int8:
            return {buffer.get_int8(position)};
        case ScalarType::uint8:
            return {buffer.get_uint8(position)};
        case ScalarType::int16:
            return {buffer.get_int16(position)};
        case ScalarType::uint16:
            return {buffer.get_uint16(position)};
        case ScalarType::int32:
            return {buffer.get_int32(position)};
        case ScalarType::uint32:
            return {buffer.get_uint32(position)};
        case ScalarType::int64:
            return {buffer.get_int64(position)};
        case ScalarType::uint64:
            return {buffer.get_uint64(position)};
        case ScalarType::float32:
            return {buffer.get_float32(position)};
        case ScalarType::float64:
            break;
    }
    return {buffer.get_float64(position)};
}

/// Reads the scalars of a struct of `types` at their offsets in its bytes,
/// which a `ByteBuffer` reads in any layout
std::vector<Field> read_struct(std::span<const std::uint8_t> bytes, const std::vector<ScalarType>& types) {
    const ByteBuffer buffer(bytes, TableLayout());
    std::vector<Field> scalars;

    std::size_t offset = 0;
    for (auto type : types) {
        scalars.push_back(read_scalar(buffer, type, offset));
        offset += size_of(type);
    }
    return scalars;
}

template <class Function>
auto with_reader(std::span<const std::uint8_t> bytes, const Layout& layout, Function function) {
    const auto unversioned = layout.unversioned();
    if (layout.versioned) {
        return function(Reader<true>(ByteBuffer(bytes, unversioned.versioned())));
    }
    return function(Reader<false>(ByteBuffer(bytes, unversioned)));
}

}  // namespace

Table read_table(std::span<const std::uint8_t> bytes, const Table& table) {
    const auto unversioned = table.layout.unversioned();

    if (table.layout.versioned) {
        const Reader<true> reader(ByteBuffer(bytes, unversioned.versioned()));
        reader.check_size(table.layout);
        const auto version = ByteBuffer(bytes, unversioned.versioned()).version();
        return {table.layout, version, reader.read_root_fields(table.fields)};
    }

    const Reader<false> reader(ByteBuffer(bytes, unversioned));
    reader.check_size(table.layout);
    return {table.layout, std::nullopt, reader.read_root_fields(table.fields)};
}

Field read_root_field(std::span<const std::uint8_t> bytes, const Layout& layout, std::size_t index,
                      const Field& field) {
    return with_reader(bytes, layout, [&](const auto& reader) { return reader.read_root_field(index, field); });
}

Field read_field(std::span<const std::uint8_t> bytes, const Layout& layout, std::size_t position, const Field& field) {
    return with_reader(bytes, layout, [&](const auto& reader) { return reader.read_field(position, field); });
}

}  // namespace tafla::test
