// Layouts, offset types and alignments, which every table's header and
// padding follow

#include <gtest/gtest.h>

#include <tafla/tafla.hpp>

namespace tafla::test {
namespace {

TEST(Layout, TheDefaultHasNoHeaderAndUint32Offsets) {
    constexpr TableLayout layout;
    static_assert(!layout.is_sized());
    static_assert(!layout.is_versioned());
    static_assert(layout.offset_type() == OffsetType::uint32);
    static_assert(layout.header_size() == 0);
    EXPECT_EQ(layout, TableLayout<>());
}

TEST(Layout, OffsetTypes) {
    struct Case {
        OffsetType offset_type;
        const char* name;
        std::size_t size;
        std::uint32_t max;
    };
    for (const auto& [offset_type, type_name, size, max] : {
             Case{OffsetType::uint8, "uint8", 1, 0xff},
             Case{OffsetType::uint16, "uint16", 2, 0xffff},
             Case{OffsetType::uint32, "uint32", 4, 0xffff'ffff},
         }) {
        const auto layout = TableLayout().with_offset_type(offset_type);
        EXPECT_EQ(name(offset_type), type_name);
        EXPECT_EQ(offset_size(offset_type), size);
        EXPECT_EQ(max_value(offset_type), max);
        EXPECT_EQ(layout.offset_type(), offset_type);
        EXPECT_EQ(layout.offset_size(), size);
    }
}

// The same cases as typescript/core/test/layout.test.ts
TEST(Layout, HeaderSize) {
    constexpr TableLayout layout;
    static_assert(layout.with_offset_type(OffsetType::uint8).header_size() == 0);
    static_assert(layout.sized().with_offset_type(OffsetType::uint8).header_size() == 1);
    static_assert(layout.versioned().with_offset_type(OffsetType::uint16).header_size() == 2);
    static_assert(layout.sized().versioned().with_offset_type(OffsetType::uint16).header_size() == 4);
    static_assert(layout.sized().versioned().header_size() == 8);
    static_assert(layout.sized().versioned().is_versioned());
}

TEST(Layout, Alignments) {
    EXPECT_EQ(alignment_bytes(Alignment::one), 1U);
    EXPECT_EQ(alignment_bytes(Alignment::two), 2U);
    EXPECT_EQ(alignment_bytes(Alignment::four), 4U);
    EXPECT_EQ(alignment_bytes(Alignment::eight), 8U);
    EXPECT_LT(Alignment::four, Alignment::eight);

    // The padding that brings an offset to a multiple of the alignment, of
    // offsets that have wrapped around too
    EXPECT_EQ(detail::padding<std::size_t>(13, Alignment::eight), 3U);
    EXPECT_EQ(detail::padding<std::size_t>(16, Alignment::eight), 0U);
    EXPECT_EQ(detail::padding<std::size_t>(SIZE_MAX, Alignment::four), 1U);
    EXPECT_EQ(detail::padding<std::size_t>(7, Alignment::one), 0U);
}

}  // namespace
}  // namespace tafla::test
