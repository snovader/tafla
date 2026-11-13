#include "common/cases.hpp"

#include <charconv>
#include <fstream>
#include <iterator>
#include <limits>
#include <stdexcept>

namespace tafla::test {

namespace {

OffsetType offset_type_of(const std::string& name) {
    if (name == "uint8") {
        return OffsetType::uint8;
    }
    if (name == "uint16") {
        return OffsetType::uint16;
    }
    if (name == "uint32") {
        return OffsetType::uint32;
    }
    throw std::invalid_argument("No offset type " + name);
}

ScalarType scalar_type_of(const std::string& name) {
    static const std::pair<const char*, ScalarType> names[] = {
        {"int8", ScalarType::int8},       {"uint8", ScalarType::uint8},   {"int16", ScalarType::int16},
        {"uint16", ScalarType::uint16},   {"int32", ScalarType::int32},   {"uint32", ScalarType::uint32},
        {"int64", ScalarType::int64},     {"uint64", ScalarType::uint64}, {"float32", ScalarType::float32},
        {"float64", ScalarType::float64},
    };
    for (const auto& [type_name, type] : names) {
        if (name == type_name) {
            return type;
        }
    }
    throw std::invalid_argument("No scalar type " + name);
}

Field scalar_of(ScalarType type, const nlohmann::json& value) {
    switch (type) {
        case ScalarType::int8:
            return {value.get<std::int8_t>()};
        case ScalarType::uint8:
            return {value.get<std::uint8_t>()};
        case ScalarType::int16:
            return {value.get<std::int16_t>()};
        case ScalarType::uint16:
            return {value.get<std::uint16_t>()};
        case ScalarType::int32:
            return {value.get<std::int32_t>()};
        case ScalarType::uint32:
            return {value.get<std::uint32_t>()};
        case ScalarType::int64:
            return {integer_of<std::int64_t>(value)};
        case ScalarType::uint64:
            return {integer_of<std::uint64_t>(value)};
        case ScalarType::float32:
            return {static_cast<float>(float_of(value))};
        case ScalarType::float64:
            break;
    }
    return {float_of(value)};
}

template <class T, class Convert>
std::vector<T> array_of(const nlohmann::json& values, Convert convert) {
    std::vector<T> array;
    for (const auto& value : values) {
        array.push_back(convert(value));
    }
    return array;
}

template <class T>
std::vector<T> plain_array_of(const nlohmann::json& values) {
    return array_of<T>(values, [](const nlohmann::json& value) { return value.get<T>(); });
}

Nested nested_of(const nlohmann::json& nested) {
    const auto& from = nested.at("from").get_ref<const std::string&>();
    return {from == "builder" ? Source::builder : Source::bytes, table_of(nested.at("table"))};
}

Alignment alignment_of_bytes(std::uint64_t bytes) {
    switch (bytes) {
        case 1:
            return Alignment::one;
        case 2:
            return Alignment::two;
        case 4:
            return Alignment::four;
        case 8:
            return Alignment::eight;
        default:
            throw std::invalid_argument("No alignment of " + std::to_string(bytes) + " bytes");
    }
}

Field field_of(const nlohmann::json& field) {
    const auto& type = field.at("type").get_ref<const std::string&>();
    const auto& value = field.contains("value") ? field.at("value") : field;

    if (type == "bool") {
        return {Bool{value.get<bool>()}};
    }
    if (type == "int8[]") {
        return {plain_array_of<std::int8_t>(value)};
    }
    if (type == "uint8[]") {
        return {plain_array_of<std::uint8_t>(value)};
    }
    if (type == "int16[]") {
        return {plain_array_of<std::int16_t>(value)};
    }
    if (type == "uint16[]") {
        return {plain_array_of<std::uint16_t>(value)};
    }
    if (type == "int32[]") {
        return {plain_array_of<std::int32_t>(value)};
    }
    if (type == "uint32[]") {
        return {plain_array_of<std::uint32_t>(value)};
    }
    if (type == "int64[]") {
        return {array_of<std::int64_t>(value, integer_of<std::int64_t>)};
    }
    if (type == "uint64[]") {
        return {array_of<std::uint64_t>(value, integer_of<std::uint64_t>)};
    }
    if (type == "float32[]") {
        return {array_of<float>(value,
                                [](const nlohmann::json& element) { return static_cast<float>(float_of(element)); })};
    }
    if (type == "float64[]") {
        return {array_of<double>(value, float_of)};
    }
    if (type == "string") {
        return {value.get<std::string>()};
    }
    if (type == "struct") {
        Struct structure;
        for (const auto& scalar : field.at("fields")) {
            structure.scalars.push_back(field_of(scalar));
        }
        return {structure};
    }
    if (type == "struct[]") {
        StructArray array;
        for (const auto& scalar : field.at("fields")) {
            array.types.push_back(scalar_type_of(scalar.get<std::string>()));
        }
        for (const auto& item : field.at("items")) {
            std::vector<Field> scalars;
            for (std::size_t index = 0; index < array.types.size(); ++index) {
                scalars.push_back(scalar_of(array.types[index], item.at(index)));
            }
            array.items.push_back(std::move(scalars));
        }
        return {array};
    }
    if (type == "table") {
        std::optional<Alignment> alignment;
        if (field.contains("alignment")) {
            alignment = alignment_of_bytes(field.at("alignment").get<std::uint64_t>());
        }
        return {TableField{nested_of(field), alignment}};
    }
    if (type == "vector") {
        VectorField vector;
        for (const auto& item : field.at("items")) {
            vector.items.push_back(nested_of(item));
        }
        return {vector};
    }
    return scalar_of(scalar_type_of(type), value);
}

}  // namespace

std::filesystem::path conformance_directory() {
    return TAFLA_CONFORMANCE;
}

std::vector<std::uint8_t> read_file(const std::filesystem::path& path) {
    std::ifstream file(path, std::ios::binary);
    if (!file) {
        throw std::runtime_error("Can't read " + path.string());
    }
    return {std::istreambuf_iterator<char>(file), std::istreambuf_iterator<char>()};
}

nlohmann::json read_json(const std::filesystem::path& path) {
    std::ifstream file(path);
    if (!file) {
        throw std::runtime_error("Can't read " + path.string());
    }
    return nlohmann::json::parse(file);
}

double float_of(const nlohmann::json& value) {
    if (value.is_string()) {
        const auto& name = value.get_ref<const std::string&>();
        if (name == "NaN") {
            return std::numeric_limits<double>::quiet_NaN();
        }
        if (name == "Infinity") {
            return std::numeric_limits<double>::infinity();
        }
        if (name == "-Infinity") {
            return -std::numeric_limits<double>::infinity();
        }
        if (name == "-0") {
            return -0.0;
        }
        throw std::invalid_argument("No float " + name);
    }
    return value.get<double>();
}

template <class T>
T integer_of(const nlohmann::json& value) {
    const auto& text = value.get_ref<const std::string&>();
    T integer{};
    const auto [end, error] = std::from_chars(text.data(), text.data() + text.size(), integer);
    if (error != std::errc() || end != text.data() + text.size()) {
        throw std::invalid_argument("No integer " + text);
    }
    return integer;
}

template std::int64_t integer_of<std::int64_t>(const nlohmann::json&);
template std::uint64_t integer_of<std::uint64_t>(const nlohmann::json&);

Table table_of(const nlohmann::json& table) {
    const auto& layout = table.at("layout");
    Table result{
        {layout.at("sized").get<bool>(), layout.at("versioned").get<bool>(),
         offset_type_of(layout.at("offsetType").get<std::string>())},
        std::nullopt,
        {},
    };

    if (table.contains("version")) {
        result.version = table.at("version").get<std::uint32_t>();
    }
    for (const auto& field : table.at("fields")) {
        result.fields.push_back(field_of(field));
    }
    return result;
}

std::vector<ValidCase> valid_cases(const nlohmann::json& suite) {
    std::vector<ValidCase> cases;
    for (const auto& valid : suite.at("valid")) {
        cases.push_back({
            valid.at("name").get<std::string>(),
            read_file(conformance_directory() / valid.at("file").get<std::string>()),
            table_of(valid.at("table")),
        });
    }
    return cases;
}

const Table& table_of_case(const std::vector<ValidCase>& valid, const nlohmann::json& test_case) {
    const auto& name = test_case.at("case").get_ref<const std::string&>();
    for (const auto& valid_case : valid) {
        if (valid_case.name == name) {
            return valid_case.table;
        }
    }
    throw std::invalid_argument("There's no valid case " + name);
}

}  // namespace tafla::test
