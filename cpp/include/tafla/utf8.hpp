// Validates the UTF-8 of strings, which the standard library can't.

#ifndef TAFLA_UTF8_HPP
#define TAFLA_UTF8_HPP

#include <bit>
#include <concepts>
#include <cstddef>
#include <cstdint>
#include <cstring>

#include "element.hpp"

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

/// Whether the bytes from `index` to `length` are UTF-8, where those before
/// `index` are ASCII
inline bool valid_utf8_from(const std::uint8_t* bytes, std::size_t index, std::size_t length) noexcept {
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
/// checks the characters one by one from the first byte that isn't ASCII,
/// which the high bits of the words or halves that it OR-ed locate, taken in
/// turn, or from the first byte of a string shorter than 4 bytes. Where
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
