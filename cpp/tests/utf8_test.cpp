// Strings must be UTF-8, which readers check, as Unicode's table 3-7 defines
// its well-formed byte sequences

#include <gtest/gtest.h>

#include <cstdint>
#include <string>
#include <vector>

#include <tafla/tafla.hpp>

namespace tafla::test {
namespace {

bool valid(const std::vector<std::uint8_t>& bytes) {
    return detail::valid_utf8(bytes.data(), bytes.size());
}

TEST(Utf8, AcceptsWellFormedSequencesAtTheirLimits) {
    const std::vector<std::vector<std::uint8_t>> cases{
        {},
        {0x00},
        {0x7f},
        {0xc2, 0x80},
        {0xdf, 0xbf},
        {0xe0, 0xa0, 0x80},
        {0xe1, 0x80, 0x80},
        {0xec, 0xbf, 0xbf},
        {0xed, 0x80, 0x80},
        {0xed, 0x9f, 0xbf},
        {0xee, 0x80, 0x80},
        {0xef, 0xbf, 0xbf},
        {0xef, 0xbb, 0xbf},
        {0xf0, 0x90, 0x80, 0x80},
        {0xf3, 0xbf, 0xbf, 0xbf},
        {0xf4, 0x80, 0x80, 0x80},
        {0xf4, 0x8f, 0xbf, 0xbf},
    };

    for (const auto& bytes : cases) {
        EXPECT_TRUE(valid(bytes)) << ::testing::PrintToString(bytes);
    }
}

TEST(Utf8, RejectsIllFormedSequences) {
    const std::vector<std::vector<std::uint8_t>> cases{
        {0x80},  // a continuation byte without its lead
        {0xbf},
        {0xc0, 0x80},  // overlong encodings
        {0xc1, 0xbf},
        {0xe0, 0x9f, 0xbf},
        {0xf0, 0x8f, 0xbf, 0xbf},
        {0xed, 0xa0, 0x80},  // surrogates
        {0xed, 0xbf, 0xbf},
        {0xf4, 0x90, 0x80, 0x80},  // past U+10FFFF
        {0xf5, 0x80, 0x80, 0x80},
        {0xff},
        {0xc2},  // cut short at the end
        {0xe1, 0x80},
        {0xf1, 0x80, 0x80},
        {0xc2, 0x41},  // a lead without its continuation
        {0xe1, 0x80, 0x41},
        {0xf1, 0x80, 0x80, 0x41},
    };

    for (const auto& bytes : cases) {
        EXPECT_FALSE(valid(bytes)) << ::testing::PrintToString(bytes);
    }
}

/// Long runs of ASCII are checked eight bytes at a time, so an invalid byte
/// is found at any position in or after them
TEST(Utf8, FindsAnInvalidByteAtAnyPositionInAsciiRuns) {
    for (std::size_t length = 1; length < 40; ++length) {
        for (std::size_t position = 0; position < length; ++position) {
            std::vector<std::uint8_t> bytes(length, 'a');
            EXPECT_TRUE(valid(bytes));

            bytes[position] = 0x80;
            EXPECT_FALSE(valid(bytes)) << length << " " << position;

            bytes[position] = 0xc3;
            EXPECT_EQ(valid(bytes), false) << length << " " << position;
            if (position + 1 < length) {
                bytes[position + 1] = 0xa5;
                EXPECT_TRUE(valid(bytes)) << length << " " << position;
            }
        }
    }
}

}  // namespace
}  // namespace tafla::test
