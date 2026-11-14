// The prototype writes every table byte for byte as `tafla::TableBuilder`
// does, fails with the same errors from the same setters, and leaves itself
// as it was when a setter fails: on the conformance suite's tables, on
// generated ones in every offset type, and with segments of every length.

#include <gtest/gtest.h>
#include <rapidcheck/gtest.h>

#include <cstddef>
#include <cstdint>
#include <optional>
#include <stdexcept>
#include <string>
#include <vector>

#include "build_with.hpp"
#include "builder_suite.hpp"
#include "common/cases.hpp"
#include "common/strategy.hpp"
#include "common/write.hpp"
#include "direct_table_builder.hpp"

namespace tafla::prototype {
namespace {

using test::Field;
using test::Layout;
using test::Table;

/// Builders that keep every run of bytes as a segment, and every run of 8
/// or more, so that generated tables, whose values are short, exercise
/// segments everywhere a value can be
using EverySegment = BasicDirectTableBuilder<1>;
using EightByteSegments = BasicDirectTableBuilder<8>;

bool same(const BuildError::Details& left, const BuildError::Details& right) {
    return left.kind == right.kind && left.offset_type == right.offset_type && left.first == right.first &&
           left.second == right.second;
}

/// What a builder did with a table: the error of its constructor, or of each
/// setter, its length and alignment after each, and then its bytes or its
/// error
struct Outcome {
    std::optional<BuildError::Details> started;
    std::vector<std::optional<BuildError::Details>> set;
    std::vector<std::pair<std::size_t, Alignment>> after;
    std::optional<std::vector<std::uint8_t>> bytes;
    std::optional<BuildError::Details> finished;
};

bool same(const std::optional<BuildError::Details>& left, const std::optional<BuildError::Details>& right) {
    return left.has_value() == right.has_value() && (!left || same(*left, *right));
}

bool operator==(const Outcome& left, const Outcome& right) {
    if (!same(left.started, right.started) || left.set.size() != right.set.size() || left.after != right.after ||
        left.bytes != right.bytes || !same(left.finished, right.finished)) {
        return false;
    }
    for (std::size_t index = 0; index < left.set.size(); ++index) {
        if (!same(left.set[index], right.set[index])) {
            return false;
        }
    }
    return true;
}

/// Builds `table` in `layout`, with a header of `fields` fields, setting
/// each of its fields in turn, and carrying on past those that fail
template <class Builder>
Outcome outcome_of(const Table& table, const Layout& layout, std::size_t fields, std::optional<OffsetType> in_place) {
    Outcome outcome;
    std::optional<Builder> builder;
    try {
        builder.emplace(builder_with<Builder>(layout, fields, table.version));
    } catch (const BuildError& error) {
        outcome.started = error.details();
        return outcome;
    }

    for (const auto& field : table.fields) {
        try {
            set_field_with(field, *builder, in_place);
            outcome.set.emplace_back();
        } catch (const BuildError& error) {
            outcome.set.emplace_back(error.details());
        }
        outcome.after.emplace_back(builder->byte_length(), builder->alignment());
    }

    try {
        outcome.bytes = builder->to_bytes().to_vector();
    } catch (const BuildError& error) {
        outcome.finished = error.details();
    }
    return outcome;
}

template <class Builder>
std::vector<std::uint8_t> bytes_with(const Table& table, std::optional<OffsetType> in_place) {
    return build_with<Builder>(table, in_place).to_bytes().to_vector();
}

/// Builds each case with each builder, its nested tables from their builders
/// and, in a copy whose nested tables have the root's layout, in place
TEST(DirectTableBuilder, RebuildsEveryConformanceTableByteForByte) {
    const auto cases = test::valid_cases(test::read_json(test::conformance_directory() / "cases.json"));
    ASSERT_FALSE(cases.empty());

    for (const auto& test_case : cases) {
        const auto& table = test_case.table;
        EXPECT_EQ(bytes_with<DirectTableBuilder>(table, std::nullopt), test_case.bytes) << test_case.name;
        EXPECT_EQ(bytes_with<CopyingTableBuilder>(table, std::nullopt), test_case.bytes) << test_case.name;
        EXPECT_EQ(bytes_with<EverySegment>(table, std::nullopt), test_case.bytes) << test_case.name;

        const auto in_place = test::nested_in_place(table);
        const auto offset_type = in_place.layout.offset_type;
        const auto expected = bytes_with<TableBuilder>(in_place, offset_type);
        EXPECT_EQ(bytes_with<DirectTableBuilder>(in_place, offset_type), expected) << test_case.name;
        EXPECT_EQ(bytes_with<CopyingTableBuilder>(in_place, offset_type), expected) << test_case.name;
        EXPECT_EQ(bytes_with<EverySegment>(in_place, offset_type), expected) << test_case.name;
        EXPECT_EQ(bytes_with<EightByteSegments>(in_place, offset_type), expected) << test_case.name;
    }
}

rc::Gen<Table> tables_of_short_offsets() {
    return test::strategy::tables(test::strategy::layouts(rc::gen::element(OffsetType::uint8, OffsetType::uint16)));
}

/// Any table, its nested tables in place or not, built in any offset type,
/// whose limits its fields then often pass, with a field fewer, as many or
/// one more in its header, writes the same bytes through every builder, or
/// fails alike from the same setters, each leaving the builder as long and
/// aligned as `tafla::TableBuilder` leaves it
RC_GTEST_PROP(DirectTableBuilder, WritesAndFailsAsTableBuilderDoes, ()) {
    auto table = *rc::gen::oneOf(test::strategy::table(), tables_of_short_offsets());
    if (*rc::gen::arbitrary<bool>()) {
        table = test::nested_in_place(table);
    }

    auto layout = table.layout;
    layout.offset_type = *test::strategy::offset_type();
    const auto fields = table.fields.size() + *rc::gen::element<std::size_t>(0, 1, 2) - (table.fields.empty() ? 0 : 1);
    const auto in_place = *rc::gen::element<std::optional<OffsetType>>(std::nullopt, layout.offset_type);

    const auto expected = outcome_of<TableBuilder>(table, layout, fields, in_place);
    RC_ASSERT(outcome_of<DirectTableBuilder>(table, layout, fields, in_place) == expected);
    RC_ASSERT(outcome_of<CopyingTableBuilder>(table, layout, fields, in_place) == expected);
    RC_ASSERT(outcome_of<EverySegment>(table, layout, fields, in_place) == expected);
    RC_ASSERT(outcome_of<EightByteSegments>(table, layout, fields, in_place) == expected);
}

/// Runs of 256 bytes or more, borrowed or owned, kept as segments wherever a
/// table holds them: at the root, in a table written in place and moved by
/// its padding, in a nested builder of a sized header, and in a vector's
/// items from builders and in place
TEST(DirectTableBuilder, KeepsLongRunsAsSegmentsWhereverTheyAre) {
    const std::vector<double> doubles(1000, 1.5);
    const std::vector<std::uint8_t> bytes(300, 7);
    const std::string string(400, 's');

    const auto build = [&]<class Builder>(Builder builder) {
        builder.set_uint8(1);
        builder.set_float64_array(std::span<const double>(doubles));
        builder.set_string(std::string(string));
        builder.set_table(2, [&](auto& nested) {
            nested.set_uint8_array(std::span<const std::uint8_t>(bytes));
            nested.set_float64_array(std::vector<double>(doubles));
        });

        Builder sized(2, TableLayout().sized());
        sized.set_string(std::string_view(string));
        sized.set_uint16(3);
        builder.set_table(std::move(sized));

        Builder item(1, TableLayout());
        item.set_uint8_array(std::span<const std::uint8_t>(bytes));
        std::vector<typename NestedOf<Builder>::type> items;
        items.emplace_back(item);
        items.emplace_back(item.to_bytes());
        items.emplace_back(std::span<const std::uint8_t>(bytes));
        builder.set_vector(std::move(items));

        builder.set_vector(3, 1, [&](std::size_t index, auto& in_place) {
            in_place.set_string(index == 1 ? std::string_view(string) : std::string_view("short"));
        });
        return builder.to_bytes().to_vector();
    };

    const auto expected = build(TableBuilder(7, TableLayout().sized()));
    EXPECT_EQ(build(DirectTableBuilder(7, TableLayout().sized())), expected);
    EXPECT_EQ(build(CopyingTableBuilder(7, TableLayout().sized())), expected);
    EXPECT_EQ(build(EverySegment(7, TableLayout().sized())), expected);
}

/// A `fill` that throws partway through a nested table or a vector's item
/// leaves the builder as it was, so that the fields set after it write the
/// table that they would have written without it
TEST(DirectTableBuilder, AFillThatThrowsLeavesTheBuilderAsItWas) {
    const std::vector<std::uint16_t> values(200, 9);

    const auto build = [&]<class Builder>(Builder builder) {
        builder.set_uint8(1);
        const auto length = builder.byte_length();
        const auto alignment = builder.alignment();

        EXPECT_THROW(builder.set_table(3,
                                       [&](auto& nested) {
                                           nested.set_uint16_array(std::span<const std::uint16_t>(values));
                                           nested.set_float64(2.5);
                                           throw std::runtime_error("fill");
                                       }),
                     std::runtime_error);
        EXPECT_THROW(builder.set_vector(4, 1,
                                        [&](std::size_t index, auto& item) {
                                            item.set_uint16_array(std::span<const std::uint16_t>(values));
                                            if (index == 2) {
                                                throw std::runtime_error("fill");
                                            }
                                        }),
                     std::runtime_error);
        EXPECT_EQ(builder.byte_length(), length);
        EXPECT_EQ(builder.alignment(), alignment);

        builder.set_table(2, [&](auto& nested) {
            nested.set_uint16(4);
            nested.set_uint16_array(std::span<const std::uint16_t>(values));
        });
        return builder.to_bytes().to_vector();
    };

    const auto expected = build(TableBuilder(2, TableLayout()));
    EXPECT_EQ(build(DirectTableBuilder(2, TableLayout())), expected);
    EXPECT_EQ(build(CopyingTableBuilder(2, TableLayout())), expected);
    EXPECT_EQ(build(EverySegment(2, TableLayout())), expected);
}

/// A nested table or vector that makes its parent too long only once it's
/// written fails with the parent's error, after its own fields fit their
/// table, and leaves the parent as it was
TEST(DirectTableBuilder, ATableWrittenInPlaceThatMakesItsParentTooLongFailsAfterItsFields) {
    const std::vector<std::uint8_t> bytes(200, 1);

    const auto build = [&]<class Builder>(Builder builder) {
        std::vector<BuildError::Details> errors;
        builder.set_uint8_array(std::span<const std::uint8_t>(bytes));
        try {
            builder.set_table(1, [&](auto& nested) { nested.set_uint8_array(std::span<const std::uint8_t>(bytes)); });
        } catch (const BuildError& error) {
            errors.push_back(error.details());
        }
        try {
            builder.set_vector(2, 1, [&](std::size_t, auto& item) {
                item.set_uint8_array(std::span<const std::uint8_t>(bytes).first(30));
            });
        } catch (const BuildError& error) {
            errors.push_back(error.details());
        }
        builder.set_uint8(5);
        return std::pair(errors, builder.to_bytes().to_vector());
    };

    const auto layout = TableLayout().with_offset_type(OffsetType::uint8);
    const auto [errors, expected] = build(TableBuilder(2, layout));
    ASSERT_EQ(errors.size(), 2U);
    for (const auto& [prototype_errors, bytes_written] :
         {build(DirectTableBuilder(2, layout)), build(CopyingTableBuilder(2, layout)),
          build(EverySegment(2, layout))}) {
        ASSERT_EQ(prototype_errors.size(), errors.size());
        for (std::size_t index = 0; index < errors.size(); ++index) {
            EXPECT_TRUE(same(prototype_errors[index], errors[index]));
        }
        EXPECT_EQ(bytes_written, expected);
    }
}

/// The prototype's builders write the comparison's workloads as its tafla
/// codec writes them, which `benchmarks()` checks before it times them
TEST(DirectTableBuilder, WritesTheComparisonsWorkloadsAsTableBuilderDoes) {
    const Inputs inputs;
    const auto rows = benchmarks(inputs);
    EXPECT_EQ(rows.size(), 5U);

    const HandWritten<DirectTableBuilder> direct;
    const HandWritten<CopyingTableBuilder> copying;
    for (const auto& document : inputs.documents) {
        const auto expected = Tafla().serialize(benchmarks::DocumentShape{}, document);
        EXPECT_EQ(direct.document(document), expected);
        EXPECT_EQ(copying.document(document), expected);
    }
    EXPECT_EQ(direct.list(inputs.list), Tafla().serialize(benchmarks::ListShape{}, inputs.list));
    EXPECT_EQ(direct.structs(inputs.list), Tafla().serialize(benchmarks::StructsShape{}, inputs.list));
    EXPECT_EQ(direct.record(inputs.record), Tafla().serialize(benchmarks::RecordShape{}, inputs.record));
}

}  // namespace
}  // namespace tafla::prototype
