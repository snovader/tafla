// Checks that every codec reads back exactly what it wrote on every workload,
// that the item access ways sum the items alike, and that the generator makes
// the TypeScript comparison's values, which these were printed from, so that
// the languages' benchmarks time the same work

#include <gtest/gtest.h>

#include <bit>
#include <cstdint>

#include "suite.hpp"

namespace tafla::benchmarks {
namespace {

/// Builds every benchmark, which checks each codec's round trip and sums,
/// and runs each variant once
TEST(Benchmarks, EveryCodecReadsBackWhatItWroteAndEveryBenchmarkRuns) {
    const Inputs inputs;
    const auto all = benchmarks(inputs);
    EXPECT_EQ(all.size(), 18U);

    for (const auto& benchmark : all) {
        EXPECT_GE(benchmark.variants.size(), 3U) << benchmark.name;
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

/// tafla's columns are read in place, as views of the bytes, where the other
/// codecs but FlatBuffers copy them
TEST(Benchmarks, TaflaReadsColumnsInPlace) {
    const auto doc = document(100);
    const auto bytes = Tafla().serialize(DocumentShape{}, doc);
    const auto read = TaflaGen().deserialize(DocumentShape{}, bytes);

    EXPECT_TRUE(read.a.is_borrowed());
    EXPECT_GE(read.a.span().data(), reinterpret_cast<const float*>(bytes.data()));
    EXPECT_FALSE(
        Protobuf().deserialize(DocumentShape{}, bytes_of(Protobuf().serialize(DocumentShape{}, doc))).a.is_borrowed());
}

}  // namespace
}  // namespace tafla::benchmarks
