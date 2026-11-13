// Validates the UTF-8 of strings, which the standard library can't.

#ifndef TAFLA_UTF8_HPP
#define TAFLA_UTF8_HPP

#include <cstddef>
#include <cstdint>
#include <cstring>

namespace tafla::detail {

/// Whether `bytes` are UTF-8 as Unicode's table 3-7 defines it: no
/// continuation byte without its lead, no overlong encoding, no surrogate,
/// nothing past U+10FFFF and no character cut short at the end
inline bool valid_utf8(const std::uint8_t* bytes, std::size_t length) noexcept {
    constexpr std::uint64_t high_bits = 0x8080'8080'8080'8080;
    const auto continues = [](std::uint8_t byte) { return (byte & 0xc0) == 0x80; };

    std::size_t index = 0;
    while (index < length) {
        const std::uint8_t lead = bytes[index];

        if (lead < 0x80) {
            // ASCII, which most strings are, eight bytes at a time
            ++index;
            while (length - index >= 8) {
                std::uint64_t word;
                std::memcpy(&word, bytes + index, sizeof word);
                if ((word & high_bits) != 0) {
                    break;
                }
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

}  // namespace tafla::detail

#endif
