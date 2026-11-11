// Reads mutated tables as every valid table of the conformance suite describes
// them, following their offsets into nested tables, vectors, arrays, strings
// and structs, as the conformance test reads them.
//
// Its seed corpus is the suite's tables, `conformance/valid/*.bin`,
// `conformance/equivalent/*.bin` and `conformance/invalid/*.bin`, so that the
// fuzzer mutates tables that the descriptions read deep into. Reads may fail,
// but only with the decode error, and finding a vector's item by index must
// find what iterating finds.

#include <cstddef>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <span>
#include <string>
#include <vector>

#include "common/cases.hpp"
#include "common/read.hpp"

namespace {

/// The descriptions of the suite's valid tables, which the conformance test
/// parses
const std::vector<tafla::test::Table>& tables() {
    static const auto descriptions = [] {
        std::vector<tafla::test::Table> tables;
        const auto suite = tafla::test::read_json(tafla::test::conformance_directory() / "cases.json");
        for (const auto& valid : suite.at("valid")) {
            tables.push_back(tafla::test::table_of(valid.at("table")));
        }
        return tables;
    }();
    return descriptions;
}

}  // namespace

extern "C" int LLVMFuzzerTestOneInput(const std::uint8_t* data, std::size_t size) {
    // A copy, so that the sanitizer finds a read past the bytes
    const std::vector<std::uint8_t> bytes(data, data + size);

    for (const auto& table : tables()) {
        try {
            // The bytes hold values of the described types where it reads
            // them, which may differ from the description's
            const auto read = tafla::test::read_table(bytes, table);
            if (read.fields.size() != table.fields.size()) {
                std::abort();
            }
        } catch (const tafla::DecodeError& error) {
            if (std::strlen(error.what()) == 0) {
                std::abort();
            }
        } catch (const tafla::test::ReadMismatch& mismatch) {
            // The bytes read, but with a size or a number of items other than
            // described, and never an item found otherwise by index than by
            // iterating
            if (std::string(mismatch.what()).find("Finding item") != std::string::npos) {
                std::fprintf(stderr, "%s\n", mismatch.what());
                std::abort();
            }
        }
    }

    return 0;
}
