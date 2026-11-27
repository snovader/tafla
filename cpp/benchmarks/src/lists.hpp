// The suite of lists of strings: lists of 10, 100 and 10,000 distinct strings,
// made as the TypeScript suite of lists makes them, and the benchmarks that
// write each list with the code that tafla-gen generates from `lists.tafla`,
// as a vector of strings and as a vector of tables of one string each, read
// every string, and read the last string. Every codec is checked to read
// back exactly what it wrote before anything is timed. README.md describes
// the suite.

#ifndef TAFLA_BENCHMARKS_LISTS_HPP
#define TAFLA_BENCHMARKS_LISTS_HPP

#include <array>
#include <cstddef>
#include <cstdint>
#include <memory>
#include <ranges>
#include <set>
#include <span>
#include <stdexcept>
#include <string>
#include <string_view>
#include <vector>

#include <tafla/tafla.hpp>

#include "harness.hpp"
#include "lists/lists.hpp"
#include "workloads.hpp"

namespace tafla::benchmarks {

/// A list of distinct strings of the same length and mix of UTF-8 widths
struct ListOfStrings {
    std::string name;
    std::vector<std::string> strings;
    /// Views of the strings, which the generated fields take
    std::vector<std::string_view> views;
};

/// A kind of string: its length in characters, of ASCII or of 3 bytes in
/// UTF-8 each, and its index among the TypeScript suite's kinds, from which
/// its shapes' seeds are counted
struct StringKind {
    std::string_view name;
    std::size_t length;
    bool three_bytes;
    std::size_t index;
};

/// The number of strings of each shape
inline constexpr std::array<std::size_t, 3> kListCounts = {10, 100, 10'000};

/// The three of the TypeScript suite's eight kinds that every language's
/// suite of lists writes and reads
inline constexpr std::array<StringKind, 3> kStringKinds = {{
    {"16 chars of ASCII", 16, false, 1},
    {"256 chars of ASCII", 256, false, 2},
    {"8 chars of 3 bytes", 8, true, 6},
}};

/// The TypeScript suite's number of kinds, which its seeds are counted with
inline constexpr std::size_t kTypeScriptKinds = 8;

namespace detail {

inline constexpr std::string_view kAsciiCharacters = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";

/// The first of the CJK ideographs, whose characters take 3 bytes in UTF-8
inline constexpr std::uint32_t kThreeBytes = 0x4e00;

/// Appends the UTF-8 of a character of 3 bytes
inline void append_three_bytes(std::string& string, std::uint32_t code) {
    string += static_cast<char>(0xe0 | (code >> 12));
    string += static_cast<char>(0x80 | ((code >> 6) & 0x3f));
    string += static_cast<char>(0x80 | (code & 0x3f));
}

/// A count as `toLocaleString('en-US')` writes it, such as 10,000
inline std::string count_name(std::size_t count) {
    auto digits = std::to_string(count);
    for (auto at = digits.size(); at > 3; at -= 3) {
        digits.insert(at - 3, ",");
    }
    return digits;
}

}  // namespace detail

/// `count` distinct strings of `kind`, each of its length in characters, from
/// the TypeScript suite's xorshift of `seed`, which picks each character in
/// turn: one of 62 letters and digits, or one of the 0x5000 ideographs
inline ListOfStrings list_of_strings(std::size_t count, const StringKind& kind, std::size_t seed) {
    Xorshift random(seed);
    ListOfStrings shape{detail::count_name(count) + " × " + std::string(kind.name), {}, {}};

    shape.strings.reserve(count);
    for (std::size_t index = 0; index < count; ++index) {
        std::string string;
        for (std::size_t character = 0; character < kind.length; ++character) {
            if (kind.three_bytes) {
                const auto code = detail::kThreeBytes + static_cast<std::uint32_t>(random.next() * 0x5000);
                detail::append_three_bytes(string, code);
            } else {
                const auto at =
                    static_cast<std::size_t>(random.next() * static_cast<double>(detail::kAsciiCharacters.size()));
                string += detail::kAsciiCharacters[at];
            }
        }
        shape.strings.push_back(std::move(string));
    }

    if (std::set<std::string_view>(shape.strings.begin(), shape.strings.end()).size() != count) {
        throw std::logic_error("The strings of " + shape.name + " aren't distinct");
    }
    shape.views.assign(shape.strings.begin(), shape.strings.end());
    return shape;
}

/// Every shape, by count and then by kind, as the TypeScript suite lists
/// them, each with the seed that it has there
inline std::vector<ListOfStrings> lists_of_strings() {
    std::vector<ListOfStrings> shapes;
    for (std::size_t count_index = 0; count_index < kListCounts.size(); ++count_index) {
        for (const auto& kind : kStringKinds) {
            const auto seed = 1 + kind.index + kTypeScriptKinds * count_index;
            shapes.push_back(list_of_strings(kListCounts[count_index], kind, seed));
        }
    }
    return shapes;
}

/// tafla-gen's `StringList`, a vector of strings, written from views of the
/// strings and read with its getter's `tafla::StringVector`
struct TaflaGenStrings {
    static constexpr std::string_view name = "tafla-gen [string]";

    [[nodiscard]] static Bytes serialize(std::span<const std::string_view> strings) {
        return lists::StringListFields{.items = strings}.serialize();
    }

    /// Every string, at once, which checks their UTF-8 as one run
    [[nodiscard]] static std::vector<std::string_view> deserialize(std::span<const std::uint8_t> bytes) {
        return lists::StringList(bytes).items().to_vector();
    }

    /// The last string, from its ends alone
    [[nodiscard]] static std::string_view read_last(std::span<const std::uint8_t> bytes) {
        const auto items = lists::StringList(bytes).items();
        return items.get(items.size() - 1);
    }
};

/// tafla-gen's `NameList`, a vector of tables of one string each, which is how
/// a schema holds a list of strings without vectors of strings. Its generic
/// fields write a view that converts each string into the fields of its
/// table as it's written, and its getters read each table's string through a
/// reader of the table.
struct TaflaGenNames {
    static constexpr std::string_view name = "tafla-gen [Name]";

    [[nodiscard]] static Bytes serialize(std::span<const std::string_view> strings) {
        const auto fields_of = [](std::string_view string) { return lists::NameFields{.value = string}; };
        const lists::generic::NameListFields fields{.items = strings | std::views::transform(fields_of)};
        return fields.serialize();
    }

    /// Every string, iterating the items, which reads each item's end once
    [[nodiscard]] static std::vector<std::string_view> deserialize(std::span<const std::uint8_t> bytes) {
        const auto items = lists::NameList(bytes).items();
        std::vector<std::string_view> strings(items.size());
        auto into = strings.begin();
        for (const auto item : items) {
            *into++ = item.value();
        }
        return strings;
    }

    /// The last string, from its table's ends alone
    [[nodiscard]] static std::string_view read_last(std::span<const std::uint8_t> bytes) {
        const auto items = lists::NameList(bytes).items();
        return items.get(items.size() - 1).value();
    }
};

/// The name of tafla-gen's vector of strings again, timed beside itself, an
/// A/A pair whose ratio shows the harness's noise
inline constexpr std::string_view kStringsAgain = "tafla-gen [string] A/A";

/// Checks that a codec reads back every string of a shape, in order, and its
/// last string, and that tafla-gen's vector of strings writes the bytes of
/// `TableBuilder`
template <class Codec>
void check_list(const ListOfStrings& shape) {
    const auto bytes = Codec::serialize(shape.views);
    const auto read = Codec::deserialize(bytes.span());
    const bool same = read.size() == shape.strings.size() && std::ranges::equal(read, shape.strings) &&
                      Codec::read_last(bytes.span()) == shape.strings.back();
    if (!same) {
        throw std::logic_error(std::string(Codec::name) + " didn't read back " + shape.name);
    }
}

inline void check_same_table(const ListOfStrings& shape) {
    TableBuilder builder(1, TableLayout());
    builder.set_string_vector(shape.views);
    if (builder.to_bytes() != TaflaGenStrings::serialize(shape.views)) {
        throw std::logic_error("tafla-gen didn't write TableBuilder's " + shape.name);
    }
}

/// Adds a codec's variants, named `name`, to a shape's serialize,
/// deserialize and last string's benchmarks. Each keeps the bytes that it
/// reads, which are allocated once.
template <class Codec>
void add_list_variants(std::array<Benchmark, 3>& groups, const ListOfStrings& shape, std::string_view name) {
    check_list<Codec>(shape);

    const auto* views = &shape.views;
    groups[0].variants.push_back(variant(std::string(name), [views] { return Codec::serialize(*opaque(views)); }));

    const auto bytes = std::make_shared<const Bytes>(Codec::serialize(shape.views));
    groups[1].variants.push_back(
        variant(std::string(name), [bytes] { return Codec::deserialize(opaque(bytes.get())->span()); }));
    groups[2].variants.push_back(
        variant(std::string(name), [bytes] { return Codec::read_last(opaque(bytes.get())->span()); }));
}

/// The benchmarks of every shape: tafla-gen's vector of strings first, its
/// vector of tables, and its vector of strings again, an A/A pair. The
/// shapes must outlive the benchmarks.
inline std::vector<Benchmark> list_benchmarks(const std::vector<ListOfStrings>& shapes) {
    std::vector<Benchmark> all;
    for (const auto& shape : shapes) {
        check_same_table(shape);

        std::array<Benchmark, 3> groups{Benchmark{shape.name + ": serialize", {}},
                                        Benchmark{shape.name + ": deserialize", {}},
                                        Benchmark{shape.name + ": read the last string", {}}};
        add_list_variants<TaflaGenStrings>(groups, shape, TaflaGenStrings::name);
        add_list_variants<TaflaGenNames>(groups, shape, TaflaGenNames::name);
        add_list_variants<TaflaGenStrings>(groups, shape, kStringsAgain);

        for (auto& group : groups) {
            all.push_back(std::move(group));
        }
    }
    return all;
}

}  // namespace tafla::benchmarks

#endif
