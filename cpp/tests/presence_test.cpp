// Absent fields, whose offsets are 0, and the fields that a reader of a later
// revision of a table finds missing, as `SPEC.md` at the root of the
// repository describes them. The tables are those of the conformance suite's
// model, as `strategy` generates them, with fields left out at every depth.
// `conformance_test` reads the suite's cases of presence.

#include <gtest/gtest.h>
#include <rapidcheck/gtest.h>

#include <algorithm>
#include <cstddef>
#include <cstdint>
#include <optional>
#include <span>
#include <sstream>
#include <string>
#include <tuple>
#include <utility>
#include <variant>
#include <vector>

#include "common/at_byte_offset.hpp"
#include "common/build.hpp"
#include "common/read.hpp"
#include "common/strategy.hpp"
#include "common/values.hpp"
#include "common/write.hpp"

namespace tafla::test {
namespace {

/// Leaves fields out of a table and the tables nested in it: each field in
/// turn, before the fields of a table that it holds, is absent when the next
/// of a cycle of absences says so
class Absences {
public:
    explicit Absences(std::vector<bool> absences) : absences_(std::move(absences)) {}

    Table within(Table table) {
        for (auto& field : table.fields) {
            field = left_out(std::move(field));
        }
        return table;
    }

private:
    Field left_out(Field field) {
        if (absences_[next_++ % absences_.size()]) {
            return {Absent{}};
        }

        if (auto* table = std::get_if<TableField>(&field.value)) {
            table->nested.table = within(std::move(table->nested.table));
        } else if (auto* vector = std::get_if<VectorField>(&field.value)) {
            for (auto& item : vector->items) {
                item.table = within(std::move(item.table));
            }
        }
        return field;
    }

