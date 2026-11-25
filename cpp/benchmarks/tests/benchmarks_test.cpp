// Checks that every codec reads back exactly what it wrote on every workload
// and every shape of strings, that the item access ways sum the items alike,
// and that the generators make the TypeScript suites' values, which these
// were printed from, so that the languages' benchmarks time the same work

#include <gtest/gtest.h>

#include <bit>
#include <cstdint>
#include <iterator>
#include <string_view>
#include <type_traits>

#include "suite.hpp"

namespace tafla::benchmarks {
namespace {

/// Builds every benchmark, which checks each codec's round trip and sums,
/// and tafla-gen's writes into reused bytes, and runs each variant once.
/// Each has a variant and an A/A pair of it at least.
TEST(Benchmarks, EveryCodecReadsBackWhatItWroteAndEveryBenchmarkRuns) {
    const Inputs inputs;
    const auto all = benchmarks(inputs);
    EXPECT_EQ(all.size(), 19U);

    for (const auto& benchmark : all) {
        EXPECT_GE(benchmark.variants.size(), 2U) << benchmark.name;
        for (const auto& variant : benchmark.variants) {
            variant.loop(1);
        }
    }
}

std::uint64_t sum(std::span<const std::uint32_t> keys) {
    std::uint64_t total = 0;
    for (const auto key : keys) {
        total += key;
    }
    return total;
}

/// Hashes the float columns' bits, row by row, as the TypeScript script that
/// printed these did with 32-bit integers
std::uint32_t hash_columns(const Document& document) {
    std::uint32_t hash = 0;
    const auto a = document.a.span();
    const auto b = document.b.span();
    for (std::size_t row = 0; row < a.size(); ++row) {
        hash = hash * 31 + std::bit_cast<std::uint32_t>(a[row]);
        hash = hash * 31 + std::bit_cast<std::uint32_t>(b[row]);
    }
    return hash;
}

TEST(Workloads, GeneratesTheTypeScriptDocumentOf100Rows) {
    const auto doc = document(100);

    EXPECT_EQ(doc.keys.span()[0], 14'727'226U);
    EXPECT_EQ(doc.keys.span()[99], 10'335'322U);
    EXPECT_EQ(sum(doc.keys.span()), 825'833'448U);
    EXPECT_EQ(std::bit_cast<std::uint32_t>(doc.a.span()[0]), 0xc221'9b29U);
    EXPECT_EQ(std::bit_cast<std::uint32_t>(doc.b.span()[99]), 0xc163'647eU);
    EXPECT_EQ(hash_columns(doc), 1'493'961'445U);
}

TEST(Workloads, GeneratesTheTypeScriptDocumentOf100000Rows) {
    const auto doc = document(100'000);

    EXPECT_EQ(doc.keys.span()[0], 13'320'945U);
    EXPECT_EQ(doc.keys.span()[99'999], 11'621'954U);
    EXPECT_EQ(sum(doc.keys.span()), 839'050'642'924U);
    EXPECT_EQ(std::bit_cast<std::uint32_t>(doc.a.span()[0]), 0xc1d4'd7f6U);
    EXPECT_EQ(std::bit_cast<std::uint32_t>(doc.b.span()[99'999]), 0x3e8c'91f5U);
    EXPECT_EQ(hash_columns(doc), 1'138'446'770U);
}

TEST(Workloads, GeneratesTheTypeScriptVectorOfNestedTables) {
    const auto items = list(kListItems).items;
    const auto bits = [](const Item& item) {
        return std::array{std::bit_cast<std::uint32_t>(item.value), std::bit_cast<std::uint32_t>(item.first),
                          std::bit_cast<std::uint32_t>(item.second)};
    };

    ASSERT_EQ(items.size(), 100U);
    EXPECT_EQ(bits(items[0]), (std::array<std::uint32_t, 3>{0x41d2'acb6, 0xbfce'db15, 0x41e3'2e23}));
    EXPECT_EQ(bits(items[1]), (std::array<std::uint32_t, 3>{0x41c1'2b3a, 0x3ea9'43c1, 0x422e'ae51}));
    EXPECT_EQ(bits(items[99]), (std::array<std::uint32_t, 3>{0x4193'd915, 0x3f97'9e05, 0x418f'26e1}));
}

/// Whether `column`'s values lie within `bytes`
template <class T>
bool lies_within(std::span<const T> column, std::span<const std::uint8_t> bytes) {
    const auto begin = reinterpret_cast<std::uintptr_t>(column.data());
    const auto start = reinterpret_cast<std::uintptr_t>(bytes.data());
    return begin >= start && begin + column.size_bytes() <= start + bytes.size();
}

/// tafla's columns are read in place, as views of the bytes, as FlatBuffers'
/// are, where protobuf's view the message that it decoded them into, which
/// the document is kept with
TEST(Benchmarks, TaflaReadsColumnsInPlace) {
    const auto doc = document(100);
    const auto bytes = Tafla().serialize(DocumentShape{}, doc);
    const auto read = TaflaGen().deserialize(DocumentShape{}, bytes);

    EXPECT_TRUE(read.a.is_borrowed());
    EXPECT_TRUE(lies_within(read.a.span(), bytes_of(bytes)));

    const auto message = Protobuf().serialize(DocumentShape{}, doc);
    const auto decoded = Protobuf().deserialize(DocumentShape{}, bytes_of(message));
    EXPECT_TRUE(decoded.value.a.is_borrowed());
    EXPECT_FALSE(lies_within(decoded.value.a.span(), bytes_of(message)));
    EXPECT_NE(decoded.owner, nullptr);
}

/// Builds every benchmark of the strings suite, which checks each codec's
/// round trip of every shape and that tafla-gen writes the tables that
/// `TableBuilder` writes, and runs each variant once. Each has a variant of
/// every codec but `bit_cast`, and tafla's A/A pair.
TEST(Strings, EveryCodecReadsBackEveryStringAndEveryBenchmarkRuns) {
    const auto workloads = string_workloads();
    const auto all = string_benchmarks(workloads);
    EXPECT_EQ(all.size(), 2 * workloads.size());

    for (const auto& benchmark : all) {
        EXPECT_EQ(benchmark.variants.size(), 6U) << benchmark.name;
        for (const auto& variant : benchmark.variants) {
            variant.loop(1);
        }
    }
}

/// A shape of strings that the TypeScript suite's generator printed: its
/// name, the UTF-8 length of each string, and a hash of their bytes
struct PrintedShape {
    std::string_view name;
    std::size_t bytes;
    std::uint32_t hash;
};

/// Printed by the TypeScript suite's `shape()`, with the seeds of
/// `string_workloads()`
constexpr PrintedShape kTypeScriptShapes[] = {
    {"empty", 0, 0U},
    {"3 chars of ASCII", 3, 1'932'538'938U},
    {"7 chars of ASCII", 7, 761'151'393U},
    {"8 chars of ASCII", 8, 2'912'978'722U},
    {"8 chars, one of 2 bytes", 9, 1'437'720'515U},
    {"16 chars of ASCII", 16, 841'955'873U},
    {"17 chars of ASCII", 17, 1'771'383'885U},
    {"16 chars, two of 2 bytes", 18, 629'651'658U},
    {"15 chars, one of 4 bytes", 18, 210'269'755U},
    {"22 chars of ASCII", 22, 260'621'963U},
    {"23 chars of ASCII", 23, 880'775'692U},
    {"8 chars of 3 bytes", 24, 1'717'679'536U},
    {"32 chars of ASCII", 32, 2'197'417'048U},
    {"33 chars of ASCII", 33, 931'439'320U},
    {"21 chars of 3 bytes", 63, 1'063'028'247U},
    {"64 chars of ASCII", 64, 1'948'179'883U},
    {"32 chars of 2 bytes", 64, 2'385'880'485U},
    {"16 chars of 4 bytes", 64, 3'602'692'366U},
    {"32 chars of 3 bytes", 96, 2'365'967'191U},
    {"256 chars of ASCII", 256, 1'342'101'721U},
    {"255 chars, first of 2 bytes", 256, 1'158'126'873U},
    {"255 chars, last of 2 bytes", 256, 2'803'774'917U},
    {"256 chars of 3 bytes", 768, 726'450'835U},
    {"1,024 chars of ASCII", 1024, 2'674'459'570U},
    {"341 chars of 3 bytes", 1023, 1'500'662'198U},
    {"512 chars of 2 bytes", 1024, 3'629'842'196U},
    {"256 chars of 4 bytes", 1024, 2'910'146'515U},
};

/// Hashes the strings' UTF-8, byte by byte, as the TypeScript script that
/// printed these did with 32-bit integers
std::uint32_t hash_strings(const Strings& strings) {
    std::uint32_t hash = 0;
    for (const auto& string : strings) {
        for (const char byte : string.view()) {
            hash = hash * 31 + static_cast<std::uint8_t>(byte);
        }
    }
    return hash;
}

TEST(StringWorkloads, GeneratesTheTypeScriptStrings) {
    const auto workloads = string_workloads();
    ASSERT_EQ(workloads.size(), std::size(kTypeScriptShapes));

    for (std::size_t index = 0; index < workloads.size(); ++index) {
        const auto& [name, strings] = workloads[index];
        const auto& expected = kTypeScriptShapes[index];

        EXPECT_EQ(name, expected.name);
        for (const auto& string : strings) {
            EXPECT_EQ(string.view().size(), expected.bytes) << name;
        }
        EXPECT_EQ(hash_strings(strings), expected.hash) << name;
    }
    EXPECT_EQ(workloads[3].strings[0].view(), "2I8eEZQc");
}

/// tafla, FlatBuffers and msgpack read strings as views of the bytes, and
/// protobuf as copies that it owns
TEST(Strings, OnlyProtobufCopiesTheStrings) {
    const auto workloads = string_workloads();
    const auto& strings = workloads[18].strings;
    ASSERT_EQ(workloads[18].name, "32 chars of 3 bytes");

    for_each_codec([&strings](const auto& codec) {
        using Codec = std::decay_t<decltype(codec)>;
        if constexpr (Encodes<Codec, StringsShape>) {
            const auto bytes = codec.serialize(StringsShape{}, strings);
            const auto read = codec.deserialize(StringsShape{}, bytes_of(bytes));
            const bool views = Codec::name != Protobuf::name;

            for (const auto& string : read) {
                const auto view = string.view();
                EXPECT_EQ(lies_within(std::span<const char>(view.data(), view.size()), bytes_of(bytes)), views)
                    << Codec::name;
            }
        }
    });
}

}  // namespace
}  // namespace tafla::benchmarks
