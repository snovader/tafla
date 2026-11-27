// The vectors of strings of `schemas/all_types.tafla`, whose getters return a
// `tafla::StringVector` and whose fields structs take spans of
// `std::string_view`s: the code writes them as `TableBuilder` writes them, in
// a message, a nested table and the items of a vector, into new bytes and into
// the start of bytes that the caller gives, reads back what it writes
// wherever the bytes lie, whichever way the strings are read, rejects a
// message too long for its offsets, and reads corrupted bytes as values or
// decode errors.

#include <gtest/gtest.h>
#include <rapidcheck/gtest.h>

#include <cstddef>
#include <cstdint>
#include <optional>
#include <ranges>
#include <span>
#include <string>
#include <string_view>
#include <utility>
#include <vector>

#include "all_types/all_types.hpp"
#include "common/at_byte_offset.hpp"
#include "common/build.hpp"
#include "common/given_bytes.hpp"
#include "common/values.hpp"

namespace tafla::test {
namespace {

/// The values of an `Entry`
struct OwnedEntry {
    std::string key;
    std::vector<std::string> aliases;
};

/// The values of a `StringVectors`, which its fields struct views
struct OwnedStringVectors {
    std::uint32_t id;
    std::vector<std::string> tags;
    std::vector<std::string> labels;
    std::vector<OwnedEntry> entries;
};

std::vector<std::string_view> views_of(const std::vector<std::string>& strings) {
    return {strings.begin(), strings.end()};
}

/// The views of an `OwnedStringVectors`, which its fields struct holds spans
/// of, and which stay where they are while the views live
class Views {
public:
    explicit Views(const OwnedStringVectors& owned)
        : id_(owned.id), tags_(views_of(owned.tags)), labels_(views_of(owned.labels)) {
        for (const auto& entry : owned.entries) {
            aliases_.push_back(views_of(entry.aliases));
        }
        for (std::size_t index = 0; index < owned.entries.size(); ++index) {
            entries_.push_back({.key = owned.entries[index].key, .aliases = aliases_[index]});
        }
    }

    Views(const Views&) = delete;
    Views& operator=(const Views&) = delete;

