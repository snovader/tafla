// Generators of values for the tests of the code that tafla-gen generates,
// whose fields hold them: numbers across their range, strings of characters
// of every length in UTF-8, arrays, and corruptions of bytes

#ifndef TAFLA_TEST_VALUES_HPP
#define TAFLA_TEST_VALUES_HPP

#include <rapidcheck.h>

#include <cstddef>
#include <cstdint>
#include <limits>
#include <string>
#include <vector>

#include "common/strategy.hpp"

namespace tafla::test::values {

/// Integers across their range, and their limits and 0 more often than chance
template <class T>
rc::Gen<T> integers() {
    using Numbers = std::numeric_limits<T>;
    return rc::gen::resize(rc::kNominalSize, rc::gen::weightedOneOf<T>({
                                                 {1, rc::gen::element<T>(Numbers::min(), T{0}, Numbers::max())},
                                                 {3, rc::gen::arbitrary<T>()},
                                             }));
}

/// Vectors of up to `max` values
template <class T>
rc::Gen<std::vector<T>> arrays(rc::Gen<T> values, std::size_t max = 24) {
    return rc::gen::resize(rc::kNominalSize,
                           rc::gen::mapcat(rc::gen::inRange<std::size_t>(0, max + 1), [values](std::size_t count) {
                               return rc::gen::container<std::vector<T>>(count, values);
                           }));
}

/// Strings of characters of one to four bytes in UTF-8, and U+FEFF
inline rc::Gen<std::string> strings() {
    return rc::gen::map(arrays(rc::gen::weightedOneOf<std::uint32_t>({
                                   {3, rc::gen::inRange<std::uint32_t>('a', 'z' + 1)},
                                   {1, rc::gen::just(std::uint32_t{0xfeff})},
                                   {1, rc::gen::inRange<std::uint32_t>(0x80, 0x800)},
                                   {1, rc::gen::inRange<std::uint32_t>(0xe000, 0x10000)},
                                   {1, rc::gen::inRange<std::uint32_t>(0x10000, 0x110000)},
                               }),
                               16),
                        [](const std::vector<std::uint32_t>& codes) {
                            std::string string;
                            for (const auto code : codes) {
                                if (code < 0x80) {
                                    string += static_cast<char>(code);
                                } else if (code < 0x800) {
                                    string += static_cast<char>(0xc0 | (code >> 6));
                                    string += static_cast<char>(0x80 | (code & 0x3f));
                                } else if (code < 0x10000) {
                                    string += static_cast<char>(0xe0 | (code >> 12));
                                    string += static_cast<char>(0x80 | ((code >> 6) & 0x3f));
                                    string += static_cast<char>(0x80 | (code & 0x3f));
                                } else {
                                    string += static_cast<char>(0xf0 | (code >> 18));
                                    string += static_cast<char>(0x80 | ((code >> 12) & 0x3f));
                                    string += static_cast<char>(0x80 | ((code >> 6) & 0x3f));
                                    string += static_cast<char>(0x80 | (code & 0x3f));
                                }
                            }
                            return string;
                        });
}

/// Ways to corrupt a table's bytes: cut short, overwritten at a few positions,
/// or replaced with arbitrary bytes
struct Corruption {
    std::vector<std::uint64_t> cuts;
    std::vector<std::pair<std::uint64_t, std::uint8_t>> overwrites;
    std::vector<std::uint8_t> arbitrary;
};

inline rc::Gen<Corruption> corruptions() {
    const auto small = rc::gen::weightedOneOf<std::uint8_t>({
        {4, rc::gen::just(std::uint8_t{0})},
        {4, rc::gen::inRange<std::uint8_t>(1, 16)},
        {1, rc::gen::just(std::uint8_t{0xff})},
        {1, rc::gen::arbitrary<std::uint8_t>()},
    });
    return rc::gen::resize(
        rc::kNominalSize, rc::gen::construct<Corruption>(
                              arrays(rc::gen::arbitrary<std::uint64_t>(), 4),
                              arrays(rc::gen::pair(rc::gen::arbitrary<std::uint64_t>(), small), 4), arrays(small, 64)));
}

/// The copies of `bytes` that `corruption` makes
inline std::vector<std::vector<std::uint8_t>> corrupted(const std::vector<std::uint8_t>& bytes,
                                                        const Corruption& corruption) {
    std::vector<std::vector<std::uint8_t>> copies;
    for (const auto cut : corruption.cuts) {
        const auto length = bytes.empty() ? 0 : cut % bytes.size();
        copies.emplace_back(bytes.begin(), bytes.begin() + static_cast<std::ptrdiff_t>(length));
    }

    if (!bytes.empty()) {
        auto overwritten = bytes;
        for (const auto& [position, byte] : corruption.overwrites) {
            // Less than the size, so within a size_t
            overwritten[static_cast<std::size_t>(position % overwritten.size())] = byte;
        }
        copies.push_back(std::move(overwritten));
    }

    copies.push_back(corruption.arbitrary);
    return copies;
}

}  // namespace tafla::test::values

#endif
