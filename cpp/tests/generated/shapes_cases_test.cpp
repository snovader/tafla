// The code generated from `schemas/shapes.tafla` writes the tables of
// `conformance/generated`, which the generated TypeScript wrote, byte for
// byte, and reads back their values. `conformance/generated/README.md`
// describes the cases in `shapes.json`.

#include <gtest/gtest.h>

#include <bit>
#include <cmath>
#include <string>
#include <vector>

#include "common/cases.hpp"
#include "shapes_describe.hpp"

namespace tafla::test {
namespace {

namespace tables = shapes_tables;

/// The quiet NaNs that the cases' `"NaN"` stands for, which JavaScript writes
constexpr std::uint32_t kQuietNan32 = 0x7fc0'0000;

std::filesystem::path generated() {
    return conformance_directory() / "generated";
}

float float32_of(const nlohmann::json& value) {
    const double number = float_of(value);
    if (std::isnan(number)) {
        return std::bit_cast<float>(kQuietNan32);
    }

    const auto narrowed = static_cast<float>(number);
    EXPECT_EQ(std::bit_cast<std::uint64_t>(static_cast<double>(narrowed)), std::bit_cast<std::uint64_t>(number))
        << number << " isn't a float32";
    return narrowed;
}

template <class T, class Convert>
std::vector<T> elements(const nlohmann::json& values, Convert convert) {
    std::vector<T> elements;
    for (const auto& value : values) {
        elements.push_back(convert(value));
    }
    return elements;
}

/// What the generated code writes from a case's fields and reads from its
/// bytes, and the values that it should read
struct Outcome {
    std::vector<std::uint8_t> written;
    Table read;
    Table expected;
};

Outcome point(const nlohmann::json& json, std::span<const std::uint8_t> bytes) {
    const auto name = json.at("name").get<std::string>();
    const shapes::PointFields fields{
        .x = float_of(json.at("x")),
        .y = float_of(json.at("y")),
        .id = integer_of<std::int64_t>(json.at("id")),
        .name = name,
    };

    return {fields.serialize().to_vector(), tables::read(shapes::Point(bytes)), tables::describe(fields)};
}

Outcome document(const nlohmann::json& json, std::span<const std::uint8_t> bytes) {
    const auto name = json.at("name").get<std::string>();
    const auto& columns = json.at("columns");
    const auto keys =
        elements<std::uint32_t>(columns.at("keys"), [](const auto& key) { return key.template get<std::uint32_t>(); });
    const auto a = elements<float>(columns.at("a"), float32_of);
    const auto b = elements<float>(columns.at("b"), float32_of);
    const shapes::DocumentFields fields{
        .x = float_of(json.at("x")),
        .y = float_of(json.at("y")),
        .name = name,
        .columns = {.base = integer_of<std::int64_t>(columns.at("base")), .keys = keys, .a = a, .b = b},
    };

    return {fields.serialize().to_vector(), tables::read(shapes::Document(bytes)), tables::describe(fields)};
}

Outcome item_list(const nlohmann::json& json, std::span<const std::uint8_t> bytes) {
    const auto items = elements<shapes::ItemFields>(json.at("items"), [](const nlohmann::json& item) {
        const auto& pair = item.at("pair");
        return shapes::ItemFields{
            .value = float32_of(item.at("value")),
            .pair = {.first = float32_of(pair.at("first")), .second = float32_of(pair.at("second"))},
        };
    });
    const shapes::ItemListFields fields{.items = items};
    const shapes::ItemList list(bytes);

    EXPECT_EQ(tables::read_view(list, true), tables::describe(fields));
    EXPECT_EQ(tables::read_view(list, false), tables::describe(fields));
    return {fields.serialize().to_vector(), tables::read(list), tables::describe(fields)};
}

Outcome sample(const nlohmann::json& json, std::span<const std::uint8_t> bytes) {
    const shapes::Sample sample{
        .key = json.at("key").get<std::uint32_t>(),
        .a = float32_of(json.at("a")),
        .b = float32_of(json.at("b")),
    };

    std::vector<std::uint8_t> written(shapes::Sample::SIZE);
    sample.to_bytes(written.data());
    return {written, tables::describe(shapes::Sample::read(bytes)), tables::describe(sample)};
}

std::vector<std::pair<std::string, Outcome>> cases() {
    const auto suite = read_json(generated() / "shapes.json");
    EXPECT_EQ(suite.at("schema"), "schemas/shapes.tafla");

    std::vector<std::pair<std::string, Outcome>> outcomes;
    for (const auto& test_case : suite.at("cases")) {
        const auto name = test_case.at("name").get<std::string>();
        const auto bytes = read_file(generated() / test_case.at("file").get<std::string>());
        const auto& fields = test_case.at("fields");
        const auto type = test_case.at("type").get<std::string>();

        Outcome outcome;
        if (type == "Point") {
            outcome = point(fields, bytes);
        } else if (type == "Document") {
            outcome = document(fields, bytes);
        } else if (type == "ItemList") {
            outcome = item_list(fields, bytes);
        } else if (type == "Sample") {
            outcome = sample(fields, bytes);
        } else {
            ADD_FAILURE() << name << ": shapes.tafla has no " << type;
            continue;
        }

        EXPECT_EQ(outcome.written, bytes) << name;
        outcomes.emplace_back(name, std::move(outcome));
    }
    return outcomes;
}

TEST(ShapesCases, TheGeneratedCodeWritesEveryCaseByteForByteAndReadsItsValues) {
    const auto outcomes = cases();
    EXPECT_EQ(outcomes.size(), 7U);

    for (const auto& [name, outcome] : outcomes) {
        EXPECT_EQ(outcome.read, outcome.expected) << name;
    }
}

}  // namespace
}  // namespace tafla::test