    [[nodiscard]] all_types::StringVectorsFields fields() const {
        return {.id = id_, .tags = tags_, .labels = {.values = labels_}, .entries = entries_};
    }

private:
    std::uint32_t id_;
    std::vector<std::string_view> tags_;
    std::vector<std::string_view> labels_;
    std::vector<std::vector<std::string_view>> aliases_;
    std::vector<all_types::EntryFields> entries_;
};

rc::Gen<std::vector<std::string>> string_lists() {
    return values::arrays(values::strings(), 6);
}

rc::Gen<OwnedStringVectors> owned_string_vectors() {
    return rc::gen::exec([] {
        const auto entries = *values::arrays(rc::gen::construct<OwnedEntry>(values::strings(), string_lists()), 3);
        return OwnedStringVectors{
            .id = *values::integers<std::uint32_t>(),
            .tags = *string_lists(),
            .labels = *string_lists(),
            .entries = entries,
        };
    });
}

Layout headerless() {
    return Layout::headerless(OffsetType::uint32);
}

Field strings_field(std::span<const std::string_view> strings) {
    return {StringVectorField{{strings.begin(), strings.end()}}};
}

/// The description of the table that a `StringVectors` writes
Table describe(const all_types::StringVectorsFields& fields) {
    VectorField entries;
    for (const auto& entry : fields.entries) {
        entries.items.push_back(
            {Source::builder, {headerless(), std::nullopt, {{std::string(entry.key)}, strings_field(entry.aliases)}}});
    }

    const Nested labels{Source::builder, {headerless(), std::nullopt, {strings_field(fields.labels.values)}}};
    return {Layout{},
            std::nullopt,
            {{fields.id}, strings_field(fields.tags), {TableField{labels, std::nullopt}}, {entries}}};
}

/// How a test reads the strings of a vector of strings, and the items of a
/// vector, from the view that the getter returns: at once with
/// `to_vector()`, one at a time by index, or by iterating the view
enum class Items { collected, by_index, iterated };

template <class View>
auto items_of(const View& view, Items way) {
    switch (way) {
        case Items::collected:
            return view.to_vector();
        case Items::by_index: {
            std::vector<typename View::value_type> items;
            for (std::size_t index = 0; index < view.size(); ++index) {
                items.push_back(view.get(index));
            }
            return items;
        }
        case Items::iterated:
            break;
    }
    return std::vector<typename View::value_type>(view.begin(), view.end());
}

Field read_strings(const StringVector& view, Items way) {
    const auto strings = items_of(view, way);
    return {StringVectorField{{strings.begin(), strings.end()}}};
}

/// Reads every field of a `StringVectors` through its getters and views
Table read(const all_types::StringVectors& table, Items way) {
    VectorField entries;
    for (const auto& entry : items_of(table.entries(), way)) {
        entries.items.push_back(
            {Source::builder,
             {headerless(), std::nullopt, {{std::string(entry.key())}, read_strings(entry.aliases(), way)}}});
    }

    const Nested labels{Source::builder, {headerless(), std::nullopt, {read_strings(table.labels().values(), way)}}};
    return {Layout{},
            std::nullopt,
            {{table.id()}, read_strings(table.tags(), way), {TableField{labels, std::nullopt}}, {entries}}};
}

/// What reading returned: the table read, or nothing for a decode error
template <class Read>
std::optional<Table> try_read(Read read) {
    try {
        return read();
    } catch (const DecodeError&) {
        return std::nullopt;
    }
}

constexpr Items kWays[] = {Items::collected, Items::by_index, Items::iterated};

RC_GTEST_PROP(GeneratedStringVectors, AreWrittenAsTableBuilderWritesThemAndReadBackWhereverTheirBytesLie, ()) {
    const auto owned = *owned_string_vectors();
    const auto byte_offset = *rc::gen::inRange<std::size_t>(0, 8);
    const Views views(owned);
    const auto fields = views.fields();
    const auto bytes = fields.serialize();

    RC_ASSERT(bytes == to_bytes(describe(fields)));
    RC_ASSERT(written_into_any_bytes(fields, bytes.span()));

    const AtByteOffset copy(bytes.span(), byte_offset);
    for (const auto way : kWays) {
        RC_ASSERT(read(all_types::StringVectors(copy.bytes()), way) == describe(fields));
    }
}

/// A generic fields struct writes its vector of entries from a view of the
/// caller's own items, beside the vectors of strings, as from the fields
RC_GTEST_PROP(GeneratedStringVectors, AreWrittenBesideAViewOfOtherItemsAsBesideTheirFields, ()) {
    const auto owned = *owned_string_vectors();
    const Views views(owned);
    const auto fields = views.fields();

    std::vector<std::vector<std::string_view>> aliases;
    for (const auto& entry : owned.entries) {
        aliases.push_back(views_of(entry.aliases));
    }
    const auto to_fields = [&](const OwnedEntry& entry) {
        const auto index = static_cast<std::size_t>(&entry - owned.entries.data());
        return all_types::EntryFields{.key = entry.key, .aliases = aliases[index]};
    };
    const all_types::generic::StringVectorsFields generic{
        .id = fields.id,
        .tags = fields.tags,
        .labels = fields.labels,
        .entries = owned.entries | std::views::transform(to_fields),
    };

    RC_ASSERT(generic.serialize() == fields.serialize());
}

/// Names of up to 8 letters, six of which fit in a table of `uint16` offsets
rc::Gen<std::string> names() {
    return rc::gen::mapcat(rc::gen::inRange<std::size_t>(0, 9), [](std::size_t length) {
        return rc::gen::container<std::string>(length, rc::gen::inRange<char>('a', 'z' + 1));
    });
}

RC_GTEST_PROP(GeneratedStringVectors, NarrowStringVectorsOfUint16OffsetsAreWrittenAndReadBack, ()) {
    const auto names = *values::arrays(tafla::test::names(), 6);
    const auto count = *values::integers<std::uint8_t>();
    const auto name_views = views_of(names);
    const all_types::NarrowStringVectorsFields fields{.names = name_views, .count = count};
    const auto bytes = fields.serialize();

    const Table described{Layout{false, false, OffsetType::uint16}, std::nullopt, {strings_field(name_views), {count}}};
    RC_ASSERT(bytes == to_bytes(described));
    RC_ASSERT(written_into_any_bytes(fields, bytes.span()));

    const all_types::NarrowStringVectors table(bytes);
    RC_ASSERT(read_strings(table.names(), Items::collected) == strings_field(name_views));
    RC_ASSERT(table.count() == count);
}

TEST(GeneratedStringVectors, NarrowStringVectorsTooLongForUint16OffsetsAreRejected) {
    const std::vector<std::string> names{std::string(0x8000, 'a'), std::string(0x8000, 'b')};
    const auto name_views = views_of(names);
    const all_types::NarrowStringVectorsFields fields{.names = name_views, .count = 0};

    try {
        static_cast<void>(fields.serialize());
        FAIL() << "The message was written";
    } catch (const BuildError& error) {
        EXPECT_EQ(error.kind(), BuildError::Kind::too_long);
        EXPECT_EQ(error.offset_type(), OffsetType::uint16);
        EXPECT_EQ(error.index(), 0U);
    }

    std::vector<std::uint8_t> buffer(0x10010);
    EXPECT_THROW(static_cast<void>(fields.serialize_into(buffer)), BuildError);
}

/// Corrupted bytes read as values or decode errors, alike whichever way the
/// strings and items are read
RC_GTEST_PROP(GeneratedStringVectors, CorruptedTablesReadAsValuesOrDecodeErrors, ()) {
    const auto owned = *owned_string_vectors();
    const auto corruption = *values::corruptions();
    const Views views(owned);

    for (const auto& bytes : values::corrupted(views.fields().serialize().to_vector(), corruption)) {
        const all_types::StringVectors table(bytes);
        const auto collected = try_read([&] { return read(table, Items::collected); });
        RC_ASSERT(try_read([&] { return read(table, Items::by_index); }) == collected);
        RC_ASSERT(try_read([&] { return read(table, Items::iterated); }) == collected);

        try {
            static_cast<void>(all_types::NarrowStringVectors(bytes).names().to_vector());
        } catch (const DecodeError&) {
            // As any read of corrupted bytes may
        }
    }
}

TEST(GeneratedStringVectors, ATagThatIsntValidUtf8FailsAndTheOthersRead) {
    const std::vector<std::string_view> tags{"ok", "bad"};
    auto bytes = all_types::StringVectorsFields{.id = 1, .tags = tags, .labels = {}}.serialize().to_vector();
    // The second tag's first byte, after the count, two ends and "ok"
    const auto position = TableReader(bytes, TableLayout()).root_offset(1);
    bytes[position + 3 * 4 + 2] = 0xc0;
    const all_types::StringVectors table(bytes);

    EXPECT_EQ(table.tags().get(0), "ok");
    EXPECT_THROW(static_cast<void>(table.tags().get(1)), DecodeError);
    EXPECT_THROW(static_cast<void>(table.tags().to_vector()), DecodeError);
    EXPECT_EQ(table.id(), 1U);
    EXPECT_TRUE(table.labels().values().empty());
}

TEST(GeneratedStringVectors, ViewsOfEmptyVectorsOfStringsHoldNoStrings) {
    const auto bytes = all_types::StringVectorsFields{}.serialize();
    const all_types::StringVectors table(bytes);

    EXPECT_TRUE(table.tags().empty());
    EXPECT_TRUE(table.tags().to_vector().empty());
    EXPECT_TRUE(table.labels().values().empty());
    EXPECT_TRUE(table.entries().empty());
    EXPECT_THROW(static_cast<void>(table.tags().get(0)), std::out_of_range);
}

}  // namespace
}  // namespace tafla::test
