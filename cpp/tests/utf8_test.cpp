// Strings must be UTF-8, which readers check, as Unicode's table 3-7 defines
// its well-formed byte sequences

#include <gtest/gtest.h>
#include <rapidcheck/gtest.h>

#include <algorithm>
#include <array>
#include <cstddef>
#include <cstdint>
#include <memory>
#include <span>
#include <string>
#include <vector>

#include <tafla/tafla.hpp>

#include "common/strategy.hpp"

namespace tafla::test {
namespace {

bool valid(const std::vector<std::uint8_t>& bytes) {
    return detail::valid_utf8(bytes.data(), bytes.size());
}

/// The bytes that one byte of a well-formed sequence may hold
struct Range {
    std::uint8_t low;
    std::uint8_t high;

    [[nodiscard]] bool holds(std::uint8_t byte) const {
        return byte >= low && byte <= high;
    }
};

/// Unicode's table 3-7, a row of each well-formed sequence's bytes
const std::vector<std::vector<Range>> kWellFormed{
    {{0x00, 0x7f}},
    {{0xc2, 0xdf}, {0x80, 0xbf}},
    {{0xe0, 0xe0}, {0xa0, 0xbf}, {0x80, 0xbf}},
    {{0xe1, 0xec}, {0x80, 0xbf}, {0x80, 0xbf}},
    {{0xed, 0xed}, {0x80, 0x9f}, {0x80, 0xbf}},
    {{0xee, 0xef}, {0x80, 0xbf}, {0x80, 0xbf}},
    {{0xf0, 0xf0}, {0x90, 0xbf}, {0x80, 0xbf}, {0x80, 0xbf}},
    {{0xf1, 0xf3}, {0x80, 0xbf}, {0x80, 0xbf}, {0x80, 0xbf}},
    {{0xf4, 0xf4}, {0x80, 0x8f}, {0x80, 0xbf}, {0x80, 0xbf}},
};

/// Whether `bytes` are a series of the table's sequences, matched a byte at a
/// time, as the reference that `valid_utf8()` must agree with
bool well_formed(std::span<const std::uint8_t> bytes) {
    std::size_t index = 0;
    while (index < bytes.size()) {
        const auto lead = bytes[index];
        const auto row =
            std::ranges::find_if(kWellFormed, [lead](const auto& ranges) { return ranges[0].holds(lead); });
        if (row == kWellFormed.end() || bytes.size() - index < row->size()) {
            return false;
        }

        for (std::size_t next = 1; next < row->size(); ++next) {
            if (!(*row)[next].holds(bytes[index + next])) {
                return false;
            }
        }
        index += row->size();
    }

    return true;
}

/// What each check of UTF-8 finds of `bytes` at `offset` from the start of
/// an allocation that ends where they do, after `offset` bytes that aren't
/// UTF-8, so that a read past their end is reported by AddressSanitizer at
/// every offset, and a read before their start is reported at offset 0 and
/// reads bytes that aren't UTF-8 at the others.
///
/// The checks are `valid_utf8()`, which reads use, and, from the string's
/// start, each check that it hands a string to from its first byte that
/// isn't ASCII, so that each is tested on any host, whichever it takes there:
/// the check of characters one by one, and on AArch64 the check of blocks of
/// 16 bytes, of strings of 16 bytes or more.
std::vector<bool> checks_at(const std::vector<std::uint8_t>& bytes, std::size_t offset) {
    // Not zeroed, since every byte is written below. Zeroed, the bytes made
    // GCC 15's -Wrestrict see the copy as overlapping on 32-bit x86 and s390x
    const auto buffer = std::make_unique_for_overwrite<std::uint8_t[]>(offset + bytes.size());
    std::fill_n(buffer.get(), offset, std::uint8_t{0xff});
    std::ranges::copy(bytes, buffer.get() + offset);

    const auto* start = buffer.get() + offset;
    std::vector<bool> checks{detail::valid_utf8(start, bytes.size()),
                             detail::valid_utf8_characters_from(start, 0, bytes.size())};
#if defined(TAFLA_UTF8_NEON)
    if (bytes.size() >= detail::kUtf8BlockLength) {
        checks.push_back(detail::valid_utf8_blocks_from(start, 0, bytes.size()));
    }
#endif
    return checks;
}

/// Whether every check finds `bytes` valid as the table matches them, at every
/// address modulo the 8 bytes that ASCII is checked in at a time
bool validates_as_table(const std::vector<std::uint8_t>& bytes) {
    const bool expected = well_formed(bytes);
    for (std::size_t offset = 0; offset < 8; ++offset) {
        for (const bool valid : checks_at(bytes, offset)) {
            if (valid != expected) {
                return false;
            }
        }
    }
    return true;
}

/// UTF-8 of `count` characters, `pattern`'s repeated
std::vector<std::uint8_t> repeated(const std::vector<std::vector<std::uint8_t>>& pattern, std::size_t count) {
    std::vector<std::uint8_t> bytes;
    for (std::size_t index = 0; index < count; ++index) {
        const auto& character = pattern[index % pattern.size()];
        bytes.insert(bytes.end(), character.begin(), character.end());
    }
    return bytes;
}

/// Strings of `bytes`, of every length to six words as likely as any other,
/// at every size, so that strings shorter than a word are common, and hold
/// bytes of any value that `bytes` generates
rc::Gen<std::vector<std::uint8_t>> strings_of(rc::Gen<std::uint8_t> bytes) {
    return rc::gen::resize(rc::kNominalSize,
                           rc::gen::mapcat(rc::gen::inRange<std::size_t>(0, 49), [bytes](std::size_t length) {
                               return rc::gen::container<std::vector<std::uint8_t>>(length, bytes);
                           }));
}

rc::Gen<std::vector<std::uint8_t>> ascii() {
    return strings_of(rc::gen::inRange<std::uint8_t>(0, 0x80));
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
        EXPECT_TRUE(well_formed(bytes)) << ::testing::PrintToString(bytes);
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
        EXPECT_FALSE(well_formed(bytes)) << ::testing::PrintToString(bytes);
    }
}

/// ASCII is checked from the first byte in blocks of four words, the last of
/// which overlaps the one before it, and in a string of a block or less in
/// fewer words, halves of a word or bytes, which overlap, so a byte that
/// isn't ASCII is found at any position of a string of any length to three
/// blocks and a word, at any address
TEST(Utf8, FindsAByteThatIsNotAsciiAtAnyPositionOfAnyLength) {
    for (std::size_t length = 0; length <= 104; ++length) {
        std::vector<std::uint8_t> bytes(length, 'a');
        EXPECT_TRUE(valid(bytes)) << length;
        EXPECT_TRUE(validates_as_table(bytes)) << length;

        for (std::size_t position = 0; position < length; ++position) {
            auto changed = bytes;
            for (const std::uint8_t byte : std::array<std::uint8_t, 4>{0x80, 0xc3, 0xf0, 0xff}) {
                // A continuation byte without its lead, or a lead cut short
                // by the ASCII after it
                changed[position] = byte;
                EXPECT_FALSE(valid(changed)) << length << " " << position << " " << int{byte};
                EXPECT_TRUE(validates_as_table(changed)) << length << " " << position << " " << int{byte};
            }

            // U+00E5 and U+1F600 where they fit
            changed = bytes;
            const std::vector<std::uint8_t> two{0xc3, 0xa5};
            std::copy_n(two.begin(), std::min(two.size(), length - position), changed.data() + position);
            EXPECT_EQ(valid(changed), position + two.size() <= length) << length << " " << position;
            EXPECT_TRUE(validates_as_table(changed)) << length << " " << position;

            changed = bytes;
            const std::vector<std::uint8_t> four{0xf0, 0x9f, 0x98, 0x80};
            std::copy_n(four.begin(), std::min(four.size(), length - position), changed.data() + position);
            EXPECT_EQ(valid(changed), position + four.size() <= length) << length << " " << position;
            EXPECT_TRUE(validates_as_table(changed)) << length << " " << position;
        }
    }
}

/// Characters of 2, 3 and 4 bytes, alone and mixed with ASCII, are checked
/// a character at a time, or in blocks of 16 bytes with the 3 before each,
/// the last of which is moved to the start of a block and followed by zeros,
/// so a byte that isn't well-formed is found at any position of a string of
/// any length to five blocks, among characters of any width, and so is a
/// character cut short at its end
TEST(Utf8, FindsAnIllFormedByteAtAnyPositionAmongCharactersOfEveryWidth) {
    const std::vector<std::uint8_t> a{'a'};
    const std::vector<std::uint8_t> two{0xc3, 0xa5};
    const std::vector<std::uint8_t> three{0xe4, 0xb8, 0x80};
    const std::vector<std::uint8_t> four{0xf0, 0x9f, 0x98, 0x80};
    const std::vector<std::vector<std::vector<std::uint8_t>>> patterns{
        {two}, {three}, {four}, {a, two, three, four}, {four, a, a, three, two, a},
    };
    // The limits of each byte of a sequence, and bytes that are never UTF-8
    const std::array<std::uint8_t, 12> bytes{0x00, 0x7f, 0x80, 0x8f, 0x9f, 0xbf, 0xc1, 0xc2, 0xe0, 0xed, 0xf4, 0xff};

    for (const auto& pattern : patterns) {
        for (std::size_t count = 1; count <= 40; ++count) {
            const auto valid = repeated(pattern, count);
            if (valid.size() > 80) {
                break;
            }
            EXPECT_TRUE(well_formed(valid));
            EXPECT_TRUE(validates_as_table(valid)) << ::testing::PrintToString(valid);

            for (std::size_t length = 0; length < valid.size(); ++length) {
                const std::vector<std::uint8_t> cut(valid.begin(), valid.begin() + static_cast<std::ptrdiff_t>(length));
                EXPECT_TRUE(validates_as_table(cut)) << ::testing::PrintToString(cut);
            }

            for (std::size_t position = 0; position < valid.size(); ++position) {
                for (const std::uint8_t byte : bytes) {
                    auto changed = valid;
                    changed[position] = byte;
                    EXPECT_TRUE(validates_as_table(changed)) << ::testing::PrintToString(changed);
                }
            }
        }
    }
}

RC_GTEST_PROP(Utf8, ValidatesAsciiAsTheTableDoes, ()) {
    const auto bytes = *ascii();
    RC_ASSERT(well_formed(bytes));
    RC_ASSERT(validates_as_table(bytes));
}

RC_GTEST_PROP(Utf8, ValidatesAnyUtf8AsTheTableDoes, ()) {
    const auto string = *strategy::strings(48);
    const std::vector<std::uint8_t> bytes(string.begin(), string.end());
    RC_ASSERT(well_formed(bytes));
    RC_ASSERT(validates_as_table(bytes));
}

RC_GTEST_PROP(Utf8, ValidatesAnyUtf8WithOneArbitraryByteAnywhereAsTheTableDoes, ()) {
    const auto string = *rc::gen::nonEmpty(strategy::strings(48));
    const std::vector<std::uint8_t> bytes(string.begin(), string.end());
    const auto byte = *rc::gen::arbitrary<std::uint8_t>();

    for (std::size_t position = 0; position < bytes.size(); ++position) {
        auto changed = bytes;
        changed[position] = byte;
        RC_ASSERT(validates_as_table(changed));
    }
}

RC_GTEST_PROP(Utf8, ValidatesAnyUtf8CutShortAsTheTableDoes, ()) {
    const auto string = *strategy::strings(48);
    const std::vector<std::uint8_t> bytes(string.begin(), string.end());

    for (std::size_t length = 0; length < bytes.size(); ++length) {
        RC_ASSERT(validates_as_table({bytes.begin(), bytes.begin() + static_cast<std::ptrdiff_t>(length)}));
    }
}

RC_GTEST_PROP(Utf8, ValidatesArbitraryBytesAsTheTableDoes, ()) {
    const auto bytes = *strings_of(rc::gen::arbitrary<std::uint8_t>());
    RC_ASSERT(validates_as_table(bytes));
}

RC_GTEST_PROP(Utf8, ValidatesAsciiWithOneArbitraryByteAnywhereAsTheTableDoes, ()) {
    const auto bytes = *rc::gen::nonEmpty(ascii());
    const auto byte = *rc::gen::resize(rc::kNominalSize, rc::gen::arbitrary<std::uint8_t>());

    for (std::size_t position = 0; position < bytes.size(); ++position) {
        auto changed = bytes;
        changed[position] = byte;
        RC_ASSERT(validates_as_table(changed));
    }
}

}  // namespace
}  // namespace tafla::test
