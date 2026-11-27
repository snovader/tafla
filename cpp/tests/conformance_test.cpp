// Reads and rebuilds the tables of tafla's conformance suite, which the
// TypeScript runtime writes, and its cases of presence, which lie apart from
// them: absent fields, and fields that a later revision of a table appends.
// See `conformance/README.md` at the root of the repository for the format of
// their descriptions in `cases.json`.

#include <gtest/gtest.h>

#include <cstddef>
#include <filesystem>
#include <string>

#include "common/build.hpp"
#include "common/cases.hpp"
#include "common/read.hpp"
#include "common/write.hpp"

namespace tafla::test {
namespace {

nlohmann::json suite() {
    return read_json(conformance_directory() / "cases.json");
}

nlohmann::json presence_suite() {
    return read_json(presence_directory() / "cases.json");
}

std::vector<std::uint8_t> bytes_of(const nlohmann::json& test_case,
                                   const std::filesystem::path& directory = conformance_directory()) {
    return read_file(directory / test_case.at("file").get<std::string>());
}

/// Reads the root fields before the invalid case's `field`, which must hold
/// their described values, and then `field`, which must fail with a decode
/// error
void read_invalid_cases(const nlohmann::json& json, const std::filesystem::path& directory) {
    const auto valid = valid_cases(json, directory);
    ASSERT_FALSE(json.at("invalid").empty());

    for (const auto& test_case : json.at("invalid")) {
        const auto name = test_case.at("name").get<std::string>();
        const auto field = test_case.at("field").get<std::size_t>();
        const auto& table = table_of_case(valid, test_case);
        const auto bytes = bytes_of(test_case, directory);

        for (std::size_t index = 0; index < field; ++index) {
            EXPECT_EQ(read_root_field(bytes, table.layout, index, table.fields[index]), table.fields[index])
                << name << "/" << index;
        }

        EXPECT_THROW(static_cast<void>(read_root_field(bytes, table.layout, field, table.fields[field])), DecodeError)
            << name << "/" << field;
    }
}

TEST(Conformance, ReadsEveryValidTable) {
    const auto cases = valid_cases(suite());
    ASSERT_FALSE(cases.empty());

    for (const auto& test_case : cases) {
        EXPECT_EQ(read_table(test_case.bytes, test_case.table), test_case.table) << test_case.name;
    }
}

TEST(Conformance, RebuildsEveryValidTableByteForByte) {
    for (const auto& test_case : valid_cases(suite())) {
        EXPECT_EQ(to_bytes(test_case.table).to_vector(), test_case.bytes) << test_case.name;
    }
}

/// The sizer and writer write a table whose nested tables they can write in
/// place byte for byte as `TableBuilder` does
TEST(Conformance, WritesEveryValidTableWithNestedTablesInPlaceAsTheBuilderDoes) {
    for (const auto& test_case : valid_cases(suite())) {
        const auto table = nested_in_place(test_case.table);
        EXPECT_EQ(write(table), to_bytes(table)) << test_case.name;
    }
}

/// Reads every equivalent table, whose bytes differ from its valid case's, as
/// the valid case's description, header included
TEST(Conformance, ReadsEquivalentTablesAsTheirValidCase) {
    const auto json = suite();
    const auto valid = valid_cases(json);
    ASSERT_FALSE(json.at("equivalent").empty());

    for (const auto& test_case : json.at("equivalent")) {
        const auto& table = table_of_case(valid, test_case);
        EXPECT_EQ(read_table(bytes_of(test_case), table), table) << test_case.at("name");
    }
}

TEST(Conformance, ReadsInvalidTablesUpToTheFieldThatFails) {
    read_invalid_cases(suite(), conformance_directory());
}

/// Reads every valid table of presence as its description sets it out: an
/// absent field's offset must be 0, and a table that a reader of a later
/// revision reads must lack each of its `missing` fields
TEST(ConformancePresence, ReadsEveryValidTable) {
    const auto cases = valid_cases(presence_suite(), presence_directory());
    ASSERT_FALSE(cases.empty());

    for (const auto& test_case : cases) {
        EXPECT_EQ(read_table(test_case.bytes, test_case.table), test_case.table) << test_case.name;
    }
}

/// Rebuilds every valid table of presence byte for byte, with
/// `TableBuilder`, with tables recorded in place, and with `TableSizer` and
/// `TableWriter`
TEST(ConformancePresence, RebuildsEveryValidTableByteForByte) {
    for (const auto& test_case : valid_cases(presence_suite(), presence_directory())) {
        const auto table = nested_in_place(test_case.table);
        EXPECT_EQ(to_bytes(test_case.table).to_vector(), test_case.bytes) << test_case.name;
        EXPECT_EQ(to_bytes_in_place(table).to_vector(), test_case.bytes) << test_case.name;
        EXPECT_EQ(write(table).to_vector(), test_case.bytes) << test_case.name;
    }
}

TEST(ConformancePresence, ReadsInvalidTablesUpToTheFieldThatFails) {
    read_invalid_cases(presence_suite(), presence_directory());
}

}  // namespace
}  // namespace tafla::test
