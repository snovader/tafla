// Validates the UTF-8 of strings, which the standard library can't.

#ifndef TAFLA_UTF8_HPP
#define TAFLA_UTF8_HPP

#include <cstddef>
#include <cstdint>
#include <cstring>

#include "element.hpp"

namespace tafla::detail {

/// Whether the 8 bytes at `bytes` are all ASCII, at any alignment and in
/// either byte order
TAFLA_ALWAYS_INLINE bool ascii_word(const std::uint8_t* bytes) noexcept {
    constexpr std::uint64_t high_bits = 0x8080'8080'8080'8080;

    std::uint64_t word;
    std::memcpy(&word, bytes, sizeof word);
    return (word & high_bits) == 0;
}

/// Whether the 4 bytes at `bytes` are all ASCII, as `ascii_word()` checks 8
TAFLA_ALWAYS_INLINE bool ascii_half_word(const std::uint8_t* bytes) noexcept {
    constexpr std::uint32_t high_bits = 0x8080'8080;

    std::uint32_t word;
    std::memcpy(&word, bytes, sizeof word);
    return (word & high_bits) == 0;
}

/// Whether the bytes from `index` to `length` are UTF-8, as `valid_utf8()`
/// defines it, where those before `index` are ASCII, checked character by
/// character
inline bool valid_utf8_from(const std::uint8_t* bytes, std::size_t index, std::size_t length) noexcept {
    const auto continues = [](std::uint8_t byte) { return (byte & 0xc0) == 0x80; };

    while (index < length) {
        const std::uint8_t lead = bytes[index];

        if (lead < 0x80) {
            // ASCII again, eight bytes at a time
            ++index;
            while (length - index >= 8 && ascii_word(bytes + index)) {
                index += 8;
            }
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

/// Whether `bytes` are UTF-8 as Unicode's table 3-7 defines it: no
/// continuation byte without its lead, no overlong encoding, no surrogate,
/// nothing past U+10FFFF and no character cut short at the end
///
/// ASCII, which most strings are, is checked a word at a time from the first
/// byte, with a last word that overlaps the one before it, and a string
/// shorter than a word by the two halves of a word, which overlap, or by its
/// first, middle and last bytes. From the first word or half that holds a
/// byte that isn't ASCII, `valid_utf8_from()` checks the characters one by
/// one. It's left to the compiler to inline, which read the comparison's
/// record faster than a call to it did. Every read stays within the bytes, at
/// any alignment, and a word's high bits are its bytes' in either byte order.
TAFLA_ALWAYS_INLINE bool valid_utf8(const std::uint8_t* bytes, std::size_t length) noexcept {
    std::size_t index = 0;
    if (length >= 8) {
        // Words, the last of which overlaps the one before it
        while (length - index > 8 && ascii_word(bytes + index)) {
            index += 8;
        }
        if (length - index <= 8 && ascii_word(bytes + length - 8)) {
            return true;
        }
    } else if (length >= 4) {
        // Two halves of a word, which overlap
        if (ascii_half_word(bytes) && ascii_half_word(bytes + length - 4)) {
            return true;
        }
    } else if (length > 0) {
        // The first, middle and last bytes, which are all of them
        if (((bytes[0] | bytes[length / 2] | bytes[length - 1]) & 0x80) == 0) {
            return true;
        }
    } else {
        return true;
    }

    return valid_utf8_from(bytes, index, length);
}

}  // namespace tafla::detail

#endif
