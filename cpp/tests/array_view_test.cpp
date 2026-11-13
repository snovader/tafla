// Arrays are read in place, as ranges of their elements, at any alignment and
// in any host byte order

#include <gtest/gtest.h>

#include <algorithm>
#include <bit>
#include <cstdint>
#include <iterator>
#include <numeric>
#include <ranges>
#include <vector>

#include <tafla/tafla.hpp>

#include "common/at_byte_offset.hpp"

namespace tafla::test {
namespace {

static_assert(std::ranges::random_access_range<ArrayView<float>>);
static_assert(std::ranges::sized_range<ArrayView<std::uint64_t>>);
static_assert(std::random_access_iterator<ArrayView<std::int16_t>::iterator>);
static_assert(std::ranges::forward_range<Vector::Items>);

/// The little-endian bytes of `values`, at `byte_offset` from an aligned
/// address
template <class T>
AtByteOffset bytes_of(const std::vector<T>& values, std::size_t byte_offset) {
    std::vector<std::uint8_t> bytes(values.size() * sizeof(T));
    for (std::size_t index = 0; index < values.size(); ++index) {
        detail::store(bytes.data() + index * sizeof(T), values[index]);
    }
    return {bytes, byte_offset};
}

TEST(ArrayView, ReadsElementsAtAnyAlignment) {
    const std::vector<std::int32_t> values{-1, 0, 0x1234'5678, INT32_MIN, INT32_MAX};

    for (std::size_t byte_offset = 0; byte_offset < 8; ++byte_offset) {
        const auto copy = bytes_of(values, byte_offset);
        const ArrayView<std::int32_t> view(copy.bytes().data(), values.size());

        EXPECT_EQ(view.size(), values.size());
        EXPECT_FALSE(view.empty());
        EXPECT_EQ(view.front(), -1);
        EXPECT_EQ(view.back(), INT32_MAX);
        EXPECT_EQ(view.to_vector(), values);
        EXPECT_TRUE(std::ranges::equal(view, values));
        EXPECT_TRUE(std::ranges::equal(view | std::views::reverse, values | std::views::reverse));
        EXPECT_EQ(view.begin()[2], 0x1234'5678);
        EXPECT_EQ(view.end() - view.begin(), 5);
        EXPECT_EQ(std::accumulate(view.begin(), view.end(), std::int64_t{0}), std::int64_t{0x1234'5678} - 2);
        EXPECT_TRUE(std::ranges::equal(view.bytes(), copy.bytes()));

        const bool in_place = detail::kHostOrder && byte_offset % alignof(std::int32_t) == 0;
        EXPECT_EQ(view.as_span().has_value(), in_place) << byte_offset;
        if (const auto span = view.as_span()) {
            EXPECT_EQ(static_cast<const void*>(span->data()), static_cast<const void*>(copy.bytes().data()));
            EXPECT_TRUE(std::ranges::equal(*span, values));
        }
    }
}

TEST(ArrayView, BytesAreAlwaysInPlace) {
    const std::vector<std::int8_t> values{-128, 0, 127};
    for (std::size_t byte_offset = 0; byte_offset < 8; ++byte_offset) {
        const auto copy = bytes_of(values, byte_offset);
        const ArrayView<std::int8_t> view(copy.bytes().data(), values.size());

        ASSERT_TRUE(view.as_span().has_value());
        EXPECT_TRUE(std::ranges::equal(*view.as_span(), values));
    }
}

TEST(ArrayView, KeepsTheBitsOfFloats) {
    const std::vector<double> values{-0.0, std::bit_cast<double>(std::uint64_t{0x7ff0'0000'0000'0001}),
                                     std::numeric_limits<double>::denorm_min()};
    const auto copy = bytes_of(values, 3);
    const ArrayView<double> view(copy.bytes().data(), values.size());

    for (std::size_t index = 0; index < values.size(); ++index) {
        EXPECT_EQ(std::bit_cast<std::uint64_t>(view[index]), std::bit_cast<std::uint64_t>(values[index]));
    }
}

TEST(ArrayView, AnEmptyArrayHasNoElements) {
    const ArrayView<double> view;
    EXPECT_TRUE(view.empty());
    EXPECT_EQ(view.begin(), view.end());
    EXPECT_TRUE(view.to_vector().empty());
    EXPECT_THROW(static_cast<void>(view.at(0)), std::out_of_range);
}

TEST(ArrayView, AtThrowsPastTheEnd) {
    const auto copy = bytes_of(std::vector<std::uint16_t>{1, 2}, 0);
    const ArrayView<std::uint16_t> view(copy.bytes().data(), 2);
    EXPECT_EQ(view.at(1), 2);
    EXPECT_THROW(static_cast<void>(view.at(2)), std::out_of_range);
}

}  // namespace
}  // namespace tafla::test
