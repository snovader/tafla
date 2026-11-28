// `Bools` read from the bytes of an array of bools, true where a byte isn't
// 0, and own them, one after another, as a span of bools views them

#include <gtest/gtest.h>

#include <algorithm>
#include <cstdint>
#include <ranges>
#include <span>
#include <utility>
#include <vector>

#include <tafla/tafla.hpp>

namespace tafla::test {
namespace {

static_assert(std::ranges::contiguous_range<Bools>);
static_assert(std::ranges::sized_range<const Bools>);

TEST(Bools, ReadBytesOtherThan0AsTrue) {
    const std::vector<std::uint8_t> bytes{0, 1, 2, 0x80, 0xff, 0};
    EXPECT_EQ(Bools::from_bytes(bytes), (Bools{false, true, true, true, true, false}));
    EXPECT_TRUE(Bools::from_bytes({}).empty());
}

TEST(Bools, AreViewedAsASpanOfTheirBools) {
    const Bools bools{true, false};
    const std::span<const bool> span = bools;

    EXPECT_EQ(span.data(), bools.data());
    EXPECT_EQ(span.size(), 2U);
    EXPECT_EQ(bools.span().data(), bools.data());
}

TEST(Bools, OfASizeAreFalse) {
    const Bools bools(3);
    EXPECT_EQ(bools, (Bools{false, false, false}));
}

TEST(Bools, CopiesAreTheirOwn) {
    const std::vector<bool> values{true, false, true};
    Bools bools(values.size());
    std::ranges::copy(values, bools.begin());

    Bools copy = bools;
    copy[0] = false;
    EXPECT_EQ(bools, (Bools{true, false, true}));
    EXPECT_EQ(copy, (Bools{false, false, true}));

    copy = bools;
    EXPECT_EQ(copy, bools);
    EXPECT_EQ(Bools(std::span<const bool>(bools.data(), 2)), (Bools{true, false}));
}

TEST(Bools, MovedFromAreEmpty) {
    Bools bools{true, true};
    const bool* const data = bools.data();

    Bools moved = std::move(bools);
    EXPECT_EQ(moved.data(), data);
    EXPECT_TRUE(bools.empty());  // NOLINT(bugprone-use-after-move): moved from, and empty

    bools = std::move(moved);
    EXPECT_EQ(bools, (Bools{true, true}));
    EXPECT_TRUE(moved.empty());  // NOLINT(bugprone-use-after-move): moved from, and empty
}

}  // namespace
}  // namespace tafla::test
