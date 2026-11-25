// Validates the UTF-8 of strings, which the standard library can't.

#ifndef TAFLA_UTF8_HPP
#define TAFLA_UTF8_HPP

#include <bit>
#include <concepts>
#include <cstddef>
#include <cstdint>
#include <cstring>

#include "element.hpp"

// AArch64's NEON, which every AArch64 processor has, checks characters that
// aren't ASCII in blocks of 16 bytes. Other hosts check them one by one.
#if (defined(__GNUC__) || defined(__clang__)) && defined(__aarch64__) && defined(__ARM_NEON) && \
    !defined(__ARM_BIG_ENDIAN)
#define TAFLA_UTF8_NEON 1
#include <arm_neon.h>
#endif

namespace tafla::detail {

/// The 8 bytes at `bytes`, at any alignment, as a word in the host's byte
/// order
TAFLA_ALWAYS_INLINE std::uint64_t word_at(const std::uint8_t* bytes) noexcept {
    std::uint64_t word;
    std::memcpy(&word, bytes, sizeof word);
    return word;
}

/// The 4 bytes at `bytes`, as `word_at()` reads 8
TAFLA_ALWAYS_INLINE std::uint32_t half_word_at(const std::uint8_t* bytes) noexcept {
    std::uint32_t half_word;
    std::memcpy(&half_word, bytes, sizeof half_word);
    return half_word;
}

/// The high bits of the bytes of a word or half, which are each byte's own
/// in either byte order, and all clear if the bytes are ASCII
template <std::unsigned_integral Word>
TAFLA_ALWAYS_INLINE Word high_bits(Word word) noexcept {
    return word & static_cast<Word>(0x8080'8080'8080'8080);
}

/// Whether the bytes of a word or half are all ASCII, which for words OR-ed
/// together is whether all of theirs are
template <std::unsigned_integral Word>
TAFLA_ALWAYS_INLINE bool ascii(Word word) noexcept {
    return high_bits(word) == 0;
}

/// Whether the 8 bytes at `bytes` are all ASCII
TAFLA_ALWAYS_INLINE bool ascii_word(const std::uint8_t* bytes) noexcept {
    return ascii(word_at(bytes));
}

/// Whether the 32 bytes at `bytes`, four words, are all ASCII, as
/// `ascii_word()` checks one
TAFLA_ALWAYS_INLINE bool ascii_block(const std::uint8_t* bytes) noexcept {
    return ascii(word_at(bytes) | word_at(bytes + 8) | word_at(bytes + 16) | word_at(bytes + 24));
}

/// The position of the first byte in memory that isn't ASCII, of a word or
/// half that holds one: by its high bits, the lowest byte's on a
/// little-endian host, and the highest byte's on a big-endian one
template <std::unsigned_integral Word>
TAFLA_ALWAYS_INLINE std::size_t first_high_byte(Word word) noexcept {
    if constexpr (std::endian::native == std::endian::little) {
        return static_cast<std::size_t>(std::countr_zero(high_bits(word))) / 8;
    } else {
        return static_cast<std::size_t>(std::countl_zero(high_bits(word))) / 8;
    }
}

/// The position of the first byte that isn't ASCII in `first`, a word or
/// half, or else in `second`, which lies `offset` bytes after it and may
/// overlap it, of two that hold one. If `first` holds none, the bytes of
/// `second` that overlap it are ASCII, so its first that isn't lies after
/// `first`.
template <std::unsigned_integral Word>
TAFLA_ALWAYS_INLINE std::size_t first_high_byte(Word first, Word second, std::size_t offset) noexcept {
    return ascii(first) ? offset + first_high_byte(second) : first_high_byte(first);
}

/// The position of the first byte that isn't ASCII in the two words at
/// `bytes`, or else in the two at `offset` from them, which may overlap
/// them, of four that hold one
TAFLA_ALWAYS_INLINE std::size_t first_high_byte_of_words(const std::uint8_t* bytes, std::size_t offset) noexcept {
    const std::uint64_t first = word_at(bytes);
    const std::uint64_t second = word_at(bytes + 8);
    if (ascii(first | second)) {
        return offset + first_high_byte(word_at(bytes + offset), word_at(bytes + offset + 8), 8);
    }
    return first_high_byte(first, second, 8);
}

/// The position of the first byte from `index` that isn't ASCII, or of one
/// of the last 7 bytes or fewer, before which every byte is ASCII. ASCII is
/// skipped a word at a time, and after a word of it 32 bytes at a time while
/// a block of four words is ASCII, so that a short run of ASCII between other
/// characters costs a word's test and a long one a test of each 32 bytes. The
/// high bits of the word that holds a byte that isn't ASCII locate it, rather
/// than steps to it a byte at a time.
TAFLA_ALWAYS_INLINE std::size_t skip_ascii(const std::uint8_t* bytes, std::size_t index, std::size_t length) noexcept {
    while (length - index >= 8) {
        const std::uint64_t word = word_at(bytes + index);
        if (!ascii(word)) {
            return index + first_high_byte(word);
        }
        index += 8;

        while (length - index >= 32 && ascii_block(bytes + index)) {
            index += 32;
        }
    }
    return index;
}

/// Whether the bytes from `index` to `length` are UTF-8, as `valid_utf8()`
/// defines it, where those before `index` are ASCII, checked character by
/// character
inline bool valid_utf8_characters_from(const std::uint8_t* bytes, std::size_t index, std::size_t length) noexcept {
    const auto continues = [](std::uint8_t byte) { return (byte & 0xc0) == 0x80; };

    while (index < length) {
        const std::uint8_t lead = bytes[index];

        if (lead < 0x80) {
            index = skip_ascii(bytes, index + 1, length);
            continue;
        }

        const std::size_t left = length - index;
        if (lead < 0xc2) {
            // A continuation byte, or the lead of an overlong 2-byte encoding
            return false;
        }

        if (lead < 0xe0) {
            if (left < 2 || !continues(bytes[index + 1])) {
                return false;
            }
            index += 2;
        } else if (lead < 0xf0) {
            if (left < 3) {
                return false;
            }

            // E0 would be overlong below A0, and ED a surrogate from A0
            const std::uint8_t second = bytes[index + 1];
            const std::uint8_t low = lead == 0xe0 ? 0xa0 : 0x80;
            const std::uint8_t high = lead == 0xed ? 0x9f : 0xbf;
            if (second < low || second > high || !continues(bytes[index + 2])) {
                return false;
            }
            index += 3;
        } else if (lead < 0xf5) {
            if (left < 4) {
                return false;
            }

            // F0 would be overlong below 90, and F4 past U+10FFFF from 90
            const std::uint8_t second = bytes[index + 1];
            const std::uint8_t low = lead == 0xf0 ? 0x90 : 0x80;
            const std::uint8_t high = lead == 0xf4 ? 0x8f : 0xbf;
            if (second < low || second > high || !continues(bytes[index + 2]) || !continues(bytes[index + 3])) {
                return false;
            }
            index += 4;
        } else {
            return false;
        }
    }

    return true;
}

#if defined(TAFLA_UTF8_NEON)

/// The bytes of a block that NEON checks at once, and the fewest from the
/// first byte that isn't ASCII that `valid_utf8_from()` checks in blocks
inline constexpr std::size_t kUtf8BlockLength = 16;

namespace utf8_blocks {

// The errors that a byte and the one before it can make, a bit each, as Keiser
// and Lemire's lookup algorithm names them ("Validating UTF-8 in less than one
// instruction per byte", 2021), which utf8_range and simdutf use. The two
// bytes' nibbles look up the errors that each allows, and those that all
// three allow are the pair's.

/// A lead or ASCII followed by a lead or ASCII where a continuation must be
inline constexpr std::uint8_t kTooShort = 1 << 0;
/// ASCII followed by a continuation byte
inline constexpr std::uint8_t kTooLong = 1 << 1;
/// E0 followed by 80 to 9F
inline constexpr std::uint8_t kOverlong3 = 1 << 2;
/// F4 followed by 90 or more, or F5 to FF followed by 90 or more
inline constexpr std::uint8_t kTooLarge = 1 << 3;
/// ED followed by A0 to BF
inline constexpr std::uint8_t kSurrogate = 1 << 4;
/// C0 or C1 followed by a continuation byte
inline constexpr std::uint8_t kOverlong2 = 1 << 5;
/// F5 to FF followed by 80 to 8F, or F0 followed by 80 to 8F
inline constexpr std::uint8_t kTooLarge1000 = 1 << 6;
inline constexpr std::uint8_t kOverlong4 = 1 << 6;
/// A continuation byte after another, which is an error unless a lead of 3
/// or 4 bytes comes 2 or 3 bytes before it, which `errors()` tests apart
inline constexpr std::uint8_t kTwoContinuations = 1 << 7;
/// The errors of a first byte whose low nibble doesn't matter
inline constexpr std::uint8_t kCarry = kTooShort | kTooLong | kTwoContinuations;

/// The errors that a first byte allows, by its high nibble
inline constexpr std::uint8_t kFirstHigh[16] = {
    // 0_______: ASCII
    kTooLong,
    kTooLong,
    kTooLong,
    kTooLong,
    kTooLong,
    kTooLong,
    kTooLong,
    kTooLong,
    // 10______: a continuation byte
    kTwoContinuations,
    kTwoContinuations,
    kTwoContinuations,
    kTwoContinuations,
    // 1100____ and 1101____: the lead of 2 bytes
    kTooShort | kOverlong2,
    kTooShort,
    // 1110____: the lead of 3 bytes
    kTooShort | kOverlong3 | kSurrogate,
    // 1111____: the lead of 4 bytes, or more
    kTooShort | kTooLarge | kTooLarge1000 | kOverlong4,
};

/// The errors that a first byte allows, by its low nibble
inline constexpr std::uint8_t kFirstLow[16] = {
    kCarry | kOverlong3 | kOverlong2 | kOverlong4,    // ____0000
    kCarry | kOverlong2,                              // ____0001
    kCarry,                                           // ____0010
    kCarry,                                           // ____0011
    kCarry | kTooLarge,                               // ____0100
    kCarry | kTooLarge | kTooLarge1000,               // ____0101
    kCarry | kTooLarge | kTooLarge1000,               // ____0110
    kCarry | kTooLarge | kTooLarge1000,               // ____0111
    kCarry | kTooLarge | kTooLarge1000,               // ____1000
    kCarry | kTooLarge | kTooLarge1000,               // ____1001
    kCarry | kTooLarge | kTooLarge1000,               // ____1010
    kCarry | kTooLarge | kTooLarge1000,               // ____1011
    kCarry | kTooLarge | kTooLarge1000,               // ____1100
    kCarry | kTooLarge | kTooLarge1000 | kSurrogate,  // ____1101
    kCarry | kTooLarge | kTooLarge1000,               // ____1110
    kCarry | kTooLarge | kTooLarge1000,               // ____1111
};

/// The errors that a second byte allows, by its high nibble
inline constexpr std::uint8_t kSecondHigh[16] = {
    // 0_______: ASCII
    kTooShort,
    kTooShort,
    kTooShort,
    kTooShort,
    kTooShort,
    kTooShort,
    kTooShort,
    kTooShort,
    // 1000____
    kTooLong | kOverlong2 | kTwoContinuations | kOverlong3 | kTooLarge1000 | kOverlong4,
    // 1001____
    kTooLong | kOverlong2 | kTwoContinuations | kOverlong3 | kTooLarge,
    // 101_____
    kTooLong | kOverlong2 | kTwoContinuations | kSurrogate | kTooLarge,
    kTooLong | kOverlong2 | kTwoContinuations | kSurrogate | kTooLarge,
    // 11______: a lead
    kTooShort,
    kTooShort,
    kTooShort,
    kTooShort,
};

/// The three tables, in registers
struct Tables {
    uint8x16_t first_high;
    uint8x16_t first_low;
    uint8x16_t second_high;
};

TAFLA_ALWAYS_INLINE Tables load_tables() noexcept {
    return {vld1q_u8(kFirstHigh), vld1q_u8(kFirstLow), vld1q_u8(kSecondHigh)};
}

/// The errors of the 16 bytes of `input`, after the 16 of `previous`, a bit
/// set in a byte for each. Each byte's errors with the one before it are
/// looked up, and a continuation byte after another is an error exactly
/// where no lead of 3 or 4 bytes comes 2 or 3 bytes before it.
TAFLA_ALWAYS_INLINE uint8x16_t errors(const Tables& tables, uint8x16_t input, uint8x16_t previous) noexcept {
    const uint8x16_t before_1 = vextq_u8(previous, input, 15);
    const uint8x16_t first_high = vqtbl1q_u8(tables.first_high, vshrq_n_u8(before_1, 4));
    const uint8x16_t first_low = vqtbl1q_u8(tables.first_low, vandq_u8(before_1, vdupq_n_u8(0x0f)));
    const uint8x16_t second_high = vqtbl1q_u8(tables.second_high, vshrq_n_u8(input, 4));
    const uint8x16_t pairs = vandq_u8(vandq_u8(first_high, first_low), second_high);

    // Only E0 to FF 2 bytes before, and F0 to FF 3 bytes before, leave the
    // high bit set
    const uint8x16_t third = vqsubq_u8(vextq_u8(previous, input, 14), vdupq_n_u8(0xe0 - 0x80));
    const uint8x16_t fourth = vqsubq_u8(vextq_u8(previous, input, 13), vdupq_n_u8(0xf0 - 0x80));
    const uint8x16_t continues = vandq_u8(vorrq_u8(third, fourth), vdupq_n_u8(0x80));
    return veorq_u8(continues, pairs);
}

/// The bytes of a block that end a character cut short: a lead of 4 bytes in
/// its last 3, of 3 bytes in its last 2, or any lead last
TAFLA_ALWAYS_INLINE uint8x16_t cut_short(uint8x16_t block) noexcept {
    static constexpr std::uint8_t kLargestComplete[16] = {
        0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xef, 0xdf, 0xbf,
    };
    return vqsubq_u8(block, vld1q_u8(kLargestComplete));
}

}  // namespace utf8_blocks

/// Whether the bytes from `index` to `length`, `kUtf8BlockLength` or more,
/// are UTF-8, where those before `index` are ASCII, checked 16 bytes at a
/// time with each byte's 3 before it. A block of ASCII after one that ends
/// with every character complete needs no other check. The last bytes are
/// read in the block that ends where the string does, and moved to its start,
/// where zeros, ASCII, follow them, after which a character cut short at the
/// end is an error as any other. Every read stays within the bytes, and the
/// errors are tested once, at the end. It's kept apart from the reads that
/// inline `valid_utf8()`, which call it.
[[gnu::noinline]] inline bool valid_utf8_blocks_from(const std::uint8_t* bytes, std::size_t index,
                                                     std::size_t length) noexcept {
    using namespace utf8_blocks;
    const Tables tables = load_tables();

    uint8x16_t previous = vdupq_n_u8(0);
    uint8x16_t error = vdupq_n_u8(0);
    while (length - index >= kUtf8BlockLength) {
        const uint8x16_t input = vld1q_u8(bytes + index);
        if (vmaxvq_u8(input) < 0x80) {
            error = vorrq_u8(error, cut_short(previous));
        } else {
            error = vorrq_u8(error, errors(tables, input, previous));
        }
        previous = input;
        index += kUtf8BlockLength;
    }

    // Table lookups past the 16th byte read zeros
    static constexpr std::uint8_t kLanes[16] = {0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15};
    const auto left = static_cast<std::uint8_t>(length - index);
    const uint8x16_t moved = vaddq_u8(vld1q_u8(kLanes), vdupq_n_u8(static_cast<std::uint8_t>(16 - left)));
    const uint8x16_t last = vqtbl1q_u8(vld1q_u8(bytes + length - kUtf8BlockLength), moved);
    error = vorrq_u8(error, errors(tables, last, previous));

    return vmaxvq_u8(error) == 0;
}

#endif

/// Whether the bytes from `index` to `length` are UTF-8, where those before
/// `index` are ASCII: on AArch64 in blocks of 16 bytes where there are 16 or
/// more, and otherwise character by character
inline bool valid_utf8_from(const std::uint8_t* bytes, std::size_t index, std::size_t length) noexcept {
#if defined(TAFLA_UTF8_NEON)
    if (length - index >= kUtf8BlockLength) {
        return valid_utf8_blocks_from(bytes, index, length);
    }
#endif
    return valid_utf8_characters_from(bytes, index, length);
}

/// Whether `bytes` are UTF-8 as Unicode's table 3-7 defines it: no
/// continuation byte without its lead, no overlong encoding, no surrogate,
/// nothing past U+10FFFF and no character cut short at the end
///
/// ASCII, which most strings are, is checked from the first byte by words
/// OR-ed together, whose high bits are tested once: a string of 8 to 16 bytes
/// as its first word and its last, which overlap, one of 17 to 32 bytes as
/// its first two words and its last two, and a longer one in blocks of four
/// words, the last of which overlaps the one before it. A string shorter than
/// a word is checked as the two halves of a word, which overlap, or as its
/// first, middle and last bytes. Where the test fails, `valid_utf8_from()`
/// checks the rest from the first byte that isn't ASCII, which the high bits
/// of the words or halves that it OR-ed locate, taken in turn, or from the
/// first byte of a string shorter than 4 bytes: on AArch64 in blocks of 16
/// bytes where 16 or more are left, and otherwise character by character,
/// skipping ASCII between them by words and blocks. Where
/// that's the first byte of the string or of its block, as in a string of
/// another script, a branch tests it on its own, so that reading its
/// character needn't wait for the words and the count. `valid_utf8_from()` is
/// left to the compiler to inline, which read the comparison's record faster
/// than a call to it did. Every read stays within the bytes, at any
/// alignment, and a word's high bits are its bytes' in either byte order.
TAFLA_ALWAYS_INLINE bool valid_utf8(const std::uint8_t* bytes, std::size_t length) noexcept {
    std::size_t index = 0;
    if (length >= 8) {
        if (length <= 16) {
            // The first word and the last, which overlap, at once
            const std::uint64_t first = word_at(bytes);
            const std::uint64_t last = word_at(bytes + length - 8);
            if (ascii(first | last)) {
                return true;
            }

            // Marking the branch likely keeps the compiler from replacing it
            // with a select of the count, which would wait for the words.
            if (bytes[0] < 0x80) [[likely]] {
                index = first_high_byte(first, last, length - 8);
            }
        } else if (length <= 32) {
            // The first two words and the last two, which overlap them, at
            // once
            const std::uint64_t first = word_at(bytes) | word_at(bytes + 8);
            const std::uint64_t last = word_at(bytes + length - 16) | word_at(bytes + length - 8);
            if (ascii(first | last)) {
                return true;
            }

            if (bytes[0] < 0x80) [[likely]] {
                index = first_high_byte_of_words(bytes, length - 16);
            }
        } else {
            // Blocks of four words, the last of which overlaps the one before
            // it
            while (length - index > 32 && ascii_block(bytes + index)) {
                index += 32;
            }
            if (length - index <= 32) {
                if (ascii_block(bytes + length - 32)) {
                    return true;
                }

                // The last block's bytes before `index` are ASCII, so its
                // first byte that isn't lies at `index` or after
                index = length - 32;
            }

            if (bytes[index] < 0x80) [[likely]] {
                index += first_high_byte_of_words(bytes + index, 16);
            }
        }
    } else if (length < 4) {
        // The first, middle and last bytes, which are all of them, if any
        if (length == 0 || ((bytes[0] | bytes[length / 2] | bytes[length - 1]) & 0x80) == 0) {
            return true;
        }
    } else {
        // Two halves of a word, which overlap, at once
        const std::uint32_t first = half_word_at(bytes);
        const std::uint32_t last = half_word_at(bytes + length - 4);
        if (ascii(first | last)) {
            return true;
        }

        if (bytes[0] < 0x80) [[likely]] {
            index = first_high_byte(first, last, length - 4);
        }
    }

    return valid_utf8_from(bytes, index, length);
}

}  // namespace tafla::detail

#endif
