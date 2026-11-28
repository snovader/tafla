// Builders write any table that fits in its offset type, and reject what they
// can't write with a `BuildError` from the setter, leaving the builder as it
// was. The examples document the errors.

#include <gtest/gtest.h>
#include <rapidcheck/gtest.h>

#include <algorithm>
#include <array>
#include <cmath>
#include <concepts>
#include <cstddef>
#include <cstdint>
#include <functional>
#include <memory>
#include <optional>
#include <span>
#include <stdexcept>
#include <string>
#include <string_view>
#include <utility>
#include <vector>

#include "common/build.hpp"
#include "common/read.hpp"
#include "common/strategy.hpp"
#include "common/write.hpp"

namespace tafla::test {
namespace {

/// Throws the build error of `build`
template <class Build>
BuildError build_error_of(Build build) {
    try {
        build();
    } catch (const BuildError& error) {
        return error;
    }
    throw std::logic_error("The build didn't fail");
}

bool same_error(const BuildError& error, BuildError::Kind kind, OffsetType offset_type, std::uint64_t first,
                std::uint64_t second) {
    const auto& details = error.details();
    return details.kind == kind && details.offset_type == offset_type && details.first == first &&
           details.second == second;
}

/// Tables of `uint8` and `uint16` offsets, whose limits a test can reach
rc::Gen<Table> tables_of_short_offsets() {
    return strategy::tables(strategy::layouts(rc::gen::element(OffsetType::uint8, OffsetType::uint16)));
}

Table with_field(const Table& table, Field field) {
    auto result = table;
    result.fields.push_back(std::move(field));
    return result;
}

/// A table of `fields` with no header and `uint32` offsets, which fits in any
/// parent
Nested nested(Source from, std::vector<Field> fields) {
    return {from, {Layout::headerless(OffsetType::uint32), std::nullopt, std::move(fields)}};
}

/// A field whose byte length grows by a unit with each of its units
enum class LongField : std::uint8_t {
    bytes,
    floats,
    string,
    /// An array of structs of a uint16 and a uint8, of 3 bytes aligned to 2
    structs,
    /// A vector of empty tables, each an end of its own
    empty_items,
    /// A vector of a table of bytes, from its builder
    item,
    /// A table of bytes, nested from its bytes
    nested_bytes,
    /// A table of bytes, nested from its builder
    nested_builder,
    /// A vector of one string
    string_vector,
    /// A vector of empty strings, each an end of its own
    empty_strings,
};

rc::Gen<LongField> long_field() {
    return rc::gen::element(LongField::bytes, LongField::floats, LongField::string, LongField::structs,
                            LongField::empty_items, LongField::item, LongField::nested_bytes, LongField::nested_builder,
                            LongField::string_vector, LongField::empty_strings);
}

Field long_field_of(LongField kind, std::size_t units) {
    const auto bytes = [units] { return std::vector<Field>{{std::vector<std::uint8_t>(units, 0)}}; };

    switch (kind) {
        case LongField::bytes:
            return {std::vector<std::uint8_t>(units, 0)};
        case LongField::floats:
            return {std::vector<double>(units, 0.0)};
        case LongField::string:
            return {std::string(units, 'a')};
        case LongField::structs:
            return {StructArray{{ScalarType::uint16, ScalarType::uint8},
                                std::vector<std::vector<Field>>(units, {{std::uint16_t{1}}, {std::uint8_t{2}}})}};
        case LongField::empty_items:
            return {VectorField{std::vector<Nested>(units, nested(Source::bytes, {}))}};
        case LongField::item:
            return {VectorField{{nested(Source::builder, bytes())}}};
        case LongField::nested_bytes:
            return {TableField{nested(Source::bytes, bytes()), std::nullopt}};
        case LongField::nested_builder:
            break;
        case LongField::string_vector:
            return {StringVectorField{{std::string(units, 'a')}}};
        case LongField::empty_strings:
            return {StringVectorField{std::vector<std::string>(units)}};
    }
    return {TableField{nested(Source::builder, bytes()), std::nullopt}};
}

/// The bytes that each unit adds in a table of `width`-byte offsets
std::size_t unit_of(LongField kind, std::size_t width) {
    switch (kind) {
        case LongField::floats:
            return 8;
        case LongField::structs:
            return 3;
        case LongField::empty_items:
        case LongField::empty_strings:
            return width;
        default:
            return 1;
    }
}

/// After any fields, a field as long as fits is written and reads back, and a
/// unit longer is rejected with the length that it would make the table
RC_GTEST_PROP(TableBuilder, TheLongestFieldThatFitsIsWrittenAndAUnitMoreIsRejected, ()) {
    const auto table = *tables_of_short_offsets();
    const auto kind = *long_field();

    const auto offset_type = table.layout.offset_type;
    const std::size_t max = max_value(offset_type);
    const auto unit = unit_of(kind, offset_size(offset_type));

    // The fields can leave no room for even the empty field
    const auto empty = with_field(table, long_field_of(kind, 0));
    std::optional<Bytes> empty_bytes;
    try {
        empty_bytes = to_bytes(empty);
    } catch (const BuildError&) {
        RC_DISCARD("No room for the empty field");
    }

    const auto units = (max - empty_bytes->size()) / unit;
    const auto longest = with_field(table, long_field_of(kind, units));
    const auto bytes = to_bytes(longest);
    RC_ASSERT(max - unit < bytes.size());
    RC_ASSERT(bytes.size() <= max);
    RC_ASSERT(read_table(bytes, longest) == longest);

    const auto too_long = long_field_of(kind, units + 1);
    auto builder = builder_of(empty);
    for (const auto& field : table.fields) {
        set_field(field, builder);
    }

    const auto length = empty_bytes->size() + (units + 1) * unit;
    const auto error = build_error_of([&] { set_field(too_long, builder); });
    RC_ASSERT(same_error(error, BuildError::Kind::too_long, offset_type, table.fields.size(), length));

    set_field(empty.fields.back(), builder);
    RC_ASSERT(builder.to_bytes() == *empty_bytes);
}

/// A field longer than the offsets address on its own, set after any of the
/// fields, is rejected, and the builder writes the same table once the other
/// fields are set
RC_GTEST_PROP(TableBuilder, AFieldTooLongForTheOffsetTypeIsRejectedAndLeavesTheBuilderAsItWas, ()) {
    const auto table = *tables_of_short_offsets();
    const auto kind = *long_field();
    RC_PRE(!table.fields.empty());
    const auto at = *rc::gen::inRange<std::size_t>(0, table.fields.size());

    const auto offset_type = table.layout.offset_type;
    const std::size_t max = max_value(offset_type);
    const auto too_long = long_field_of(kind, max / unit_of(kind, offset_size(offset_type)) + 1);

    auto builder = builder_of(table);
    for (std::size_t index = 0; index < at; ++index) {
        set_field(table.fields[index], builder);
    }

    const auto error = build_error_of([&] { set_field(too_long, builder); });
    RC_ASSERT(error.kind() == BuildError::Kind::too_long);
    RC_ASSERT(error.index() == at);
    RC_ASSERT(error.length() > max);
    RC_ASSERT(error.offset_type() == offset_type);

    for (std::size_t index = at; index < table.fields.size(); ++index) {
        set_field(table.fields[index], builder);
    }
    RC_ASSERT(builder.to_bytes() == to_bytes(table));
}

/// The most bytes that `parent_of()` writes for a table of `length` bytes,
/// with `offset_type` offsets
std::size_t longest_parent(OffsetType offset_type, std::size_t before, std::size_t length) {
    const auto width = offset_size(offset_type);
    return 3 * width + (width + before) + (8 - 1 + length) + (3 * width + 2 * length);
}

/// A parent of `before` bytes in a `uint8[]`, the table at alignment 8, and a
/// vector of the table twice
template <class Nest>
Bytes parent_of(Nest nest, std::size_t before, TableLayout<false> layout) {
    TableBuilder parent(3, layout);
    parent.set_uint8_array(std::vector<std::uint8_t>(before, 0));
    parent.set_table(nest(), Alignment::eight);

    std::vector<NestedTable> items;
    items.push_back(nest());
    items.push_back(nest());
    parent.set_vector(std::move(items));

    return parent.to_bytes();
}

/// A table nested from its builder, borrowed or owned, writes the same parent
/// as from its bytes, whatever its layout. The table starts at the alignment
/// it's set with and holds its bytes, and the vector's items read back as its
/// bytes.
RC_GTEST_PROP(TableBuilder, ATableNestedFromItsBuilderWritesTheSameParentAsFromItsBytes, ()) {
    const auto table = *strategy::table();
    const auto before = *rc::gen::inRange<std::size_t>(0, 8);
    const auto bytes = to_bytes(table);

    // Of the table's offset type when the parent fits in it, so that a builder
    // of each offset type is nested in a parent of its own, and otherwise of
    // uint32 offsets
    auto offset_type = table.layout.offset_type;
    if (longest_parent(offset_type, before, bytes.size()) > max_value(offset_type)) {
        offset_type = OffsetType::uint32;
    }
    const auto layout = TableLayout().with_offset_type(offset_type);

    const auto borrowed = build(table);
    const auto from_borrowed = parent_of([&] { return NestedTable(borrowed); }, before, layout);
    const auto from_owned = parent_of([&] { return NestedTable(build(table)); }, before, layout);
    const auto from_bytes = parent_of([&] { return NestedTable(bytes.span()); }, before, layout);
    RC_ASSERT(from_borrowed == from_bytes);
    RC_ASSERT(from_owned == from_bytes);

    const TableReader reader(from_bytes, layout);
    const auto position = reader.root_offset(1);
    RC_ASSERT(position % 8 == 0U);
    RC_ASSERT(std::ranges::equal(from_bytes.span().subspan(position, bytes.size()), bytes.span()));

    for (auto item : reader.get_vector(reader.root_offset(2)).items()) {
        RC_ASSERT(std::ranges::equal(item, bytes.span()));
    }
}

/// What building a table returned: its bytes, or its error
struct Built {
    std::optional<std::vector<std::uint8_t>> bytes;
    std::optional<BuildError> error;
};

Built built_by(const std::function<Bytes()>& build) {
    try {
        return {build().to_vector(), std::nullopt};
    } catch (const BuildError& error) {
        return {std::nullopt, error};
    }
}

bool same_built(const Built& left, const Built& right) {
    if (left.bytes || right.bytes) {
        return left.bytes == right.bytes;
    }
    const auto& details = right.error->details();
    return same_error(*left.error, details.kind, details.offset_type, details.first, details.second);
}

/// Whether a table written in place failed for a vector whose count and
/// items' ends alone make its table too long, which fails before its items
/// are written: with that length, where its builders failed for the vector
/// with its items too, which is longer, or for an item of it first
bool fails_before_its_items(const Built& in_place, const Built& from_builders) {
    if (!in_place.error || !from_builders.error) {
        return false;
    }
    const auto& early = in_place.error->details();
    const auto& late = from_builders.error->details();
    if (early.kind != BuildError::Kind::too_long || late.kind != BuildError::Kind::too_long) {
        return false;
    }

    const bool same_field = early.first == late.first && early.offset_type == late.offset_type;
    return !same_field || early.second < late.second;
}

/// Tables nested from builders and written in place, with
/// `set_table(fields, fill)` and `set_vector(count, fields, fill)`, write the
/// bytes that their builders write, and fail as they fail, for tables of any
/// offset type and of short ones, whose limits the fields reach. A vector
/// whose count and items' ends alone make its table too long fails before its
/// items, where its builders fail for them or count them too.
RC_GTEST_PROP(TableBuilder, TablesWrittenInPlaceWriteWhatTheirBuildersWrite, ()) {
    const auto table = nested_in_place(*rc::gen::oneOf(strategy::table(), tables_of_short_offsets()));

    const auto in_place = built_by([&] { return to_bytes_in_place(table); });
    const auto from_builders = built_by([&] { return to_bytes(table); });
    RC_ASSERT(same_built(in_place, from_builders) || fails_before_its_items(in_place, from_builders));
}

/// The two tables of the vector of nested tables that the benchmarks write:
/// an item of a float32 and a pair of float32s
std::vector<std::uint8_t> items_of(std::span<const float> values, TableBuilder& builder) {
    builder.set_vector(values.size(), 2, [&](std::size_t index, TableBuilder::Item& item) {
        item.set_float32(values[index]);
        item.set_table(2, [&](TableBuilder::Item& pair) {
            pair.set_float32(values[index] + 1);
            pair.set_float32(values[index] + 2);
        });
    });
    return builder.to_bytes().to_vector();
}

std::vector<std::uint8_t> items_from_builders(std::span<const float> values) {
    std::vector<NestedTable> items;
    for (const auto value : values) {
        TableBuilder pair(2, TableLayout());
        pair.set_float32(value + 1);
        pair.set_float32(value + 2);

        TableBuilder item(2, TableLayout());
        item.set_float32(value);
        item.set_table(std::move(pair));
        items.emplace_back(std::move(item));
    }

    TableBuilder builder(1, TableLayout());
    builder.set_vector(std::move(items));
    return builder.to_bytes().to_vector();
}

/// Whatever throws while items are written leaves the builder as it was,
/// with none of their fields, so that the vector can be set again
TEST(TableBuilder, AnItemThatFailsLeavesTheBuilderAsItWas) {
    constexpr std::array<float, 3> values{1, 2, 3};
    const auto expected = items_from_builders(values);

    TableBuilder thrown(1, TableLayout());
    EXPECT_THROW(thrown.set_vector(3, 2,
                                   [](std::size_t index, TableBuilder::Item& item) {
                                       item.set_float32(1);
                                       if (index == 2) {
                                           throw std::runtime_error("The caller's");
                                       }
                                       item.set_float32(2);
                                   }),
                 std::runtime_error);
    EXPECT_EQ(items_of(values, thrown), expected);

    TableBuilder incomplete(1, TableLayout());
    const auto error = build_error_of([&] {
        incomplete.set_vector(3, 2, [](std::size_t index, TableBuilder::Item& item) {
            item.set_float32(1);
            if (index != 1) {
                item.set_float32(2);
            }
        });
    });
    EXPECT_TRUE(same_error(error, BuildError::Kind::incomplete, OffsetType::uint32, 1, 2));
    EXPECT_EQ(items_of(values, incomplete), expected);

    TableBuilder nested_incomplete(1, TableLayout());
    EXPECT_THROW(nested_incomplete.set_vector(3, 2,
                                              [](std::size_t, TableBuilder::Item& item) {
                                                  item.set_float32(1);
                                                  item.set_table(2,
                                                                 [](TableBuilder::Item& pair) { pair.set_float32(2); });
                                              }),
                 BuildError);
    EXPECT_EQ(items_of(values, nested_incomplete), expected);
}

/// A `fill` that throws partway through a nested table or a vector's item,
/// after values that the builder keeps where they are, leaves the builder as
/// it was, so that the fields set after it write the table that they would
/// have written without it
TEST(TableBuilder, AFillThatThrowsAfterLongValuesLeavesTheBuilderAsItWas) {
    const std::vector<std::uint16_t> values(200, 9);
    const auto finish = [&](TableBuilder& builder) {
        builder.set_table(2, [&](TableBuilder::Item& nested) {
            nested.set_uint16(4);
            nested.set_uint16_array(values);
        });
        return builder.to_bytes();
    };

    TableBuilder expected(2, TableLayout());
    expected.set_uint8(1);

    TableBuilder builder(2, TableLayout());
    builder.set_uint8(1);
    const auto length = builder.byte_length();
    const auto alignment = builder.alignment();

    EXPECT_THROW(builder.set_table(3,
                                   [&](TableBuilder::Item& nested) {
                                       nested.set_uint16_array(values);
                                       nested.set_float64(2.5);
                                       throw std::runtime_error("The caller's");
                                   }),
                 std::runtime_error);
    EXPECT_THROW(builder.set_vector(4, 1,
                                    [&](std::size_t index, TableBuilder::Item& item) {
                                        item.set_uint16_array(values);
                                        if (index == 2) {
                                            throw std::runtime_error("The caller's");
                                        }
                                    }),
                 std::runtime_error);
    EXPECT_EQ(builder.byte_length(), length);
    EXPECT_EQ(builder.alignment(), alignment);
    EXPECT_EQ(finish(builder), finish(expected));
}

/// A table or a vector written in place whose fields fit their own table,
/// but which makes its parent too long once it's written, is rejected with
/// the parent's error, and leaves the parent as it was
TEST(TableBuilder, RejectsATableWrittenInPlaceThatMakesItsParentTooLong) {
    const auto layout = TableLayout().with_offset_type(OffsetType::uint8);
    const std::vector<std::uint8_t> bytes(200, 1);

    TableBuilder expected(2, layout);
    expected.set_uint8_array(bytes);
    expected.set_uint8(5);

    // The parent's header and array are 203 bytes, the table 202, and each
    // item 32, after the vector's count and ends
    TableBuilder builder(2, layout);
    builder.set_uint8_array(bytes);
    const auto in_table = build_error_of(
        [&] { builder.set_table(1, [&](TableBuilder::Item& nested) { nested.set_uint8_array(bytes); }); });
    EXPECT_TRUE(same_error(in_table, BuildError::Kind::too_long, OffsetType::uint8, 1, 405));

    const auto in_vector = build_error_of([&] {
        builder.set_vector(2, 1, [&](std::size_t, TableBuilder::Item& item) {
            item.set_uint8_array(std::span<const std::uint8_t>(bytes).first(30));
        });
    });
    EXPECT_TRUE(same_error(in_vector, BuildError::Kind::too_long, OffsetType::uint8, 1, 270));

    builder.set_uint8(5);
    EXPECT_EQ(builder.to_bytes(), expected.to_bytes());
}

/// A vector written in place whose count and items' ends alone make its
/// table too long reports that length, since its items aren't written, where
/// a vector of their builders reports the length with its items
TEST(TableBuilder, AVectorTooLongBeforeItsItemsReportsItsLengthWithoutThem) {
    const auto layout = TableLayout().with_offset_type(OffsetType::uint8);
    const std::vector<std::uint8_t> bytes(249, 0);
    TableBuilder builder(2, layout);
    builder.set_uint8_array(bytes);
    TableBuilder item(1, layout);
    item.set_uint8(1);

    // 2 offsets, a count and 249 bytes, and a vector of a count and 3 ends,
    // and of 3 items of an offset and a byte
    const auto in_place = build_error_of(
        [&] { builder.set_vector(3, 1, [](std::size_t, TableBuilder::Item& written) { written.set_uint8(1); }); });
    EXPECT_TRUE(same_error(in_place, BuildError::Kind::too_long, OffsetType::uint8, 1, 256));

    const auto from_builders = build_error_of([&] {
        std::vector<NestedTable> items;
        for (int index = 0; index < 3; ++index) {
            items.emplace_back(item);
        }
        builder.set_vector(std::move(items));
    });
    EXPECT_TRUE(same_error(from_builders, BuildError::Kind::too_long, OffsetType::uint8, 1, 262));
}

TEST(TableBuilder, RejectsAnItemFieldBeyondItsCount) {
    TableBuilder builder(1, TableLayout());
    const auto error = build_error_of([&] {
        builder.set_table(1, [](TableBuilder::Item& item) {
            item.set_uint8(1);
            item.set_uint8(2);
        });
    });
    EXPECT_EQ(error.kind(), BuildError::Kind::too_many_fields);

    builder.set_table(1, [](TableBuilder::Item& item) { item.set_uint8(1); });
    EXPECT_EQ(builder.to_bytes().to_vector(), (std::vector<std::uint8_t>{4, 0, 0, 0, 4, 0, 0, 0, 1}));
}

/// Sets the fields of a table `depth` deep: its depth, and the table nested
/// in it, in a field or as a vector's one item, or 0 for the deepest
void nest_in_place(TableBuilder::Item& table, std::uint32_t depth, bool in_vector) {
    table.set_uint32(depth);
    if (depth == 0) {
        table.set_uint32(0);
    } else if (in_vector) {
        table.set_vector(1, 2, [&](std::size_t, TableBuilder::Item& item) { nest_in_place(item, depth - 1, true); });
    } else {
        table.set_table(2, [&](TableBuilder::Item& nested) { nest_in_place(nested, depth - 1, false); });
    }
}

/// The table that `nest_in_place()` writes, with each nested table written
/// into bytes of its own first
Bytes nest_as_bytes(std::uint32_t depth, bool in_vector) {
    TableBuilder table(2, TableLayout());
    table.set_uint32(depth);
    if (depth == 0) {
        table.set_uint32(0);
    } else if (in_vector) {
        std::vector<NestedTable> items;
        items.emplace_back(nest_as_bytes(depth - 1, true));
        table.set_vector(std::move(items));
    } else {
        table.set_table(nest_as_bytes(depth - 1, false), Alignment::one);
    }
    return table.to_bytes();
}

/// Tables nested in place, however deep, write what they write as bytes
TEST(TableBuilder, WritesTablesNestedInPlaceAtAnyDepth) {
    constexpr std::uint32_t depth = 40;
    for (const bool in_vector : {false, true}) {
        TableBuilder in_place(1, TableLayout());
        in_place.set_table(2, [&](TableBuilder::Item& table) { nest_in_place(table, depth, in_vector); });

        TableBuilder as_bytes(1, TableLayout());
        as_bytes.set_table(nest_as_bytes(depth, in_vector), Alignment::one);

        EXPECT_EQ(in_place.to_bytes(), as_bytes.to_bytes()) << "in a vector: " << in_vector;
    }
}

TEST(TableBuilder, RejectsAFieldBeyondItsCount) {
    TableBuilder builder(1, TableLayout());
    builder.set_uint8(1);
    const auto bytes = builder.to_bytes();

    const auto error = build_error_of([&] { builder.set_uint8(2); });
    EXPECT_TRUE(same_error(error, BuildError::Kind::too_many_fields, OffsetType::uint32, 1, 0));
    EXPECT_EQ(error.fields(), 1U);
    EXPECT_STREQ(error.what(), "Can't add field 2 to a table of 1");
    EXPECT_EQ(builder.to_bytes(), bytes);
}

TEST(TableBuilder, RejectsWritingBeforeEveryFieldIsSet) {
    for (std::size_t set = 0; set < 5; ++set) {
        TableBuilder builder(set + 1, TableLayout());
        for (std::size_t field = 0; field < set; ++field) {
            builder.set_uint8(1);
        }

        const auto error = build_error_of([&] { static_cast<void>(builder.to_bytes()); });
        EXPECT_EQ(error.kind(), BuildError::Kind::incomplete);
        EXPECT_EQ(error.set(), set);
        EXPECT_EQ(error.fields(), set + 1);
        EXPECT_EQ(std::string(error.what()),
                  "Only " + std::to_string(set) + " of the table's " + std::to_string(set + 1) + " fields were set");
    }
}

/// A sized table of six strings, which the builder is given to own
TableBuilder six_strings(std::string_view value) {
    TableBuilder builder(6, TableLayout().sized().with_offset_type(OffsetType::uint16));
    for (std::size_t field = 0; field < 6; ++field) {
        builder.set_string(std::string(value));
    }
    return builder;
}

/// A builder that's moved from is left as it was started, with its layout
/// and number of fields but no fields, whether it's moved into a new
/// builder or assigned to another, and its length is its header's
TEST(TableBuilder, AMovedFromBuilderIsLeftWithoutFields) {
    const auto expected = six_strings("moved").to_bytes();

    auto from = six_strings("moved");
    const TableBuilder constructed(std::move(from));
    auto assigned = six_strings("replaced");
    auto assigned_from = six_strings("moved");
    assigned = std::move(assigned_from);

    EXPECT_EQ(constructed.to_bytes(), expected);
    EXPECT_EQ(assigned.to_bytes(), expected);
    for (auto* moved : {&from, &assigned_from}) {
        // NOLINTNEXTLINE(bugprone-use-after-move): the state after it is the point
        const auto error = build_error_of([&] { static_cast<void>(moved->to_bytes()); });
        EXPECT_EQ(error.kind(), BuildError::Kind::incomplete);
        EXPECT_EQ(error.set(), 0U);
        EXPECT_EQ(error.fields(), 6U);
        EXPECT_EQ(moved->byte_length(), 7 * offset_size(OffsetType::uint16));

        for (std::size_t field = 0; field < 6; ++field) {
            moved->set_string("again");
        }
        EXPECT_EQ(moved->to_bytes(), six_strings("again").to_bytes());
    }
}

TEST(TableBuilder, RejectsAFieldThatMakesTheTableTooLong) {
    for (const auto offset_type : {OffsetType::uint8, OffsetType::uint16}) {
        const auto layout = TableLayout().with_offset_type(offset_type);
        const std::size_t max = max_value(offset_type);
        const auto width = offset_size(offset_type);

        // Two offsets, the array's length prefix and bytes, and then one byte
        const std::vector<std::uint8_t> fitting(max - 3 * width - 1, 0);
        TableBuilder fits(2, layout);
        fits.set_uint8_array(fitting);
        fits.set_uint8(7);

        const auto table = fits.to_bytes();
        const TableReader reader(table, layout);
        EXPECT_EQ(table.size(), max);
        EXPECT_EQ(reader.get_uint8(reader.root_offset(1)), 7);

        const std::vector<std::uint8_t> filling(max - 3 * width, 0);
        TableBuilder too_long(2, layout);
        too_long.set_uint8_array(filling);

        const auto error = build_error_of([&] { too_long.set_uint8(7); });
        EXPECT_TRUE(same_error(error, BuildError::Kind::too_long, offset_type, 1, max + 1));
        EXPECT_EQ(std::string(error.what()), "Field 2 makes the table " + std::to_string(max + 1) +
                                                 " bytes, too long for " + std::string(name(offset_type)) + " offsets");
    }
}

TEST(TableBuilder, RejectsBytesThatArentWholeStructs) {
    const std::vector<std::tuple<std::size_t, std::size_t, const char*>> cases{
        {5, 2, "5 bytes aren't a whole number of 2-byte structs"},
        {4, 0, "A struct can't be 0 bytes"},
        {0, 0, "A struct can't be 0 bytes"},
    };

    for (const auto& [length, size, message] : cases) {
        TableBuilder builder(1, TableLayout());
        const auto error = build_error_of(
            [&] { builder.set_struct_array(std::vector<std::uint8_t>(length, 0), size, Alignment::one); });
        EXPECT_TRUE(same_error(error, BuildError::Kind::not_whole_structs, OffsetType::uint32, length, size));
        EXPECT_STREQ(error.what(), message);

        // The builder was left as it was
        const std::vector<std::uint8_t> structs{1, 2, 3, 4};
        builder.set_struct_array(structs, 2, Alignment::two);
        EXPECT_EQ(builder.to_bytes().to_vector(), (std::vector<std::uint8_t>{4, 0, 0, 0, 2, 0, 0, 0, 1, 2, 3, 4}));
    }
}

TEST(TableBuilder, RejectsAHeaderTooLong) {
    for (const auto offset_type : {OffsetType::uint8, OffsetType::uint16}) {
        const std::size_t max = max_value(offset_type);
        const auto width = offset_size(offset_type);

        for (const bool sized : {false, true}) {
            auto layout = TableLayout().with_offset_type(offset_type);
            if (sized) {
                layout = layout.sized();
            }
            const auto fields = max / width - (sized ? 1 : 0);

            EXPECT_NO_THROW(TableBuilder(fields, layout));
            const auto error = build_error_of([&] { TableBuilder(fields + 1, layout); });
            EXPECT_TRUE(same_error(error, BuildError::Kind::header_too_long, offset_type, fields + 1, 0));
        }
    }

    EXPECT_EQ(build_error_of([] { TableBuilder(SIZE_MAX, TableLayout()); }).kind(), BuildError::Kind::header_too_long);
}

// A table of 4 GiB can't be allocated in a test, but a vector that nests a
// builder of 64 MiB 65 times, or of 65 views of the same 64 MiB string, is
// that long, and rejected before it's copied, also where a size_t has 32 bits
TEST(TableBuilder, RejectsATableOf32BitOffsetsPast4GiBBeforeWritingIt) {
    const std::string string(std::size_t{1} << 26, 's');
    TableBuilder nested(1, TableLayout());
    nested.set_string(string);
    std::vector<NestedTable> builders;
    for (std::size_t item = 0; item < 65; ++item) {
        builders.emplace_back(nested);
    }
    const std::vector<std::string_view> views(65, string);

    TableBuilder builder(1, TableLayout());
    for (const auto& error : {build_error_of([&] { builder.set_vector(builders); }),
                              build_error_of([&] { builder.set_string_vector(views); })}) {
        EXPECT_EQ(error.kind(), BuildError::Kind::too_long);
        EXPECT_EQ(error.index(), 0U);
        EXPECT_GT(error.length(), std::uint64_t{1} << 32);
    }
    EXPECT_EQ(builder.byte_length(), 4U);
}

TEST(TableBuilder, RejectsAVersionTooLargeForItsSlot) {
    for (const auto& [offset_type, version] :
         {std::pair{OffsetType::uint8, 0x100U}, std::pair{OffsetType::uint16, 0x1'0000U}}) {
        const auto layout = TableLayout().with_offset_type(offset_type);

        const auto error = build_error_of([&] { TableBuilder(1, layout.with_version(version)); });
        EXPECT_TRUE(same_error(error, BuildError::Kind::version_too_large, offset_type, version, 0));
        EXPECT_EQ(error.version(), version);
        EXPECT_EQ(std::string(error.what()), "Version " + std::to_string(version) + " doesn't fit in a " +
                                                 std::string(name(offset_type)) + " slot");
        EXPECT_NO_THROW(TableBuilder(1, layout.with_version(version - 1)));
    }
}

TEST(TableBuilder, RejectsAnIncompleteNestedBuilder) {
    TableBuilder incomplete(2, TableLayout());
    incomplete.set_uint8(1);

    TableBuilder parent(2, TableLayout());
    const auto expect_incomplete = [](const BuildError& error) {
        EXPECT_EQ(error.kind(), BuildError::Kind::incomplete);
        EXPECT_EQ(error.set(), 1U);
        EXPECT_EQ(error.fields(), 2U);
    };
    expect_incomplete(build_error_of([&] { parent.set_table(incomplete); }));

    const std::vector<std::uint8_t> bytes{0};
    std::vector<NestedTable> items;
    items.emplace_back(bytes);
    items.emplace_back(incomplete);
    expect_incomplete(build_error_of([&] { parent.set_vector(std::move(items)); }));

    // The parent was left as it was
    TableBuilder complete(2, TableLayout());
    complete.set_uint8(1);
    complete.set_uint8(2);
    parent.set_table(complete);
    std::vector<NestedTable> complete_items;
    complete_items.emplace_back(complete);
    parent.set_vector(std::move(complete_items));

    const auto table = parent.to_bytes();
    const TableReader reader(table, TableLayout());
    const auto position = reader.root_offset(0);
    EXPECT_EQ(position, 8U);
    EXPECT_EQ(reader.get_uint8(reader.field_offset(position, 1)), 2);

    const auto vector = reader.get_vector(reader.root_offset(1));
    ASSERT_EQ(vector.size(), 1U);
    EXPECT_TRUE(std::ranges::equal(vector.get(0), complete.to_bytes().span()));
}

/// Each setter copies its value, so a value may change or be destroyed once
/// its setter returns, whether it was a view, a container or a temporary. An
/// rvalue is left as it was.
TEST(TableBuilder, CopiesEachValueWhenItsSet) {
    TableBuilder builder(6, TableLayout());
    std::vector<std::uint16_t> values{1, 2, 3};
    std::string string(300, 's');
    std::vector<std::string> strings{"a", std::string(100, 'b')};
    {
        const std::vector<std::uint8_t> destroyed{9, 8, 7};
        builder.set_uint16_array(values);
        builder.set_string(std::move(string));
        builder.set_string_vector(strings);
        builder.set_struct(destroyed);
        builder.set_float64_array(std::vector<double>{1.5, -0.0});
        builder.set_bool_array(std::array<bool, 2>{true, false});
    }
    values[0] = 100;
    strings[1] = "changed";
    EXPECT_EQ(string, std::string(300, 's'));  // NOLINT(bugprone-use-after-move): left as it was is the point

    const auto bytes = builder.to_bytes();
    const TableReader reader(bytes, TableLayout());
    EXPECT_EQ(reader.get_uint16_array(reader.root_offset(0)).to_vector(), (std::vector<std::uint16_t>{1, 2, 3}));
    EXPECT_EQ(reader.get_string(reader.root_offset(1)), std::string(300, 's'));
    EXPECT_EQ(reader.get_string_vector(reader.root_offset(2)).to_vector(),
              (std::vector<std::string_view>{"a", std::string(100, 'b')}));
    EXPECT_TRUE(std::ranges::equal(reader.get_struct(reader.root_offset(3), 3), std::vector<std::uint8_t>{9, 8, 7}));
    const auto doubles = reader.get_float64_array(reader.root_offset(4)).to_vector();
    ASSERT_EQ(doubles.size(), 2U);
    EXPECT_TRUE(std::signbit(doubles[1]));
    EXPECT_EQ(reader.get_bool_array(reader.root_offset(5)), (Bools{true, false}));
}

TEST(TableBuilder, LeavesAnRvalueItRejectsWithTheCaller) {
    TableBuilder builder(1, TableLayout().with_offset_type(OffsetType::uint8));
    std::vector<std::uint8_t> values(300, 1);
    Bools bools(300);

    EXPECT_THROW(builder.set_uint8_array(std::move(values)), BuildError);
    EXPECT_EQ(values.size(), 300U);  // NOLINT(bugprone-use-after-move): rejected, so not moved
    EXPECT_THROW(builder.set_bool_array(std::move(bools)), BuildError);
    EXPECT_EQ(bools.size(), 300U);  // NOLINT(bugprone-use-after-move): rejected, so not moved

    std::vector<std::string> strings(300);
    EXPECT_THROW(builder.set_string_vector(std::move(strings)), BuildError);
    EXPECT_EQ(strings.size(), 300U);  // NOLINT(bugprone-use-after-move): rejected, so not moved
}

/// A byte of 1 or 0 for each bool, as a `uint8` array of those bytes, from a
/// span or `Bools`, and in a table written in place
TEST(TableBuilder, WritesArraysOfBoolsAsBytesOf1And0) {
    const std::array<bool, 3> borrowed{true, false, true};
    const std::vector<std::uint8_t> bytes{1, 0, 1};

    TableBuilder expected(3, TableLayout());
    expected.set_uint8_array(bytes);
    expected.set_uint8_array(bytes);
    expected.set_table(1, [&](TableBuilder::Item& item) { item.set_uint8_array(bytes); });

    TableBuilder builder(3, TableLayout());
    builder.set_bool_array(borrowed);
    builder.set_bool_array(Bools(borrowed));
    builder.set_table(1, [&](TableBuilder::Item& item) { item.set_bool_array(Bools{true, false, true}); });
    EXPECT_EQ(builder.to_bytes(), expected.to_bytes());
}

/// Structs of 64 bytes or more, from a view or a temporary, read back as they
/// were set
TEST(TableBuilder, WritesLongStructs) {
    std::vector<std::uint8_t> borrowed(300);
    for (std::size_t index = 0; index < borrowed.size(); ++index) {
        borrowed[index] = static_cast<std::uint8_t>(index);
    }
    const std::vector<std::uint8_t> reversed(borrowed.rbegin(), borrowed.rend());

    TableBuilder builder(3, TableLayout());
    builder.set_struct(borrowed);
    builder.set_struct(std::vector<std::uint8_t>(reversed));
    builder.set_uint8(7);

    const auto bytes = builder.to_bytes();
    const TableReader buffer(bytes, TableLayout());
    EXPECT_TRUE(std::ranges::equal(buffer.get_struct(buffer.root_offset(0), 300), borrowed));
    EXPECT_TRUE(std::ranges::equal(buffer.get_struct(buffer.root_offset(1), 300), reversed));
    EXPECT_EQ(buffer.get_uint8(buffer.root_offset(2)), 7);
}

TEST(TableBuilder, WritesAnEmptyArrayOfStructsOfAnySize) {
    TableBuilder expected(1, TableLayout());
    expected.set_struct_array(std::span<const std::uint8_t>(), 4, Alignment::four);

    // Past 32 bits where a size_t has them
    for (const std::uint64_t size : {std::uint64_t{UINT32_MAX} + 1, std::uint64_t{SIZE_MAX}}) {
        if (size > SIZE_MAX) {
            continue;
        }

        TableBuilder builder(1, TableLayout());
        builder.set_struct_array(std::span<const std::uint8_t>(), static_cast<std::size_t>(size), Alignment::four);
        EXPECT_EQ(builder.to_bytes(), expected.to_bytes());
    }
}

/// A table of more fields than the strategy's, whose header is longer than
/// a small table's bytes, and whose fields are written after it
TEST(TableBuilder, WritesATableOfManyFields) {
    constexpr std::size_t fields = 20'000;
    TableBuilder builder(fields, TableLayout().sized());
    for (std::size_t field = 0; field < fields; ++field) {
        builder.set_uint16(static_cast<std::uint16_t>(field));
    }

    const auto bytes = builder.to_bytes();
    const TableReader buffer(bytes, TableLayout().sized());
    EXPECT_EQ(bytes.size(), 4 + fields * 6);
    for (std::size_t field = 0; field < fields; ++field) {
        EXPECT_EQ(buffer.get_uint16(buffer.root_offset(field)), field);
    }
}

/// A builder that was moved from, after any of its fields were set, writes
/// the table that a new builder writes once its fields are set again, from
/// the first, whether or not the move took them
RC_GTEST_PROP(TableBuilder, ABuilderMovedFromWritesWhatANewOneWrites, ()) {
    const auto table = *strategy::table();
    const auto set = *rc::gen::inRange<std::size_t>(0, table.fields.size() + 1);

    auto from = builder_of(table);
    for (std::size_t index = 0; index < set; ++index) {
        set_field(table.fields[index], from);
    }
    const TableBuilder moved(std::move(from));

    for (const auto& field : table.fields) {
        set_field(field, from);  // NOLINT(bugprone-use-after-move): the state after it is the point
    }
    RC_ASSERT(from.to_bytes() == to_bytes(table));
}

// A setter copies its value, so it takes a view, a container or a temporary
// of the field's elements

template <class Value>
concept SetsInt32Array =
    requires(TableBuilder& builder, Value&& value) { builder.set_int32_array(std::forward<Value>(value)); };

template <class Value>
concept SetsBoolArray =
    requires(TableBuilder& builder, Value&& value) { builder.set_bool_array(std::forward<Value>(value)); };

template <class Value>
concept SetsString = requires(TableBuilder& builder, Value&& value) { builder.set_string(std::forward<Value>(value)); };

template <class Value>
concept SetsStruct = requires(TableBuilder& builder, Value&& value) { builder.set_struct(std::forward<Value>(value)); };

template <class Value>
concept SetsStructArray = requires(TableBuilder& builder, Value&& value) {
    builder.set_struct_array(std::forward<Value>(value), 1, Alignment::one);
};

/// Whether a `NestedTable`, which `set_table()` and `set_vector()` take,
/// can be made of a `Value`
template <class Value>
concept Nests = std::constructible_from<NestedTable, Value>;

using Int32s = std::array<std::int32_t, 3>;
using Bytes4 = std::array<std::uint8_t, 4>;

static_assert(SetsInt32Array<Int32s&>);
static_assert(SetsInt32Array<const Int32s&>);
static_assert(SetsInt32Array<std::vector<std::int32_t>&>);
static_assert(SetsInt32Array<std::vector<std::int32_t>>);
static_assert(SetsInt32Array<std::span<const std::int32_t>>);
static_assert(SetsInt32Array<Int32s>);
static_assert(SetsInt32Array<const Int32s>);
static_assert(SetsInt32Array<const std::vector<std::int32_t>>);
static_assert(!SetsInt32Array<std::vector<std::uint32_t>&>);

// Bools that `TableReader::get_bool_array()` reads are set as another array's
// vector is. A `std::vector<bool>` packs its bools into bits, which no span of
// bools views.
static_assert(SetsBoolArray<std::array<bool, 3>&>);
static_assert(SetsBoolArray<Bools&>);
static_assert(SetsBoolArray<const Bools&>);
static_assert(SetsBoolArray<Bools>);
static_assert(SetsBoolArray<std::span<const bool>>);
static_assert(SetsBoolArray<std::array<bool, 3>>);
static_assert(SetsBoolArray<const Bools>);
static_assert(!SetsBoolArray<std::vector<bool>&>);

static_assert(SetsString<std::string&>);
static_assert(SetsString<std::string>);
static_assert(SetsString<std::string_view>);
static_assert(SetsString<const char*>);
static_assert(SetsString<const char (&)[4]>);
static_assert(SetsString<const std::string>);

static_assert(SetsStruct<Bytes4&>);
static_assert(SetsStruct<std::vector<std::uint8_t>>);
static_assert(SetsStruct<std::span<const std::uint8_t>>);
static_assert(SetsStruct<Bytes4>);
static_assert(SetsStruct<const std::vector<std::uint8_t>>);

static_assert(SetsStructArray<Bytes4&>);
static_assert(SetsStructArray<std::vector<std::uint8_t>>);
static_assert(SetsStructArray<Bytes4>);

// A `NestedTable` may outlive the call that makes it, in a vector of items,
// so it owns an rvalue and rejects a temporary that it can't own
static_assert(Nests<TableBuilder&>);
static_assert(Nests<TableBuilder>);
static_assert(Nests<Bytes&>);
static_assert(Nests<Bytes>);
static_assert(Nests<std::vector<std::uint8_t>&>);
static_assert(Nests<std::vector<std::uint8_t>>);
static_assert(Nests<Bytes4&>);
static_assert(!Nests<const TableBuilder>);
static_assert(!Nests<const Bytes>);
static_assert(!Nests<const std::vector<std::uint8_t>>);
static_assert(!Nests<Bytes4>);

/// A builder of a table without a header, of a string, an array from a
/// temporary and `borrowed`
TableBuilder owning(std::span<const std::uint16_t> borrowed) {
    TableBuilder builder(3, TableLayout());
    builder.set_string(std::string(40, 's'));
    builder.set_float64_array(std::vector<double>{1.5, -0.0});
    builder.set_uint16_array(borrowed);
    return builder;
}

/// A parent of a builder nested in place and in a vector, from `nest()`
template <class Nest>
Bytes parent_of(Nest nest) {
    TableBuilder parent(2, TableLayout());
    parent.set_table(nest());
    std::vector<NestedTable> items;
    items.emplace_back(nest());
    items.emplace_back(nest());
    parent.set_vector(std::move(items));
    return parent.to_bytes();
}

/// A builder nested from a `const` reference is copied into its parent, so
/// it may be destroyed or reused before the parent is written, whatever its
/// layout
TEST(TableBuilder, ABuilderNestedByReferenceMayBeDestroyedBeforeItsParentIsWritten) {
    const std::vector<std::uint16_t> borrowed{7, 8, 9};
    const auto expected = parent_of([&] { return owning(borrowed); });

    TableBuilder destroyed(2, TableLayout());
    TableBuilder reused(2, TableLayout());
    {
        auto nested = std::make_unique<TableBuilder>(owning(borrowed));
        destroyed.set_table(*nested);
        reused.set_table(*nested);

        std::vector<NestedTable> items;
        items.emplace_back(*nested);
        items.emplace_back(*nested);
        destroyed.set_vector(std::move(items));
        nested.reset();
    }

    auto nested = owning(borrowed);
    std::vector<NestedTable> items;
    items.emplace_back(nested);
    items.emplace_back(nested);
    reused.set_vector(std::move(items));
    nested = TableBuilder(3, TableLayout());

    EXPECT_EQ(destroyed.to_bytes(), expected);
    EXPECT_EQ(reused.to_bytes(), expected);
}

/// So may a builder with a header, whose own bytes are nested
TEST(TableBuilder, ABuilderWithAHeaderNestedByReferenceMayBeDestroyedBeforeItsParentIsWritten) {
    const auto sized = [] {
        TableBuilder builder(1, TableLayout().sized());
        builder.set_string(std::string(40, 's'));
        return builder;
    };
    const auto expected = parent_of(sized);

    TableBuilder parent(2, TableLayout());
    {
        const auto nested = sized();
        parent.set_table(nested);
        std::vector<NestedTable> items;
        items.emplace_back(nested);
        items.emplace_back(nested);
        parent.set_vector(std::move(items));
    }
    EXPECT_EQ(parent.to_bytes(), expected);
}

/// Sets the table's fields on `builder`, which started it
void set_fields(const Table& table, TableBuilder& builder) {
    for (const auto& field : table.fields) {
        set_field(field, builder);
    }
}

/// Builds the table with a builder from `start(fields, layout)`, and returns
/// a copy of the table that `finish()` lends
template <class Start>
Bytes finished(const Table& table, Start start) {
    auto builder = start_table(table, start);
    set_fields(table, builder);
    return Bytes::copy_of(builder.finish());
}

/// A table is written alike by `to_bytes()`, `finish()` and `into_buffer()`,
/// from a new builder, from one that took a vector of other bytes, after a
/// reset following another table, finished or not, and into the caller's
/// bytes, of the table's length, also with its tables nested in place, which
/// fail one byte shorter. They fail alike too.
RC_GTEST_PROP(TableBuilder, EveryWayOfTakingATableWritesTheSameTable, ()) {
    const auto table = *strategy::table();
    const auto expected = built_by([&] { return to_bytes(table); });

    const auto finish = built_by(
        [&] { return finished(table, [](std::size_t fields, auto layout) { return TableBuilder(fields, layout); }); });
    RC_ASSERT(same_built(finish, expected));

    const auto into_buffer = built_by([&] {
        auto builder = builder_of(table);
        set_fields(table, builder);
        return Bytes::copy_of(std::move(builder).into_buffer());
    });
    RC_ASSERT(same_built(into_buffer, expected));

    const auto filled = *rc::gen::container<std::vector<std::uint8_t>>(rc::gen::arbitrary<std::uint8_t>());
    const auto given = built_by([&] {
        return finished(
            table, [&](std::size_t fields, auto layout) { return TableBuilder::with_buffer(filled, fields, layout); });
    });
    RC_ASSERT(same_built(given, expected));

    const auto other = *strategy::table();
    const auto set = *rc::gen::inRange<std::size_t>(0, other.fields.size() + 1);
    const auto reset = built_by([&] {
        auto builder = builder_of(other);
        try {
            for (std::size_t index = 0; index < set; ++index) {
                set_field(other.fields[index], builder);
            }
            if (set == other.fields.size()) {
                static_cast<void>(builder.finish());
            }
        } catch (const BuildError&) {
            // Another table that fails leaves the builder to reset all the same
        }
        start_table(table, [&](std::size_t fields, auto layout) { builder.reset(fields, layout); });
        set_fields(table, builder);
        return Bytes::copy_of(builder.finish());
    });
    RC_ASSERT(same_built(reset, expected));

    if (expected.bytes) {
        auto bytes = *expected.bytes;
        const auto into = [](std::span<std::uint8_t> to) {
            return [to](std::size_t fields, auto layout) { return TableBuilder::with_bytes(to, fields, layout); };
        };
        RC_ASSERT(finished(table, into(bytes)).to_vector() == *expected.bytes);

        // With the tables nested in place that can be, which fit too
        auto in_place = start_table(table, into(bytes));
        for (const auto& field : table.fields) {
            set_field_on(field, in_place, table.layout.offset_type);
        }
        RC_ASSERT(std::ranges::equal(in_place.finish(), *expected.bytes));

        if (!bytes.empty()) {
            const auto shorter = std::span(bytes).first(bytes.size() - 1);
            RC_ASSERT_THROWS_AS(finished(table, into(shorter)), std::invalid_argument);
        }
    }
}

/// `finish()` writes a sized table's length and lends the table where the
/// builder wrote it, and rejects an incomplete table, as `into_buffer()` does,
/// which leaves the builder as it was
TEST(TableBuilder, FinishesOnlyACompleteTable) {
    const auto layout = TableLayout().sized().with_version(3);
    TableBuilder builder(2, layout);
    builder.set_uint8(1);

    const auto expect_incomplete = [](const BuildError& error) {
        EXPECT_TRUE(same_error(error, BuildError::Kind::incomplete, OffsetType::uint32, 1, 2));
    };
    expect_incomplete(build_error_of([&] { static_cast<void>(builder.finish()); }));
    expect_incomplete(build_error_of([&] { static_cast<void>(std::move(builder).into_buffer()); }));

    builder.set_string("two");  // NOLINT(bugprone-use-after-move): rejected, so not moved
    const auto expected = builder.to_bytes();
    const auto table = builder.finish();
    EXPECT_TRUE(std::ranges::equal(table, expected));
    EXPECT_EQ(TableReader(table, layout).get_uint8(TableReader(table, layout).root_offset(0)), 1);
    EXPECT_EQ(std::move(builder).into_buffer(), expected.to_vector());
}

/// A builder writes into the caller's vector, and hands it back as long as
/// the table, with its capacity, leaving the builder as a moved-from one
TEST(TableBuilder, HandsBackTheCallersVector) {
    TableBuilder expected(2, TableLayout().sized());
    expected.set_uint32(7);
    expected.set_string("seven");

    std::vector<std::uint8_t> buffer;
    buffer.reserve(4096);
    const auto* const data = buffer.data();
    const auto capacity = buffer.capacity();

    auto builder = TableBuilder::with_buffer(std::move(buffer), 2, TableLayout().sized());
    builder.set_uint32(7);
    builder.set_string("seven");
    const auto table = std::move(builder).into_buffer();
    EXPECT_EQ(table, expected.to_bytes().to_vector());
    EXPECT_EQ(table.data(), data);
    EXPECT_EQ(table.capacity(), capacity);

    // NOLINTBEGIN(bugprone-use-after-move): the state after it is the point
    EXPECT_EQ(builder.byte_length(), 12U);
    builder.set_uint32(7);
    builder.set_string("seven");
    EXPECT_EQ(builder.to_bytes(), expected.to_bytes());
    // NOLINTEND(bugprone-use-after-move)
}

/// A builder writes into the caller's bytes, which it never grows: a field
/// that they can't hold is rejected with `std::invalid_argument`, leaving the
/// builder as it was, and the table is lent where it lies in them
TEST(TableBuilder, WritesIntoTheCallersBytes) {
    const auto layout = TableLayout().sized().with_offset_type(OffsetType::uint8);
    TableBuilder expected(3, layout);
    expected.set_uint32(1);
    expected.set_uint16(2);
    expected.set_uint8(3);

    std::array<std::uint8_t, 11> bytes{};
    auto builder = TableBuilder::with_bytes(bytes, 3, layout);
    builder.set_uint32(1);
    EXPECT_THROW(builder.set_string("too long"), std::invalid_argument);
    EXPECT_THROW(builder.set_table(2,
                                   [](TableBuilder::Item& item) {
                                       item.set_uint8(1);
                                       item.set_uint8(2);
                                   }),
                 std::invalid_argument);
    EXPECT_EQ(builder.byte_length(), 8U);
    builder.set_uint16(2);
    builder.set_uint8(3);

    const auto table = builder.finish();
    EXPECT_EQ(table.data(), bytes.data());
    EXPECT_TRUE(std::ranges::equal(table, expected.to_bytes()));
    EXPECT_EQ(std::move(builder).into_buffer(), expected.to_bytes().to_vector());

    EXPECT_THROW(static_cast<void>(TableBuilder::with_bytes(std::span(bytes).first(4), 4, layout)),
                 std::invalid_argument);
}

/// A vector in place whose first item is longer than the others fits in the
/// caller's bytes of the table's length: the room that a builder makes for
/// the other items, as long as the first, is only a hint in them
TEST(TableBuilder, WritesAnUnevenVectorInPlaceIntoTheCallersBytes) {
    const auto fill = [](std::size_t index, TableBuilder::Item& item) {
        item.set_string(index == 0 ? std::string(40, 'f') : std::string());
    };
    TableBuilder expected(1, TableLayout());
    expected.set_vector(3, 1, fill);
    const auto table = expected.to_bytes();

    std::vector<std::uint8_t> bytes(table.size());
    auto builder = TableBuilder::with_bytes(bytes, 1, TableLayout());
    builder.set_vector(3, 1, fill);
    EXPECT_TRUE(std::ranges::equal(builder.finish(), table));
}

/// A reset that fails, for a version or a header too large, leaves the
/// builder as it was, and one that doesn't starts its table anew, in any
/// layout
TEST(TableBuilder, AResetThatFailsLeavesTheBuilderAsItWas) {
    const auto layout = TableLayout().with_offset_type(OffsetType::uint8);
    TableBuilder expected(2, layout);
    expected.set_uint8(1);
    expected.set_uint8(2);

    TableBuilder builder(2, layout);
    builder.set_uint8(1);
    EXPECT_EQ(build_error_of([&] { builder.reset(1, layout.with_version(256)); }).kind(),
              BuildError::Kind::version_too_large);
    EXPECT_EQ(build_error_of([&] { builder.reset(300, layout); }).kind(), BuildError::Kind::header_too_long);
    builder.set_uint8(2);
    EXPECT_EQ(builder.to_bytes(), expected.to_bytes());

    builder.reset(1, TableLayout().sized().with_version(9));
    builder.set_string("next");
    TableBuilder next(1, TableLayout().sized().with_version(9));
    next.set_string("next");
    EXPECT_TRUE(std::ranges::equal(builder.finish(), next.to_bytes()));
}

/// A builder of no fields that's moved from writes its header alone, and
/// rejects any field
TEST(TableBuilder, AMovedFromBuilderOfNoFieldsWritesItsHeader) {
    const auto layout = TableLayout().sized().with_version(3);
    const auto expected = TableBuilder(0, layout).to_bytes();

    TableBuilder from(0, layout);
    const TableBuilder moved(std::move(from));
    // NOLINTBEGIN(bugprone-use-after-move): the state after it is the point
    EXPECT_EQ(from.byte_length(), 8U);
    EXPECT_EQ(from.to_bytes(), expected);
    TableBuilder parent(1, TableLayout());
    parent.set_table(from);
    TableBuilder expected_parent(1, TableLayout());
    expected_parent.set_table(expected);
    EXPECT_EQ(parent.to_bytes(), expected_parent.to_bytes());

    EXPECT_TRUE(same_error(build_error_of([&] { from.set_uint8(1); }), BuildError::Kind::too_many_fields,
                           OffsetType::uint32, 0, 0));
    EXPECT_TRUE(std::ranges::equal(from.finish(), expected));
    // NOLINTEND(bugprone-use-after-move)
    EXPECT_EQ(moved.to_bytes(), expected);
}

/// A table written in place with an alignment is padded for it before its
/// fields are set: as a builder nested with that alignment is, when its
/// fields need no more, and as one nested with theirs, after it's moved,
/// when they need more. The parent's alignment is the larger.
TEST(TableBuilder, WritesATableInPlaceWithTheAlignmentItsGiven) {
    const std::array<double, 2> doubles{1.5, -0.0};
    const std::array<std::uint16_t, 3> shorts{1, 2, 3};
    const auto fill = [&](TableBuilder::Item& item, bool wide) {
        item.set_uint8(9);
        if (wide) {
            item.set_float64_array(doubles);
        } else {
            item.set_uint16_array(shorts);
        }
    };

    for (const bool wide : {false, true}) {
        for (const auto alignment : {Alignment::one, Alignment::two, Alignment::four, Alignment::eight}) {
            TableBuilder nested(2, TableLayout());
            nested.set_uint8(9);
            if (wide) {
                nested.set_float64_array(doubles);
            } else {
                nested.set_uint16_array(shorts);
            }

            TableBuilder expected(2, TableLayout());
            expected.set_uint8(1);
            expected.set_table(nested, (std::max)(alignment, nested.alignment()));

            TableBuilder builder(2, TableLayout());
            builder.set_uint8(1);
            builder.set_table(2, alignment, [&](TableBuilder::Item& item) { fill(item, wide); });
            EXPECT_EQ(builder.to_bytes(), expected.to_bytes());
            EXPECT_EQ(builder.alignment(), expected.alignment());
        }
    }
}

}  // namespace
}  // namespace tafla::test
