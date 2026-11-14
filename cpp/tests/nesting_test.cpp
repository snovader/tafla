// The code that tafla-gen generates from tables that each hold the next three
// times, as deep as tables nest, which compiles in a time that grows linearly
// with their depth, writes and reads them.

#include <gtest/gtest.h>

#include <array>
#include <cstdint>

#include "nesting/nesting.hpp"

namespace tafla::test {
namespace {

/// The fields of the innermost table that `fields` hold in their `level`
template <class Fields>
auto& innermost_fields(Fields& fields) {
    if constexpr (requires { fields.level; }) {
        return innermost_fields(fields.level);
    } else {
        return fields;
    }
}

/// The reader of the innermost table that `table` holds in its `level`
template <class Table>
auto innermost(const Table& table) {
    if constexpr (requires { table.level(); }) {
        return innermost(table.level());
    } else {
        return table;
    }
}

TEST(Nesting, WritesAndReadsTablesNestedAsDeepAsTheyNest) {
    nesting::RootFields fields;
    innermost_fields(fields).value = 7;
    const std::array<nesting::Level2Fields, 2> items{};
    fields.level.second = items;

    const auto bytes = fields.serialize();
    const nesting::Root root(bytes);
    EXPECT_EQ(innermost(root).value(), 7);
    EXPECT_EQ(root.first().size(), 0U);
    EXPECT_EQ(root.level().second().size(), 2U);
    EXPECT_EQ(innermost(root.level().second().get(1)).value(), 0);
}

}  // namespace
}  // namespace tafla::test