    std::vector<bool> absences_;
    std::size_t next_ = 0;
};

/// Tables of the conformance suite's model with fields left out, which fit
/// their layouts, as the tables that they were made from do
rc::Gen<Table> presence_tables() {
    const auto absences = rc::gen::mapcat(rc::gen::inRange<std::size_t>(1, 25), [](std::size_t count) {
        return rc::gen::container<std::vector<bool>>(count, rc::gen::arbitrary<bool>());
    });

    return rc::gen::map(rc::gen::tuple(strategy::table(), absences), [](std::tuple<Table, std::vector<bool>> parts) {
        return Absences(std::move(std::get<1>(parts))).within(std::move(std::get<0>(parts)));
    });
}

bool is_absent(const Field& field) {
    return std::holds_alternative<Absent>(field.value);
}

/// The bytes that a write returns, or nothing for a table too long for its
/// offsets, as one with its nested tables made those written in place may be
template <class Write>
std::optional<std::vector<std::uint8_t>> written_by(Write write) {
    try {
        return write().to_vector();
    } catch (const BuildError&) {
        return std::nullopt;
    }
}

/// The offsets of the root's fields, which follow its header
std::vector<std::size_t> root_offsets(std::span<const std::uint8_t> bytes, const Table& table) {
    const auto layout = table.layout.unversioned();

    std::vector<std::size_t> offsets;
    for (std::size_t index = 0; index < table.fields.size(); ++index) {
        offsets.push_back(table.layout.versioned ? TableReader(bytes, layout.with_version(0)).root_offset(index)
                                                 : TableReader(bytes, layout).root_offset(index));
    }
    return offsets;
}

RC_GTEST_PROP(AbsentFields, ATableWithAbsentFieldsReadsBackItsValuesAtAnyAddress, ()) {
    const auto table = *presence_tables();
    const auto byte_offset = *rc::gen::inRange<std::size_t>(0, 8);

    const auto bytes = to_bytes(table);
    const AtByteOffset copy(bytes, byte_offset);

    RC_ASSERT(read_table(copy.bytes(), table) == table);
}

/// `read_table()` checks the offsets of the absent fields of nested tables
/// and vector items too
RC_GTEST_PROP(AbsentFields, AnAbsentFieldHasTheOffset0AndEveryOtherOneAfterTheOffsets, ()) {
    const auto table = *presence_tables();
    const auto bytes = to_bytes(table);
    const auto first_value = table.layout.header_size() + table.fields.size() * offset_size(table.layout.offset_type);

    const auto offsets = root_offsets(bytes, table);
    for (std::size_t index = 0; index < table.fields.size(); ++index) {
        if (is_absent(table.fields[index])) {
            RC_ASSERT(offsets[index] == 0U);
        } else {
            RC_ASSERT(offsets[index] >= first_value);
        }
    }
}

/// Bytes that the caller gives aren't zero, so the writer must write each
/// absent field's offset
RC_GTEST_PROP(AbsentFields, TableSizerAndTableWriterWriteWhatTableBuilderWritesIntoNewAndGivenBytes, ()) {
    const auto table = nested_in_place(*presence_tables());
    const auto built = written_by([&] { return to_bytes(table); });
    RC_PRE(built.has_value());

    RC_ASSERT(write(table).to_vector() == *built);
    RC_ASSERT(to_bytes_in_place(table).to_vector() == *built);

    std::vector<std::uint8_t> given(built->size() + 8, 0xff);
    const auto length = write_into(table, given);
    RC_ASSERT(length == built->size());
    RC_ASSERT(std::ranges::equal(std::span(given).first(length), *built));
}

TEST(AbsentFields, AreReadByComparingTheirOffsetsWith0) {
    TableBuilder builder(3, TableLayout());
    builder.set_uint8(1);
    builder.set_absent();
    builder.set_string("a");
    const auto bytes = builder.to_bytes();

    // The offsets, the uint8 and the string, with nothing for the absent field
    const std::vector<std::uint8_t> expected = {12, 0, 0, 0, 0, 0, 0, 0, 13, 0, 0, 0, 1, 1, 0, 0, 0, 'a'};
    EXPECT_EQ(bytes.to_vector(), expected);

    const TableReader reader(bytes, TableLayout());
    EXPECT_EQ(reader.get_uint8(reader.root_offset(0)), 1);
    EXPECT_EQ(reader.root_offset(1), 0U);
    EXPECT_EQ(reader.get_string(reader.root_offset(2)), "a");
}

TEST(AbsentFields, TableBuilderCountsAnAbsentFieldAmongItsFields) {
    TableBuilder builder(1, TableLayout());
    builder.set_absent();

    try {
        builder.set_absent();
        ADD_FAILURE() << "A field past the table's was set";
    } catch (const BuildError& error) {
        EXPECT_EQ(error.kind(), BuildError::Kind::too_many_fields);
        EXPECT_EQ(error.fields(), 1U);
    }
    EXPECT_EQ(builder.to_bytes().to_vector(), std::vector<std::uint8_t>(4, 0));
}

TEST(AbsentFields, TableWriterReportsAnAbsentFieldPastItsTable) {
    TableSizer sizer(1, TableLayout());
    sizer.add_uint8();

    auto writer = sizer.writer();
    writer.set_uint8(0, 1);
    writer.set_absent(1);

    try {
        static_cast<void>(writer.finish());
        ADD_FAILURE() << "A field past the table's was written";
    } catch (const BuildError& error) {
        EXPECT_EQ(error.kind(), BuildError::Kind::too_many_fields);
        EXPECT_EQ(error.fields(), 1U);
    }
}

/// Tables whose first field is present, as a table with later revisions needs
rc::Gen<Table> revised_tables() {
    return rc::gen::suchThat(presence_tables(),
                             [](const Table& table) { return !table.fields.empty() && !is_absent(table.fields[0]); });
}

RC_GTEST_PROP(LaterRevisions, FindEveryFieldAfterTheTablesOwnMissing, ()) {
    auto table = *revised_tables();
    table.missing = *rc::gen::inRange<std::size_t>(1, 5);

    RC_ASSERT(read_table(to_bytes(table), table) == table);
}

/// A reader of an earlier revision, of the table's first fields, finds the
/// fields after them missing only where they're absent
RC_GTEST_PROP(LaterRevisions, ReadTheFieldsOfAnEarlierRevisionAndFindTheOthersUnlessTheyreAbsent, ()) {
    const auto table = *revised_tables();
    const auto known = 1 + *rc::gen::inRange<std::size_t>(0, table.fields.size());

    auto earlier = table;
    earlier.fields.resize(known);
    earlier.missing = table.fields.size() - known;

    auto expected = earlier;
    expected.missing = static_cast<std::size_t>(
        std::count_if(table.fields.begin() + static_cast<std::ptrdiff_t>(known), table.fields.end(), is_absent));

    RC_ASSERT(read_table(to_bytes(table), earlier) == expected);
}

/// What reading a table returned: the table, or the message of the decode
/// error or of a mismatch of bytes that read as another shape
std::string outcome_of(std::span<const std::uint8_t> bytes, const Table& table) {
    try {
        std::ostringstream text;
        text << read_table(bytes, table);
        return text.str();
    } catch (const DecodeError& error) {
        return std::string("DecodeError: ") + error.what();
    } catch (const ReadMismatch& error) {
        return std::string("ReadMismatch: ") + error.what();
    }
}

/// Bytes read alike between bytes of 0x00 and of 0xff, so a read never leaves
/// them, and every error is a decode error or a mismatch, never another
RC_GTEST_PROP(LaterRevisions, CorruptedTablesWithAbsentAndMissingFieldsReadAsValuesOrDecodeErrors, ()) {
    auto table = *revised_tables();
    table.missing = *rc::gen::inRange<std::size_t>(0, 5);
    const auto corruption = *values::corruptions();

    for (const auto& corrupted : values::corrupted(to_bytes(table).to_vector(), corruption)) {
        std::vector<std::uint8_t> zeros(corrupted.size() + 16, 0x00);
        std::vector<std::uint8_t> ones(corrupted.size() + 16, 0xff);
        std::ranges::copy(corrupted, zeros.begin() + 8);
        std::ranges::copy(corrupted, ones.begin() + 8);

        const auto within_zeros = std::span(zeros).subspan(8, corrupted.size());
        const auto within_ones = std::span(ones).subspan(8, corrupted.size());
        RC_ASSERT(outcome_of(within_zeros, table) == outcome_of(within_ones, table));
    }
}

}  // namespace
}  // namespace tafla::test
