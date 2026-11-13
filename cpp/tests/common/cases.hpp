// Parses the conformance suite's descriptions of tables from `cases.json`.
// See `conformance/README.md` at the root of the repository.

#ifndef TAFLA_TEST_CASES_HPP
#define TAFLA_TEST_CASES_HPP

#include <cstdint>
#include <filesystem>
#include <string>
#include <vector>

#include <nlohmann/json.hpp>

#include "common/description.hpp"

namespace tafla::test {

/// The conformance suite's directory
std::filesystem::path conformance_directory();

/// Reads a file's bytes
std::vector<std::uint8_t> read_file(const std::filesystem::path& path);

/// Parses a JSON file
nlohmann::json read_json(const std::filesystem::path& path);

/// The table that a case's `table` describes
Table table_of(const nlohmann::json& table);

/// A float that JSON gives as a number or a name: `NaN`, `Infinity`,
/// `-Infinity` or `-0`
double float_of(const nlohmann::json& value);

/// A 64-bit integer that JSON gives as a decimal string
template <class T>
T integer_of(const nlohmann::json& value);

struct ValidCase {
    std::string name;
    std::vector<std::uint8_t> bytes;
    Table table;
};

/// The valid cases of `cases.json`, with their bytes
std::vector<ValidCase> valid_cases(const nlohmann::json& suite);

/// The description of the valid case that an equivalent or invalid case names
/// as its `case`
const Table& table_of_case(const std::vector<ValidCase>& valid, const nlohmann::json& test_case);

}  // namespace tafla::test

#endif
