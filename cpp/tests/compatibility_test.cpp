// Tables that the TypeScript runtime wrote, which this runtime must write byte
// for byte from the same fields and read back. The bytes come from the
// TypeScript `TableBuilder` with the same calls, field for field, as the
// builders here.

#include <gtest/gtest.h>

#include <bit>
#include <cstdint>
#include <limits>
#include <string>
#include <vector>

#include <tafla/tafla.hpp>

namespace tafla::test {
namespace {

const std::vector<std::uint8_t> kKinds{
    96,  0,   0,   0,   97,  0,   0,   0,   98,  0,   0,   0,   100, 0,   0,   0,   102, 0,   0,   0,  106, 0,   0,
    0,   110, 0,   0,   0,   118, 0,   0,   0,   126, 0,   0,   0,   130, 0,   0,   0,   138, 0,   0,  0,   144, 0,
    0,   0,   150, 0,   0,   0,   158, 0,   0,   0,   164, 0,   0,   0,   172, 0,   0,   0,   188, 0,  0,   0,   204,
    0,   0,   0,   216, 0,   0,   0,   236, 0,   0,   0,   252, 0,   0,   0,   0,   1,   0,   0,   16, 1,   0,   0,
    20,  1,   0,   0,   128, 255, 0,   128, 255, 255, 0,   0,   0,   128, 255, 255, 255, 255, 0,   0,  0,   0,   0,
    0,   0,   128, 255, 255, 255, 255, 255, 255, 255, 255, 0,   0,   192, 63,  0,   0,   0,   0,   0,  0,   2,   192,
    2,   0,   0,   0,   255, 2,   1,   0,   0,   0,   3,   0,   2,   0,   0,   0,   252, 255, 5,   0,  1,   0,   0,
    0,   6,   0,   1,   0,   0,   0,   249, 255, 255, 255, 2,   0,   0,   0,   8,   0,   0,   0,   9,  0,   0,   0,
    0,   0,   0,   0,   1,   0,   0,   0,   246, 255, 255, 255, 255, 255, 255, 255, 0,   0,   0,   0,  1,   0,   0,
    0,   11,  0,   0,   0,   0,   0,   0,   0,   3,   0,   0,   0,   0,   0,   72,  65,  0,   0,   0,  128, 0,   0,
    128, 127, 0,   0,   0,   0,   1,   0,   0,   0,   0,   0,   0,   0,   0,   0,   43,  192, 0,   0,  0,   0,   0,
    0,   0,   0,   12,  0,   0,   0,   195, 169, 226, 130, 172, 240, 159, 152, 128, 239, 187, 191, 0,  0,   0,   0,
    2,   0,   0,   0,   14,  0,   0,   0,   14,  0,   0,   0,   1,   2};

const std::vector<std::uint8_t> kDocument{
    146, 0,   0,   0,   3,  0,   0,   0,   28,  0,   0,   0,   36,  0,   0,   0,  56, 0, 0,   0,  108,
    0,   0,   0,   109, 0,  0,   0,   82,  73,  157, 128, 38,  170, 77,  64,  16, 0,  0, 0,   69, 117,
    114, 111, 112, 101, 47, 83,  116, 111, 99,  107, 104, 111, 108, 109, 12,  0,  0,  0, 20,  0,  0,
    0,   36,  0,   0,   0,  128, 98,  97,  218, 255, 255, 255, 255, 3,   0,   0,  0,  0, 0,   0,  0,
    128, 81,  1,   0,   0,  163, 2,   0,   3,   0,   0,   0,   0,   0,   174, 65, 0,  0, 148, 65, 0,
    0,   80,  192, 251, 2,  0,   0,   0,   34,  0,   0,   0,   37,  0,   0,   0,  8,  0, 0,   0,  12,
    0,   0,   0,   0,   0,  0,   63,  3,   0,   0,   0,   7,   0,   8,   0,   9,  0,  1, 2,   3};

const std::vector<std::uint8_t> kSmall{15, 4, 7, 12, 2, 104, 105, 2, 1, 0, 2, 0, 7, 7, 7};

const std::vector<std::uint8_t> kMedium{44, 0, 1,  2, 10, 0,  20, 0, 0, 0, 2, 0, 0, 0, 128, 63, 0, 0, 0, 64, 2,   0,
                                        8,  0, 24, 0, 1,  42, 4,  0, 0, 0, 1, 0, 0, 0, 0,   0,  0, 0, 0, 0,  244, 63};

std::vector<std::uint8_t> vector_of(std::span<const std::uint8_t> bytes) {
    return {bytes.begin(), bytes.end()};
}

TEST(Compatibility, EveryKindOfField) {
    const std::vector<std::int8_t> int8s{-1, 2};
    const std::vector<std::uint8_t> uint8s{3};
    const std::vector<std::int16_t> int16s{-4, 5};
    const std::vector<std::uint16_t> uint16s{6};
    const std::vector<std::int32_t> int32s{-7};
    const std::vector<std::uint32_t> uint32s{8, 9};
    const std::vector<std::int64_t> int64s{-10};
    const std::vector<std::uint64_t> uint64s{11};
    const std::vector<float> float32s{12.5F, -0.0F, std::numeric_limits<float>::infinity()};
    const std::vector<double> float64s{-13.5};
    const std::vector<std::uint8_t> first{1, 2};
    const std::vector<std::uint8_t> second;
    const std::string string = "\u00e9\u20ac\U0001F600\uFEFF";

    TableBuilder builder(24, TableLayout());
    builder.set_int8(std::numeric_limits<std::int8_t>::min());
    builder.set_uint8(std::numeric_limits<std::uint8_t>::max());
    builder.set_int16(std::numeric_limits<std::int16_t>::min());
    builder.set_uint16(std::numeric_limits<std::uint16_t>::max());
    builder.set_int32(std::numeric_limits<std::int32_t>::min());
    builder.set_uint32(std::numeric_limits<std::uint32_t>::max());
    builder.set_int64(std::numeric_limits<std::int64_t>::min());
    builder.set_uint64(std::numeric_limits<std::uint64_t>::max());
    builder.set_float32(1.5F);
    builder.set_float64(-2.25);
    builder.set_int8_array(int8s);
    builder.set_uint8_array(uint8s);
    builder.set_int16_array(int16s);
    builder.set_uint16_array(uint16s);
    builder.set_int32_array(int32s);
    builder.set_uint32_array(uint32s);
    builder.set_int64_array(int64s);
    builder.set_uint64_array(uint64s);
    builder.set_float32_array(float32s);
    builder.set_float64_array(float64s);
    builder.set_float64_array(std::span<const double>());
    builder.set_string(string);
    builder.set_string("");
    std::vector<NestedTable> items;
    items.emplace_back(first);
    items.emplace_back(second);
    builder.set_vector(std::move(items));

    EXPECT_EQ(builder.to_bytes().to_vector(), kKinds);

    const ByteBuffer buffer(kKinds, TableLayout());
    const auto at = [&buffer](std::size_t index) { return buffer.root_offset(index); };

    EXPECT_EQ(buffer.get_int8(at(0)), std::numeric_limits<std::int8_t>::min());
    EXPECT_EQ(buffer.get_uint8(at(1)), std::numeric_limits<std::uint8_t>::max());
    EXPECT_EQ(buffer.get_int16(at(2)), std::numeric_limits<std::int16_t>::min());
    EXPECT_EQ(buffer.get_uint16(at(3)), std::numeric_limits<std::uint16_t>::max());
    EXPECT_EQ(buffer.get_int32(at(4)), std::numeric_limits<std::int32_t>::min());
    EXPECT_EQ(buffer.get_uint32(at(5)), std::numeric_limits<std::uint32_t>::max());
    EXPECT_EQ(buffer.get_int64(at(6)), std::numeric_limits<std::int64_t>::min());
    EXPECT_EQ(buffer.get_uint64(at(7)), std::numeric_limits<std::uint64_t>::max());
    EXPECT_EQ(buffer.get_float32(at(8)), 1.5F);
    EXPECT_EQ(buffer.get_float64(at(9)), -2.25);
    EXPECT_EQ(buffer.get_int8_array(at(10)).to_vector(), int8s);
    EXPECT_EQ(vector_of(buffer.get_uint8_array(at(11))), uint8s);
    EXPECT_EQ(buffer.get_int16_array(at(12)).to_vector(), int16s);
    EXPECT_EQ(buffer.get_uint16_array(at(13)).to_vector(), uint16s);
    EXPECT_EQ(buffer.get_int32_array(at(14)).to_vector(), int32s);
    EXPECT_EQ(buffer.get_uint32_array(at(15)).to_vector(), uint32s);
    EXPECT_EQ(buffer.get_int64_array(at(16)).to_vector(), int64s);
    EXPECT_EQ(buffer.get_uint64_array(at(17)).to_vector(), uint64s);

    std::vector<std::uint32_t> bits;
    for (float value : buffer.get_float32_array(at(18))) {
        bits.push_back(std::bit_cast<std::uint32_t>(value));
    }
    std::vector<std::uint32_t> expected;
    for (float value : float32s) {
        expected.push_back(std::bit_cast<std::uint32_t>(value));
    }
    EXPECT_EQ(bits, expected);

    EXPECT_EQ(buffer.get_float64_array(at(19)).to_vector(), float64s);
    EXPECT_TRUE(buffer.get_float64_array(at(20)).empty());
    EXPECT_EQ(buffer.get_string(at(21)), string);
    EXPECT_EQ(buffer.get_string(at(22)), "");

    std::vector<std::vector<std::uint8_t>> read;
    for (auto item : buffer.get_vector(at(23)).items()) {
        read.push_back(vector_of(item));
    }
    EXPECT_EQ(read, (std::vector<std::vector<std::uint8_t>>{first, second}));
}

TEST(Compatibility, VersionedDocumentWithNestedTables) {
    constexpr auto layout = TableLayout().sized().versioned();
    const std::vector<std::uint32_t> keys{0, 86'400, 172'800};
    const std::vector<float> values{21.75F, 18.5F, -3.25F};
    const std::vector<std::uint16_t> item_values{7, 8, 9};
    const std::vector<std::uint8_t> bytes_item{1, 2, 3};

    TableBuilder columns(3, TableLayout());
    columns.set_int64(-631'152'000);
    columns.set_uint32_array(keys);
    columns.set_float32_array(values);

    TableBuilder item(2, TableLayout());
    item.set_float32(0.5F);
    item.set_uint16_array(item_values);

    TableBuilder document(5, layout, 3);
    document.set_float64(59.3293);
    document.set_string("Europe/Stockholm");
    document.set_table(std::move(columns));
    document.set_int8(-5);
    std::vector<NestedTable> items;
    items.emplace_back(std::move(item));
    items.emplace_back(bytes_item);
    document.set_vector(std::move(items));

    EXPECT_EQ(document.to_bytes().to_vector(), kDocument);

    const ByteBuffer buffer(kDocument, layout);
    EXPECT_EQ(buffer.size(), kDocument.size());
    EXPECT_EQ(buffer.version(), 3U);
    EXPECT_EQ(buffer.get_float64(buffer.root_offset(0)), 59.3293);
    EXPECT_EQ(buffer.get_string(buffer.root_offset(1)), "Europe/Stockholm");

    const auto position = buffer.root_offset(2);
    EXPECT_EQ(buffer.get_int64(buffer.field_offset(position, 0)), -631'152'000);
    EXPECT_EQ(buffer.get_uint32_array(buffer.field_offset(position, 1)).to_vector(), keys);
    EXPECT_EQ(buffer.get_float32_array(buffer.field_offset(position, 2)).to_vector(), values);
    EXPECT_EQ(buffer.get_int8(buffer.root_offset(3)), -5);

    const auto vector = buffer.get_vector(buffer.root_offset(4));
    ASSERT_EQ(vector.size(), 2U);
    const ByteBuffer first(vector.get(0), TableLayout());
    EXPECT_EQ(first.get_float32(first.root_offset(0)), 0.5F);
    EXPECT_EQ(first.get_uint16_array(first.root_offset(1)).to_vector(), item_values);
    EXPECT_EQ(vector_of(vector.get(1)), bytes_item);
}

TEST(Compatibility, Uint8OffsetsWithATableNestedAsBytes) {
    const auto layout = TableLayout().sized().with_offset_type(OffsetType::uint8);
    const std::vector<std::uint16_t> values{1, 2};
    const std::vector<std::uint8_t> nested{7, 7, 7};

    TableBuilder small(3, layout);
    small.set_string("hi");
    small.set_uint16_array(values);
    small.set_table(nested);

    EXPECT_EQ(small.to_bytes().to_vector(), kSmall);

    const ByteBuffer buffer(kSmall, layout);
    const auto position = buffer.root_offset(2);
    EXPECT_EQ(position % alignment_bytes(Alignment::four), 0U);
    EXPECT_EQ(buffer.get_string(buffer.root_offset(0)), "hi");
    EXPECT_EQ(buffer.get_uint16_array(buffer.root_offset(1)).to_vector(), values);
    EXPECT_EQ(std::vector<std::uint8_t>(kSmall.begin() + static_cast<std::ptrdiff_t>(position), kSmall.end()), nested);
}

TEST(Compatibility, Uint16OffsetsWithVectorItemsOfOtherOffsetTypes) {
    const auto layout = TableLayout().sized().with_offset_type(OffsetType::uint16).versioned();
    const auto narrow_layout = TableLayout().with_offset_type(OffsetType::uint8);
    const std::vector<double> wide_values{1.25};
    const std::vector<float> values{1.0F, 2.0F};

    TableBuilder narrow(1, narrow_layout);
    narrow.set_uint8(42);
    TableBuilder wide(1, TableLayout());
    wide.set_float64_array(wide_values);

    TableBuilder medium(2, layout, 513);
    medium.set_float32_array(values);
    std::vector<NestedTable> items;
    items.emplace_back(std::move(narrow));
    items.emplace_back(std::move(wide));
    medium.set_vector(std::move(items));

    EXPECT_EQ(medium.to_bytes().to_vector(), kMedium);

    const ByteBuffer buffer(kMedium, layout);
    EXPECT_EQ(buffer.version(), 513U);
    EXPECT_EQ(buffer.get_float32_array(buffer.root_offset(0)).to_vector(), values);

    const auto vector = buffer.get_vector(buffer.root_offset(1));
    ASSERT_EQ(vector.size(), 2U);
    const ByteBuffer narrow_item(vector.get(0), narrow_layout);
    const ByteBuffer wide_item(vector.get(1), TableLayout());
    EXPECT_EQ(narrow_item.get_uint8(narrow_item.root_offset(0)), 42);
    EXPECT_EQ(wide_item.get_float64_array(wide_item.root_offset(0)).to_vector(), wide_values);
}

}  // namespace
}  // namespace tafla::test
