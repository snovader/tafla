// The code generated from `typescript/core/test/presence.tafla`: optional
// fields, which its writers leave out with the offset 0, as
// `TableBuilder::set_absent()` does, and fields that a later version of a
// message or a later revision of a table appends, which its readers find
// missing from the tables that don't hold them. A getter returns
// `std::nullopt` for a missing field, or a scalar's default, which
// `has_<field>()` tells apart. The messages and tables named `V1` are the
// first version or revision of the others, whose bytes the tests read with
// them.

#include <gtest/gtest.h>
#include <rapidcheck/gtest.h>

#include <cstddef>
#include <cstdint>
#include <optional>
#include <ranges>
#include <span>
#include <stdexcept>
#include <string>
#include <string_view>
#include <vector>

#include "common/at_byte_offset.hpp"
#include "common/build.hpp"
#include "common/given_bytes.hpp"
#include "common/values.hpp"
#include "presence/presence.hpp"

namespace tafla::test {
namespace {

// The values of a record, which its fields structs view

struct OwnedEntry {
    std::uint16_t key;
    std::optional<std::string> note;
    float weight;
};

struct OwnedRecord {
    std::uint32_t id;
    std::optional<std::string> name;
    std::optional<std::vector<double>> values;
    std::optional<presence::PointFields> origin;
    std::optional<bool> flag;
    std::optional<std::vector<OwnedEntry>> entries;
    std::optional<std::int64_t> total;
    std::string label;
    std::optional<presence::Extent> extent;
};

/// A value, or nothing as often as not
template <class T>
rc::Gen<std::optional<T>> optional(rc::Gen<T> values) {
    return rc::gen::oneOf(rc::gen::just(std::optional<T>()),
                          rc::gen::map(values, [](T value) { return std::optional<T>(std::move(value)); }));
}

rc::Gen<OwnedEntry> entries() {
    return rc::gen::exec([] {
        return OwnedEntry{*values::integers<std::uint16_t>(), *optional(values::strings()), *strategy::float32()};
    });
}

rc::Gen<OwnedRecord> records() {
    return rc::gen::exec([] {
        const auto points = rc::gen::exec([] {
            return presence::PointFields{
                .x = *strategy::float64(), .y = *strategy::float64(), .z = *strategy::float64()};
        });
        const auto extents = rc::gen::exec(
            [] { return presence::Extent{.width = *strategy::float32(), .height = *strategy::float32()}; });

        return OwnedRecord{
            .id = *values::integers<std::uint32_t>(),
            .name = *optional(values::strings()),
            .values = *optional(values::arrays(strategy::float64(), 4)),
            .origin = *optional(points),
            .flag = *optional(rc::gen::arbitrary<bool>()),
            .entries = *optional(values::arrays(entries(), 3)),
            .total = *optional(values::integers<std::int64_t>()),
            .label = *values::strings(),
            .extent = *optional(extents),
        };
    });
}

std::optional<std::string_view> view_of(const std::optional<std::string>& string) {
    if (string) {
        return *string;
    }
    return std::nullopt;
}

presence::EntryFields entry_fields_of(const OwnedEntry& entry) {
    return {.key = entry.key, .note = view_of(entry.note), .weight = entry.weight};
}

/// The fields of a record, which view its values, and its entries' fields,
/// which `entries` keeps
presence::RecordFields fields_of(const OwnedRecord& record, std::vector<presence::EntryFields>& entries) {
    std::optional<std::span<const presence::EntryFields>> entry_fields;
    if (record.entries) {
        for (const auto& entry : *record.entries) {
            entries.push_back(entry_fields_of(entry));
        }
        entry_fields = entries;
    }

    return {
        .id = record.id,
        .name = view_of(record.name),
        .values = record.values,
        .origin = record.origin,
        .flag = record.flag,
        .entries = entry_fields,
        .total = record.total,
        .label = record.label,
        .extent = record.extent,
    };
}

/// The fields of the first version of a record, without those that the
/// second added
presence::RecordV1Fields first_version_of(const OwnedRecord& record, std::vector<presence::EntryV1Fields>& entries) {
    std::optional<std::span<const presence::EntryV1Fields>> entry_fields;
    if (record.entries) {
        for (const auto& entry : *record.entries) {
            entries.push_back({.key = entry.key, .note = view_of(entry.note)});
        }
        entry_fields = entries;
    }

    std::optional<presence::PointV1Fields> origin;
    if (record.origin) {
        origin = presence::PointV1Fields{.x = record.origin->x, .y = record.origin->y};
    }

    return {
        .id = record.id,
        .name = view_of(record.name),
        .values = record.values,
        .origin = origin,
        .flag = record.flag,
        .entries = entry_fields,
        .total = record.total,
    };
}

// The descriptions of what the fields structs write, with each field that
// they leave out absent

const Layout kRecordLayout{true, true, OffsetType::uint32};

Field absent() {
    return {Absent{}};
}

/// The description of a value that may be left out, or of an absent field
template <class T, class Describe>
Field described(const std::optional<T>& value, Describe describe) {
    return value ? describe(*value) : absent();
}

Field nested(std::vector<Field> fields) {
    return {TableField{{Source::builder, {Layout::headerless(OffsetType::uint32), std::nullopt, std::move(fields)}},
                       std::nullopt}};
}

Nested item(std::vector<Field> fields) {
    return {Source::builder, {Layout::headerless(OffsetType::uint32), std::nullopt, std::move(fields)}};
}

Field string_field(std::string_view string) {
    return {std::string(string)};
}

Field extent_field(const presence::Extent& extent) {
    return {Struct{{{extent.width}, {extent.height}}}};
}

Table describe(const presence::RecordFields& fields) {
    const auto entries = [](std::span<const presence::EntryFields> items) {
        VectorField vector;
        for (const auto& entry : items) {
            vector.items.push_back(item({{entry.key}, described(entry.note, string_field), {entry.weight}}));
        }
        return Field{vector};
    };

    return {kRecordLayout,
            presence::Record::FORMAT_VERSION,
            {
                {fields.id},
                described(fields.name, string_field),
                described(fields.values,
                          [](std::span<const double> values) {
                              return Field{std::vector<double>(values.begin(), values.end())};
                          }),
                described(fields.origin,
                          [](const presence::PointFields& point) { return nested({{point.x}, {point.y}, {point.z}}); }),
                described(fields.flag, [](bool flag) { return Field{Bool{flag}}; }),
                described(fields.entries, entries),
                described(fields.total, [](std::int64_t total) { return Field{total}; }),
                string_field(fields.label),
                described(fields.extent, extent_field),
            }};
}

Table describe(const presence::RecordV1Fields& fields) {
    const auto entries = [](std::span<const presence::EntryV1Fields> items) {
        VectorField vector;
        for (const auto& entry : items) {
            vector.items.push_back(item({{entry.key}, described(entry.note, string_field)}));
        }
        return Field{vector};
    };

    return {kRecordLayout,
            presence::RecordV1::FORMAT_VERSION,
            {
                {fields.id},
                described(fields.name, string_field),
                described(fields.values,
                          [](std::span<const double> values) {
                              return Field{std::vector<double>(values.begin(), values.end())};
                          }),
                described(fields.origin,
                          [](const presence::PointV1Fields& point) { return nested({{point.x}, {point.y}}); }),
                described(fields.flag, [](bool flag) { return Field{Bool{flag}}; }),
                described(fields.entries, entries),
                described(fields.total, [](std::int64_t total) { return Field{total}; }),
            }};
}

/// The description of a table of the first version as the reader of the
/// second reads it: the fields that the second version added to the record,
/// and the second revision to its point and its entries, are missing
Table as_second_version(Table table) {
    auto& fields = table.fields;
    if (auto* origin = std::get_if<TableField>(&fields[3].value)) {
        origin->nested.table.fields.push_back(absent());
    }
    if (auto* entries = std::get_if<VectorField>(&fields[5].value)) {
        for (auto& entry : entries->items) {
            entry.table.fields.push_back(absent());
        }
    }

    fields.push_back(absent());
    fields.push_back(absent());
    return table;
}

// The descriptions of what the readers read, through their getters

/// A scalar that may be missing, as its getter and `has_<field>()` read it:
/// its value when the table holds it, and otherwise absent, when the getter
/// returns its default, as it must
Field scalar_read(Field value, bool held, const Field& default_value) {
    if (held) {
        return value;
    }
    if (!(value == default_value)) {
        throw std::logic_error("A scalar that the table doesn't hold read as other than its default");
    }
    return absent();
}

Field point_read(const presence::Point& point) {
    return nested({{point.x()}, {point.y()}, scalar_read({point.z()}, point.has_z(), {-0.5})});
}

Field entries_read(const presence::EntryVector& entries) {
    VectorField vector;
    for (const auto& entry : entries.to_vector()) {
        vector.items.push_back(item({{entry.key()},
                                     described(entry.note(), string_field),
                                     scalar_read({entry.weight()}, entry.has_weight(), {0.1F})}));
    }
    return {vector};
}

Table read_record(std::span<const std::uint8_t> bytes) {
    const presence::Record record(bytes);

    return {kRecordLayout,
            presence::Record::version_of(bytes),
            {
                {record.id()},
                described(record.name(), string_field),
                described(record.values(), [](const ArrayView<double>& values) { return Field{values.to_vector()}; }),
                described(record.origin(), point_read),
                scalar_read({Bool{record.flag()}}, record.has_flag(), {Bool{true}}),
                described(record.entries(), entries_read),
                scalar_read({record.total()}, record.has_total(), {std::int64_t{-1}}),
                described(record.label(), string_field),
                described(record.extent(), extent_field),
            }};
}

/// Writes a record's fields, as tables nested in place, and checks that
/// `TableBuilder` writes the same bytes from their description, absent fields
/// with `set_absent()`
RC_GTEST_PROP(GeneratedPresence, ARecordWithAbsentFieldsIsWrittenAsTableBuilderWritesItAndReadsBackAtAnyAddress, ()) {
    const auto record = *records();
    const auto byte_offset = *rc::gen::inRange<std::size_t>(0, 8);
    std::vector<presence::EntryFields> entries;
    const auto fields = fields_of(record, entries);

    const auto bytes = fields.serialize();
    RC_ASSERT(bytes == to_bytes(describe(fields)));
    RC_ASSERT(written_into_any_bytes(fields, bytes.span()));

    const AtByteOffset copy(bytes, byte_offset);
    RC_ASSERT(read_record(copy.bytes()) == describe(fields));
}

/// The generic fields struct writes an optional view of the caller's own
/// entries, which it converts as it writes them, as the plain one writes a
/// span of their fields
RC_GTEST_PROP(GeneratedPresence, AGenericRecordWritesAViewOfTheCallersEntriesAsTheirFields, ()) {
    const auto record = *records();
    std::vector<presence::EntryFields> entries;
    const auto fields = fields_of(record, entries);

    const std::vector<OwnedEntry> none;
    const auto view = (record.entries ? *record.entries : none) | std::views::transform(entry_fields_of);
    using View = std::remove_const_t<decltype(view)>;
    const presence::generic::RecordFields<View> generic{
        .id = fields.id,
        .name = fields.name,
        .values = fields.values,
        .origin = fields.origin,
        .flag = fields.flag,
        .entries = record.entries ? std::optional<View>(view) : std::nullopt,
        .total = fields.total,
        .label = fields.label,
        .extent = fields.extent,
    };

    const auto bytes = generic.serialize();
    RC_ASSERT(bytes == fields.serialize());
    RC_ASSERT(written_into_any_bytes(generic, bytes.span()));
}

TEST(GeneratedPresence, AbsentFieldsReadAsNulloptOrTheirDefaults) {
    const auto bytes = presence::RecordFields{.id = 7, .label = "a"}.serialize();
    const presence::Record record(bytes);

    EXPECT_EQ(record.id(), 7U);
    EXPECT_EQ(record.name(), std::nullopt);
    EXPECT_FALSE(record.values().has_value());
    EXPECT_FALSE(record.origin().has_value());
    EXPECT_TRUE(record.flag());
    EXPECT_FALSE(record.has_flag());
    EXPECT_FALSE(record.entries().has_value());
    EXPECT_EQ(record.total(), -1);
    EXPECT_FALSE(record.has_total());
    EXPECT_EQ(record.label(), "a");
    EXPECT_EQ(record.extent(), std::nullopt);
}

/// The reader of the second version reads the first, whose bytes lack the
/// fields that the second added, and whose points and entries lack those
/// that their second revisions appended
RC_GTEST_PROP(GeneratedPresence, ARecordOfTheFirstVersionReadsWithTheFieldsThatTheSecondAddedMissing, ()) {
    const auto record = *records();
    std::vector<presence::EntryV1Fields> entries;
    const auto first = first_version_of(record, entries);

    const auto bytes = first.serialize();
    RC_ASSERT(bytes == to_bytes(describe(first)));
    RC_ASSERT(read_record(bytes) == as_second_version(describe(first)));
}

RC_GTEST_PROP(GeneratedPresence, ARecordOfAVersionBeforeTheFirstOrAfterItsOwnIsRejected, ()) {
    const auto version = *rc::gen::oneOf(rc::gen::inRange<std::uint32_t>(0, 4), values::integers<std::uint32_t>());

    // The version follows the length in the header, which `at()` shows GCC,
    // whose warnings otherwise see a write into no bytes
    auto bytes = presence::RecordFields{.label = "a"}.serialize().to_vector();
    for (std::size_t byte = 0; byte < 4; ++byte) {
        bytes.at(4 + byte) = static_cast<std::uint8_t>(version >> (8 * byte));
    }

    RC_ASSERT(presence::Record::version_of(bytes) == version);
    if (version >= presence::Record::FIRST_VERSION && version <= presence::Record::FORMAT_VERSION) {
        static_cast<void>(presence::Record(bytes));
        return;
    }

    try {
        static_cast<void>(presence::Record(bytes));
        RC_FAIL("A record of a version that its reader doesn't read was read");
    } catch (const DecodeError& error) {
        RC_ASSERT(error.kind() == DecodeError::Kind::unsupported_version);
        RC_ASSERT(error.version() == version);
        RC_ASSERT(error.first_supported() == 1U);
        RC_ASSERT(error.supported() == 2U);
    }
}

TEST(GeneratedPresence, AReaderOfOneVersionKeepsRejectingEveryOther) {
    const auto bytes = presence::RecordFields{.id = 1, .label = ""}.serialize();

    EXPECT_EQ(presence::Record::FIRST_VERSION, 1U);
    EXPECT_EQ(presence::Record::FORMAT_VERSION, 2U);
    EXPECT_THROW(static_cast<void>(presence::RecordV1(bytes)), DecodeError);
}

/// A message without a version has revisions, whose reader finds the field
/// that the second appended missing from the first's bytes, from where its
/// first value starts; it's written whole, every field at a constant
/// position
RC_GTEST_PROP(GeneratedPresence, ASampleOfTheFirstRevisionReadsTheDefaultOfTheFieldThatTheSecondAppended, ()) {
    const auto key = *values::integers<std::uint32_t>();
    const auto count = *values::integers<std::uint16_t>();
    const presence::SampleV1Fields first_fields{.key = key};
    const presence::SampleFields second_fields{.key = key, .count = count};

    const auto first = first_fields.serialize();
    const auto second = second_fields.serialize();
    RC_ASSERT(first == to_bytes({Layout{}, std::nullopt, {{key}}}));
    RC_ASSERT(second == to_bytes({Layout{}, std::nullopt, {{key}, {count}}}));
    RC_ASSERT(written_into_any_bytes(second_fields, second.span()));

    RC_ASSERT(presence::Sample(first).key() == key);
    RC_ASSERT(presence::Sample(first).count() == 7U);
    RC_ASSERT(!presence::Sample(first).has_count());
    RC_ASSERT(presence::Sample(second).count() == count);
    RC_ASSERT(presence::Sample(second).has_count());

    // A reader of the first revision reads its fields of a later one
    RC_ASSERT(presence::SampleV1(second).key() == key);
}

TEST(GeneratedPresence, AnEmptyTableReadsAsDecodeErrorsAsItsGettersCheckWhetherItHoldsAField) {
    EXPECT_THROW(static_cast<void>(presence::Point().z()), DecodeError);
    EXPECT_THROW(static_cast<void>(presence::Point().has_z()), DecodeError);
    EXPECT_THROW(static_cast<void>(presence::Entry().note()), DecodeError);
    EXPECT_THROW(static_cast<void>(presence::Entry().has_weight()), DecodeError);
}

/// Corrupted records of either version read as values, `std::nullopt`,
/// defaults or decode errors, never another exception, and a scalar that
/// `has_<field>()` finds missing reads as its default
RC_GTEST_PROP(GeneratedPresence, CorruptedRecordsReadAsValuesOrDecodeErrors, ()) {
    const auto record = *records();
    const auto corruption = *values::corruptions();
    std::vector<presence::EntryFields> entries;
    std::vector<presence::EntryV1Fields> first_entries;

    auto copies = values::corrupted(fields_of(record, entries).serialize().to_vector(), corruption);
    const auto first = values::corrupted(first_version_of(record, first_entries).serialize().to_vector(), corruption);
    copies.insert(copies.end(), first.begin(), first.end());

    for (const auto& bytes : copies) {
        try {
            static_cast<void>(read_record(bytes));
        } catch (const DecodeError&) {
            // Corrupted bytes may fail to read
        }
    }
}

/// Bytes, mostly small, so that offsets often lie within them
rc::Gen<std::vector<std::uint8_t>> small_bytes() {
    return values::arrays(rc::gen::weightedOneOf<std::uint8_t>({
                              {4, rc::gen::just(std::uint8_t{0})},
                              {4, rc::gen::inRange<std::uint8_t>(1, 16)},
                              {1, rc::gen::arbitrary<std::uint8_t>()},
                          }),
                          48);
}

RC_GTEST_PROP(GeneratedPresence, ArbitraryBytesReadAsASampleOfValuesOrDecodeErrors, ()) {
    const auto bytes = *small_bytes();
    const presence::Sample sample(bytes);

    try {
        static_cast<void>(sample.key());
    } catch (const DecodeError&) {
        // Arbitrary bytes may fail to read
    }

    try {
        if (!sample.has_count()) {
            RC_ASSERT(sample.count() == 7U);
        }
        static_cast<void>(sample.count());
    } catch (const DecodeError&) {
        // Arbitrary bytes may fail to read
    }
}

}  // namespace
}  // namespace tafla::test
