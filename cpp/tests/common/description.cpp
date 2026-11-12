#include "common/description.hpp"

#include <algorithm>
#include <bit>
#include <cstring>
#include <type_traits>

namespace tafla::test {

namespace {

template <class... Functions>
struct Overloaded : Functions... {
    using Functions::operator()...;
};

/// Whether two values are the same, with floats compared by their bits
template <class T>
bool same(const T& left, const T& right) {
    if constexpr (std::is_floating_point_v<T>) {
        return std::bit_cast<detail::BitsOf<T>>(left) == std::bit_cast<detail::BitsOf<T>>(right);
    } else if constexpr (std::is_same_v<T, std::vector<float>> || std::is_same_v<T, std::vector<double>>) {
        return std::ranges::equal(left, right, [](auto a, auto b) { return same(a, b); });
    } else if constexpr (std::is_same_v<T, Bool>) {
        return left.value == right.value;
    } else if constexpr (std::is_same_v<T, Struct>) {
        return left.scalars == right.scalars;
    } else if constexpr (std::is_same_v<T, StructArray>) {
        return left.types == right.types && left.items == right.items;
    } else if constexpr (std::is_same_v<T, TableField>) {
        return left.nested == right.nested && left.alignment == right.alignment;
    } else if constexpr (std::is_same_v<T, VectorField>) {
        return left.items == right.items;
    } else {
        return left == right;
    }
}

const char* name_of(ScalarType type) {
    switch (type) {
        case ScalarType::int8:
            return "int8";
        case ScalarType::uint8:
            return "uint8";
        case ScalarType::int16:
            return "int16";
        case ScalarType::uint16:
            return "uint16";
        case ScalarType::int32:
            return "int32";
        case ScalarType::uint32:
            return "uint32";
        case ScalarType::int64:
            return "int64";
        case ScalarType::uint64:
            return "uint64";
        case ScalarType::float32:
            return "float32";
        case ScalarType::float64:
            break;
    }
    return "float64";
}

template <class T>
void print_number(std::ostream& stream, T value) {
    if constexpr (std::is_floating_point_v<T>) {
        stream << value << "(0x" << std::hex << std::bit_cast<detail::BitsOf<T>>(value) << std::dec << ")";
    } else if constexpr (sizeof(T) == 1) {
        stream << static_cast<int>(value);
    } else {
        stream << value;
    }
}

void print_fields(std::ostream& stream, const std::vector<Field>& fields) {
    stream << "[";
    for (std::size_t index = 0; index < fields.size(); ++index) {
        stream << (index == 0 ? "" : ", ") << fields[index];
    }
    stream << "]";
}

}  // namespace

std::size_t size_of(ScalarType type) {
    switch (type) {
        case ScalarType::int8:
        case ScalarType::uint8:
            return 1;
        case ScalarType::int16:
        case ScalarType::uint16:
            return 2;
        case ScalarType::int32:
        case ScalarType::uint32:
        case ScalarType::float32:
            return 4;
        case ScalarType::int64:
        case ScalarType::uint64:
        case ScalarType::float64:
            break;
    }
    return 8;
}

Alignment alignment_of(ScalarType type) {
    return static_cast<Alignment>(size_of(type));
}

TableLayout<false> Layout::unversioned() const {
    auto layout = TableLayout().with_offset_type(offset_type);
    return sized ? layout.sized() : layout;
}

std::size_t Layout::header_size() const {
    return ((sized ? 1U : 0U) + (versioned ? 1U : 0U)) * offset_size(offset_type);
}

Alignment Table::alignment() const {
    auto alignment = Alignment::one;
    for (const auto& field : fields) {
        alignment = std::max(alignment, field.alignment());
    }
    return alignment;
}

std::optional<ScalarType> Field::scalar_type() const {
    return std::visit(Overloaded{
                          [](std::int8_t) -> std::optional<ScalarType> { return ScalarType::int8; },
                          [](std::uint8_t) -> std::optional<ScalarType> { return ScalarType::uint8; },
                          [](std::int16_t) -> std::optional<ScalarType> { return ScalarType::int16; },
                          [](std::uint16_t) -> std::optional<ScalarType> { return ScalarType::uint16; },
                          [](std::int32_t) -> std::optional<ScalarType> { return ScalarType::int32; },
                          [](std::uint32_t) -> std::optional<ScalarType> { return ScalarType::uint32; },
                          [](std::int64_t) -> std::optional<ScalarType> { return ScalarType::int64; },
                          [](std::uint64_t) -> std::optional<ScalarType> { return ScalarType::uint64; },
                          [](float) -> std::optional<ScalarType> { return ScalarType::float32; },
                          [](double) -> std::optional<ScalarType> { return ScalarType::float64; },
                          [](const auto&) -> std::optional<ScalarType> { return std::nullopt; },
                      },
                      value);
}

std::vector<std::uint8_t> Field::scalar_bytes() const {
    return std::visit(
        [](const auto& scalar) -> std::vector<std::uint8_t> {
            using T = std::decay_t<decltype(scalar)>;
            if constexpr (std::is_arithmetic_v<T>) {
                std::vector<std::uint8_t> bytes(sizeof(T));
                detail::store(bytes.data(), scalar);
                return bytes;
            } else {
                throw std::logic_error("A struct holds scalars");
            }
        },
        value);
}

Alignment Field::alignment() const {
    return std::visit(
        [](const auto& field) -> Alignment {
            using T = std::decay_t<decltype(field)>;
            if constexpr (std::is_same_v<T, StructArray>) {
                return struct_alignment(field.types);
            } else if constexpr (std::is_same_v<T, TableField>) {
                if (field.alignment) {
                    return *field.alignment;
                }
                return field.nested.from == Source::builder ? field.nested.table.alignment() : Alignment::four;
            } else if constexpr (std::is_same_v<T, std::vector<std::uint8_t>> ||
                                 std::is_same_v<T, std::vector<std::int8_t>> || std::is_same_v<T, std::string>) {
                return Alignment::one;
            } else if constexpr (requires { typename T::value_type; }) {
                return static_cast<Alignment>(sizeof(typename T::value_type));
            } else {
                return Alignment::one;
            }
        },
        value);
}

bool operator==(const Field& left, const Field& right) {
    if (left.value.index() != right.value.index()) {
        return false;
    }

    return std::visit(
        [&right](const auto& value) {
            using T = std::decay_t<decltype(value)>;
            return same(value, std::get<T>(right.value));
        },
        left.value);
}

bool operator==(const Table& left, const Table& right) {
    return left.layout == right.layout && left.version == right.version && left.fields == right.fields;
}

bool operator==(const Nested& left, const Nested& right) {
    return left.from == right.from && left.table == right.table;
}

std::ostream& operator<<(std::ostream& stream, const Field& field) {
    std::visit(Overloaded{
                   [&stream](const Bool& value) { stream << (value.value ? "true" : "false"); },
                   [&stream](const std::string& value) { stream << '"' << value << '"'; },
                   [&stream](const Struct& value) {
                       stream << "struct";
                       print_fields(stream, value.scalars);
                   },
                   [&stream](const StructArray& value) {
                       stream << "struct[" << value.items.size() << "] of";
                       for (auto type : value.types) {
                           stream << " " << name_of(type);
                       }
                       for (const auto& item : value.items) {
                           print_fields(stream, item);
                       }
                   },
                   [&stream](const TableField& value) {
                       stream << "table(" << (value.nested.from == Source::builder ? "builder" : "bytes");
                       if (value.alignment) {
                           stream << ", aligned " << alignment_bytes(*value.alignment);
                       }
                       stream << ") " << value.nested.table;
                   },
                   [&stream](const VectorField& value) {
                       stream << "vector[";
                       for (const auto& item : value.items) {
                           stream << (item.from == Source::builder ? "builder " : "bytes ") << item.table << "; ";
                       }
                       stream << "]";
                   },
                   [&stream](const auto& value) {
                       using T = std::decay_t<decltype(value)>;
                       if constexpr (std::is_arithmetic_v<T>) {
                           print_number(stream, value);
                       } else {
                           stream << "[";
                           for (std::size_t index = 0; index < value.size(); ++index) {
                               stream << (index == 0 ? "" : ", ");
                               print_number(stream, value[index]);
                           }
                           stream << "]";
                       }
                   },
               },
               field.value);
    return stream;
}

std::ostream& operator<<(std::ostream& stream, const Table& table) {
    stream << "{" << (table.layout.sized ? "sized " : "") << (table.layout.versioned ? "versioned " : "")
           << name(table.layout.offset_type);
    if (table.version) {
        stream << " v" << *table.version;
    }
    stream << " ";
    print_fields(stream, table.fields);
    return stream << "}";
}

std::size_t struct_size(const std::vector<ScalarType>& types) {
    std::size_t size = 0;
    for (auto type : types) {
        size += size_of(type);
    }
    return size;
}

Alignment struct_alignment(const std::vector<ScalarType>& types) {
    auto alignment = Alignment::one;
    for (auto type : types) {
        alignment = std::max(alignment, alignment_of(type));
    }
    return alignment;
}

std::vector<ScalarType> struct_types(const std::vector<Field>& scalars) {
    std::vector<ScalarType> types;
    types.reserve(scalars.size());
    for (const auto& scalar : scalars) {
        types.push_back(scalar.scalar_type().value());
    }
    return types;
}

std::vector<std::uint8_t> struct_bytes(const std::vector<Field>& scalars) {
    std::vector<std::uint8_t> bytes;
    for (const auto& scalar : scalars) {
        const auto scalar_bytes = scalar.scalar_bytes();
        bytes.insert(bytes.end(), scalar_bytes.begin(), scalar_bytes.end());
    }
    return bytes;
}

}  // namespace tafla::test
