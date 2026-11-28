// Tables read back the values that they were built from. The examples show
// builders borrowing and owning values, `ArrayView` reading arrays in place or
// copying them, arrays of bools read into bools from their bytes, structs read
// from their bytes, and vector items found from their ends.

#include <gtest/gtest.h>
#include <rapidcheck/gtest.h>

#include <algorithm>
#include <bit>
#include <cmath>
#include <cstdint>
#include <limits>
#include <span>
#include <string>
#include <vector>

#include "common/at_byte_offset.hpp"
#include "common/build.hpp"
#include "common/read.hpp"
#include "common/strategy.hpp"

namespace tafla::test {
namespace {

std::vector<std::uint8_t> point(std::uint16_t x, float y) {
    std::vector<std::uint8_t> bytes(6);
    detail::store(bytes.data(), x);
    detail::store(bytes.data() + 2, y);
    return bytes;
}

template <class Values>
std::vector<std::uint64_t> bits_of(const Values& values) {
    std::vector<std::uint64_t> bits;
    for (double value : values) {
        bits.push_back(std::bit_cast<std::uint64_t>(value));
    }
    return bits;
}

std::vector<std::uint8_t> vector_of(std::span<const std::uint8_t> bytes) {
    return {bytes.begin(), bytes.end()};
}

/// Floats compare by their bits, so that -0 and NaN count. The table is read
/// at any byte offset from an aligned address, so that its arrays are read in
/// place when they're aligned in memory and copied when they aren't.
RC_GTEST_PROP(RoundTrip, TablesReadBackTheValuesTheyWereBuiltFrom, ()) {
    const auto table = *strategy::table();
    const auto byte_offset = *rc::gen::inRange<std::size_t>(0, 8);

    const auto bytes = to_bytes(table);
    const AtByteOffset copy(bytes, byte_offset);

    RC_ASSERT(read_table(copy.bytes(), table) == table);
}

TEST(RoundTrip, OwnedValues) {
    TableBuilder builder(3, TableLayout());
    builder.set_float64_array(std::vector<double>{1.5, -2.5});
    builder.set_string(std::string("owned and longer than a short string's buffer"));

    std::vector<NestedTable> items;
    items.emplace_back(std::vector<std::uint8_t>{1, 2});
    items.emplace_back(std::vector<std::uint8_t>{3});
    builder.set_vector(std::move(items));

    // Moving the builder keeps what it owns where its fields point
    const TableBuilder moved = std::move(builder);
    const auto bytes = moved.to_bytes();
    const TableReader reader(bytes, TableLayout());

    EXPECT_EQ(reader.get_float64_array(reader.root_offset(0)).to_vector(), (std::vector<double>{1.5, -2.5}));
    EXPECT_EQ(reader.get_string(reader.root_offset(1)), "owned and longer than a short string's buffer");

    const auto vector = reader.get_vector(reader.root_offset(2));
    std::vector<std::vector<std::uint8_t>> read;
    for (auto item : vector.items()) {
        read.push_back(vector_of(item));
    }
    EXPECT_EQ(read, (std::vector<std::vector<std::uint8_t>>{{1, 2}, {3}}));
}

TEST(RoundTrip, VectorItemsFoundFromTheirEnds) {
    const std::vector<std::uint8_t> first{1};
    const std::vector<std::uint8_t> second;
    const std::vector<std::uint8_t> third{2, 3};

    TableBuilder builder(1, TableLayout());
    std::vector<NestedTable> items;
    items.emplace_back(first);
    items.emplace_back(second);
    items.emplace_back(third);
    builder.set_vector(std::move(items));

    const auto bytes = builder.to_bytes().to_vector();
    const TableReader reader(bytes, TableLayout());
    const auto position = reader.root_offset(0);

    // The count and the three ends, at 4, then the items, from 20
    EXPECT_EQ(std::vector<std::uint8_t>(bytes.begin() + static_cast<std::ptrdiff_t>(position), bytes.begin() + 20),
              (std::vector<std::uint8_t>{3, 0, 0, 0, 17, 0, 0, 0, 17, 0, 0, 0, 19, 0, 0, 0}));
    EXPECT_EQ(std::vector<std::uint8_t>(bytes.begin() + 20, bytes.end()), (std::vector<std::uint8_t>{1, 2, 3}));

    const auto vector = reader.get_vector(position);
    EXPECT_EQ(vector.size(), 3U);
    EXPECT_EQ(vector_of(vector.get(2)), third);
    EXPECT_EQ(vector_of(vector.get(1)), second);
    EXPECT_EQ(vector_of(vector.get_in_place(1)), third);
    EXPECT_THROW(static_cast<void>(vector.get(3)), std::out_of_range);
}

TEST(RoundTrip, Structs) {
    // A struct of a uint16 and a float32, of 6 bytes, whose arrays are aligned
    // to 4
    auto points = point(1, 1.5F);
    const auto second = point(2, -0.25F);
    points.insert(points.end(), second.begin(), second.end());

    TableBuilder builder(3, TableLayout());
    builder.set_uint8(7);
    builder.set_struct(point(3, 2.5F));
    builder.set_struct_array(points, 6, Alignment::four);

    const auto bytes = builder.to_bytes();
    const TableReader reader(bytes, TableLayout());

    // The struct follows the uint8 at 12 unpadded, at 13
    auto position = reader.root_offset(1);
    EXPECT_EQ(position, 13U);
    EXPECT_EQ(vector_of(reader.get_struct(position, 6)), point(3, 2.5F));

    // The array's count is padded from 19 to 20, so that its structs start at
    // 24
    position = reader.root_offset(2);
    EXPECT_EQ(position, 20U);
    EXPECT_EQ(reader.get_uint32(position), 2U);

    const auto structs = reader.get_struct_array(position, 6);
    EXPECT_EQ(vector_of(structs), points);
    std::vector<float> ys;
    for (std::size_t start = 0; start < structs.size(); start += 6) {
        const TableReader item(structs.subspan(start, 6), TableLayout());
        ys.push_back(item.get_float32(2));
    }
    EXPECT_EQ(ys, (std::vector<float>{1.5F, -0.25F}));

    // Structs of 0 bytes take none, whatever the count
    EXPECT_TRUE(reader.get_struct_array(position, 0).empty());
}

TEST(RoundTrip, ArraysInPlaceWhenAligned) {
    const std::vector<double> values{1.5, -0.0, std::numeric_limits<double>::quiet_NaN()};
    const std::vector<std::uint8_t> bytes_values{1, 2, 3};

    TableBuilder builder(2, TableLayout());
    builder.set_uint8_array(bytes_values);
    builder.set_float64_array(values);
    const auto bytes = builder.to_bytes();

    for (std::size_t byte_offset = 0; byte_offset < 8; ++byte_offset) {
        const AtByteOffset copy(bytes, byte_offset);
        const TableReader reader(copy.bytes(), TableLayout());

        const auto uint8s = reader.get_uint8_array(reader.root_offset(0));
        EXPECT_GE(uint8s.data(), copy.bytes().data());
        EXPECT_LT(uint8s.data(), copy.bytes().data() + copy.bytes().size());

        // The array lies a multiple of 8 bytes from the table's start, and is
        // in place where the host aligns double to the table's offset
        const auto float64s = reader.get_float64_array(reader.root_offset(1));
        const bool in_place = detail::kHostOrder && byte_offset % alignof(double) == 0;

        EXPECT_EQ(float64s.as_span().has_value(), in_place) << byte_offset;
        EXPECT_EQ(bits_of(float64s.to_vector()), bits_of(values));
        EXPECT_EQ(bits_of(float64s), bits_of(values));
        EXPECT_EQ(std::bit_cast<std::uint64_t>(float64s[1]), std::bit_cast<std::uint64_t>(-0.0));
        EXPECT_THROW(static_cast<void>(float64s.at(3)), std::out_of_range);

        std::vector<double> reversed(float64s.begin(), float64s.end());
        std::vector<double> backwards;
        for (auto it = float64s.end(); it != float64s.begin();) {
            backwards.push_back(*--it);
        }
        std::reverse(reversed.begin(), reversed.end());
        EXPECT_EQ(bits_of(backwards), bits_of(reversed));
    }
}

TEST(RoundTrip, ArraysOfBools) {
    const bool borrowed[] = {true, false, true};

    TableBuilder builder(3, TableLayout());
    builder.set_bool_array(borrowed);
    builder.set_bool_array(Bools{false, true});
    builder.set_bool_array(std::span<const bool>());
    const auto bytes = builder.to_bytes();
    const TableReader reader(bytes, TableLayout());

    // A byte of 1 or 0 for each bool, after the count, unpadded
    EXPECT_EQ(reader.root_offset(1), 19U);
    EXPECT_EQ(vector_of(reader.get_uint8_array(reader.root_offset(0))), (std::vector<std::uint8_t>{1, 0, 1}));
    EXPECT_EQ(reader.get_bool_array(reader.root_offset(0)), (Bools{true, false, true}));
    EXPECT_EQ(reader.get_bool_array(reader.root_offset(1)), (Bools{false, true}));
    EXPECT_TRUE(reader.get_bool_array(reader.root_offset(2)).empty());
}

TEST(RoundTrip, BytesOfBoolsOtherThan0And1ReadAsTrueAndRebuildAs1) {
    TableBuilder builder(1, TableLayout());
    builder.set_bool_array(Bools{true, false, true, true});
    const auto written = builder.to_bytes();

    // The bools follow the offset and the count, at 8
    auto bytes = written.to_vector();
    const std::vector<std::uint8_t> others{2, 0, 0x80, 0xff};
    std::ranges::copy(others, bytes.begin() + 8);

    const TableReader reader(bytes, TableLayout());
    auto bools = reader.get_bool_array(reader.root_offset(0));
    EXPECT_EQ(bools, (Bools{true, false, true, true}));

    TableBuilder rebuilt(1, TableLayout());
    rebuilt.set_bool_array(std::move(bools));
    EXPECT_EQ(rebuilt.to_bytes(), written);
}

TEST(RoundTrip, BuilderNestedSeveralTimes) {
    const std::vector<double> values{1.0, 2.0};
    TableBuilder nested(2, TableLayout());
    nested.set_float64_array(values);
    nested.set_string("nested");
    const auto own = nested.to_bytes();

    TableBuilder borrowed(2, TableLayout());
    borrowed.set_table(nested);
    std::vector<NestedTable> builders;
    builders.emplace_back(nested);
    builders.emplace_back(nested);
    borrowed.set_vector(std::move(builders));

    TableBuilder copied(2, TableLayout());
    copied.set_table(own, Alignment::eight);
    std::vector<NestedTable> copies;
    copies.emplace_back(own);
    copies.emplace_back(own);
    copied.set_vector(std::move(copies));

    const auto bytes = borrowed.to_bytes();
    EXPECT_EQ(bytes, copied.to_bytes());

    // The builder is unchanged by nesting
    EXPECT_EQ(nested.to_bytes(), own);

    const TableReader reader(bytes, TableLayout());
    const auto vector = reader.get_vector(reader.root_offset(1));
    for (auto item : vector.items()) {
        EXPECT_EQ(vector_of(item), own.to_vector());
    }
}

}  // namespace
}  // namespace tafla::test
