// The strings suite's tables: for each shape, 16 distinct strings of the same
// length and mix of UTF-8 widths, from ASCII to characters of 4 bytes, and 16
// empty strings, which time what a call takes without any.
//
// The strings come from a port of the TypeScript strings suite's generator,
// `typescript/benchmarks/strings/shapes.ts`, with its seeds and alphabets and
// its lengths in characters, encoded as UTF-8, so that the languages' tables
// hold the same bytes. The shapes that the TypeScript suite has are its own;
// those at C++'s thresholds are this suite's.

#ifndef TAFLA_BENCHMARKS_STRING_WORKLOADS_HPP
#define TAFLA_BENCHMARKS_STRING_WORKLOADS_HPP

#include <algorithm>
#include <array>
#include <cstddef>
#include <cstdint>
#include <stdexcept>
#include <string>
#include <string_view>
#include <utility>
#include <vector>

#include "workloads.hpp"

namespace tafla::benchmarks {

/// The strings of each table
inline constexpr std::size_t kStrings = 16;

/// A table's 16 strings, viewed where a codec reads them in place and owned
/// otherwise
using Strings = std::array<Text, kStrings>;

struct StringsShape {
    using Value = Strings;
    static constexpr std::string_view name = "16 strings";
};

/// The strings of one shape, and its name
struct StringsWorkload {
    std::string name;
    Strings strings;
};

inline bool same(const Strings& left, const Strings& right) {
    return std::equal(left.begin(), left.end(), right.begin(),
                      [](const Text& one, const Text& other) { return one.view() == other.view(); });
}

namespace detail {

inline constexpr std::string_view kAscii = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
/// Latin-1 letters, of two bytes in UTF-8
inline constexpr std::u32string_view kTwoBytes = U"àáâãäåæçèéêëìíîïñòóôõöøùúûüý";
/// The first of the CJK ideographs, of three bytes in UTF-8
inline constexpr char32_t kThreeBytes = 0x4e00;
/// The first of the emoji, of four bytes in UTF-8
inline constexpr char32_t kFourBytes = 0x1'f600;

/// `Math.floor(random() * count)`, an index below `count`
inline std::size_t index_below(Xorshift& random, std::size_t count) {
    return static_cast<std::size_t>(random.next() * static_cast<double>(count));
}

using Pick = char32_t (*)(Xorshift&);

inline char32_t ascii(Xorshift& random) {
    return static_cast<char32_t>(kAscii[index_below(random, kAscii.size())]);
}

inline char32_t two_bytes(Xorshift& random) {
    return kTwoBytes[index_below(random, kTwoBytes.size())];
}

inline char32_t three_bytes(Xorshift& random) {
    return kThreeBytes + static_cast<char32_t>(index_below(random, 0x5000));
}

inline char32_t four_bytes(Xorshift& random) {
    return kFourBytes + static_cast<char32_t>(index_below(random, 0x50));
}

inline void append_utf8(std::string& text, char32_t code_point) {
    const auto byte = [&text](char32_t bits) { text.push_back(static_cast<char>(bits)); };

    if (code_point < 0x80) {
        byte(code_point);
    } else if (code_point < 0x800) {
        byte(0xc0 | code_point >> 6);
        byte(0x80 | (code_point & 0x3f));
    } else if (code_point < 0x1'0000) {
        byte(0xe0 | code_point >> 12);
        byte(0x80 | (code_point >> 6 & 0x3f));
        byte(0x80 | (code_point & 0x3f));
    } else {
        byte(0xf0 | code_point >> 18);
        byte(0x80 | (code_point >> 12 & 0x3f));
        byte(0x80 | (code_point >> 6 & 0x3f));
        byte(0x80 | (code_point & 0x3f));
    }
}

/// 16 distinct strings of `length` characters, of which those at the indexes
/// that `special` selects come from `pick`, and the others from ASCII, as the
/// TypeScript suite's `shape()` makes them
template <class Special>
StringsWorkload shape(std::string name, std::size_t length, std::size_t seed, Special special, Pick pick) {
    Xorshift random(seed);

    Strings strings;
    for (auto& string : strings) {
        std::string text;
        for (std::size_t index = 0; index < length; ++index) {
            append_utf8(text, special(index) ? pick(random) : ascii(random));
        }
        string = Text::owned(std::move(text));
    }

    for (std::size_t index = 1; index < kStrings; ++index) {
        const auto view = strings[index].view();
        if (std::any_of(strings.begin(), strings.begin() + static_cast<std::ptrdiff_t>(index),
                        [view](const Text& before) { return before.view() == view; })) {
            throw std::logic_error("The strings of " + name + " aren't distinct");
        }
    }
    return {std::move(name), std::move(strings)};
}

inline StringsWorkload ascii_shape(std::string name, std::size_t length, std::size_t seed) {
    return shape(std::move(name), length, seed, [](std::size_t) { return false; }, ascii);
}

inline StringsWorkload every_shape(std::string name, std::size_t length, std::size_t seed, Pick pick) {
    return shape(std::move(name), length, seed, [](std::size_t) { return true; }, pick);
}

}  // namespace detail

/// Every shape, in the order that results list them: by their strings'
/// length in bytes, ASCII first.
///
/// Beside the TypeScript suite's shapes, which keep its seeds, are those of
/// C++'s thresholds: ASCII of 3, 7, 17 and 33 bytes, which with 8, 16 and 32
/// reach every branch of tafla's `valid_utf8()`, of 22 and 23 bytes, either
/// side of the longest string that libc++ holds without allocating, which
/// protobuf's strings are, and 64 and 1,024 bytes of characters of 2, 3 and 4
/// bytes, and 256 of ASCII with a character of 2 bytes first or last, which
/// part a validator that checks characters one by one from one that checks
/// blocks of bytes. 3-byte characters don't divide 64 or 1,024, so those
/// strings take 63 and 1,023 bytes.
inline std::vector<StringsWorkload> string_workloads() {
    using namespace detail;
    const auto at = [](std::size_t first) { return [first](std::size_t index) { return index == first; }; };

    std::vector<StringsWorkload> shapes;
    shapes.push_back({"empty", {}});
    shapes.push_back(ascii_shape("3 chars of ASCII", 3, 20));
    shapes.push_back(ascii_shape("7 chars of ASCII", 7, 21));
    shapes.push_back(ascii_shape("8 chars of ASCII", 8, 1));
    shapes.push_back(shape("8 chars, one of 2 bytes", 8, 2, at(3), two_bytes));
    shapes.push_back(ascii_shape("16 chars of ASCII", 16, 17));
    shapes.push_back(ascii_shape("17 chars of ASCII", 17, 22));
    shapes.push_back(shape(
        "16 chars, two of 2 bytes", 16, 4, [](std::size_t index) { return index == 2 || index == 11; }, two_bytes));
    shapes.push_back(shape("15 chars, one of 4 bytes", 15, 5, at(7), four_bytes));
    shapes.push_back(ascii_shape("22 chars of ASCII", 22, 23));
    shapes.push_back(ascii_shape("23 chars of ASCII", 23, 24));
    shapes.push_back(every_shape("8 chars of 3 bytes", 8, 3, three_bytes));
    shapes.push_back(ascii_shape("32 chars of ASCII", 32, 19));
    shapes.push_back(ascii_shape("33 chars of ASCII", 33, 11));
    shapes.push_back(every_shape("21 chars of 3 bytes", 21, 26, three_bytes));
    shapes.push_back(ascii_shape("64 chars of ASCII", 64, 7));
    shapes.push_back(every_shape("32 chars of 2 bytes", 32, 25, two_bytes));
    shapes.push_back(every_shape("16 chars of 4 bytes", 16, 27, four_bytes));
    shapes.push_back(every_shape("32 chars of 3 bytes", 32, 6, three_bytes));
    shapes.push_back(ascii_shape("256 chars of ASCII", 256, 8));
    shapes.push_back(shape("255 chars, first of 2 bytes", 255, 28, at(0), two_bytes));
    shapes.push_back(shape("255 chars, last of 2 bytes", 255, 29, at(254), two_bytes));
    shapes.push_back(every_shape("256 chars of 3 bytes", 256, 9, three_bytes));
    shapes.push_back(ascii_shape("1,024 chars of ASCII", 1024, 10));
    shapes.push_back(every_shape("341 chars of 3 bytes", 341, 31, three_bytes));
    shapes.push_back(every_shape("512 chars of 2 bytes", 512, 30, two_bytes));
    shapes.push_back(every_shape("256 chars of 4 bytes", 256, 32, four_bytes));
    return shapes;
}

}  // namespace tafla::benchmarks

#endif
