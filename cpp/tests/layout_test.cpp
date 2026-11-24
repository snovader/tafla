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
    static_assert(layout.with_version(0).with_offset_type(OffsetType::uint16).header_size() == 2);
    static_assert(layout.sized().with_version(0).with_offset_type(OffsetType::uint16).header_size() == 4);
    static_assert(layout.sized().with_version(0).header_size() == 8);
    static_assert(layout.sized().with_version(0).is_versioned());
}

TEST(Layout, Version) {
    constexpr auto first = TableLayout().sized().with_version(1);
    constexpr auto second = first.with_version(2);
    static_assert(second.version() == 2 && second.is_sized());
    static_assert(first != second);

    // A layout without a version is no longer for those that have one
    static_assert(sizeof(TableLayout<false>) < sizeof(TableLayout<true>));
    static_assert(sizeof(TableLayout<false>) == 2);

    const auto bytes = TableBuilder(0, second).to_bytes();

    // A reader reads a table of any version, whatever its layout's
    EXPECT_EQ(TableReader(bytes, first).version(), 2U);
    EXPECT_EQ(TableReader(bytes, second).version(), 2U);
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
