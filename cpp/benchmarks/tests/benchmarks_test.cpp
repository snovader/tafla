// Checks that every codec reads back exactly what it wrote on every workload
// and list of strings, that the item access ways sum the items alike, and
// that the generators make the values of the TypeScript comparison and suite
// of lists, which these were printed from, so that the languages' benchmarks
// time the same work

#include <gtest/gtest.h>

#include <bit>
#include <cstdint>
#include <string>
#include <vector>

#include "lists.hpp"
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

/// Builds every benchmark of the suite of lists of strings, which checks that
/// each variant reads back every string and the last, and runs each once
TEST(Lists, EveryVariantReadsBackWhatItWroteAndEveryBenchmarkRuns) {
    const auto shapes = lists_of_strings();
    const auto all = list_benchmarks(shapes);
    EXPECT_EQ(all.size(), 27U);

    for (const auto& benchmark : all) {
        EXPECT_EQ(benchmark.variants.size(), 3U) << benchmark.name;
        for (const auto& variant : benchmark.variants) {
            variant.loop(1);
        }
    }
}

/// Hashes the strings' UTF-8, one after another, as the TypeScript script
/// that printed these did with 32-bit integers
std::uint32_t hash_utf8(const std::vector<std::string>& strings) {
    std::uint32_t hash = 0;
    for (const auto& string : strings) {
        for (const auto byte : string) {
            hash = hash * 31 + static_cast<std::uint8_t>(byte);
        }
    }
    return hash;
}

TEST(Lists, GeneratesTheTypeScriptListsOfStrings) {
    struct Expected {
        const char* name;
        std::size_t count;
        const char* first;
        std::uint32_t hash;
    };
    // The first 8 characters of each list's first string
    const std::vector<Expected> expected{
        {"10 × 16 chars of ASCII", 10, "2FsyeXxp", 772'871'957},
        {"10 × 256 chars of ASCII", 10, "2GYBM2rv", 112'022'847},
        {"10 × 8 chars of 3 bytes", 10, "鑡絞眙蜩榩眳矝愞", 297'373'238},
        {"100 × 16 chars of ASCII", 100, "2N45gJLJ", 4'043'576'274},
        {"100 × 256 chars of ASCII", 100, "2OC9KSUH", 1'275'637'513},
        {"100 × 8 chars of 3 bytes", 100, "鑗蝝姘徃栲鈞甕顽", 2'874'631'186},
        {"10,000 × 16 chars of ASCII", 10'000, "2VGBSOLG", 3'888'143'744},
        {"10,000 × 256 chars of ASCII", 10'000, "2WgxnHUM", 1'626'208'024},
        {"10,000 × 8 chars of 3 bytes", 10'000, "鑶鄷攴臚鷑靆畖鱯", 2'475'471'517},
    };

    const auto shapes = lists_of_strings();
    ASSERT_EQ(shapes.size(), expected.size());
    for (std::size_t index = 0; index < shapes.size(); ++index) {
        const auto& shape = shapes[index];
        const std::string first = expected[index].first;
        EXPECT_EQ(shape.name, expected[index].name);
        EXPECT_EQ(shape.strings.size(), expected[index].count) << shape.name;
        EXPECT_EQ(shape.strings.front().substr(0, first.size()), first) << shape.name;
        EXPECT_EQ(hash_utf8(shape.strings), expected[index].hash) << shape.name;
    }
}

}  // namespace
}  // namespace tafla::benchmarks
